//! `#[tauri::command]` surface — one command per CLI verb, so the GUI covers
//! everything `bin/edu.js` can do.
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::client::RawResponse;
use crate::cookies::{self, Jar};
use crate::error::{Error, Result};
use crate::exporter::{
    self, ChallengeResult, CourseResult, ImageMode, Progress, ShixunResult,
};
use crate::state::{self, AppState, Config, CookieStatus};

// ---- Account / cookies ----

#[tauri::command]
pub fn cookie_status(state: State<'_, AppState>) -> CookieStatus {
    state.status()
}

/// Loads a cookies.txt and remembers it for next launch.
#[tauri::command]
pub fn load_cookies_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<CookieStatus> {
    let path = PathBuf::from(path);
    let jar = cookies::load_file(&path)?;
    let status = state.set(&jar, Some(path.display().to_string()))?;
    state::save_config(&app, &Config { cookies_path: Some(path.display().to_string()) })?;
    Ok(status)
}

/// Accepts pasted text in either Netscape cookies.txt or `Cookie:` header form.
/// It is always written to the app config dir so the session survives a restart
/// — and so the Node CLI can point `$EDUCODER_COOKIES` at the same file.
#[tauri::command]
pub fn load_cookies_text(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
) -> Result<CookieStatus> {
    let jar = cookies::parse_any(&text);
    cookies::validate(&jar, "粘贴的内容")?;
    let saved = state::config_dir(&app).map(|d| d.join("cookies.txt"));
    if let Some(path) = &saved {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, cookies::to_netscape(&jar))?;
        state::save_config(&app, &Config { cookies_path: Some(path.display().to_string()) })?;
    }
    state.set(&jar, saved.map(|p| p.display().to_string()))
}

/// Writes the active jar out as a Netscape cookies.txt the CLI can read.
#[tauri::command]
pub fn export_cookies_file(state: State<'_, AppState>, path: String) -> Result<String> {
    let status = state.status();
    if !status.loaded {
        return Err(Error::cookies("没有可导出的 Cookie"));
    }
    // The jar itself never leaves the backend, so re-read it from the source
    // file when there is one; otherwise the config-dir copy is authoritative.
    let source = status
        .source
        .ok_or_else(|| Error::msg("当前 Cookie 没有对应的文件来源"))?;
    let jar = cookies::load_file(Path::new(&source))?;
    std::fs::write(&path, cookies::to_netscape(&jar))?;
    Ok(path)
}

#[tauri::command]
pub fn clear_cookies(app: AppHandle, state: State<'_, AppState>) -> Result<CookieStatus> {
    state.clear();
    state::save_config(&app, &Config { cookies_path: None })?;
    Ok(state.status())
}

// ---- Browser login ----

const LOGIN_LABEL: &str = "login";
const LOGIN_URL: &str = "https://www.educoder.net/login";
/// Give up watching after this long so a forgotten window doesn't poll forever.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Opens a real browser window on the EduCoder login page. Once the user signs
/// in, `watch_login` picks the session cookie straight out of that webview's
/// cookie store — no copying anything by hand.
#[tauri::command]
pub async fn open_login_window(app: AppHandle) -> Result<()> {
    if let Some(existing) = app.get_webview_window(LOGIN_LABEL) {
        let _ = existing.set_focus();
        return Ok(());
    }
    let url = LOGIN_URL
        .parse()
        .map_err(|e| Error::msg(format!("登录地址无效: {e}")))?;
    WebviewWindowBuilder::new(&app, LOGIN_LABEL, WebviewUrl::External(url))
        .title("登录 EduCoder —— 登录成功后本窗口会自动关闭")
        .inner_size(1024.0, 768.0)
        .center()
        .build()
        .map_err(|e| Error::msg(format!("无法打开登录窗口: {e}")))?;
    watch_login(app);
    Ok(())
}

#[tauri::command]
pub fn close_login_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LOGIN_LABEL) {
        let _ = w.close();
    }
}

/// Polls the login webview's cookie store until the session cookie shows up.
///
/// Runs on a blocking thread on purpose: reading cookies from the UI thread
/// deadlocks WebView2 on Windows (wry#583).
fn watch_login(app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let Ok(url) = reqwest::Url::parse(LOGIN_URL) else { return };
        let deadline = Instant::now() + LOGIN_TIMEOUT;
        loop {
            std::thread::sleep(Duration::from_millis(800));

            // The user closed the window: stop watching, stay logged out.
            let Some(window) = app.get_webview_window(LOGIN_LABEL) else {
                let _ = app.emit("login:closed", ());
                return;
            };
            if Instant::now() > deadline {
                let _ = window.close();
                let _ = app.emit("login:timeout", ());
                return;
            }

            let Ok(found) = window.cookies_for_url(url.clone()) else { continue };
            let jar: Jar = found
                .iter()
                .map(|c| (c.name().to_string(), c.value().to_string()))
                .collect();
            if !jar.contains_key(cookies::SESSION) {
                continue;
            }

            let _ = window.close();
            match adopt_login(&app, &jar) {
                Ok(status) => {
                    let _ = app.emit("login:success", status);
                }
                Err(e) => {
                    let _ = app.emit("login:error", e);
                }
            }
            return;
        }
    });
}

/// Persists a jar captured from the login window and makes it the active session.
fn adopt_login(app: &AppHandle, jar: &Jar) -> Result<CookieStatus> {
    let saved = state::config_dir(app).map(|d| d.join("cookies.txt"));
    if let Some(path) = &saved {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, cookies::to_netscape(jar))?;
        state::save_config(app, &Config { cookies_path: Some(path.display().to_string()) })?;
    }
    app.state::<AppState>().set(jar, saved.map(|p| p.display().to_string()))
}

// ---- Queries (one per CLI command) ----

/// `edu me`
#[tauri::command]
pub async fn me(state: State<'_, AppState>) -> Result<Value> {
    state.client()?.get_user_info().await
}

/// `edu courses [login]`
#[tauri::command]
pub async fn courses(state: State<'_, AppState>, login: Option<String>) -> Result<Value> {
    state.client()?.get_courses(login.as_deref()).await
}

/// `edu homeworks <courseId> [type]`
#[tauri::command]
pub async fn homeworks(
    state: State<'_, AppState>,
    course_id: String,
    kind: Option<i64>,
) -> Result<Value> {
    state.client()?.get_homeworks(&course_id, kind.unwrap_or(4), 1, 50).await
}

/// `edu challenges <shixunId>` — includes each 关's gameId when an instance exists.
#[tauri::command]
pub async fn challenges(state: State<'_, AppState>, shixun_id: String) -> Result<Value> {
    state.client()?.challenges_with_games(&shixun_id).await
}

/// `edu task <gameId>`
#[tauri::command]
pub async fn task(state: State<'_, AppState>, game_id: String) -> Result<Value> {
    state.client()?.get_task(&game_id).await
}

/// `edu code <gameId> <path>`
#[tauri::command]
pub async fn file_content(
    state: State<'_, AppState>,
    game_id: String,
    path: String,
) -> Result<String> {
    state.client()?.get_file_content(&game_id, &path).await
}

/// `edu report <reportId>`
#[tauri::command]
pub async fn report(state: State<'_, AppState>, report_id: String) -> Result<Value> {
    state.client()?.get_work_report(&report_id).await
}

/// `edu enter <shixunId>` — creates an instance if none exists.
#[tauri::command]
pub async fn enter_shixun(state: State<'_, AppState>, shixun_id: String) -> Result<Value> {
    state.client()?.enter_shixun(&shixun_id).await
}

/// `edu get/post/raw` — arbitrary signed request, unparsed response.
#[tauri::command]
pub async fn raw_request(
    state: State<'_, AppState>,
    method: String,
    path: String,
    body: Option<String>,
) -> Result<RawResponse> {
    let body = body.filter(|b| !b.trim().is_empty());
    state.client()?.request_raw(&method, &path, body).await
}

// ---- Export ----

/// Guards against two exports running at once and wires up the progress sink.
/// `run` receives the client, the destination base directory and the progress
/// handle; log lines are streamed to the frontend as `export:log` events.
async fn with_export<T, F, Fut>(
    app: &AppHandle,
    state: &State<'_, AppState>,
    run: F,
) -> Result<T>
where
    F: FnOnce(crate::client::EduClient, Progress) -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    if state.exporting.swap(true, Ordering::SeqCst) {
        return Err(Error::msg("已有导出任务在进行中"));
    }
    state.cancel.store(false, Ordering::SeqCst);

    let client = match state.client() {
        Ok(c) => c,
        Err(e) => {
            state.exporting.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };
    let handle = app.clone();
    let progress = Progress::new(
        Box::new(move |m: &str| {
            let _ = handle.emit("export:log", m.to_string());
        }),
        state.cancel.clone(),
    );

    let result = run(client, progress).await;
    state.exporting.store(false, Ordering::SeqCst);
    let _ = app.emit("export:done", result.is_ok());
    result
}

/// `edu export challenge <gameId> [dir]`
#[tauri::command]
pub async fn export_challenge(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
    dest: String,
    name: Option<String>,
    images: Option<ImageMode>,
) -> Result<ChallengeResult> {
    let images = images.unwrap_or_default();
    with_export(&app, &state, |client, p| async move {
        exporter::export_challenge(&client, &game_id, Path::new(&dest), name.as_deref(), images, &p)
            .await
    })
    .await
}

/// `edu export shixun <myshixunId> [dir]`
#[tauri::command]
pub async fn export_shixun(
    app: AppHandle,
    state: State<'_, AppState>,
    myshixun_id: String,
    dest: String,
    name: Option<String>,
    images: Option<ImageMode>,
) -> Result<ShixunResult> {
    let images = images.unwrap_or_default();
    with_export(&app, &state, |client, p| async move {
        exporter::export_shixun(
            &client,
            &myshixun_id,
            Path::new(&dest),
            name.as_deref(),
            images,
            &p,
        )
        .await
    })
    .await
}

/// `edu export course <courseId> [dir]`
#[tauri::command]
pub async fn export_course(
    app: AppHandle,
    state: State<'_, AppState>,
    course_id: String,
    dest: String,
    name: Option<String>,
    enter_if_needed: Option<bool>,
    images: Option<ImageMode>,
) -> Result<CourseResult> {
    let enter = enter_if_needed.unwrap_or(true);
    let images = images.unwrap_or_default();
    with_export(&app, &state, |client, p| async move {
        exporter::export_course(
            &client,
            &course_id,
            Path::new(&dest),
            name.as_deref(),
            enter,
            images,
            &p,
        )
        .await
    })
    .await
}

/// Asks the running export to stop at the next checkpoint.
#[tauri::command]
pub fn cancel_export(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn is_exporting(state: State<'_, AppState>) -> bool {
    state.exporting.load(Ordering::SeqCst)
}
