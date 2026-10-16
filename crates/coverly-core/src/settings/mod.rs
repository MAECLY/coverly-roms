//! Persisted settings. M3 needs the "Library" group of spec §9: ignored
//! folders and the recent libraries list, stored as JSON in the app config
//! dir (see [`crate::paths::config_dir`]). Loading never fails: a missing
//! file gives the defaults and a corrupt one is renamed to
//! `library.json.bad` and replaced by the defaults.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::scan::ScanOptions;

/// File name of the library settings inside the config dir.
pub const LIBRARY_SETTINGS_FILE: &str = "library.json";
/// Recent libraries kept.
pub const MAX_RECENT: usize = 10;

/// One library the user opened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentLibrary {
    /// Folder as the user picked it.
    pub path: PathBuf,
    /// Display name (the folder name).
    pub name: String,
    /// Profile it was opened with.
    pub profile_id: String,
    /// ROMs found the last time.
    pub roms: u32,
    /// When it was last opened, in Unix seconds.
    pub opened_at: i64,
}

/// The "Library" settings group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibrarySettings {
    /// Format version of the file.
    pub version: u32,
    /// Folder names never scanned (case-insensitive, at any depth).
    pub ignored_dirs: Vec<String>,
    /// Most recent first, at most [`MAX_RECENT`].
    pub recent: Vec<RecentLibrary>,
}

impl Default for LibrarySettings {
    fn default() -> Self {
        LibrarySettings {
            version: 1,
            ignored_dirs: ScanOptions::default().ignored_dirs,
            recent: Vec::new(),
        }
    }
}

impl LibrarySettings {
    /// Reads `path`; defaults when it is missing or unreadable. A file that
    /// is not valid JSON is renamed to `<name>.bad` first, so it is never
    /// silently overwritten.
    pub fn load(path: &Path) -> LibrarySettings {
        let Ok(text) = fs::read_to_string(path) else {
            return LibrarySettings::default();
        };
        match serde_json::from_str::<LibrarySettings>(&text) {
            Ok(mut settings) => {
                let dirs = std::mem::take(&mut settings.ignored_dirs);
                settings.set_ignored_dirs(dirs);
                settings.recent.truncate(MAX_RECENT);
                settings
            }
            Err(_) => {
                let _ = fs::rename(path, path.with_extension("json.bad"));
                LibrarySettings::default()
            }
        }
    }

    /// Writes `path` atomically (temp file + rename), creating its folder.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)
    }

    /// Puts `entry` first, dropping an older entry with the same path and
    /// anything beyond [`MAX_RECENT`].
    pub fn remember(&mut self, entry: RecentLibrary) {
        self.recent.retain(|r| r.path != entry.path);
        self.recent.insert(0, entry);
        self.recent.truncate(MAX_RECENT);
    }

    /// Drops the library at `path` from the recent list.
    pub fn forget(&mut self, path: &Path) {
        self.recent.retain(|r| r.path != path);
    }

    /// Sets the ignored folders: trimmed, without empty names, `.`/`..` or
    /// names holding a path separator, without case-insensitive duplicates
    /// (the first spelling wins).
    pub fn set_ignored_dirs(&mut self, dirs: Vec<String>) {
        let mut out: Vec<String> = Vec::new();
        for dir in dirs {
            let dir = dir.trim();
            if dir.is_empty() || dir == "." || dir == ".." || dir.contains(['/', '\\']) {
                continue;
            }
            if out.iter().any(|o| o.to_lowercase() == dir.to_lowercase()) {
                continue;
            }
            out.push(dir.to_string());
        }
        self.ignored_dirs = out;
    }

    /// Scan options with these ignored folders and the other defaults.
    pub fn scan_options(&self) -> ScanOptions {
        ScanOptions {
            ignored_dirs: self.ignored_dirs.clone(),
            ..ScanOptions::default()
        }
    }
}
