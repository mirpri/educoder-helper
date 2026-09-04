//! Local agent CLIs (Claude Code / Codex) as a chat backend.
//!
//! These are agents, not completion endpoints, so three things need care:
//!   * **tools are disabled** — we want prose, not a process that goes and edits
//!     files on its own;
//!   * **the prompt goes over stdin** — a report section prompt runs past 30 KB,
//!     well over the Windows command-line limit;
//!   * **the child is killed on cancel** — the cooperative checkpoint the
//!     exporter uses cannot interrupt a call blocked on another process.
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::error::{Error, Result};

/// One generation can legitimately take minutes; past this something is stuck.
const CALL_TIMEOUT: Duration = Duration::from_secs(900);

/// Built-in tools we never want the agent to reach for. Passed to Claude Code
/// as `--disallowedTools`; Codex is confined with its own read-only sandbox.
const DISALLOWED_TOOLS: &str =
    "Bash,Read,Write,Edit,Glob,Grep,WebFetch,WebSearch,Task,Agent,TodoWrite,NotebookEdit";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CliKind {
    ClaudeCode,
    Codex,
}

impl CliKind {
    /// The bare command name, as it would appear on PATH.
    pub fn program(self) -> &'static str {
        match self {
            CliKind::ClaudeCode => "claude",
            CliKind::Codex => "codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CliKind::ClaudeCode => "Claude Code",
            CliKind::Codex => "Codex",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliConfig {
    /// Explicit executable path. Empty means "find it on PATH" — which a GUI
    /// launched from the file manager cannot always do, hence the override.
    #[serde(default)]
    pub path: String,
    /// Passed through as `--model` / `-m`; empty means the CLI's own default.
    #[serde(default)]
    pub model: String,
}

/// Executable extensions worth trying, best first. `.exe` is a real binary;
/// `.cmd`/`.bat` are npm shims that have to go through `cmd.exe`.
#[cfg(windows)]
const EXE_EXTS: &[&str] = &["exe", "cmd", "bat"];
#[cfg(not(windows))]
const EXE_EXTS: &[&str] = &[""];

/// Extra places to look when PATH comes up empty — a GUI process often has a
/// leaner PATH than the shell the user installed these tools from.
fn extra_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = home_dir() {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".bun").join("bin"));
        dirs.push(home.join("AppData").join("Roaming").join("npm"));
        dirs.push(home.join(".npm-global").join("bin"));
    }
    dirs
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Looks for `name` on PATH and in [`extra_dirs`], preferring a real executable
/// over a shim.
pub fn find_program(name: &str) -> Option<PathBuf> {
    let path_dirs = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    for ext in EXE_EXTS {
        for dir in path_dirs.iter().chain(extra_dirs().iter()) {
            let candidate =
                if ext.is_empty() { dir.join(name) } else { dir.join(format!("{name}.{ext}")) };
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// What the settings panel shows for the two local backends.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedClis {
    pub claude_code: Option<String>,
    pub codex: Option<String>,
}

pub fn detect() -> DetectedClis {
    DetectedClis {
        claude_code: find_program("claude").map(|p| p.display().to_string()),
        codex: find_program("codex").map(|p| p.display().to_string()),
    }
}

pub struct CliClient {
    kind: CliKind,
    program: PathBuf,
    model: String,
    cancel: Arc<AtomicBool>,
}

impl CliClient {
    pub fn new(kind: CliKind, config: &CliConfig, cancel: Arc<AtomicBool>) -> Result<Self> {
        let program = if config.path.trim().is_empty() {
            find_program(kind.program()).ok_or_else(|| {
                Error::msg(format!(
                    "在 PATH 上找不到 {}。请在设置里填写 {} 可执行文件的完整路径。",
                    kind.program(),
                    kind.label()
                ))
            })?
        } else {
            let p = PathBuf::from(config.path.trim());
            if !p.is_file() {
                return Err(Error::msg(format!("找不到可执行文件: {}", p.display())));
            }
            p
        };
        Ok(Self { kind, program, model: config.model.trim().to_string(), cancel })
    }

    pub fn label(&self) -> String {
        match self.model.is_empty() {
            true => self.kind.label().to_string(),
            false => format!("{} · {}", self.kind.label(), self.model),
        }
    }

    /// One generation. `system` is passed as a real system prompt where the CLI
    /// has one, and prepended to the user message where it does not.
    pub async fn chat(&self, system: &str, user: &str) -> Result<String> {
        // Codex has no system-prompt flag, so the instructions ride along at
        // the top of the message instead.
        let (args, stdin_text, out_file) = match self.kind {
            CliKind::ClaudeCode => {
                let mut args = vec![
                    "-p".to_string(),
                    "--output-format".to_string(),
                    "json".to_string(),
                    "--system-prompt".to_string(),
                    system.to_string(),
                ];
                if !self.model.is_empty() {
                    args.push("--model".to_string());
                    args.push(self.model.clone());
                }
                // Variadic (`<tools...>`), so it goes last — anything after it
                // that is not a flag would be swallowed as another tool name.
                args.push("--disallowed-tools".to_string());
                args.push(DISALLOWED_TOOLS.to_string());
                (args, user.to_string(), None)
            }
            CliKind::Codex => {
                let out = std::env::temp_dir()
                    .join(format!("educoder-codex-{}.txt", uuid::Uuid::new_v4()));
                let mut args = vec![
                    "exec".to_string(),
                    "--skip-git-repo-check".to_string(),
                    "--color".to_string(),
                    "never".to_string(),
                    "-s".to_string(),
                    "read-only".to_string(),
                    "-o".to_string(),
                    out.display().to_string(),
                ];
                if !self.model.is_empty() {
                    args.push("-m".to_string());
                    args.push(self.model.clone());
                }
                args.push("-".to_string()); // read the prompt from stdin
                (args, format!("{system}\n\n---\n\n{user}"), Some(out))
            }
        };

        let output = self.run(&args, &stdin_text).await;
        // Clean up the scratch file whichever way the call went.
        let result = match (&output, &out_file) {
            (Ok(_), Some(f)) => std::fs::read_to_string(f).map_err(|e| {
                Error::msg(format!("{} 没有写出结果文件: {e}", self.kind.label()))
            }),
            _ => output.map(|(stdout, _)| stdout),
        };
        if let Some(f) = &out_file {
            let _ = std::fs::remove_file(f);
        }
        let raw = result?;

        let text = match self.kind {
            CliKind::ClaudeCode => parse_claude_json(&raw)?,
            CliKind::Codex => raw,
        };
        // Agents open with a line of acknowledgement far more often than a
        // plain completion endpoint does.
        let text = strip_preamble(&text).trim().to_string();
        if text.is_empty() {
            return Err(Error::msg(format!("{} 返回了空内容。", self.kind.label())));
        }
        Ok(text)
    }

    /// Spawns the child, feeds it stdin, and waits — killing it if the user
    /// cancels or the call outlives [`CALL_TIMEOUT`].
    async fn run(&self, args: &[String], stdin_text: &str) -> Result<(String, String)> {
        let mut cmd = self.command(args);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Agents colour and animate their output when they think a human is
            // watching; this keeps stdout parseable.
            .env("NO_COLOR", "1")
            .env("CI", "1");
        #[cfg(windows)]
        // Don't flash a console window out of a GUI app.
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

        let mut child = cmd.spawn().map_err(|e| {
            Error::msg(format!("无法启动 {}: {e}", self.program.display()))
        })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(stdin_text.as_bytes()).await.map_err(|e| {
                Error::msg(format!("向 {} 写入提示词失败: {e}", self.kind.label()))
            })?;
            stdin.shutdown().await.ok(); // EOF, or the agent waits forever
        }

        let deadline = tokio::time::Instant::now() + CALL_TIMEOUT;
        let wait = child.wait_with_output();
        tokio::pin!(wait);
        let output = loop {
            tokio::select! {
                done = &mut wait => break done.map_err(|e| {
                    Error::msg(format!("等待 {} 结束失败: {e}", self.kind.label()))
                })?,
                _ = tokio::time::sleep(Duration::from_millis(250)) => {
                    if self.cancel.load(Ordering::Relaxed) {
                        return Err(Error::msg("已取消"));
                    }
                    if tokio::time::Instant::now() >= deadline {
                        return Err(Error::msg(format!(
                            "{} 超过 {} 秒没有返回，已放弃。",
                            self.kind.label(),
                            CALL_TIMEOUT.as_secs()
                        )));
                    }
                }
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !output.status.success() {
            let detail: String = if stderr.trim().is_empty() { &stdout } else { &stderr }
                .trim()
                .chars()
                .take(500)
                .collect();
            return Err(Error::msg(format!(
                "{} 退出码 {}：{detail}",
                self.kind.label(),
                output.status.code().unwrap_or(-1)
            )));
        }
        Ok((stdout, stderr))
    }

    /// `.cmd`/`.bat` shims are not executables — Windows can only run them
    /// through the command interpreter.
    fn command(&self, args: &[String]) -> tokio::process::Command {
        let is_shim = matches!(
            self.program.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
            Some("cmd") | Some("bat")
        );
        if is_shim && cfg!(windows) {
            let mut cmd = tokio::process::Command::new("cmd");
            cmd.arg("/C").arg(&self.program).args(args);
            cmd
        } else {
            let mut cmd = tokio::process::Command::new(&self.program);
            cmd.args(args);
            cmd
        }
    }
}

/// `claude -p --output-format json` wraps the answer in a result envelope.
fn parse_claude_json(raw: &str) -> Result<String> {
    let v: serde_json::Value = serde_json::from_str(raw.trim()).map_err(|e| {
        Error::msg(format!(
            "无法解析 Claude Code 的输出: {e}；原始输出: {}",
            raw.chars().take(300).collect::<String>()
        ))
    })?;
    if v.get("is_error").and_then(serde_json::Value::as_bool) == Some(true) {
        let msg = v
            .get("result")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("未知错误");
        return Err(Error::msg(format!("Claude Code 报错: {msg}")));
    }
    v.get("result")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::msg("Claude Code 的输出里没有 result 字段。"))
}

/// Agents like to open with a line of chat before the content. Drop a leading
/// acknowledgement so it does not land in the middle of the report.
fn strip_preamble(text: &str) -> String {
    let t = text.trim_start();
    let Some((first, rest)) = t.split_once('\n') else {
        return t.to_string();
    };
    let head = first.trim();
    let chatty = head.len() < 60
        && (head.ends_with('：')
            || head.ends_with(':')
            || head.starts_with("好的")
            || head.starts_with("以下")
            || head.starts_with("这是")
            || head.starts_with("Here"));
    if chatty && !head.starts_with('#') && !head.starts_with('>') {
        return rest.trim_start().to_string();
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn parses_the_claude_result_envelope() {
        let raw = r#"{"type":"result","is_error":false,"result":"正文内容","total_cost_usd":0.01}"#;
        assert_eq!(parse_claude_json(raw).unwrap(), "正文内容");
    }

    #[test]
    fn surfaces_claude_errors_rather_than_the_text() {
        let raw = r#"{"is_error":true,"result":"Credit balance too low"}"#;
        let e = parse_claude_json(raw).unwrap_err();
        assert!(e.message.contains("Credit balance too low"), "{}", e.message);
        assert!(parse_claude_json("not json").unwrap_err().message.contains("无法解析"));
        assert!(parse_claude_json("{}").unwrap_err().message.contains("result"));
    }

    #[test]
    fn strips_only_a_chatty_opening_line() {
        assert_eq!(strip_preamble("好的，以下是内容：\n\n### 2.1 视图"), "### 2.1 视图");
        // A real heading is content, not chatter.
        assert_eq!(strip_preamble("### 2.1 视图\n正文"), "### 2.1 视图\n正文");
        // A long first line is prose, not an acknowledgement.
        let long = "本实训主要考察视图的创建与使用，包含若干关卡，下面逐一说明具体实现。\n正文";
        assert_eq!(strip_preamble(long), long);
    }

    #[test]
    fn cli_kind_maps_to_program_names() {
        assert_eq!(CliKind::ClaudeCode.program(), "claude");
        assert_eq!(CliKind::Codex.program(), "codex");
    }

    #[test]
    fn missing_explicit_path_is_reported_clearly() {
        let cfg = CliConfig { path: "Z:/nope/claude.exe".into(), model: String::new() };
        let e = match CliClient::new(CliKind::ClaudeCode, &cfg, Arc::new(AtomicBool::new(false))) {
            Ok(_) => panic!("一个不存在的路径不该构造成功"),
            Err(e) => e,
        };
        assert!(e.message.contains("找不到可执行文件"), "{}", e.message);
    }
}
