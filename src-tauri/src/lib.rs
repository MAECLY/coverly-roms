//! Tauri shell of Coverly ROMs: a thin IPC layer over `coverly-core`.

pub mod ipc;

pub use ipc::{app_info, AppInfo, APP_NAME};

/// Builds and runs the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ipc::app_info])
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
