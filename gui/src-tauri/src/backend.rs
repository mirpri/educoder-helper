//! Which engine writes the report.
//!
//! `report.rs` only ever calls [`ChatBackend::chat`], so adding an engine is a
//! variant here rather than a change to the orchestration. An enum instead of a
//! trait object because `async fn` in traits would mean another dependency for
//! no benefit at two implementations.
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ai::{AiClient, AiConfig};
use crate::cli::{CliClient, CliConfig, CliKind};
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackendKind {
    /// Any OpenAI-compatible HTTP endpoint, with the user's own API key.
    #[default]
    Api,
    /// The `claude` CLI already installed on this machine.
    ClaudeCode,
    /// The `codex` CLI already installed on this machine.
    Codex,
}

/// Flat rather than a tagged union: the settings panel keeps both halves filled
/// in as the user switches between them, and the config file round-trips.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendConfig {
    #[serde(default)]
    pub kind: BackendKind,
    #[serde(default)]
    pub api: AiConfig,
    #[serde(default)]
    pub cli: CliConfig,
}

pub enum ChatBackend {
    Api(AiClient),
    Cli(CliClient),
}

impl ChatBackend {
    pub fn new(config: BackendConfig, cancel: Arc<AtomicBool>) -> Result<Self> {
        Ok(match config.kind {
            BackendKind::Api => ChatBackend::Api(AiClient::new(config.api)?),
            BackendKind::ClaudeCode => {
                ChatBackend::Cli(CliClient::new(CliKind::ClaudeCode, &config.cli, cancel)?)
            }
            BackendKind::Codex => {
                ChatBackend::Cli(CliClient::new(CliKind::Codex, &config.cli, cancel)?)
            }
        })
    }

    /// Shown in the progress log so the user can see what is doing the writing.
    pub fn label(&self) -> String {
        match self {
            ChatBackend::Api(c) => c.model().to_string(),
            ChatBackend::Cli(c) => c.label(),
        }
    }

    pub async fn chat(&self, system: &str, user: &str) -> Result<String> {
        match self {
            ChatBackend::Api(c) => c.chat(system, user).await,
            ChatBackend::Cli(c) => c.chat(system, user).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_backend_still_validates_its_config() {
        // Empty key: the API backend must refuse before any request goes out.
        let cfg = BackendConfig::default();
        let e = match ChatBackend::new(cfg, Arc::new(AtomicBool::new(false))) {
            Ok(_) => panic!("空 API Key 不该构造成功"),
            Err(e) => e,
        };
        assert!(e.message.contains("API Key"), "{}", e.message);
    }

    #[test]
    fn kind_round_trips_as_camel_case() {
        let json = serde_json::to_string(&BackendKind::ClaudeCode).unwrap();
        assert_eq!(json, "\"claudeCode\"");
        let back: BackendKind = serde_json::from_str("\"codex\"").unwrap();
        assert_eq!(back, BackendKind::Codex);
    }

    #[test]
    fn config_defaults_when_fields_are_absent() {
        let c: BackendConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c.kind, BackendKind::Api);
        assert!(c.cli.path.is_empty());
    }
}
