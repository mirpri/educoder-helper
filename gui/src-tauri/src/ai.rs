//! Minimal OpenAI-compatible chat client.
//!
//! Deliberately not tied to one vendor: every provider a student is likely to
//! have a key for (DeepSeek, Kimi, 通义, 智谱, SiliconFlow, OpenAI itself)
//! speaks `POST {base}/chat/completions` with the same body, so the base URL is
//! part of the user's settings rather than a compile-time constant.
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Where to send chat completions, and as whom.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    /// API root, with or without a trailing `/v1` — see [`AiConfig::endpoint`].
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.deepseek.com".to_string(),
            api_key: String::new(),
            model: "deepseek-chat".to_string(),
        }
    }
}

impl AiConfig {
    /// Full chat-completions URL. Accepts the three shapes people paste:
    /// `https://host`, `https://host/v1` and the complete endpoint.
    fn endpoint(&self) -> String {
        let base = self.base_url.trim().trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            return base.to_string();
        }
        if base.ends_with("/v1") || base.ends_with("/compatible-mode/v1") {
            return format!("{base}/chat/completions");
        }
        format!("{base}/v1/chat/completions")
    }

    pub fn validate(&self) -> Result<()> {
        if self.api_key.trim().is_empty() {
            return Err(Error::msg("尚未填写 API Key。"));
        }
        if self.base_url.trim().is_empty() {
            return Err(Error::msg("尚未填写 API 地址。"));
        }
        if self.model.trim().is_empty() {
            return Err(Error::msg("尚未填写模型名称。"));
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    temperature: f32,
    stream: bool,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: Option<String>,
}

/// What a provider sends back when it rejects the request. Every vendor nests
/// the human-readable part differently, so this is best-effort.
fn error_message(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.pointer("/message"))
                .or_else(|| v.pointer("/error"))
                .and_then(|m| m.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    let hint = match status {
        401 | 403 => "（API Key 无效或没有该模型的权限）",
        404 => "（API 地址或模型名可能填错了）",
        429 => "（请求过于频繁或余额不足）",
        _ => "",
    };
    format!("AI 接口返回 HTTP {status}{hint}: {detail}")
}

pub struct AiClient {
    http: reqwest::Client,
    config: AiConfig,
}

impl AiClient {
    pub fn new(config: AiConfig) -> Result<Self> {
        config.validate()?;
        let http = reqwest::Client::builder()
            // Long generations are the norm here, not a hung connection.
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .map_err(|e| Error::msg(format!("无法创建 HTTP 客户端: {e}")))?;
        Ok(Self { http, config })
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// One round trip. Returns the assistant's text.
    pub async fn chat(&self, system: &str, user: &str) -> Result<String> {
        let body = ChatRequest {
            model: &self.config.model,
            messages: vec![
                Message { role: "system", content: system },
                Message { role: "user", content: user },
            ],
            temperature: 0.6,
            stream: false,
        };
        let resp = self
            .http
            .post(self.config.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(Error::msg(error_message(status.as_u16(), &text)));
        }
        let parsed: ChatResponse = serde_json::from_str(&text).map_err(|e| {
            Error::msg(format!("无法解析 AI 响应: {e}；原始响应: {}", &text.chars().take(300).collect::<String>()))
        })?;
        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| Error::msg("AI 返回了空的 choices。"))?;
        let content = choice.message.content.unwrap_or_default();
        if content.trim().is_empty() {
            let reason = choice.finish_reason.unwrap_or_else(|| "unknown".into());
            return Err(Error::msg(format!("AI 返回了空内容（finish_reason: {reason}）。")));
        }
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(base: &str) -> AiConfig {
        AiConfig { base_url: base.into(), api_key: "k".into(), model: "m".into() }
    }

    #[test]
    fn endpoint_accepts_every_shape_people_paste() {
        assert_eq!(
            cfg("https://api.deepseek.com").endpoint(),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            cfg("https://api.deepseek.com/").endpoint(),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            cfg("https://api.openai.com/v1").endpoint(),
            "https://api.openai.com/v1/chat/completions"
        );
        // 通义千问's OpenAI-compatible base ends in a path, not a bare /v1.
        assert_eq!(
            cfg("https://dashscope.aliyuncs.com/compatible-mode/v1").endpoint(),
            "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
        );
        // Already complete: left alone rather than doubled.
        assert_eq!(
            cfg("https://x.y/v1/chat/completions").endpoint(),
            "https://x.y/v1/chat/completions"
        );
    }

    #[test]
    fn validate_names_the_missing_field() {
        let mut c = cfg("https://h");
        c.api_key = String::new();
        assert!(c.validate().unwrap_err().message.contains("API Key"));
        let mut c = cfg("https://h");
        c.model = String::new();
        assert!(c.validate().unwrap_err().message.contains("模型"));
    }

    #[test]
    fn error_message_digs_out_the_vendor_text() {
        let body = r#"{"error":{"message":"Insufficient Balance","type":"x"}}"#;
        let m = error_message(402, body);
        assert!(m.contains("Insufficient Balance"), "{m}");
        assert!(error_message(401, "{}").contains("API Key 无效"));
    }
}
