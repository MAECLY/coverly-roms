//! Tauri shell of Coverly ROMs: a thin IPC layer over `coverly-core`.

pub mod error;
pub mod ipc;
pub mod jobs;
pub mod library;
pub mod state;

pub use ipc::{app_info, AppInfo, APP_NAME};

use coverly_core::paths::{config_dir, AppDirs};
use tauri::Manager;

/// Builds and runs the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config = config_dir().ok_or("cannot find the app config folder")?;
            let dirs = AppDirs::resolve(None, None).ok_or("cannot find the app data folders")?;
            app.manage(state::AppState::new(
                config,
                jobs::FillConfig::new(dirs.cache, dirs.data),
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::app_info,
            ipc::open_library,
            ipc::recent_libraries,
            ipc::forget_recent_library,
            ipc::library_settings,
            ipc::set_library_settings,
            ipc::read_art,
            ipc::identify_entry,
            ipc::reveal_entry,
            ipc::start_fill,
            ipc::fill_job,
            ipc::fill_job_items,
            ipc::decide_fill,
            ipc::run_fill,
            ipc::resume_fill,
            ipc::cancel_fill,
            ipc::candidate_art,
            ipc::undo_last_job,
            ipc::job_history,
            ipc::entry_candidates,
            ipc::entry_candidate_art,
            ipc::set_entry_art,
            ipc::set_entry_art_from_file,
            ipc::remove_entry_art,
            ipc::export_audit,
            ipc::export_job_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Coverly ROMs");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_name_and_versions() {
        let info = app_info();
        assert_eq!(info.name, "Coverly ROMs");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.core_version, coverly_core::VERSION);
    }

    #[test]
    fn app_info_serializes_with_snake_case_fields() {
        let json = serde_json::to_value(app_info()).unwrap();
        assert_eq!(json["name"], "Coverly ROMs");
        assert!(json.get("core_version").is_some());
    }
}
