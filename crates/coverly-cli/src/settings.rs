//! The app's settings, read-only (spec §9: the CLI uses the same file;
//! flags override it). The CLI never writes, migrates or renames the
//! settings file: a broken file reads as defaults and is left alone.

use std::path::{Path, PathBuf};

use coverly_core::paths::{config_dir, AppDirs};
use coverly_core::profiles::builtin_profiles;
use coverly_core::profiles::custom::{ProfileFile, ProfileStore, PROFILES_DIR};
use coverly_core::settings::{read_settings, Settings, SETTINGS_FILE};

/// Settings file override: unset = the app's, empty = built-in defaults,
/// else the path of a settings file.
pub const SETTINGS_ENV: &str = "COVERLY_SETTINGS";

/// The settings and the custom profiles they point at.
pub struct CliSettings {
    pub settings: Settings,
    pub custom: Vec<ProfileFile>,
}

/// Built-in and custom profile ids (what `default_profile` may name).
fn ids(custom: &[ProfileFile]) -> Vec<String> {
    builtin_profiles()
        .into_iter()
        .map(|p| p.id)
        .chain(custom.iter().map(|c| c.id.clone()))
        .collect()
}

/// The settings file named by [`SETTINGS_ENV`] (see there).
fn settings_path() -> Option<PathBuf> {
    match std::env::var_os(SETTINGS_ENV) {
        Some(v) if v.is_empty() => None,
        Some(v) => Some(PathBuf::from(v)),
        None => config_dir().map(|d| d.join(SETTINGS_FILE)),
    }
}

/// Reads the settings (and custom profiles) without touching the disk.
pub fn load_cli_settings() -> CliSettings {
    let Some(path) = settings_path() else {
        return CliSettings {
            settings: Settings::default(),
            custom: Vec::new(),
        };
    };
    let base = path
        .parent()
        .map(|p| p.join(PROFILES_DIR))
        .unwrap_or_default();
    let (mut custom, mut warnings) = ProfileStore::new(base).load();
    let mut settings = read_settings(&path, &ids(&custom));
    // The custom folder is itself a setting: read the file again with the
    // ids of the folder it names.
    if let Some(dir) = settings.advanced.custom_dir.clone() {
        (custom, warnings) = ProfileStore::new(dir).load();
        settings = read_settings(&path, &ids(&custom));
    }
    for w in warnings {
        eprintln!(
            "{}",
            crate::table::terminal_safe(&format!("warning: custom profile {w}"))
        );
    }
    CliSettings { settings, custom }
}

/// The job data folder: `--data-dir`, else `safety.backup_dir`, else the
/// platform data dir (`None` only without a home dir).
pub fn data_dir(flag: Option<&Path>, s: &Settings) -> Option<PathBuf> {
    match flag.or(s.safety.backup_dir.as_deref()) {
        Some(p) => Some(p.to_path_buf()),
        None => AppDirs::resolve(None, None).map(|d| d.data),
    }
}
