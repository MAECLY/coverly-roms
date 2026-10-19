//! The settings types and their defaults (spec §9).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::identify::MatchMethod;
use crate::matching::StrategyKind;
use crate::organize::target::default_companion_exts;
use crate::organize::{NameMode, DEFAULT_LAYOUT};
use crate::profiles::Media;
use crate::scan::ScanOptions;

/// Format version written by this build (`library.json` was 1).
pub const SETTINGS_VERSION: u32 = 2;
/// Recent libraries kept.
pub const MAX_RECENT: usize = 10;
/// Every art source the settings know, in default order.
pub const KNOWN_SOURCES: [&str; 3] = ["libretro", "steamgriddb", "screenscraper"];

/// Sources this build can use: libretro and SteamGridDB always (SteamGridDB
/// is skipped at fill time without a key), ScreenScraper only with
/// developer credentials (D36).
pub fn available_sources() -> Vec<&'static str> {
    available_sources_with(crate::credentials::DevCredentials::from_build().is_some())
}

/// [`available_sources`] for a build with (`true`) or without ScreenScraper.
pub fn available_sources_with(screenscraper: bool) -> Vec<&'static str> {
    let mut v = vec!["libretro", "steamgriddb"];
    if screenscraper {
        v.push("screenscraper");
    }
    v
}

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
choice!(/// Organise transfer mode (`auto` = move on the same volume, copy otherwise).
    OrganizeModeSetting { Auto, Move, Copy });

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
    /// Scan into symlinked folders (off by default). Art is still only
    /// written inside the canonical library root (`write::ensure_within`):
    /// a linked folder that points elsewhere gets per-item `OutsideRoot`.
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
                    enabled: *id == "libretro",
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

/// Organising a pile of loose ROMs (organise spec §12).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrganizeSettings {
    /// Profile id of the layout proposed by default.
    pub default_layout: String,
    /// System folders go under the layout's ROMs folder (`<picked>/ROMS/<SYS>`);
    /// off = straight in the picked folder.
    pub nest_in_roms_root: bool,
    pub mode: OrganizeModeSetting,
    pub names: NameMode,
    /// Download missing DATs while planning.
    pub download_dats: bool,
    /// 0–1; items at or above it are moved without asking.
    pub auto_threshold: f32,
    /// 0–1, at most `auto_threshold`; items at or above it go to review.
    pub review_threshold: f32,
    /// Lowercase extensions that follow their ROM (`state*` = prefix).
    pub companion_exts: Vec<String>,
    /// Profile id → system id → folder name.
    pub folder_names: BTreeMap<String, BTreeMap<String, String>>,
    /// Profile id → arcade system order (missing = the profile's own).
    pub arcade_order: BTreeMap<String, Vec<String>>,
    /// Files modified more recently than this are left in place.
    pub recent_seconds: u32,
    /// How long organise journals are kept for undo.
    pub keep_organize_journals_days: u32,
}

impl Default for OrganizeSettings {
    fn default() -> Self {
        OrganizeSettings {
            default_layout: DEFAULT_LAYOUT.to_string(),
            nest_in_roms_root: true,
            mode: OrganizeModeSetting::Auto,
            names: NameMode::Keep,
            download_dats: true,
            auto_threshold: 0.85,
            review_threshold: 0.50,
            companion_exts: default_companion_exts(),
            folder_names: BTreeMap::new(),
            arcade_order: BTreeMap::new(),
            recent_seconds: 60,
            keep_organize_journals_days: 90,
        }
    }
}

choice!(/// A dedupe level (duplicates spec §5): exact copies, the same game in
    /// another format, other versions of a game (acts only with
    /// `one_game_one_rom`, reported otherwise).
    DedupeLevel { Exact, Format, Version });
choice!(/// Where extra copies go: the quarantine folder or the OS Trash.
    DedupeActionSetting { Quarantine, Trash });
choice!(/// What Organise does with ROMs already in the library (spec §10).
    OrganizeDedupeMode { SetAside, Report, Off });

/// A keeper rule that can be reordered or turned off (duplicates spec
/// §6.1). The user's pick (always first) and the tie-breaker (always last)
/// are not settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeepRule {
    HasSave,
    Played,
    InLibrary,
    Verified,
    Retail,
    Region,
    Revision,
    Container,
    Cover,
    CatalogueName,
}

impl KeepRule {
    /// Every rule, in the default order.
    pub const ALL: [KeepRule; 10] = [
        KeepRule::HasSave,
        KeepRule::Played,
        KeepRule::InLibrary,
        KeepRule::Verified,
        KeepRule::Retail,
        KeepRule::Region,
        KeepRule::Revision,
        KeepRule::Container,
        KeepRule::Cover,
        KeepRule::CatalogueName,
    ];
}

/// One keeper rule in the user's order, on or off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleToggle {
    pub rule: KeepRule,
    pub on: bool,
}

/// The kind of system a container order applies to, after the system id
/// itself (`container_order` keys `disc`, `cart`, `arcade`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerClass {
    Disc,
    Cart,
    Arcade,
}

impl ContainerClass {
    /// The `container_order` key of the class.
    pub fn key(self) -> &'static str {
        match self {
            ContainerClass::Disc => "disc",
            ContainerClass::Cart => "cart",
            ContainerClass::Arcade => "arcade",
        }
    }
}

/// `container_order` keys that are not system ids.
pub const CONTAINER_CLASS_KEYS: [&str; 4] = ["disc", "cart", "arcade", "*"];

/// Disc order of the handheld profiles (duplicates spec §6.3).
const HANDHELD_DISC_ORDER: [&str; 6] = ["chd", "pbp", "cue", "iso", "img", "bin"];
/// Disc order of the frontends, which read cue sheets well.
const FRONTEND_DISC_ORDER: [&str; 6] = ["chd", "cue", "pbp", "iso", "img", "bin"];

fn default_container_order() -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
    let disc = |order: &[&str]| {
        BTreeMap::from([(
            "disc".to_string(),
            order.iter().map(|c| (*c).to_string()).collect(),
        )])
    };
    let mut out = BTreeMap::new();
    for p in ["anbernic-stock", "garlicos", "onion", "muos", "minui"] {
        out.insert(p.to_string(), disc(&HANDHELD_DISC_ORDER));
    }
    for p in ["esde", "batocera", "retroarch"] {
        out.insert(p.to_string(), disc(&FRONTEND_DISC_ORDER));
    }
    // Never prefer an unzipped arcade set.
    out.insert(
        "*".to_string(),
        BTreeMap::from([("arcade".to_string(), vec!["zip".to_string()])]),
    );
    out
}

/// The container order for one system in a `container_order` map (as
/// [`DedupeSettings::container_for`]); `None` = no preference.
pub fn container_order_for<'a>(
    map: &'a BTreeMap<String, BTreeMap<String, Vec<String>>>,
    profile: &str,
    system: &str,
    class: ContainerClass,
) -> Option<&'a [String]> {
    [profile, "*"]
        .iter()
        .filter_map(|p| map.get(*p))
        .flat_map(|m| [system, class.key(), "*"].map(|k| m.get(k)))
        .flatten()
        .next()
        .map(Vec::as_slice)
        .filter(|order| !order.is_empty())
}

/// Finding and setting aside duplicate ROMs (duplicates spec §13).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DedupeSettings {
    /// Levels looked for; `version` acts only with `one_game_one_rom`.
    pub levels: Vec<DedupeLevel>,
    /// Also set aside other versions of a game (1G1R).
    pub one_game_one_rom: bool,
    pub action: DedupeActionSetting,
    /// Absolute; `None` = next to the ROMs folder.
    pub quarantine_dir: Option<PathBuf>,
    /// Quarantine folder name, fixed at first run from the app language
    /// (`_Duplicados` / `_Duplicates`); `None` = not chosen yet.
    pub quarantine_name: Option<String>,
    /// Every [`KeepRule`] once, in the order they are weighed.
    pub keep_rules: Vec<RuleToggle>,
    /// Profile id (or `*`) → system id, `disc`, `cart`, `arcade` (or `*`)
    /// → containers, preferred first (lowercase extensions, `zip`, `raw`).
    /// Missing or empty = no preference.
    pub container_order: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// Folder names or system ids never looked at (lowercase).
    pub exclude_systems: Vec<String>,
    /// DATs a library run may download.
    pub max_dat_downloads: u32,
    pub organize_step: OrganizeDedupeMode,
}

impl Default for DedupeSettings {
    fn default() -> Self {
        DedupeSettings {
            levels: vec![
                DedupeLevel::Exact,
                DedupeLevel::Format,
                DedupeLevel::Version,
            ],
            one_game_one_rom: false,
            action: DedupeActionSetting::Quarantine,
            quarantine_dir: None,
            quarantine_name: None,
            keep_rules: KeepRule::ALL
                .map(|rule| RuleToggle { rule, on: true })
                .to_vec(),
            container_order: default_container_order(),
            exclude_systems: ["ports", "bios", "apps"].map(String::from).to_vec(),
            max_dat_downloads: 12,
            organize_step: OrganizeDedupeMode::SetAside,
        }
    }
}

impl DedupeSettings {
    /// The container order for one system: the profile's entries before
    /// `*`'s, and in each the system id before its class before `*`.
    /// `None` = no preference.
    pub fn container_for(
        &self,
        profile: &str,
        system: &str,
        class: ContainerClass,
    ) -> Option<&[String]> {
        container_order_for(&self.container_order, profile, system, class)
    }

    /// Spelling and duplicates of the lists; missing rules appended, on.
    fn normalise(&mut self) {
        let mut excluded: Vec<String> = Vec::new();
        for e in &self.exclude_systems {
            let e = e.trim().to_lowercase();
            if !e.is_empty() && !excluded.contains(&e) {
                excluded.push(e);
            }
        }
        self.exclude_systems = excluded;
        if let Some(name) = &mut self.quarantine_name {
            *name = name.trim().to_string();
        }
        for rule in KeepRule::ALL {
            if !self.keep_rules.iter().any(|t| t.rule == rule) {
                self.keep_rules.push(RuleToggle { rule, on: true });
            }
        }
    }
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
    pub organize: OrganizeSettings,
    pub dedupe: DedupeSettings,
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
            organize: OrganizeSettings::default(),
            dedupe: DedupeSettings::default(),
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
        let available = available_sources();
        let mut order: Vec<SourceEntry> = Vec::new();
        for e in &self.sources.order {
            if KNOWN_SOURCES.contains(&e.id.as_str()) && !order.iter().any(|o| o.id == e.id) {
                order.push(SourceEntry {
                    id: e.id.clone(),
                    enabled: e.enabled && available.contains(&e.id.as_str()),
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
        let mut companions: Vec<String> = Vec::new();
        for e in &self.organize.companion_exts {
            let e = e.trim().trim_start_matches('.').to_ascii_lowercase();
            if !e.is_empty() && !companions.contains(&e) {
                companions.push(e);
            }
        }
        self.organize.companion_exts = companions;
        self.organize.default_layout = self.organize.default_layout.trim().to_string();
        self.dedupe.normalise();
    }
}
