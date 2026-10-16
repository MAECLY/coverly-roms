//! IPC commands exposed to the webview.

use serde::Serialize;

/// Product name shown in the UI and reported over IPC.
pub const APP_NAME: &str = "Coverly ROMs";

/// Name and versions of the app and of the core library it is built on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub core_version: String,
}

/// Who we are and which versions are running.
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: APP_NAME.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        core_version: coverly_core::VERSION.to_owned(),
    }
}
