//! One error type for the whole backend, serialised straight to the frontend.
//!
//! The Node client throws with `err.status` / `err.data` attached; this keeps
//! the same shape so the UI can show the server's own message.
use serde::Serialize;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    /// Human-readable message, already in the language the UI shows.
    pub message: String,
    /// HTTP status, when the failure came from the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// The parsed response body, when there was one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Set when the problem is the cookie jar, so the UI can send the user to
    /// the account page instead of showing a bare error.
    pub needs_cookies: bool,
}

impl Error {
    pub fn msg(message: impl Into<String>) -> Self {
        Self { message: message.into(), status: None, data: None, needs_cookies: false }
    }

    pub fn cookies(message: impl Into<String>) -> Self {
        Self { needs_cookies: true, ..Self::msg(message) }
    }

    pub fn http(status: u16, message: impl Into<String>, data: Option<serde_json::Value>) -> Self {
        Self {
            message: message.into(),
            status: Some(status),
            data,
            // 401/403 on this API means the session cookie died.
            needs_cookies: matches!(status, 401 | 403),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::msg(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::msg(format!("网络请求失败: {e}"))
    }
}
