//! Cookie loading — port of `src/cookies.js`, plus GUI-only conveniences
//! (pasting a raw `Cookie:` header, writing the jar back out as cookies.txt).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub type Jar = BTreeMap<String, String>;

/// The cookie every request needs; its absence means "logged out or expired".
pub const SESSION: &str = "_educoder_session";

/// Parses a Netscape cookies.txt into a `{ name: value }` map.
pub fn parse_netscape(text: &str) -> Jar {
    let mut jar = Jar::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 7 {
            jar.insert(parts[5].trim().to_string(), parts[6].trim().to_string());
        }
    }
    jar
}

/// Parses a browser `Cookie:` header value — `a=1; b=2`.
pub fn parse_header(text: &str) -> Jar {
    let mut jar = Jar::new();
    for pair in text.split(';') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        if let Some((k, v)) = pair.split_once('=') {
            jar.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    jar
}

/// Accepts either format: tab-separated Netscape lines, or a `Cookie:` header.
pub fn parse_any(text: &str) -> Jar {
    let netscape = parse_netscape(text);
    if netscape.contains_key(SESSION) {
        return netscape;
    }
    let header = parse_header(text.trim().trim_start_matches("Cookie:").trim());
    if header.contains_key(SESSION) {
        return header;
    }
    // Neither form yielded a session cookie; return whichever found more, so the
    // caller's error message can report what it did see.
    if netscape.len() >= header.len() {
        netscape
    } else {
        header
    }
}

/// Renders a jar back out in Netscape format so the CLI can read the same file.
pub fn to_netscape(jar: &Jar) -> String {
    let mut out = String::from(
        "# Netscape HTTP Cookie File\n\
         # https://curl.haxx.se/rfc/cookie_spec.html\n\
         # Written by EduCoder Helper GUI.\n\n",
    );
    // Far-future expiry: the server decides when the session actually dies.
    for (k, v) in jar {
        out.push_str(&format!(".educoder.net\tTRUE\t/\tFALSE\t2147483647\t{k}\t{v}\n"));
    }
    out
}

pub fn load_file(path: &Path) -> Result<Jar> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::cookies(format!("无法读取 {}: {e}", path.display())))?;
    let jar = parse_any(&text);
    validate(&jar, &path.display().to_string())?;
    Ok(jar)
}

pub fn validate(jar: &Jar, source: &str) -> Result<()> {
    if jar.contains_key(SESSION) {
        return Ok(());
    }
    Err(Error::cookies(format!(
        "{source} 中没有 {SESSION} Cookie（未登录或已过期？共解析到 {} 个 Cookie）",
        jar.len()
    )))
}

/// Resolution order, mirroring the CLI: explicit path > `$EDUCODER_COOKIES` >
/// `./cookies.txt` in the working directory > the app's own config directory.
pub fn candidates(explicit: Option<&Path>, app_config_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Some(p) = explicit {
        out.push(p.to_path_buf());
    }
    if let Ok(env) = std::env::var("EDUCODER_COOKIES") {
        if !env.trim().is_empty() {
            out.push(PathBuf::from(env));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join("cookies.txt"));
    }
    if let Some(dir) = app_config_dir {
        out.push(dir.join("cookies.txt"));
    }
    out
}

/// First existing candidate, or `None` when nothing is on disk yet.
pub fn resolve(explicit: Option<&Path>, app_config_dir: Option<&Path>) -> Option<PathBuf> {
    candidates(explicit, app_config_dir)
        .into_iter()
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETSCAPE: &str = "# comment\n.educoder.net\tTRUE\t/\tFALSE\t123\t_educoder_session\tabc\n.educoder.net\tTRUE\t/\tFALSE\t123\tautologin_trustie\tdef\n";

    #[test]
    fn parses_netscape() {
        let jar = parse_any(NETSCAPE);
        assert_eq!(jar.get(SESSION).map(String::as_str), Some("abc"));
        assert_eq!(jar.get("autologin_trustie").map(String::as_str), Some("def"));
    }

    #[test]
    fn parses_cookie_header() {
        let jar = parse_any("Cookie: _educoder_session=abc; autologin_trustie=def");
        assert_eq!(jar.get(SESSION).map(String::as_str), Some("abc"));
        assert_eq!(jar.len(), 2);
    }

    #[test]
    fn round_trips_through_netscape() {
        let jar = parse_any(NETSCAPE);
        assert_eq!(parse_any(&to_netscape(&jar)), jar);
    }

    #[test]
    fn validate_rejects_jar_without_session() {
        let jar = parse_any("foo=bar");
        assert!(validate(&jar, "test").is_err());
    }
}
