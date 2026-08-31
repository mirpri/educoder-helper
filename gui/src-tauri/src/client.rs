//! `EduClient`: authenticated client for the EduCoder (educoder.net) API.
//! Port of `src/client.js`.
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::Serialize;
use serde_json::Value;

use crate::cookies::Jar;
use crate::error::{Error, Result};
use crate::sign::signature;

pub const HOST: &str = "www.educoder.net";

/// Raw (unparsed) response, mirroring the CLI's `--raw` output for get/post/raw.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawResponse {
    pub status: u16,
    pub redirected: bool,
    pub url: String,
    pub body: String,
}

/// Raw bytes plus the two headers that say what they are.
pub struct BinaryResponse {
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub filename: String,
}

/// Pulls the filename out of a `Content-Disposition`, tolerating the
/// `filename*=UTF-8''name` form and the unquoted, semicolon-separated one
/// EduCoder actually sends (`attachment;filename=image.png;attachment_id=1`).
fn filename_from_disposition(header: &str) -> String {
    for part in header.split(';') {
        let part = part.trim();
        let Some((key, value)) = part.split_once('=') else { continue };
        let key = key.trim().to_ascii_lowercase();
        if key != "filename" && key != "filename*" {
            continue;
        }
        let value = value.trim().trim_matches('"');
        let value = value.strip_prefix("UTF-8''").unwrap_or(value);
        return value.to_string();
    }
    String::new()
}

/// Cheap to clone: the HTTP pool and the clock offset are shared, so cloning a
/// client for one request does not re-sync the server clock.
#[derive(Clone)]
pub struct EduClient {
    host: String,
    cookie_header: String,
    session: String,
    http: reqwest::Client,
    /// serverMs - localMs
    clock_offset: Arc<AtomicI64>,
    clock_synced: Arc<AtomicBool>,
}

impl EduClient {
    pub fn new(jar: &Jar, host: Option<String>) -> Result<Self> {
        let session = jar
            .get(crate::cookies::SESSION)
            .cloned()
            .ok_or_else(|| Error::cookies("缺少 _educoder_session Cookie"))?;
        let cookie_header = jar
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|e| Error::msg(format!("无法创建 HTTP 客户端: {e}")))?;
        Ok(Self {
            host: host.unwrap_or_else(|| HOST.to_string()),
            cookie_header,
            session,
            http,
            clock_offset: Arc::new(AtomicI64::new(0)),
            clock_synced: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    fn local_now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    /// Aligns to the server clock once (the local PC clock is often skewed).
    async fn server_now(&self) -> i64 {
        if !self.clock_synced.load(Ordering::Relaxed) {
            let offset = match self.http.head(format!("https://{}/", self.host)).send().await {
                Ok(resp) => resp
                    .headers()
                    .get(reqwest::header::DATE)
                    .and_then(|d| d.to_str().ok())
                    .and_then(|d| httpdate::parse_http_date(d).ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64 - Self::local_now_ms())
                    .unwrap_or(0),
                Err(_) => 0,
            };
            self.clock_offset.store(offset, Ordering::Relaxed);
            self.clock_synced.store(true, Ordering::Relaxed);
        }
        Self::local_now_ms() + self.clock_offset.load(Ordering::Relaxed)
    }

    async fn headers(&self, method: &str) -> Result<reqwest::header::HeaderMap> {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
        let t = self.server_now().await;
        let sig = signature(method, t);
        let origin = format!("https://{}", self.host);
        let pairs: Vec<(&str, String)> = vec![
            ("user-agent", "Mozilla/5.0".to_string()),
            ("accept", "application/json, text/plain, */*".to_string()),
            ("x-edu-type", "pc".to_string()),
            ("x-edu-timestamp", t.to_string()),
            ("x-edu-signature", sig),
            ("pc-authorization", self.session.clone()),
            ("x-original-protocol", "https:".to_string()),
            ("x-original-host", self.host.clone()),
            ("x-original-origin", origin.clone()),
            ("x-request-id", uuid::Uuid::new_v4().to_string()),
            ("referer", format!("{origin}/")),
            ("cookie", self.cookie_header.clone()),
        ];
        let mut headers = HeaderMap::with_capacity(pairs.len() + 1);
        for (name, value) in pairs {
            let name = HeaderName::from_static(name);
            let value = HeaderValue::from_str(&value).map_err(|_| {
                Error::cookies(format!("请求头 {name} 含有非法字符（Cookie 是否粘贴完整？）"))
            })?;
            headers.insert(name, value);
        }
        Ok(headers)
    }

    fn absolute(&self, path_or_url: &str) -> String {
        if path_or_url.starts_with("http") {
            path_or_url.to_string()
        } else if path_or_url.starts_with('/') {
            format!("https://{}{}", self.host, path_or_url)
        } else {
            format!("https://{}/{}", self.host, path_or_url)
        }
    }

    async fn send(
        &self,
        method: &str,
        path_or_url: &str,
        body: Option<String>,
    ) -> Result<(reqwest::Response, String)> {
        let method = method.to_uppercase();
        let url = self.absolute(path_or_url);
        let verb = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|_| Error::msg(format!("不支持的 HTTP 方法: {method}")))?;
        let mut headers = self.headers(&method).await?;
        let mut req = self.http.request(verb, &url);
        if let Some(body) = body {
            headers.insert(
                reqwest::header::CONTENT_TYPE,
                reqwest::header::HeaderValue::from_static("application/json"),
            );
            req = req.body(body);
        }
        let resp = req.headers(headers).send().await?;
        Ok((resp, url))
    }

    /// Raw signed request: never fails on a non-2xx, returns the body verbatim.
    pub async fn request_raw(
        &self,
        method: &str,
        path_or_url: &str,
        body: Option<String>,
    ) -> Result<RawResponse> {
        let (resp, url) = self.send(method, path_or_url, body).await?;
        let status = resp.status().as_u16();
        let final_url = resp.url().to_string();
        let text = resp.text().await?;
        Ok(RawResponse { status, redirected: final_url != url, url: final_url, body: text })
    }

    /// Signed request returning parsed JSON. Non-2xx becomes an `Error` carrying
    /// the status and the parsed body, the way the Node client throws.
    pub async fn request(
        &self,
        method: &str,
        path_or_url: &str,
        body: Option<String>,
    ) -> Result<Value> {
        let (resp, url) = self.send(method, path_or_url, body).await?;
        let status = resp.status();
        let text = resp.text().await?;
        let data: Value = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text.clone()))
        };
        if !status.is_success() {
            return Err(Error::http(
                status.as_u16(),
                format!("HTTP {} for {} {}", status.as_u16(), method.to_uppercase(), url),
                Some(data),
            ));
        }
        // A login redirect lands on an HTML page with a 200; treat that as an
        // expired session rather than letting the UI show a string blob.
        if data.is_string() && text.trim_start().starts_with('<') {
            return Err(Error::cookies(
                "服务器返回了 HTML 而非 JSON，通常表示登录态已过期，请重新导入 Cookie。",
            ));
        }
        Ok(data)
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        self.request("GET", path, None).await
    }

    /// Signed GET returning raw bytes — attachments/images, which redirect to
    /// the object store. `filename` comes from Content-Disposition and is only
    /// good enough for its extension.
    pub async fn get_bytes(&self, path_or_url: &str) -> Result<BinaryResponse> {
        let (resp, url) = self.send("GET", path_or_url, None).await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Error::http(
                status.as_u16(),
                format!("HTTP {} for GET {}", status.as_u16(), url),
                None,
            ));
        }
        let header = |name: reqwest::header::HeaderName| {
            resp.headers().get(name).and_then(|v| v.to_str().ok()).unwrap_or("").to_string()
        };
        let content_type = header(reqwest::header::CONTENT_TYPE)
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let filename = filename_from_disposition(&header(reqwest::header::CONTENT_DISPOSITION));
        let bytes = resp.bytes().await?.to_vec();
        Ok(BinaryResponse { bytes, content_type, filename })
    }

    // ---- Convenience wrappers for common endpoints ----

    pub async fn get_user_info(&self) -> Result<Value> {
        self.get("/api/users/get_user_info.json").await
    }

    /// `login` defaults to the authenticated user's login.
    pub async fn get_courses(&self, login: Option<&str>) -> Result<Value> {
        let login = match login {
            Some(l) if !l.is_empty() => l.to_string(),
            _ => self
                .get_user_info()
                .await?
                .get("login")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::cookies("响应中没有 login 字段，登录态可能已失效"))?
                .to_string(),
        };
        self.get(&format!("/api/users/{login}/courses.json")).await
    }

    /// `kind`: 1=图文作业, 4=实训作业 (others exist per course).
    pub async fn get_homeworks(
        &self,
        course_id: &str,
        kind: i64,
        page: i64,
        limit: i64,
    ) -> Result<Value> {
        self.get(&format!(
            "/api/courses/{course_id}/homework_commons.json?type={kind}&page={page}&limit={limit}"
        ))
        .await
    }

    /// Challenges (关卡) of a shixun by its identifier (e.g. "gu8nbv56").
    pub async fn get_challenges(&self, shixun_identifier: &str) -> Result<Value> {
        self.get(&format!("/api/shixuns/{shixun_identifier}/challenges.json")).await
    }

    /// Shixun detail by identifier. Includes `myshixun_id`: the numeric id of the
    /// caller's EXISTING instance, or 0 if none. Read-only — does NOT create an
    /// instance (unlike `enter_shixun`).
    pub async fn get_shixun(&self, shixun_identifier: &str) -> Result<Value> {
        self.get(&format!("/api/shixuns/{shixun_identifier}.json")).await
    }

    /// The student's instantiated challenges (games) for a myshixun identifier.
    pub async fn get_my_challenges(&self, myshixun_identifier: &str) -> Result<Value> {
        self.get(&format!("/api/myshixuns/{myshixun_identifier}/challenges.json")).await
    }

    /// A single challenge "game". The task description markdown is at
    /// `result.challenge.task_pass`.
    pub async fn get_task(&self, game_identifier: &str) -> Result<Value> {
        self.get(&format!("/api/tasks/{game_identifier}.json")).await
    }

    /// Decoded text of a file in the student's repo (reads git HEAD = current code).
    pub async fn get_file_content(&self, game_identifier: &str, path: &str) -> Result<String> {
        let encoded = urlencode(path);
        let r = self
            .get(&format!("/api/tasks/{game_identifier}/rep_content.json?path={encoded}"))
            .await?;
        // `content` is either the base64 string itself or an object wrapping it.
        let b64 = match r.get("content") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(obj) => obj.get("content").and_then(Value::as_str).map(str::to_string),
            None => None,
        };
        let b64 = b64.ok_or_else(|| Error { data: Some(r), ..Error::msg("响应中没有文件内容") })?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| Error::msg(format!("文件内容 Base64 解码失败: {e}")))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// The student's work report for one submission: per-challenge scores, time,
    /// diff_code_count, etc. (`stage_list`).
    pub async fn get_work_report(&self, student_work_id: &str) -> Result<Value> {
        self.get(&format!("/api/student_works/{student_work_id}/shixun_work_report.json")).await
    }

    /// Enter a shixun: instantiates a fresh myshixun/repo if none exists and
    /// returns `{ game_identifier }` of the first challenge.
    /// NOTE: if a previous instance was reset, this creates a NEW instance.
    pub async fn enter_shixun(&self, shixun_identifier: &str) -> Result<Value> {
        self.get(&format!("/api/shixuns/{shixun_identifier}/shixun_exec.json")).await
    }

    /// Challenge list with each challenge's `game_identifier` attached from the
    /// caller's EXISTING instance, if any — the behaviour of `edu challenges`.
    /// Uses the read-only `get_shixun`, so it never creates an instance.
    pub async fn challenges_with_games(&self, shixun_identifier: &str) -> Result<Value> {
        let mut data = self.get_challenges(shixun_identifier).await?;
        let my_id = self
            .get_shixun(shixun_identifier)
            .await
            .ok()
            .and_then(|s| s.get("myshixun_id").cloned())
            .and_then(id_if_present);
        if let Some(my_id) = my_id {
            if let Ok(mine) = self.get_my_challenges(&my_id).await {
                let by_pos: std::collections::HashMap<i64, String> = mine
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|g| {
                        Some((
                            g.get("position")?.as_i64()?,
                            g.get("identifier")?.as_str()?.to_string(),
                        ))
                    })
                    .collect();
                if let Some(list) = data.get_mut("challenge_list").and_then(Value::as_array_mut) {
                    for c in list.iter_mut() {
                        let pos = c.get("position").and_then(Value::as_i64);
                        if let Some(id) = pos.and_then(|p| by_pos.get(&p)) {
                            c["game_identifier"] = Value::String(id.clone());
                        }
                    }
                }
            }
        }
        Ok(data)
    }
}

/// `myshixun_id` is `0` (or absent) when the caller has no instance yet.
fn id_if_present(v: Value) -> Option<String> {
    match v {
        Value::Number(n) if n.as_i64() == Some(0) => None,
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if s != "0" && !s.is_empty() => Some(s),
        _ => None,
    }
}

/// Percent-encodes everything outside the unreserved set, matching
/// `encodeURIComponent` closely enough for repo paths.
fn urlencode(s: &str) -> String {
    const SAFE: &[u8] = b"-_.!~*'()";
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || SAFE.contains(b) {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencodes_like_encode_uri_component() {
        assert_eq!(urlencode("src/shell1.sh"), "src%2Fshell1.sh");
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("步骤.md"), "%E6%AD%A5%E9%AA%A4.md");
    }

    #[test]
    fn treats_zero_myshixun_id_as_absent() {
        assert_eq!(id_if_present(serde_json::json!(0)), None);
        assert_eq!(id_if_present(serde_json::json!("0")), None);
        assert_eq!(id_if_present(serde_json::json!(123)), Some("123".into()));
    }
}
