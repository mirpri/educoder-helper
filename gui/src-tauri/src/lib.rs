//! EduCoder Helper GUI backend.
//!
//! A Rust port of the Node core (`src/*.js`) exposed to the webview as Tauri
//! commands, so the desktop app ships as a single binary with no Node runtime.
mod ai;
mod backend;
mod cli;
mod client;
mod commands;
mod cookies;
mod error;
mod exporter;
mod prompts;
mod report;
mod sign;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .setup(|app| {
            // Best-effort: pick up a cookies.txt the user already has so the app
            // opens logged in rather than on an empty account page.
            let handle = app.handle().clone();
            let state = app.state::<AppState>();
            state::autoload(&handle, &state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::cookie_status,
            commands::load_cookies_file,
            commands::load_cookies_text,
            commands::export_cookies_file,
            commands::clear_cookies,
            commands::open_login_window,
            commands::close_login_window,
            commands::me,
            commands::courses,
            commands::homeworks,
            commands::challenges,
            commands::task,
            commands::file_content,
            commands::report,
            commands::enter_shixun,
            commands::raw_request,
            commands::export_challenge,
            commands::export_shixun,
            commands::export_course,
            commands::export_selection,
            commands::cancel_export,
            commands::is_exporting,
            commands::ai_settings,
            commands::detect_cli_backends,
            commands::save_ai_settings,
            commands::report_tree,
            commands::generate_report,
            commands::cancel_report,
            commands::is_generating,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
