//! IPC commands exposed to the webview. Each is a thin wrapper over
//! `state`/`library`; blocking work runs on the blocking thread pool.

use std::path::Path;

use coverly_core::profiles::Media;
use coverly_core::systems::SystemDb;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::IpcError;
use crate::library::art::{entry_path, read_art_bytes};
use crate::library::view::{LibrarySettingsView, LibraryView, RecentView};
use crate::state::AppState;

/// Product name shown in the UI and reported over IPC.
pub const APP_NAME: &str = "Coverly ROMs";
/// Event carrying `ScanProgressEvent` while a library opens.
pub const SCAN_PROGRESS_EVENT: &str = "library-scan-progress";

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

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, IpcError> + Send + 'static,
) -> Result<T, IpcError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| IpcError::Io(e.to_string()))?
}

/// Opens a library folder, emitting `library-scan-progress` events.
#[tauri::command]
pub async fn open_library(
    app: AppHandle,
    path: String,
    profile_id: Option<String>,
) -> Result<LibraryView, IpcError> {
    blocking(move || {
        let state = app.state::<AppState>();
        state.open(
            Path::new(&path),
            profile_id.as_deref(),
            &SystemDb::builtin(),
            &mut |event| {
                let _ = app.emit(SCAN_PROGRESS_EVENT, event);
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn recent_libraries(app: AppHandle) -> Result<Vec<RecentView>, IpcError> {
    blocking(move || Ok(app.state::<AppState>().recent())).await
}

#[tauri::command]
pub fn forget_recent_library(
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<RecentView>, IpcError> {
    state.forget(Path::new(&path))
}

#[tauri::command]
pub fn library_settings(state: State<'_, AppState>) -> LibrarySettingsView {
    state.library_settings()
}

#[tauri::command]
pub fn set_library_settings(
    state: State<'_, AppState>,
    ignored_dirs: Vec<String>,
) -> Result<LibrarySettingsView, IpcError> {
    state.set_ignored_dirs(ignored_dirs)
}

/// The raw bytes of one art image (an `ArrayBuffer` in JS).
#[tauri::command]
pub async fn read_art(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
) -> Result<tauri::ipc::Response, IpcError> {
    blocking(move || {
        let session = app.state::<AppState>().session(generation)?;
        read_art_bytes(&session, entry_id, media).map(tauri::ipc::Response::new)
    })
    .await
}

/// Shows the entry's file in Finder / Explorer / the file manager.
#[tauri::command]
pub fn reveal_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    generation: u64,
    entry_id: u32,
) -> Result<(), IpcError> {
    let session = state.session(generation)?;
    let path = entry_path(&session, entry_id)?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| IpcError::Io(e.to_string()))
}
