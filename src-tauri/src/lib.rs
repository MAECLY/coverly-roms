//! Tauri shell of Coverly ROMs: a thin IPC layer over `coverly-core`.

pub mod error;
pub mod ipc;
pub mod jobs;
pub mod keychain;
pub mod library;
pub mod logging;
pub mod prefs;
pub mod profiles;
pub mod sources;
pub mod state;
pub mod update;

pub use ipc::{app_info, AppInfo, APP_NAME};

use coverly_core::paths::{config_dir, AppDirs};
use tauri::Manager;

/// Builds and runs the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let config = config_dir().ok_or("cannot find the app config folder")?;
            let dirs = AppDirs::resolve(None, None).ok_or("cannot find the app data folders")?;
            let state = state::AppState::new(config, jobs::FillConfig::new(dirs.cache, dirs.data))
                .with_credentials(std::sync::Arc::new(keychain::KeychainStore::new()));
            logging::init(&state.logs_dir(), state.settings().advanced.log_level);
            log::info!("Coverly ROMs {} started", env!("CARGO_PKG_VERSION"));
            app.manage(state);
            // The key comes from crates/coverly-core/updater.pub, never from
            // tauri.conf.json (which only holds CONF_PUBKEY_MARKER).
            let updates = update::UpdateManager::new(
                Box::new(update::PluginDriver::new(app.handle().clone())),
                coverly_core::update::release_pubkey().map(String::from),
                std::env::var(coverly_core::update::ENDPOINT_ENV)
                    .ok()
                    .filter(|v| !v.trim().is_empty()),
            );
            app.manage(updates);
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
            ipc::get_settings,
            ipc::update_settings,
            ipc::reset_settings,
            ipc::open_config_folder,
            ipc::open_logs_folder,
            ipc::open_link,
            ipc::cache_usage,
            ipc::clear_cache,
            ipc::list_profiles,
            ipc::save_profile,
            ipc::delete_profile,
            ipc::preview_profile,
            ipc::credential_status,
            ipc::set_credential,
            ipc::clear_credential,
            ipc::sources_info,
            ipc::test_source_key,
            ipc::search_entry_art,
            ipc::entry_search_art,
            ipc::set_entry_art_from_search,
            ipc::clear_identity_override,
            ipc::check_update,
            ipc::download_update,
            ipc::install_update,
            ipc::restart_app,
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
