//! AI "does the lab" — given a challenge's task description and whatever code
//! is already in the repo, ask the model for completed files ready to paste
//! straight back into EduCoder and submit.
//!
//! One AI call per challenge, not per 实训 like the report: the point here is
//! working code per关卡, so a failure on one challenge should not cost the
//! others, and the log can show real per-challenge progress.
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::backend::ChatBackend;
use crate::client::EduClient;
use crate::error::{Error, Result};
use crate::exporter::{sanitize, split_paths, Progress, SelectedChallenge, SelectedHomework};
use crate::prompts::{self, SOLVE_END_MARK, SOLVE_FILE_MARK};
use crate::report::unfence;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveRequest {
    pub course_name: String,
    pub dest: String,
    pub folder: Option<String>,
    pub homeworks: Vec<SelectedHomework>,
    /// Freeform extra constraints ("禁止用递归", 语言版本 etc.), all optional.
    #[serde(default)]
    pub requirements: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolvedChallenge {
    pub name: String,
    pub dir: String,
    pub files: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveResult {
    pub dir: String,
    pub challenges: Vec<SolvedChallenge>,
    pub solved: usize,
    pub failed: usize,
    /// One file with every challenge's answer as a fenced code block, so the
    /// student can copy-paste without hunting through the per-关卡 folders.
    pub summary_file: String,
}

/// Fence tag for a file's code block, guessed from its extension. An unknown
/// or missing extension gets a bare ``` fence rather than a wrong guess.
fn lang_for(path: &str) -> &'static str {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "java" => "java",
        "py" => "python",
        "c" => "c",
        "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" => "typescript",
        "jsx" => "jsx",
        "tsx" => "tsx",
        "go" => "go",
        "rb" => "ruby",
        "php" => "php",
        "sql" => "sql",
        "sh" | "bash" => "bash",
        "html" | "htm" => "html",
        "css" => "css",
        "json" => "json",
        "xml" => "xml",
        "yml" | "yaml" => "yaml",
        "kt" | "kts" => "kotlin",
        "cs" => "csharp",
        "swift" => "swift",
        "scala" => "scala",
        "rs" => "rust",
        "r" => "r",
        "lua" => "lua",
        "pl" => "perl",
        _ => "",
    }
}

/// Splits the model's answer into `(path, content)` pairs on the
/// `>>>>> FILE: … / <<<<< END` markers from `prompts.rs`. A missing closing
/// marker on the last file is tolerated — whatever was collected still flushes.
fn parse_files(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cur: Option<(String, Vec<&str>)> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(path) = trimmed.strip_prefix(SOLVE_FILE_MARK) {
            if let Some((p, body)) = cur.take() {
                out.push((p, body.join("\n").trim_matches('\n').to_string()));
            }
            cur = Some((path.trim().to_string(), Vec::new()));
        } else if trimmed == SOLVE_END_MARK {
            if let Some((p, body)) = cur.take() {
                out.push((p, body.join("\n").trim_matches('\n').to_string()));
            }
        } else if let Some((_, body)) = cur.as_mut() {
            body.push(line);
        }
    }
    if let Some((p, body)) = cur.take() {
        out.push((p, body.join("\n").trim_matches('\n').to_string()));
    }
    out
}

/// Keeps a model-supplied path inside the challenge's own directory: drops any
/// leading slash and `..` components rather than trusting it outright.
fn safe_relative(path: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in Path::new(path.trim()).components() {
        if let Component::Normal(p) = comp {
            out.push(p);
        }
    }
    if out.as_os_str().is_empty() {
        out.push("未命名文件.txt");
    }
    out
}

/// Fetches one challenge, asks the model to solve it, and writes the result.
/// Returns the challenge summary plus the `(relative path, content)` pairs
/// actually written, so the caller can fold them into the root summary file.
async fn solve_one(
    client: &EduClient,
    ai: &ChatBackend,
    dir: &Path,
    number: &str,
    selected: &SelectedChallenge,
    requirements: &str,
    p: &Progress,
) -> Result<(SolvedChallenge, Vec<(String, String)>)> {
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

    let mut files = Vec::new();
    for rel in split_paths(ch.get("path").and_then(Value::as_str).unwrap_or_default()) {
        p.check()?;
        match client.get_file_content(&selected.game_id, &rel).await {
            Ok(code) => files.push((rel, code)),
            Err(e) => p.say(format!("      ! {rel} 读取失败: {e}")),
        }
    }

    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("题目.md"), format!("# {name}\n\n{task}\n"))?;

    let prompt = prompts::solve_challenge(&name, &task, &files, requirements);
    let text = ai.chat(prompts::SOLVE_SYSTEM, &prompt).await?;
    let parsed = parse_files(&unfence(&text));

    let saved: Vec<(String, String)> = if parsed.is_empty() {
        // The model ignored the marker format; keep its raw answer rather than
        // losing the call entirely.
        std::fs::write(dir.join("AI 输出（未按格式）.md"), &text)?;
        vec![("AI 输出（未按格式）.md".to_string(), text)]
    } else {
        let mut out = Vec::new();
        for (rel, code) in parsed {
            let safe = safe_relative(&rel);
            let dest = dir.join(&safe);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&dest, &code)?;
            out.push((safe.display().to_string(), code));
        }
        out
    };

    p.say(format!("      ✓ {number} {name}（{} 个文件）", saved.len()));
    let files = saved.iter().map(|(path, _)| path.clone()).collect();
    Ok((SolvedChallenge { name, dir: dir.display().to_string(), files, error: None }, saved))
}

/// The whole run: one AI call per selected 关卡, best-effort past the first.
pub async fn solve(
    client: &EduClient,
    ai: &ChatBackend,
    req: &SolveRequest,
    p: &Progress,
) -> Result<SolveResult> {
    p.check()?;
    if req.homeworks.is_empty() {
        return Err(Error::msg("没有选择任何关卡。"));
    }

    let folder = req
        .folder
        .as_deref()
        .filter(|f| !f.trim().is_empty())
        .map(sanitize)
        .unwrap_or_else(|| sanitize(&format!("{}-AI答案", req.course_name)));
    let dir: PathBuf = Path::new(&req.dest).join(folder);
    std::fs::create_dir_all(&dir)?;

    let mut results = Vec::new();
    let mut solved = 0usize;
    let mut failed = 0usize;
    let mut summary = format!(
        "# {} · AI 做实验答案\n\n> 本文件由 AI 生成，供复制粘贴到平台使用；请核对后再提交。\n\n",
        req.course_name
    );

    for (i, hw) in req.homeworks.iter().enumerate() {
        p.check()?;
        let no = i + 1;
        p.say(format!("# {no} {}（{} 关）", hw.name, hw.challenges.len()));
        let hw_dir = dir.join(sanitize(&format!("{no:02}_{}", hw.name)));
        summary.push_str(&format!("## {no} {}\n\n", hw.name));

        for (j, c) in hw.challenges.iter().enumerate() {
            p.check()?;
            let number = format!("{no}.{}", j + 1);
            let label =
                c.position.map(|n| format!("{n:02}")).unwrap_or_else(|| format!("{:02}", j + 1));
            let c_dir = hw_dir.join(sanitize(&format!("{label}_{}", c.name)));
            p.say(format!("    → 正在生成 {number} {}…（{}）", c.name, ai.label()));
            match solve_one(client, ai, &c_dir, &number, c, &req.requirements, p).await {
                Ok((r, saved)) => {
                    solved += 1;
                    summary.push_str(&format!("### {number} {}\n\n", r.name));
                    for (path, code) in &saved {
                        summary.push_str(&format!(
                            "`{path}`\n\n```{}\n{code}\n```\n\n",
                            lang_for(path)
                        ));
                    }
                    results.push(r);
                }
                Err(e) if e.message == "已取消" => return Err(e),
                Err(e) => {
                    // A failure on the very first call almost always means the
                    // key/URL/model is wrong — stop instead of burning quota.
                    if solved == 0 && failed == 0 {
                        return Err(e);
                    }
                    p.say(format!("    ! {} 生成失败: {e}", c.name));
                    failed += 1;
                    summary.push_str(&format!("### {number} {}\n\n> ⚠️ 生成失败：{e}\n\n", c.name));
                    results.push(SolvedChallenge {
                        name: c.name.clone(),
                        dir: c_dir.display().to_string(),
                        files: Vec::new(),
                        error: Some(e.message.clone()),
                    });
                }
            }
        }
    }

    let summary_file = dir.join("答案汇总.md");
    std::fs::write(&summary_file, &summary)?;

    p.say(format!("完成：{solved} 关成功，{failed} 关失败"));
    Ok(SolveResult {
        dir: dir.display().to_string(),
        challenges: results,
        solved,
        failed,
        summary_file: summary_file.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_files_splits_on_markers() {
        let text = format!(
            "{}a/B.java\n第一行\n第二行\n{}\n{}c.py\nprint(1)\n{}",
            SOLVE_FILE_MARK, SOLVE_END_MARK, SOLVE_FILE_MARK, SOLVE_END_MARK
        );
        let files = parse_files(&text);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], ("a/B.java".to_string(), "第一行\n第二行".to_string()));
        assert_eq!(files[1], ("c.py".to_string(), "print(1)".to_string()));
    }

    #[test]
    fn parse_files_tolerates_a_missing_final_end_marker() {
        let text = format!("{}x.txt\nhello", SOLVE_FILE_MARK);
        let files = parse_files(&text);
        assert_eq!(files, vec![("x.txt".to_string(), "hello".to_string())]);
    }

    #[test]
    fn parse_files_returns_empty_when_the_model_ignored_the_format() {
        assert!(parse_files("好的，这是代码：\n```java\nclass A {}\n```").is_empty());
    }

    #[test]
    fn lang_for_maps_known_extensions_and_falls_back_to_bare_fence() {
        assert_eq!(lang_for("src/Main.java"), "java");
        assert_eq!(lang_for("a/b.CPP"), "cpp");
        assert_eq!(lang_for("query.sql"), "sql");
        assert_eq!(lang_for("Makefile"), "");
        assert_eq!(lang_for("data.weird"), "");
    }

    #[test]
    fn safe_relative_strips_traversal_and_absolute_roots() {
        assert_eq!(safe_relative("../../etc/passwd"), PathBuf::from("etc/passwd"));
        assert_eq!(safe_relative("/etc/passwd"), PathBuf::from("etc/passwd"));
        assert_eq!(safe_relative("src/Main.java"), PathBuf::from("src/Main.java"));
        assert_eq!(safe_relative("   "), PathBuf::from("未命名文件.txt"));
    }
}
