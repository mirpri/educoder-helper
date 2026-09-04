//! Backend state: the current cookie jar / client, plus the small config file
//! that remembers which cookies.txt the user picked last time.
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::client::EduClient;
use crate::cookies::{self, Jar};
use crate::error::{Error, Result};

/// Where the active cookies came from, for display on the account page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieStatus {
    pub loaded: bool,
    /// Path of the cookies.txt in use, or `null` when pasted by hand.
    pub source: Option<String>,
    /// Cookie names present in the jar (values are never sent to the frontend).
    pub names: Vec<String>,
    pub host: String,
}

impl CookieStatus {
    pub fn empty() -> Self {
        Self { loaded: false, source: None, names: Vec::new(), host: crate::client::HOST.into() }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub cookies_path: Option<String>,
    /// Remembered report-backend settings. The API key is stored only when
    /// the user asks for it — `remember_api_key` says whether they did.
    #[serde(default)]
    pub ai: Option<crate::backend::BackendConfig>,
    #[serde(default)]
    pub remember_api_key: bool,
}

pub struct AppState {
    client: Mutex<Option<EduClient>>,
    status: Mutex<CookieStatus>,
    /// Set while an export is running so the UI can request a stop.
    pub cancel: Arc<AtomicBool>,
    pub exporting: Arc<AtomicBool>,
    /// The report run has its own pair: it is much slower than an export, and
    /// cancelling one must not stop the other.
    pub report_cancel: Arc<AtomicBool>,
    pub generating: Arc<AtomicBool>,
    /// The API key for this session when the user chose not to persist it.
    session_api_key: Mutex<Option<String>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            client: Mutex::new(None),
            status: Mutex::new(CookieStatus::empty()),
            cancel: Arc::new(AtomicBool::new(false)),
            exporting: Arc::new(AtomicBool::new(false)),
            report_cancel: Arc::new(AtomicBool::new(false)),
            generating: Arc::new(AtomicBool::new(false)),
            session_api_key: Mutex::new(None),
        }
    }

    pub fn set_session_api_key(&self, key: Option<String>) {
        *self.session_api_key.lock().expect("state poisoned") = key;
    }

    pub fn session_api_key(&self) -> Option<String> {
        self.session_api_key.lock().expect("state poisoned").clone()
    }

    /// A clone of the live client, or a "needs cookies" error the UI knows to
    /// route to the account page.
    pub fn client(&self) -> Result<EduClient> {
        self.client
            .lock()
            .expect("state poisoned")
            .clone()
            .ok_or_else(|| Error::cookies("尚未导入 Cookie，请先在「账号」页导入 cookies.txt。"))
    }

    pub fn status(&self) -> CookieStatus {
        self.status.lock().expect("state poisoned").clone()
    }

    pub fn set(&self, jar: &Jar, source: Option<String>) -> Result<CookieStatus> {
        let client = EduClient::new(jar, None)?;
        let status = CookieStatus {
            loaded: true,
            source,
            names: jar.keys().cloned().collect(),
            host: client.host().to_string(),
        };
        *self.client.lock().expect("state poisoned") = Some(client);
        *self.status.lock().expect("state poisoned") = status.clone();
        Ok(status)
    }

    pub fn clear(&self) {
        *self.client.lock().expect("state poisoned") = None;
        *self.status.lock().expect("state poisoned") = CookieStatus::empty();
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn config_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok()
}

pub fn config_file(app: &AppHandle) -> Option<PathBuf> {
    config_dir(app).map(|d| d.join("config.json"))
}

pub fn load_config(app: &AppHandle) -> Config {
    config_file(app)
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(app: &AppHandle, config: &Config) -> Result<()> {
    let Some(file) = config_file(app) else {
        return Ok(()); // No config dir on this platform; remembering is best-effort.
    };
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&file, serde_json::to_string_pretty(config).unwrap_or_default())?;
    Ok(())
}

/// Updates only the remembered cookies path. Callers used to build a whole
/// `Config` for this, which silently dropped every other setting.
pub fn save_cookies_path(app: &AppHandle, path: Option<String>) -> Result<()> {
    let mut config = load_config(app);
    config.cookies_path = path;
    save_config(app, &config)
}

/// Startup path: remembered file > `$EDUCODER_COOKIES` > `./cookies.txt` >
/// the app config dir. Silently does nothing when none of them exist.
pub fn autoload(app: &AppHandle, state: &AppState) -> Option<CookieStatus> {
    let config = load_config(app);
    let explicit = config.cookies_path.as_deref().map(Path::new);
    let dir = config_dir(app);
    let path = cookies::resolve(explicit, dir.as_deref())?;
    let jar = cookies::load_file(&path).ok()?;
    state.set(&jar, Some(path.display().to_string())).ok()
}
