//! AI-generated course practice report.
//!
//! Two halves:
//!   * [`build_tree`] — course → 实训 → 关卡, so the UI can offer a checkbox tree;
//!   * [`generate`]   — fetch the selected challenges, save them as material
//!     files, then drive the prompts in `prompts.rs` one 实训 at a time and
//!     assemble the pieces into a single `报告.md`.
//!
//! Generation is deliberately one AI call per section rather than one giant
//! call: it keeps each request inside a small model's context, lets the log
//! show real progress, and means one bad section does not cost the whole run.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::backend::ChatBackend;
use crate::client::EduClient;
use crate::error::{Error, Result};
use crate::exporter::{sanitize, split_paths, Progress, SelectedChallenge, SelectedHomework};
use crate::prompts::{self, ChallengeInput};

// ---------------------------------------------------------------- the tree

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeChallenge {
    pub position: Option<i64>,
    pub name: String,
    pub game_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeHomework {
    pub name: String,
    pub shixun_id: Option<String>,
    pub myshixun_id: Option<String>,
    pub challenges: Vec<TreeChallenge>,
    /// Why this 实训 has no challenges to offer, when that is the case.
    pub skipped: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportTree {
    pub course_name: String,
    pub homeworks: Vec<TreeHomework>,
}

/// Course → 实训 → 关卡. A 实训 the student never entered has no instance and
/// therefore no game ids; it is listed with `skipped` set rather than dropped,
/// so the UI can say why it cannot be picked.
pub async fn build_tree(client: &EduClient, course_id: &str, p: &Progress) -> Result<ReportTree> {
    p.check()?;
    let course_name = client
        .get(&format!("/api/courses/{course_id}.json"))
        .await
        .ok()
        .and_then(|c| c.get("name").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| course_id.to_string());

    let hw = client.get_homeworks(course_id, 4, 1, 50).await?;
    let homeworks = hw.get("homeworks").and_then(Value::as_array).cloned().unwrap_or_default();

    let mut out = Vec::new();
    for h in &homeworks {
        p.check()?;
        if h.get("is_shixun").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let name = h
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("未命名作业")
            .to_string();
        let shixun_id = h.get("shixun_identifier").and_then(Value::as_str).map(str::to_string);
        let myshixun_id =
            h.get("myshixun_identifier").and_then(Value::as_str).map(str::to_string);

        let Some(mysh) = myshixun_id.clone() else {
            p.say(format!("  {name}：未进入，跳过"));
            out.push(TreeHomework {
                name,
                shixun_id,
                myshixun_id: None,
                challenges: Vec::new(),
                skipped: Some("尚未进入该实训，没有你的关卡实例".into()),
            });
            continue;
        };

        match client.get_my_challenges(&mysh).await {
            Ok(games) => {
                let challenges = games
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|g| {
                        Some(TreeChallenge {
                            position: g.get("position").and_then(Value::as_i64),
                            name: g
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("未命名关卡")
                                .to_string(),
                            game_id: g.get("identifier").and_then(Value::as_str)?.to_string(),
                        })
                    })
                    .collect::<Vec<_>>();
                p.say(format!("  {name}：{} 关", challenges.len()));
                out.push(TreeHomework {
                    name,
                    shixun_id,
                    myshixun_id: Some(mysh),
                    challenges,
                    skipped: None,
                });
            }
            Err(e) => {
                p.say(format!("  {name}：读取关卡失败 {e}"));
                out.push(TreeHomework {
                    name,
                    shixun_id,
                    myshixun_id: Some(mysh),
                    challenges: Vec::new(),
                    skipped: Some(format!("读取关卡失败: {e}")),
                });
            }
        }
    }

    Ok(ReportTree { course_name, homeworks: out })
}

// ------------------------------------------------------------- generation

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    pub course_name: String,
    pub dest: String,
    pub folder: Option<String>,
    pub homeworks: Vec<SelectedHomework>,
    #[serde(default)]
    pub task_book: String,
    #[serde(default)]
    pub requirements: String,
    #[serde(default)]
    pub template: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportResult {
    pub dir: String,
    pub file: String,
    pub sections: usize,
    /// Sections whose AI call failed; their placeholder is in the report.
    pub failed: Vec<String>,
    /// How many spots the student still has to fill in by hand.
    pub placeholders: usize,
    pub materials: usize,
}

/// Models sometimes wrap the whole answer in a fence despite being told not to.
fn unfence(text: &str) -> String {
    let t = text.trim();
    let Some(rest) = t.strip_prefix("```") else {
        return t.to_string();
    };
    // Drop the info string on the opening fence, and the closing fence.
    let Some((_, body)) = rest.split_once('\n') else {
        return t.to_string();
    };
    match body.rsplit_once("```") {
        Some((inner, tail)) if tail.trim().is_empty() => inner.trim_end().to_string(),
        _ => t.to_string(),
    }
}

/// Spots the student still has to fill in, per the prompt's two markers.
pub fn count_placeholders(md: &str) -> usize {
    md.matches("待插入截图").count() + md.matches("待补充运行结果").count()
}

/// Fetches one challenge and writes it under `材料目录`, returning what the
/// prompt needs. A challenge whose code cannot be read still contributes its
/// task description — the prompt says to note the gap rather than invent code.
async fn collect_challenge(
    client: &EduClient,
    dir: &Path,
    number: &str,
    selected: &SelectedChallenge,
    p: &Progress,
) -> Result<ChallengeInput> {
    p.check()?;
    let t = client.get_task(&selected.game_id).await?;
    let ch = t.get("challenge").cloned().unwrap_or(Value::Null);
    let name = ch
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or(&selected.name)
        .to_string();
    let task = ch
        .get("task_pass")
        .and_then(Value::as_str)
        .unwrap_or("(无任务描述)")
        .to_string();

    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("题目.md"), format!("# {name}\n\n{task}\n"))?;

    let mut files = Vec::new();
    for rel in split_paths(ch.get("path").and_then(Value::as_str).unwrap_or_default()) {
        p.check()?;
        match client.get_file_content(&selected.game_id, &rel).await {
            Ok(code) => {
                let dest = dir.join(Path::new(&rel).file_name().unwrap_or_default());
                std::fs::write(&dest, &code)?;
                files.push((rel, code));
            }
            Err(e) => p.say(format!("      ! {rel} 读取失败: {e}")),
        }
    }
    p.say(format!("      ✓ {number} {name}（{} 个代码文件）", files.len()));
    Ok(ChallengeInput { number: number.to_string(), name, task, files })
}

/// The whole run: materials, then one AI call per 实训, then chapters 1 and 3.
pub async fn generate(
    client: &EduClient,
    ai: &ChatBackend,
    req: &ReportRequest,
    p: &Progress,
) -> Result<ReportResult> {
    p.check()?;
    if req.homeworks.is_empty() {
        return Err(Error::msg("没有选择任何关卡。"));
    }

    let folder = req
        .folder
        .as_deref()
        .filter(|f| !f.trim().is_empty())
        .map(sanitize)
        .unwrap_or_else(|| sanitize(&format!("{}-实践报告", req.course_name)));
    let dir: PathBuf = Path::new(&req.dest).join(folder);
    std::fs::create_dir_all(dir.join("images"))?;
    let materials_dir = dir.join("素材");
    std::fs::create_dir_all(&materials_dir)?;

    // The report requirements ask for the template's structure, not its
    // typography; both blobs go to the model as one "how to write it" context.
    let requirements = if req.template.trim().is_empty() {
        req.requirements.clone()
    } else {
        format!(
            "{}\n\n=== 报告模板（据此确定章节结构；字体、行距、页码等排版要求请忽略）===\n{}",
            req.requirements, req.template
        )
    };

    let names: Vec<String> = req.homeworks.iter().map(|h| h.name.clone()).collect();
    let mut sections: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let mut digest = String::new();
    let mut materials = 0usize;

    for (i, hw) in req.homeworks.iter().enumerate() {
        p.check()?;
        let no = i + 1;
        p.say(format!("# 2.{no} {}（{} 关）", hw.name, hw.challenges.len()));

        let hw_dir = materials_dir.join(sanitize(&format!("{no:02}_{}", hw.name)));
        let mut inputs = Vec::new();
        digest.push_str(&format!("\n## {}\n", hw.name));

        for (j, c) in hw.challenges.iter().enumerate() {
            let number = format!("{no}.{}", j + 1);
            let label = c.position.map(|n| format!("{n:02}")).unwrap_or_else(|| format!("{:02}", j + 1));
            let c_dir = hw_dir.join(sanitize(&format!("{label}_{}", c.name)));
            match collect_challenge(client, &c_dir, &number, c, p).await {
                Ok(input) => {
                    digest.push_str(&format!("- {}\n", input.name));
                    materials += 1;
                    inputs.push(input);
                }
                Err(e) if e.message == "已取消" => return Err(e),
                Err(e) => p.say(format!("      ! {} 取材失败: {e}", c.name)),
            }
        }

        if inputs.is_empty() {
            p.say("    ! 本实训没有取到任何材料，跳过");
            failed.push(hw.name.clone());
            sections.push(format!(
                "### 2.{no} {}\n\n> ⚠️ 本节素材获取失败，未能生成内容。\n",
                hw.name
            ));
            continue;
        }

        p.say(format!("    → 正在生成第 2.{no} 节（{}）…", ai.label()));
        let prompt = prompts::section(
            &req.course_name,
            no,
            &hw.name,
            hw.total.max(inputs.len()),
            &inputs,
            &req.task_book,
            &requirements,
        );
        match ai.chat(prompts::SYSTEM, &prompt).await {
            Ok(text) => {
                let text = unfence(&text);
                p.say(format!("    ✓ 第 2.{no} 节完成（{} 字）", text.chars().count()));
                sections.push(text);
            }
            Err(e) => {
                // A failure on the very first section almost always means the
                // key/URL/model is wrong — stop instead of burning the quota.
                if sections.is_empty() {
                    return Err(e);
                }
                p.say(format!("    ! 第 2.{no} 节生成失败: {e}"));
                failed.push(hw.name.clone());
                sections.push(format!(
                    "### 2.{no} {}\n\n> ⚠️ 本节自动生成失败：{e}\n>\n> 素材已保存在 `素材/` 目录下，可手动补写或重试。\n",
                    hw.name
                ));
            }
        }
    }

    p.check()?;
    p.say("→ 正在生成第 2 章绪言…");
    let preamble = ai
        .chat(prompts::SYSTEM, &prompts::chapter2_preamble(&req.course_name, &names))
        .await
        .map(|t| unfence(&t))
        .unwrap_or_else(|e| {
            p.say(format!("  ! 绪言生成失败: {e}"));
            String::new()
        });

    p.check()?;
    p.say("→ 正在生成第 1 章 课程任务概述…");
    let intro = ai
        .chat(
            prompts::SYSTEM,
            &prompts::introduction(&req.course_name, &names, &req.task_book),
        )
        .await
        .map(|t| unfence(&t))
        .unwrap_or_else(|e| {
            p.say(format!("  ! 第 1 章生成失败: {e}"));
            failed.push("第 1 章 课程任务概述".into());
            format!("## 1 课程任务概述\n\n> ⚠️ 本章自动生成失败：{e}\n")
        });

    p.check()?;
    p.say("→ 正在生成第 3 章 课程总结…");
    let conclusion = ai
        .chat(
            prompts::SYSTEM,
            &prompts::conclusion(&req.course_name, &digest, &requirements),
        )
        .await
        .map(|t| unfence(&t))
        .unwrap_or_else(|e| {
            p.say(format!("  ! 第 3 章生成失败: {e}"));
            failed.push("第 3 章 课程总结".into());
            format!("## 3 课程总结\n\n> ⚠️ 本章自动生成失败：{e}\n")
        });

    let markdown = assemble(&req.course_name, &intro, &preamble, &sections, &conclusion);
    let file = dir.join("报告.md");
    std::fs::write(&file, &markdown)?;
    std::fs::write(materials_dir.join("说明.md"), materials_readme(&req.course_name))?;

    let placeholders = count_placeholders(&markdown);
    p.say(format!(
        "完成：{} 节，{} 份素材，{} 处待手动补充",
        sections.len(),
        materials,
        placeholders
    ));

    Ok(ReportResult {
        dir: dir.display().to_string(),
        file: file.display().to_string(),
        sections: sections.len(),
        failed,
        placeholders,
        materials,
    })
}

/// Stitches the chapters together in the template's order.
fn assemble(
    course_name: &str,
    intro: &str,
    preamble: &str,
    sections: &[String],
    conclusion: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", report_title(course_name)));
    out.push_str("> 本文由 EduCoder Helper 依据平台上的关卡任务描述与你提交的代码自动起草。\n");
    out.push_str("> 带 🖼️ / 📋 标记的段落需要你补充截图和运行结果，📝 标记处需要你填写个人信息。\n");
    out.push_str("> 请务必通读全文核对内容后再提交。\n\n");
    out.push_str("> 📝 **待填写**：专业、班级、学号、姓名、指导教师、完成日期。\n\n");
    out.push_str("---\n\n");
    out.push_str(intro.trim());
    out.push_str("\n\n## 2 任务实施过程与分析\n\n");
    if !preamble.trim().is_empty() {
        out.push_str(preamble.trim());
        out.push_str("\n\n");
    }
    for s in sections {
        out.push_str(s.trim());
        out.push_str("\n\n");
    }
    out.push_str(conclusion.trim());
    out.push_str("\n\n## 附录\n\n");
    out.push_str(
        "> 📋 **待补充运行结果**：附录为可选项。若正文因篇幅限制未能展示的结果表格、图片、复杂流程图，可放在这里；若无必要可整节删除。\n",
    );
    out
}

/// Course names already carry words like "实践" / "实验"; blindly appending
/// "实践报告" produced titles such as "…实践实践报告".
fn report_title(course_name: &str) -> String {
    let name = course_name.trim();
    if name.is_empty() {
        return "课程实践报告".to_string();
    }
    if name.ends_with("报告") {
        return name.to_string();
    }
    if name.ends_with("实践") || name.ends_with("实验") {
        return format!("{name}报告");
    }
    format!("{name}实践报告")
}

fn materials_readme(course_name: &str) -> String {
    format!(
        "# {course_name} 素材目录\n\n\
         本目录由 EduCoder Helper 自动导出，按「实训 / 关卡」两级组织：\n\n\
         - `NN_实训名称/` —— 一个实训任务\n\
         - `NN_实训名称/NN_关卡名称/题目.md` —— 该关卡在平台上的任务描述\n\
         - `NN_实训名称/NN_关卡名称/<源文件>` —— 该关卡提交的代码\n\n\
         上一级目录的 `报告.md` 即依据这些素材起草。提交作业时，本目录可作为\n\
         「存储所有源代码的目录」一并打包。\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn unfence_strips_a_wrapping_code_fence() {
        assert_eq!(unfence("```markdown\n# 标题\n正文\n```"), "# 标题\n正文");
        assert_eq!(unfence("```\nabc\n```"), "abc");
        // Untouched when there is no wrapper…
        assert_eq!(unfence("# 标题\n正文"), "# 标题\n正文");
        // …and when the fence is an inner code block rather than a wrapper.
        let inner = "正文\n\n```sql\nselect 1;\n```\n\n后续";
        assert_eq!(unfence(inner), inner);
    }

    #[test]
    fn counts_both_placeholder_markers() {
        let md = "a 待插入截图 b 待补充运行结果 c 待插入截图";
        assert_eq!(count_placeholders(md), 3);
        assert_eq!(count_placeholders("干净的正文"), 0);
    }

    #[test]
    fn assemble_orders_chapters_and_keeps_section_text() {
        let md = assemble(
            "数据库系统原理",
            "## 1 课程任务概述\n概述正文",
            "绪言正文",
            &["### 2.1 视图\n正文".to_string()],
            "## 3 课程总结\n总结正文",
        );
        let one = md.find("## 1 课程任务概述").unwrap();
        let two = md.find("## 2 任务实施过程与分析").unwrap();
        let sec = md.find("### 2.1 视图").unwrap();
        let three = md.find("## 3 课程总结").unwrap();
        let app = md.find("## 附录").unwrap();
        assert!(one < two && two < sec && sec < three && three < app, "{md}");
        assert!(md.contains("绪言正文"));
        assert!(md.starts_with("# 数据库系统原理实践报告"), "{md}");
    }

    #[test]
    fn report_title_does_not_stutter() {
        assert_eq!(report_title("数据库系统原理实践"), "数据库系统原理实践报告");
        assert_eq!(report_title("数据库系统原理"), "数据库系统原理实践报告");
        assert_eq!(report_title("计算机组成实验"), "计算机组成实验报告");
        assert_eq!(report_title("某课程实践报告"), "某课程实践报告");
        assert_eq!(report_title("  "), "课程实践报告");
    }
}
