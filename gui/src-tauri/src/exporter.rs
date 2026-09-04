//! Export EduCoder shixun code + task descriptions to local files.
//! Port of `src/exporter.js`.
//!
//! Layout produced:
//!   `<dir>/README.md`      (challenge task description)
//!   `<dir>/<repo-path...>` (editable files, repo structure preserved)
//!   `<dir>/images/<id>.<ext>` (task-description assets, `ImageMode::Download`)
//! Shixun export nests challenges as `<dir>/NN_<name>/...`
//! Course export nests shixuns as   `<dir>/<homework name>/...`
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
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

    pub(crate) fn say(&self, msg: impl AsRef<str>) {
        (self.log)(msg.as_ref());
    }

    pub(crate) fn check(&self) -> Result<()> {
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
    /// Assets saved next to the README, when `ImageMode::Download` was used.
    pub images: Vec<String>,
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

/// One 关卡 the user ticked in the selection tree. Shared by the 导出 page and
/// the 实验报告 page — both let you pick across a whole course.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedChallenge {
    pub position: Option<i64>,
    pub name: String,
    pub game_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedHomework {
    pub name: String,
    /// How many challenges the 实训 has in total, so a report section can say
    /// "选取其中 N 个" honestly when only some were picked.
    #[serde(default)]
    pub total: usize,
    pub challenges: Vec<SelectedChallenge>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseResult {
    pub dir: String,
    pub summary: Vec<HomeworkEntry>,
}

/// What to do with the `/api/attachments/...` references inside a task
/// description. The platform emits them site-relative, so a markdown viewer on
/// your disk resolves them against the local filesystem and shows a broken
/// image — hence the two repair modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageMode {
    /// Leave the markdown untouched.
    Keep,
    /// Rewrite to an absolute https URL, which any viewer loads over the network.
    #[default]
    Link,
    /// Fetch each asset into `<dir>/images/` and point at it relatively.
    Download,
}

const ATTACHMENT_MARKER: &str = "/api/attachments/";

/// Characters that can only sit outside a URL: markdown `](url)`, an HTML
/// `src="url"` and a bare URL all end at one of these.
fn is_url_delim(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '(' | ')' | '<' | '>' | ']')
}

/// Byte spans of every attachment URL in `md`, in order and non-overlapping.
/// Each span grows left over any `https://host` prefix and right over the id
/// and its `?query`.
fn attachment_spans(md: &str) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for (pos, _) in md.match_indices(ATTACHMENT_MARKER) {
        let mut lo = pos;
        for (i, c) in md[..pos].char_indices().rev() {
            if is_url_delim(c) {
                break;
            }
            lo = i;
        }
        let id_start = pos + ATTACHMENT_MARKER.len();
        let mut hi = id_start;
        for (i, c) in md[id_start..].char_indices() {
            if is_url_delim(c) {
                break;
            }
            hi = id_start + i + c.len_utf8();
        }
        if hi == id_start {
            continue; // marker with no id after it
        }
        if spans.last().is_some_and(|&(_, prev_hi)| lo < prev_hi) {
            continue; // overlaps the previous match
        }
        spans.push((lo, hi));
    }
    spans
}

/// Rebuilds `md` with each span replaced by its mapping. One pass over the
/// original, so a short id that prefixes a longer one cannot be clobbered and
/// substituted text is never rescanned.
fn splice(md: &str, spans: &[(usize, usize)], map: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(md.len());
    let mut last = 0;
    for &(lo, hi) in spans {
        out.push_str(&md[last..lo]);
        match map.get(&md[lo..hi]) {
            Some(replacement) => out.push_str(replacement),
            None => out.push_str(&md[lo..hi]),
        }
        last = hi;
    }
    out.push_str(&md[last..]);
    out
}

/// The id part of an attachment URL: everything after the marker, minus any
/// `?query`. Usually numeric, sometimes a base64 blob.
fn attachment_id(url: &str) -> &str {
    let after = url.split_once(ATTACHMENT_MARKER).map_or("", |(_, tail)| tail);
    after.split('?').next().unwrap_or("")
}

/// Local basename for an attachment. Numeric ids are already filename-safe and
/// stay readable; anything else (base64 ids can carry `/ + =`) becomes a short
/// hash, so the name is deterministic and legal on every platform.
fn asset_name(id: &str) -> String {
    if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
        return id.to_string();
    }
    let digest = Md5::digest(id.as_bytes());
    format!("{digest:x}").chars().take(10).collect()
}

/// Extension for a downloaded asset: the Content-Type first, then whatever the
/// Content-Disposition filename carried.
fn ext_for(content_type: &str, filename: &str) -> String {
    let by_type = match content_type {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "image/bmp" => Some("bmp"),
        "image/avif" => Some("avif"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some("ico"),
        _ => None,
    };
    if let Some(ext) = by_type {
        return format!(".{ext}");
    }
    let from_name = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| !e.is_empty() && e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()));
    match from_name {
        Some(e) => format!(".{e}"),
        None if content_type.starts_with("image/") => ".img".to_string(),
        None => ".bin".to_string(),
    }
}

/// Rewrites every attachment reference in `md` according to `mode`. In download
/// mode a failed fetch keeps its original URL, so the README never points at a
/// file that is not there.
async fn rewrite_attachments(
    md: &str,
    mode: ImageMode,
    client: &EduClient,
    dir: &Path,
    p: &Progress,
) -> Result<(String, Vec<String>)> {
    if mode == ImageMode::Keep || md.is_empty() {
        return Ok((md.to_string(), Vec::new()));
    }
    let spans = attachment_spans(md);
    if spans.is_empty() {
        return Ok((md.to_string(), Vec::new()));
    }

    let mut map: HashMap<&str, String> = HashMap::new();
    let mut images = Vec::new();

    match mode {
        ImageMode::Keep => unreachable!("returned above"),
        ImageMode::Link => {
            for &(lo, hi) in &spans {
                let url = &md[lo..hi];
                let absolute = if url.starts_with("http") {
                    url.to_string()
                } else {
                    format!("https://{}{}", client.host(), url)
                };
                map.insert(url, absolute);
            }
        }
        ImageMode::Download => {
            // The same image is often referenced several times in one
            // description: fetch it once, point every reference at it.
            let mut by_id: Vec<(&str, Vec<&str>)> = Vec::new();
            for &(lo, hi) in &spans {
                let url = &md[lo..hi];
                let id = attachment_id(url);
                if id.is_empty() {
                    continue;
                }
                match by_id.iter_mut().find(|(known, _)| *known == id) {
                    Some((_, urls)) => urls.push(url),
                    None => by_id.push((id, vec![url])),
                }
            }
            for (id, urls) in by_id {
                p.check()?;
                match client.get_bytes(urls[0]).await {
                    Ok(asset) => {
                        let name = format!(
                            "{}{}",
                            asset_name(id),
                            ext_for(&asset.content_type, &asset.filename)
                        );
                        let dir_images = dir.join("images");
                        std::fs::create_dir_all(&dir_images)?;
                        std::fs::write(dir_images.join(&name), &asset.bytes)?;
                        let rel = format!("images/{name}");
                        p.say(format!("    ✓ {rel} ({} B)", asset.bytes.len()));
                        for url in urls {
                            map.insert(url, rel.clone());
                        }
                        images.push(rel);
                    }
                    Err(e) => p.say(format!("    ! 附件 {id} 下载失败: {e}")),
                }
            }
        }
    }

    Ok((splice(md, &spans, &map), images))
}

/// Mirrors the JS `sanitize`: strip characters that are illegal in a filename
/// (plus space and `-`), collapse whitespace runs, and never return "".
pub(crate) fn sanitize(name: &str) -> String {
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
pub(crate) fn split_paths(p: &str) -> Vec<String> {
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
    images: ImageMode,
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
    let (task_pass, saved_images) =
        rewrite_attachments(&task_pass, images, client, &dir, p).await?;
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

    Ok(ChallengeResult {
        name: subject,
        dir: dir.display().to_string(),
        files,
        images: saved_images,
    })
}

/// Export a whole shixun (by myshixun identifier) into `base/<name>`, one
/// folder per 关.
pub async fn export_shixun(
    client: &EduClient,
    myshixun_identifier: &str,
    base: &Path,
    name: Option<&str>,
    images: ImageMode,
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
        match export_challenge(client, &identifier, &dir, Some(&sub_name), images, p).await {
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

/// Export exactly the 关卡 the user ticked, laid out like a course export:
/// `base/<name>/<实训>/<NN_关卡>/…`. Unlike [`export_course`] this never enters
/// a shixun or touches anything the user did not pick.
pub async fn export_selection(
    client: &EduClient,
    homeworks: &[SelectedHomework],
    base: &Path,
    name: Option<&str>,
    images: ImageMode,
    p: &Progress,
) -> Result<CourseResult> {
    p.check()?;
    if homeworks.is_empty() {
        return Err(Error::msg("没有选择任何关卡。"));
    }
    let dir = base.join(sanitize(name.unwrap_or("自选导出")));
    std::fs::create_dir_all(&dir)?;

    let mut summary = Vec::new();
    for hw in homeworks {
        p.check()?;
        p.say(format!("# {} （{} 关）", hw.name, hw.challenges.len()));
        let hw_dir = dir.join(sanitize(&hw.name));
        std::fs::create_dir_all(&hw_dir)?;

        let mut done = 0usize;
        let mut last_error = None;
        for (i, c) in hw.challenges.iter().enumerate() {
            p.check()?;
            let pos = c.position.unwrap_or((i + 1) as i64);
            let label = format!("{pos:02}_{}", c.name);
            p.say(format!("  {pos:02} {}", c.name));
            match export_challenge(client, &c.game_id, &hw_dir, Some(&label), images, p).await {
                Ok(_) => done += 1,
                Err(e) if e.message == "已取消" => return Err(e),
                Err(e) => {
                    p.say(format!("    ! 导出失败: {e}"));
                    last_error = Some(e.message);
                }
            }
        }
        summary.push(HomeworkEntry {
            name: hw.name.clone(),
            challenges: Some(done),
            skipped: None,
            error: last_error,
        });
    }

    Ok(CourseResult { dir: dir.display().to_string(), summary })
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
    images: ImageMode,
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

        match export_shixun(client, &mysh, &dir, Some(&hw_name), images, p).await {
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

    // The forms the platform actually emits, plus the syntaxes it might.
    const SAMPLE: &str = concat!(
        "![](/api/attachments/544786)\n",
        "![,](/api/attachments/3387597?type=image/png)\n",
        "<img src=\"/api/attachments/eFAxamFzeVF1VTI0Zy9XbkUxOXJldz09\">\n",
        "已是绝对的 https://www.educoder.net/api/attachments/608058 也要认出来\n",
    );

    #[test]
    fn finds_every_attachment_syntax() {
        let urls: Vec<&str> =
            attachment_spans(SAMPLE).iter().map(|&(lo, hi)| &SAMPLE[lo..hi]).collect();
        assert_eq!(
            urls,
            vec![
                "/api/attachments/544786",
                "/api/attachments/3387597?type=image/png",
                "/api/attachments/eFAxamFzeVF1VTI0Zy9XbkUxOXJldz09",
                "https://www.educoder.net/api/attachments/608058",
            ]
        );
    }

    #[test]
    fn link_mode_absolutises_only_the_relative_ones() {
        let spans = attachment_spans(SAMPLE);
        let mut map = HashMap::new();
        for &(lo, hi) in &spans {
            let url = &SAMPLE[lo..hi];
            let abs = if url.starts_with("http") {
                url.to_string()
            } else {
                format!("https://www.educoder.net{url}")
            };
            map.insert(url, abs);
        }
        let out = splice(SAMPLE, &spans, &map);
        assert!(out.contains("![](https://www.educoder.net/api/attachments/544786)"));
        assert!(out.contains("src=\"https://www.educoder.net/api/attachments/eFAxamFz"));
        // The already-absolute one must not gain a second host.
        assert!(!out.contains("educoder.net/https"));
        assert!(out.contains("已是绝对的 https://www.educoder.net/api/attachments/608058 也"));
        assert_eq!(out.matches("/api/attachments/").count(), 4);
    }

    // A short id that prefixes a longer one must not be substituted inside the
    // longer one's replacement — the reason `splice` rebuilds in one pass.
    #[test]
    fn splice_does_not_rescan_replacements() {
        let md = "![](/api/attachments/54) ![](/api/attachments/544786)";
        let spans = attachment_spans(md);
        let mut map = HashMap::new();
        map.insert("/api/attachments/54", "images/54.png".to_string());
        map.insert("/api/attachments/544786", "images/544786.png".to_string());
        assert_eq!(splice(md, &spans, &map), "![](images/54.png) ![](images/544786.png)");
    }

    #[test]
    fn asset_names_are_filename_safe() {
        assert_eq!(asset_name("544786"), "544786");
        // Base64 ids can carry / + =, which a filename cannot.
        let hashed = asset_name("TFRaWGFxNmlxaGk3bUdEVEFWZ2pQQT09");
        assert_eq!(hashed.len(), 10);
        assert!(hashed.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hashed, asset_name("TFRaWGFxNmlxaGk3bUdEVEFWZ2pQQT09"), "must be stable");
    }

    #[test]
    fn extension_prefers_content_type() {
        assert_eq!(ext_for("image/png", "image.jpg"), ".png");
        assert_eq!(ext_for("", "image.jpeg"), ".jpeg");
        assert_eq!(ext_for("image/x-unknown", ""), ".img");
        assert_eq!(ext_for("application/zip", ""), ".bin");
    }

    #[test]
    fn attachment_id_drops_the_query() {
        assert_eq!(attachment_id("/api/attachments/3387597?type=image/png"), "3387597");
        assert_eq!(attachment_id("https://h/api/attachments/544786"), "544786");
    }

    #[test]
    fn safe_join_rejects_traversal() {
        let base = Path::new("/tmp/x");
        assert_eq!(safe_join(base, "src/a.c").unwrap(), base.join("src").join("a.c"));
        assert!(safe_join(base, "../escape").is_err());
        assert!(safe_join(base, "/etc/passwd").is_err());
    }
}
