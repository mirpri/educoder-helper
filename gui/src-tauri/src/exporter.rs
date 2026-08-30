//! Export EduCoder shixun code + task descriptions to local files.
//! Port of `src/exporter.js`.
//!
//! Layout produced:
//!   `<dir>/README.md`      (challenge task description)
//!   `<dir>/<repo-path...>` (editable files, repo structure preserved)
//! Shixun export nests challenges as `<dir>/NN_<name>/...`
//! Course export nests shixuns as   `<dir>/<homework name>/...`
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::client::EduClient;
use crate::error::{Error, Result};

/// Progress sink + cancellation, supplied by the command layer so exports can
/// stream a live log into the UI and be stopped mid-run. Owns its pieces so the
/// futures that carry it stay `'static`.
pub struct Progress {
    log: Box<dyn Fn(&str) + Send + Sync>,
    cancel: Arc<AtomicBool>,
}

impl Progress {
    pub fn new(log: Box<dyn Fn(&str) + Send + Sync>, cancel: Arc<AtomicBool>) -> Self {
        Self { log, cancel }
    }

    fn say(&self, msg: impl AsRef<str>) {
        (self.log)(msg.as_ref());
    }

    fn check(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(Error::msg("已取消"));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeResult {
    pub name: Option<String>,
    pub dir: String,
    pub files: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeEntry {
    pub position: Option<i64>,
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShixunResult {
    pub dir: String,
    pub challenges: Vec<ChallengeEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeworkEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenges: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseResult {
    pub dir: String,
    pub summary: Vec<HomeworkEntry>,
}

/// Mirrors the JS `sanitize`: strip characters that are illegal in a filename
/// (plus space and `-`), collapse whitespace runs, and never return "".
fn sanitize(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | ' ' | '-' => '_',
            c => c,
        })
        .collect();
    // Whitespace other than the plain space survived the map above; collapse
    // each run to one space, then trim — the JS `.replace(/\s+/g, ' ').trim()`.
    let mut collapsed = String::with_capacity(replaced.len());
    let mut pending_space = false;
    for c in replaced.chars() {
        if c.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space {
                collapsed.push(' ');
                pending_space = false;
            }
            collapsed.push(c);
        }
    }
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// `challenge.path` holds the editable file(s), separated by `;` or `；`,
/// sometimes with a trailing separator.
fn split_paths(p: &str) -> Vec<String> {
    p.split([';', '；'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Joins a repo-relative path under `dir`, refusing to escape it. The API's
/// paths are trusted-ish, but a `..` in one must not write outside the export.
fn safe_join(dir: &Path, rel: &str) -> Result<PathBuf> {
    let mut out = dir.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return Err(Error::msg(format!("拒绝写入越界路径: {rel}"))),
        }
    }
    if out == dir {
        return Err(Error::msg(format!("非法文件路径: {rel}")));
    }
    Ok(out)
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn write_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok(())
}

/// Export ONE challenge (by its game identifier) into `base/<name>`.
/// Writes README.md (task description) + each editable file at its repo path.
pub async fn export_challenge(
    client: &EduClient,
    game_identifier: &str,
    base: &Path,
    name: Option<&str>,
    p: &Progress,
) -> Result<ChallengeResult> {
    p.check()?;
    let t = client.get_task(game_identifier).await?;
    let ch = t.get("challenge").cloned().unwrap_or(Value::Null);
    let subject = str_field(&ch, "subject");

    let dir = match name {
        Some(n) => base.join(sanitize(n)),
        None => {
            let pos = ch
                .get("position")
                .and_then(Value::as_i64)
                .map(|n| format!("{n:02}_"))
                .unwrap_or_default();
            let label = subject.clone().unwrap_or_else(|| game_identifier.to_string());
            base.join(sanitize(&format!("{pos}{label}")))
        }
    };
    std::fs::create_dir_all(&dir)?;

    let title = subject.clone().unwrap_or_else(|| game_identifier.to_string());
    let task_pass = str_field(&ch, "task_pass").unwrap_or_else(|| "(无任务描述)".to_string());
    write_file(&dir.join("README.md"), &format!("# {title}\n\n{task_pass}\n"))?;

    let mut files = Vec::new();
    for rel in split_paths(&str_field(&ch, "path").unwrap_or_default()) {
        p.check()?;
        let code = match client.get_file_content(game_identifier, &rel).await {
            Ok(code) => code,
            Err(e) => {
                p.say(format!("    ! {rel} 读取失败: {e}"));
                format!("/* 读取失败: {e} */\n")
            }
        };
        match safe_join(&dir, &rel) {
            Ok(dest) => {
                write_file(&dest, &code)?;
                p.say(format!("    ✓ {rel}"));
                files.push(rel);
            }
            Err(e) => p.say(format!("    ! {e}")),
        }
    }

    Ok(ChallengeResult { name: subject, dir: dir.display().to_string(), files })
}

/// Export a whole shixun (by myshixun identifier) into `base/<name>`, one
/// folder per 关.
pub async fn export_shixun(
    client: &EduClient,
    myshixun_identifier: &str,
    base: &Path,
    name: Option<&str>,
    p: &Progress,
) -> Result<ShixunResult> {
    p.check()?;
    let games = client.get_my_challenges(myshixun_identifier).await?;
    let games = games.as_array().cloned().unwrap_or_default();

    let dir = match name {
        Some(n) => base.join(sanitize(n)),
        None => {
            let derived = match games.first().and_then(|g| str_field(g, "identifier")) {
                Some(first) => client
                    .get_task(&first)
                    .await
                    .ok()
                    .and_then(|t| t.get("shixun").and_then(|s| str_field(s, "name")))
                    .unwrap_or_else(|| myshixun_identifier.to_string()),
                None => myshixun_identifier.to_string(),
            };
            base.join(sanitize(&derived))
        }
    };
    std::fs::create_dir_all(&dir)?;

    let mut challenges = Vec::new();
    for g in &games {
        p.check()?;
        let position = g.get("position").and_then(Value::as_i64);
        let game_name = str_field(g, "name");
        let pos_label = position.map(|n| format!("{n:02}")).unwrap_or_else(|| "??".into());
        p.say(format!("  {pos_label} {}", game_name.clone().unwrap_or_default()));

        let Some(identifier) = str_field(g, "identifier") else {
            challenges.push(ChallengeEntry {
                position,
                name: game_name,
                files: None,
                error: Some("关卡缺少 identifier".into()),
            });
            continue;
        };
        let sub_name = format!("{pos_label}_{}", game_name.clone().unwrap_or_default());
        match export_challenge(client, &identifier, &dir, Some(&sub_name), p).await {
            Ok(r) => challenges.push(ChallengeEntry {
                position,
                name: r.name.or(game_name),
                files: Some(r.files),
                error: None,
            }),
            Err(e) => {
                p.say(format!("    ! {pos_label} 导出失败: {e}"));
                if e.message == "已取消" {
                    return Err(e);
                }
                challenges.push(ChallengeEntry {
                    position,
                    name: game_name,
                    files: None,
                    error: Some(e.message),
                });
            }
        }
    }

    Ok(ShixunResult { dir: dir.display().to_string(), challenges })
}

/// Export every 实训作业 (shixun homework) of a course into `base/<name>`, one
/// folder per homework. Shixuns without an instance are entered first, which
/// creates a fresh instance from the template.
pub async fn export_course(
    client: &EduClient,
    course_id: &str,
    base: &Path,
    name: Option<&str>,
    enter_if_needed: bool,
    p: &Progress,
) -> Result<CourseResult> {
    p.check()?;
    let dir = match name {
        Some(n) => base.join(sanitize(n)),
        None => {
            let derived = client
                .get(&format!("/api/courses/{course_id}.json"))
                .await
                .ok()
                .and_then(|c| str_field(&c, "name"))
                .unwrap_or_else(|| course_id.to_string());
            base.join(sanitize(&derived))
        }
    };
    std::fs::create_dir_all(&dir)?;

    let hw = client.get_homeworks(course_id, 4, 1, 50).await?;
    let homeworks = hw
        .get("homeworks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut summary = Vec::new();
    for h in &homeworks {
        p.check()?;
        if h.get("is_shixun").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let hw_name = str_field(h, "name").unwrap_or_else(|| "未命名作业".into());
        p.say(format!("# {hw_name}"));

        let mut mysh = str_field(h, "myshixun_identifier");
        if mysh.is_none() && enter_if_needed {
            if let Some(shixun) = str_field(h, "shixun_identifier") {
                match enter_and_resolve(client, &shixun).await {
                    Ok(id) => {
                        p.say(format!("  (新建实例 {id})"));
                        mysh = Some(id);
                    }
                    Err(e) => p.say(format!("  跳过 (无法进入: {e})")),
                }
            }
        }

        let Some(mysh) = mysh else {
            summary.push(HomeworkEntry {
                name: hw_name,
                challenges: None,
                skipped: Some("no instance".into()),
                error: None,
            });
            continue;
        };

        match export_shixun(client, &mysh, &dir, Some(&hw_name), p).await {
            Ok(r) => summary.push(HomeworkEntry {
                name: hw_name,
                challenges: Some(r.challenges.len()),
                skipped: None,
                error: None,
            }),
            Err(e) => {
                p.say(format!("  ! 导出失败: {e}"));
                if e.message == "已取消" {
                    return Err(e);
                }
                summary.push(HomeworkEntry {
                    name: hw_name,
                    challenges: None,
                    skipped: None,
                    error: Some(e.message),
                });
            }
        }
    }

    Ok(CourseResult { dir: dir.display().to_string(), summary })
}

/// Enter a shixun and read back the myshixun identifier of the new instance.
async fn enter_and_resolve(client: &EduClient, shixun_identifier: &str) -> Result<String> {
    let e = client.enter_shixun(shixun_identifier).await?;
    let game = str_field(&e, "game_identifier")
        .ok_or_else(|| Error::msg("进入实训的响应中没有 game_identifier"))?;
    let t = client.get_task(&game).await?;
    t.get("myshixun")
        .and_then(|m| str_field(m, "identifier"))
        .ok_or_else(|| Error::msg("关卡详情中没有 myshixun.identifier"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expectations captured from the Node `sanitize` in src/exporter.js, so the
    // GUI and the CLI produce byte-identical folder names.
    #[test]
    fn sanitize_matches_js_behaviour() {
        assert_eq!(sanitize("第1关: Hello/World"), "第1关__Hello_World");
        assert_eq!(sanitize("a-b c"), "a_b_c");
        assert_eq!(sanitize(""), "unnamed");
        assert_eq!(sanitize(" x "), "_x_");
        assert_eq!(sanitize("a  b"), "a__b");
        // Spaces become '_' before the \s+ collapse, so a blank name is "___".
        assert_eq!(sanitize("   "), "___");
        // Tabs survive the character class and are collapsed to one space.
        assert_eq!(sanitize("a\tb"), "a b");
    }

    #[test]
    fn splits_on_both_semicolons() {
        assert_eq!(split_paths("a.c；b.c; ;"), vec!["a.c", "b.c"]);
        assert!(split_paths("").is_empty());
    }

    #[test]
    fn safe_join_rejects_traversal() {
        let base = Path::new("/tmp/x");
        assert_eq!(safe_join(base, "src/a.c").unwrap(), base.join("src").join("a.c"));
        assert!(safe_join(base, "../escape").is_err());
        assert!(safe_join(base, "/etc/passwd").is_err());
    }
}
