//! The settings types and their defaults (spec §9).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::identify::MatchMethod;
use crate::matching::StrategyKind;
use crate::profiles::Media;
use crate::scan::ScanOptions;

/// Format version written by this build (`library.json` was 1).
pub const SETTINGS_VERSION: u32 = 2;
/// Recent libraries kept.
pub const MAX_RECENT: usize = 10;
/// Every art source the settings know, in default order.
pub const KNOWN_SOURCES: [&str; 3] = ["libretro", "steamgriddb", "screenscraper"];
/// Sources this build can use; the others stay off until they ship (M6).
pub const AVAILABLE_SOURCES: [&str; 1] = ["libretro"];

macro_rules! choice {
    ($(#[$doc:meta])* $name:ident { $(#[$d:meta])* $default:ident $(, $other:ident)* }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum $name {
            $(#[$d])*
            #[default]
            $default,
            $($other),*
        }
    };
}

choice!(/// UI language (`auto` follows the system).
    LanguageChoice { Auto, En, Es });
choice!(/// UI theme (`system` follows the OS).
    ThemeChoice { System, Light, Dark });
choice!(/// Update channel (the updater arrives in M7).
    UpdateChannel { Stable, Beta });
choice!(/// What a fill does with existing art (D25: `ask` = the wizard asks).
    OverwriteSetting { Never, Ask, Backup });
choice!(/// Output image format (`profile` = what the layout says).
    OutputFormat { Profile, Png, Jpg });
choice!(/// Proxy source (D26: `system` reads the standard environment variables).
    ProxyChoice { System, Env, Manual });

/// App log level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub language: LanguageChoice,
    pub theme: ThemeChoice,
    pub check_updates_on_launch: bool,
    pub update_channel: UpdateChannel,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        GeneralSettings {
            language: LanguageChoice::Auto,
            theme: ThemeChoice::System,
            check_updates_on_launch: true,
            update_channel: UpdateChannel::Stable,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryGroup {
    /// Profile used when opening a library; `None` = detect.
    pub default_profile: Option<String>,
    /// Folder names never scanned (case-insensitive, at any depth).
    pub ignored_dirs: Vec<String>,
    /// Extensions never taken as ROMs (lowercase, no dot).
    pub ignored_exts: Vec<String>,
    pub include_hidden: bool,
    /// Scan into symlinked folders (off by default). Art for ROMs under a
    /// linked folder is written through the link, so it can land outside
    /// the library root; the Library screen says so (Task 9) and a
    /// writer-side containment check is an open follow-up (TRACKER M5-01).
    pub follow_symlinks: bool,
    /// Most recent first, at most [`MAX_RECENT`].
    pub recent: Vec<RecentLibrary>,
}

impl Default for LibraryGroup {
    fn default() -> Self {
        let scan = ScanOptions::default();
        LibraryGroup {
            default_profile: None,
            ignored_dirs: scan.ignored_dirs,
            ignored_exts: scan.ignored_exts,
            include_hidden: scan.include_hidden,
            follow_symlinks: scan.follow_symlinks,
            recent: Vec::new(),
        }
    }
}

impl LibraryGroup {
    /// Trimmed, without empty names, `.`/`..` or names holding a path
    /// separator, without case-insensitive duplicates (first spelling wins).
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

    /// Trimmed, lowercase, without a leading dot; only ASCII letters,
    /// digits, `-` and `_`; no duplicates.
    pub fn set_ignored_exts(&mut self, exts: Vec<String>) {
        let mut out: Vec<String> = Vec::new();
        for ext in exts {
            let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
            let ok = !ext.is_empty()
                && ext.len() <= 16
                && ext
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            if ok && !out.contains(&ext) {
                out.push(ext);
            }
        }
        self.ignored_exts = out;
    }
}

/// A box an image must fit in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaxSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtSettings {
    /// Media a fill handles by default (filtered by what the profile stores).
    pub media: Vec<Media>,
    /// Media written to the one slot of single-slot layouts (spec §5).
    pub single_slot_media: Media,
    pub output_format: OutputFormat,
    /// Per profile id: the largest image written (overrides the profile).
    pub max_size: BTreeMap<String, MaxSize>,
    /// Off: images are stretched to exactly the box.
    pub keep_aspect: bool,
    /// 1–100.
    pub jpeg_quality: u8,
    pub overwrite: OverwriteSetting,
}

impl Default for ArtSettings {
    fn default() -> Self {
        ArtSettings {
            media: Media::ALL.to_vec(),
            single_slot_media: Media::Cover,
            output_format: OutputFormat::Profile,
            max_size: BTreeMap::new(),
            keep_aspect: true,
            jpeg_quality: 90,
            overwrite: OverwriteSetting::Never,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchingSettings {
    pub strategy: StrategyKind,
    /// `rom`, `world`, `usa`, … (see `matching::parse_region_order`).
    pub regions: Vec<String>,
    /// Preferred languages, two-letter lowercase codes (D28).
    pub languages: Vec<String>,
    pub auto_threshold: f32,
    pub review_threshold: f32,
    pub fuzzy: bool,
    pub arcade_translation: bool,
    /// Spec §16.3; `name-only` is "hashing off".
    pub match_method: MatchMethod,
    /// Larger ROMs are identified by name.
    pub max_hash_mb: u32,
}

impl Default for MatchingSettings {
    fn default() -> Self {
        MatchingSettings {
            strategy: StrategyKind::RomRegion,
            regions: ["rom", "world", "usa", "europe", "japan"]
                .map(String::from)
                .to_vec(),
            languages: vec!["en".to_string()],
            auto_threshold: 0.90,
            review_threshold: 0.65,
            fuzzy: true,
            arcade_translation: true,
            match_method: MatchMethod::HashThenName,
            max_hash_mb: 64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEntry {
    pub id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourcesSettings {
    /// Every known source once, preferred first.
    pub order: Vec<SourceEntry>,
}

impl Default for SourcesSettings {
    fn default() -> Self {
        SourcesSettings {
            order: KNOWN_SOURCES
                .iter()
                .map(|id| SourceEntry {
                    id: (*id).to_string(),
                    enabled: AVAILABLE_SOURCES.contains(id),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    pub concurrency: u32,
    pub timeout_secs: u32,
    pub retries: u32,
    pub proxy: ProxyChoice,
    /// Used when `proxy` is `manual`.
    pub proxy_url: String,
    /// Downloaded-art store size limit.
    pub cache_max_mb: u32,
    /// How long a libretro listing stays fresh (§16.5: about 2 days).
    pub index_refresh_hours: u32,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        NetworkSettings {
            concurrency: 4,
            timeout_secs: 30,
            retries: 3,
            proxy: ProxyChoice::System,
            proxy_url: String::new(),
            cache_max_mb: 2048,
            index_refresh_hours: 48,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SafetySettings {
    /// Always on (D27): Undo needs the backups.
    pub backup_before_overwrite: bool,
    /// Job data folder (journals + backups); `None` = the app data dir.
    pub backup_dir: Option<PathBuf>,
    pub dry_run_default: bool,
    /// Delete AppleDouble `._*` junk on removable volumes after a job.
    pub clean_apple_double: bool,
    /// Jobs kept for undo.
    pub undo_depth: u32,
}

impl Default for SafetySettings {
    fn default() -> Self {
        SafetySettings {
            backup_before_overwrite: true,
            backup_dir: None,
            dry_run_default: true,
            clean_apple_double: true,
            undo_depth: 10,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvancedSettings {
    pub log_level: LogLevel,
    /// Custom profiles folder; `None` = `<config dir>/profiles`.
    pub custom_dir: Option<PathBuf>,
}

/// Every setting of spec §9.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub general: GeneralSettings,
    pub library: LibraryGroup,
    pub art: ArtSettings,
    pub matching: MatchingSettings,
    pub sources: SourcesSettings,
    pub network: NetworkSettings,
    pub safety: SafetySettings,
    pub advanced: AdvancedSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: SETTINGS_VERSION,
            general: GeneralSettings::default(),
            library: LibraryGroup::default(),
            art: ArtSettings::default(),
            matching: MatchingSettings::default(),
            sources: SourcesSettings::default(),
            network: NetworkSettings::default(),
            safety: SafetySettings::default(),
            advanced: AdvancedSettings::default(),
        }
    }
}

impl Settings {
    /// Puts `entry` first, dropping an older entry with the same path and
    /// anything beyond [`MAX_RECENT`].
    pub fn remember(&mut self, entry: RecentLibrary) {
        self.library.recent.retain(|r| r.path != entry.path);
        self.library.recent.insert(0, entry);
        self.library.recent.truncate(MAX_RECENT);
    }

    /// Drops the library at `path` from the recent list.
    pub fn forget(&mut self, path: &Path) {
        self.library.recent.retain(|r| r.path != path);
    }

    /// Cleans what can be cleaned without asking: list spelling and
    /// duplicates, unknown sources dropped, missing ones appended (off),
    /// sources this build cannot use kept off, the version stamped.
    pub fn normalise(&mut self) {
        self.version = SETTINGS_VERSION;
        let dirs = std::mem::take(&mut self.library.ignored_dirs);
        self.library.set_ignored_dirs(dirs);
        let exts = std::mem::take(&mut self.library.ignored_exts);
        self.library.set_ignored_exts(exts);
        self.library.recent.truncate(MAX_RECENT);
        self.art.media = Media::ALL
            .into_iter()
            .filter(|m| self.art.media.contains(m))
            .collect();
        let mut regions: Vec<String> = Vec::new();
        for r in &self.matching.regions {
            let r = r.trim().to_ascii_lowercase();
            if !r.is_empty() && !regions.contains(&r) {
                regions.push(r);
            }
        }
        // Duplicates in another spelling (`usa`/`USA`) collapse here; a
        // duplicate the user typed is reported by `validate` before this.
        self.matching.regions = regions;
        let mut langs: Vec<String> = Vec::new();
        for l in &self.matching.languages {
            let l = l.trim().to_ascii_lowercase();
            if !l.is_empty() && !langs.contains(&l) {
                langs.push(l);
            }
        }
        self.matching.languages = langs;
        let mut order: Vec<SourceEntry> = Vec::new();
        for e in &self.sources.order {
            if KNOWN_SOURCES.contains(&e.id.as_str()) && !order.iter().any(|o| o.id == e.id) {
                order.push(SourceEntry {
                    id: e.id.clone(),
                    enabled: e.enabled && AVAILABLE_SOURCES.contains(&e.id.as_str()),
                });
            }
        }
        for id in KNOWN_SOURCES {
            if !order.iter().any(|o| o.id == id) {
                order.push(SourceEntry {
                    id: id.to_string(),
                    enabled: false,
                });
            }
        }
        self.sources.order = order;
        self.network.proxy_url = self.network.proxy_url.trim().to_string();
    }
}
