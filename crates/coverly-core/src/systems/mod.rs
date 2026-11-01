//! System database: platforms, their folder aliases and ROM extensions.
//!
//! The built-in database is `systems.toml`, embedded at compile time. Users
//! can extend or override it with a TOML file in the same format.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identify::hash::HeaderRule;
use crate::profiles::Profile;

/// The embedded built-in database.
const BUILTIN_TOML: &str = include_str!("systems.toml");

/// The kind of machine a system is, used to group systems in pickers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    /// Home console (the default).
    #[default]
    Console,
    /// Handheld console.
    Handheld,
    /// Home computer.
    Computer,
    /// Arcade board or ROM set collection.
    Arcade,
    /// Game engine, port or fantasy console run by an interpreter.
    Engine,
}

impl Family {
    /// Every family, in picker order.
    pub const ALL: [Family; 5] = [
        Family::Console,
        Family::Handheld,
        Family::Computer,
        Family::Arcade,
        Family::Engine,
    ];

    /// Parses a TOML value, ignoring case and surrounding spaces.
    pub fn parse(s: &str) -> Option<Family> {
        let key = s.trim().to_ascii_lowercase();
        Family::ALL.into_iter().find(|f| f.as_str() == key)
    }

    /// The TOML (and IPC) spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Family::Console => "console",
            Family::Handheld => "handheld",
            Family::Computer => "computer",
            Family::Arcade => "arcade",
            Family::Engine => "engine",
        }
    }
}

/// One platform (e.g. Super Nintendo) as Coverly ROMs knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct System {
    /// Canonical id, lowercase, e.g. `snes`.
    pub id: String,
    /// Display name, e.g. `Super Nintendo`.
    pub name: String,
    /// Folder names, compared case-insensitively.
    pub aliases: Vec<String>,
    /// ROM extensions, lowercase and without the dot.
    pub extensions: Vec<String>,
    /// libretro-thumbnails playlist name, exactly as on the server.
    pub libretro: Option<String>,
    /// Extra libretro-thumbnails folders tried after `libretro`, for covers
    /// only (spec D9). `{libretro}` in templates keeps the primary.
    pub libretro_also: Vec<String>,
    /// Arcade platform (ROM set names instead of game titles).
    pub arcade: bool,
    /// Disc system: cue/bin, m3u, gdi, chd.
    pub multi_file: bool,
    /// Header rule for content hashing.
    pub header: HeaderRule,
    /// libretro-database files (repo-relative) to identify ROMs with;
    /// empty = derived from the libretro name.
    pub dats: Vec<String>,
    /// ScreenScraper system id (`systemesListe.php`); `None` = not
    /// available on ScreenScraper.
    pub screenscraper: Option<u32>,
    /// Kind of machine (picker grouping).
    pub family: Family,
    /// Extensions accepted inside this system's folder, or with its header
    /// or hash, but never evidence by themselves: they do not count for
    /// [`SystemDb::extension_class`]. Lowercase, without the dot.
    pub weak_extensions: Vec<String>,
    /// Aliases (lowercase) that resolve only under a profile that names
    /// them, never through the global [`SystemDb::resolve_folder`].
    pub scoped_aliases: Vec<String>,
}

impl System {
    /// Strong or weak: may this system's folder hold a file with `ext`
    /// (any case, leading dot optional)?
    pub fn accepts_extension(&self, ext: &str) -> bool {
        let ext = normalize_extension(ext);
        !ext.is_empty() && (self.extensions.contains(&ext) || self.weak_extensions.contains(&ext))
    }

    /// Whether `name` is one of this system's profile-scoped aliases.
    /// Any case: compared lowercased, like [`SystemDb::resolve_folder`].
    pub fn is_scoped_alias(&self, name: &str) -> bool {
        let key = name.trim().to_lowercase();
        self.scoped_aliases.contains(&key)
    }
}

/// Errors loading a systems TOML file.
#[derive(Debug, Error)]
pub enum SystemsError {
    /// The TOML text could not be parsed.
    #[error("invalid systems TOML: {0}")]
    Parse(#[from] toml::de::Error),
    /// An entry has an empty `id`.
    #[error("a system entry has an empty id")]
    EmptyId,
    /// An entry introduces a new id without a `name`.
    #[error("system `{id}` is not built in and needs a `name`")]
    MissingName {
        /// The offending id.
        id: String,
    },
    /// A `header` value is not a known rule.
    #[error(
        "system `{id}`: unknown header rule `{value}` (expected plain, nes, fds, snes-copier, lynx or a7800)"
    )]
    BadHeader {
        /// The offending id.
        id: String,
        /// The unknown value.
        value: String,
    },
    /// A `dats` entry is not a safe relative repository path.
    #[error(
        "system `{id}`: DAT path `{value}` must be relative, with no `..`, empty segments, `\\` or control characters"
    )]
    BadDat {
        /// The offending id.
        id: String,
        /// The rejected path.
        value: String,
    },
}

/// A `[[system]]` table as written in TOML. Every field but `id` is
/// optional so that user files can override only what they need.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSystem {
    id: String,
    name: Option<String>,
    aliases: Option<Vec<String>>,
    extensions: Option<Vec<String>>,
    libretro: Option<String>,
    libretro_also: Option<Vec<String>>,
    arcade: Option<bool>,
    multi_file: Option<bool>,
    header: Option<String>,
    dats: Option<Vec<String>>,
    screenscraper: Option<u32>,
    family: Option<String>,
    weak_extensions: Option<Vec<String>>,
    scoped_aliases: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    system: Vec<RawSystem>,
}

/// The set of known systems.
#[derive(Debug, Clone, Default)]
pub struct SystemDb {
    systems: Vec<System>,
}

impl SystemDb {
    /// Parses the embedded `systems.toml`.
    pub fn builtin() -> SystemDb {
        SystemDb::default()
            .with_overrides(BUILTIN_TOML)
            .expect("embedded systems.toml is valid")
    }

    /// Merges a user TOML file (same format as `systems.toml`).
    ///
    /// Ids are trimmed and lowercased, aliases are trimmed (empty ones are
    /// dropped). An entry whose `id` already exists overrides it: `name`, `libretro`,
    /// `arcade`, `multi_file`, `header`, `dats` and `screenscraper` replace the current values when given;
    /// `aliases`, `extensions`, `weak_extensions`, `scoped_aliases` and
    /// `libretro_also` are added to the current lists (without repeats). An
    /// alias claimed by an override is removed from any other system (and from its scoped aliases), so the user file
    /// always wins. A strong extension removes the same weak one. `family`
    /// replaces the current value when it is known; an unknown value is
    /// ignored, so user files stay lenient. An entry with a new `id` is
    /// appended and must have a `name`; its family defaults to `arcade` when
    /// `arcade = true`, otherwise `console`.
    pub fn with_overrides(mut self, toml_text: &str) -> Result<SystemDb, SystemsError> {
        let file: RawFile = toml::from_str(toml_text)?;
        for raw in file.system {
            self.merge(raw)?;
        }
        Ok(self)
    }

    fn merge(&mut self, raw: RawSystem) -> Result<(), SystemsError> {
        // Ids are canonical lowercase so overrides match regardless of case.
        let id = raw.id.trim().to_lowercase();
        if id.is_empty() {
            return Err(SystemsError::EmptyId);
        }
        let aliases: Vec<String> = raw
            .aliases
            .unwrap_or_default()
            .iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();
        let extensions: Vec<String> = raw
            .extensions
            .unwrap_or_default()
            .iter()
            .map(|e| normalize_extension(e))
            .filter(|e| !e.is_empty())
            .collect();
        let weak_extensions: Vec<String> = raw
            .weak_extensions
            .unwrap_or_default()
            .iter()
            .map(|e| normalize_extension(e))
            .filter(|e| !e.is_empty())
            .collect();
        let libretro_also: Vec<String> = raw
            .libretro_also
            .unwrap_or_default()
            .iter()
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect();
        let scoped_aliases: Vec<String> = raw
            .scoped_aliases
            .unwrap_or_default()
            .iter()
            .map(|a| a.trim().to_lowercase())
            .filter(|a| !a.is_empty())
            .collect();

        // The user file wins: an alias belongs to one system only.
        for alias in &aliases {
            let key = alias.to_lowercase();
            for other in self.systems.iter_mut().filter(|s| s.id != id) {
                other.aliases.retain(|a| a.to_lowercase() != key);
                other.scoped_aliases.retain(|a| *a != key);
            }
        }

        let header = match raw.header {
            Some(value) => Some(HeaderRule::parse(&value).ok_or(SystemsError::BadHeader {
                id: id.clone(),
                value,
            })?),
            None => None,
        };

        if let Some(bad) = raw.dats.iter().flatten().find(|d| !is_safe_repo_path(d)) {
            return Err(SystemsError::BadDat {
                id,
                value: bad.clone(),
            });
        }

        let index = match self.systems.iter().position(|s| s.id == id) {
            Some(index) => index,
            None => {
                let name = raw
                    .name
                    .clone()
                    .ok_or(SystemsError::MissingName { id: id.clone() })?;
                self.systems.push(System {
                    id,
                    name,
                    aliases: Vec::new(),
                    extensions: Vec::new(),
                    libretro: None,
                    libretro_also: Vec::new(),
                    arcade: false,
                    multi_file: false,
                    header: HeaderRule::Plain,
                    dats: Vec::new(),
                    screenscraper: None,
                    family: if raw.arcade == Some(true) {
                        Family::Arcade
                    } else {
                        Family::Console
                    },
                    weak_extensions: Vec::new(),
                    scoped_aliases: Vec::new(),
                });
                self.systems.len() - 1
            }
        };
        let system = &mut self.systems[index];
        if let Some(name) = raw.name {
            system.name = name;
        }
        if raw.libretro.is_some() {
            system.libretro = raw.libretro;
        }
        if let Some(arcade) = raw.arcade {
            system.arcade = arcade;
        }
        if let Some(multi_file) = raw.multi_file {
            system.multi_file = multi_file;
        }
        if let Some(header) = header {
            system.header = header;
        }
        if let Some(dats) = raw.dats {
            system.dats = dats;
        }
        if raw.screenscraper.is_some() {
            system.screenscraper = raw.screenscraper;
        }
        // An unknown family is ignored silently (the crate has no logger):
        // the current value, or the default of a new system, is kept.
        if let Some(family) = raw.family.as_deref().and_then(Family::parse) {
            system.family = family;
        }
        for alias in aliases {
            let key = alias.to_lowercase();
            if !system.aliases.iter().any(|a| a.to_lowercase() == key) {
                system.aliases.push(alias);
            }
        }
        for ext in extensions {
            if !system.extensions.contains(&ext) {
                system.extensions.push(ext);
            }
        }
        for ext in weak_extensions {
            if !system.weak_extensions.contains(&ext) {
                system.weak_extensions.push(ext);
            }
        }
        // The user wins: an extension listed as strong is never also weak.
        let strong = &system.extensions;
        system.weak_extensions.retain(|w| !strong.contains(w));
        for name in libretro_also {
            if !system.libretro_also.contains(&name) {
                system.libretro_also.push(name);
            }
        }
        for alias in scoped_aliases {
            if !system.scoped_aliases.contains(&alias) {
                system.scoped_aliases.push(alias);
            }
        }
        Ok(())
    }

    /// Finds the system whose alias matches `folder_name`, ignoring case.
    /// Profile-scoped aliases never match here.
    pub fn resolve_folder(&self, folder_name: &str) -> Option<&System> {
        if folder_name.is_empty() {
            return None;
        }
        let key = folder_name.to_lowercase();
        self.systems.iter().find(|s| {
            s.aliases
                .iter()
                .any(|a| a.to_lowercase() == key && !s.is_scoped_alias(a))
        })
    }

    /// Like [`SystemDb::resolve_folder`] under `profile`: its `Named`
    /// folders first, then every alias, scoped ones included. `None` (no
    /// layout known: profile detection) is the global lookup.
    pub fn resolve_folder_in(&self, profile: Option<&Profile>, name: &str) -> Option<&System> {
        let Some(profile) = profile else {
            return self.resolve_folder(name);
        };
        if name.is_empty() {
            return None;
        }
        if let Some(s) = profile.folder_owner(name).and_then(|id| self.get(id)) {
            return Some(s);
        }
        let key = name.to_lowercase();
        self.systems
            .iter()
            .find(|s| s.aliases.iter().any(|a| a.to_lowercase() == key))
    }

    /// Checks the invariants of the built-in database; `Err` lists every
    /// violation. Used by a test, never at runtime, so user files stay
    /// lenient.
    pub fn check_builtin_invariants(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let mut owners: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
        for s in &self.systems {
            if !s.libretro_also.is_empty() && s.libretro.is_none() {
                errors.push(format!("{}: libretro_also without libretro", s.id));
            }
            for also in &s.libretro_also {
                if Some(also) == s.libretro.as_ref() {
                    errors.push(format!("{}: libretro_also repeats libretro {also}", s.id));
                }
            }
            for w in &s.weak_extensions {
                if s.extensions.contains(w) {
                    errors.push(format!("{}: .{w} is both strong and weak", s.id));
                }
                if CONTAINER_EXTS.contains(&w.as_str()) {
                    errors.push(format!("{}: container .{w} cannot be weak", s.id));
                }
            }
            for scoped in &s.scoped_aliases {
                if !s.aliases.iter().any(|a| a.to_lowercase() == *scoped) {
                    errors.push(format!(
                        "{}: scoped alias {scoped} is not one of its aliases",
                        s.id
                    ));
                }
            }
            for alias in &s.aliases {
                if let Some(other) = owners.insert(alias.to_lowercase(), &s.id) {
                    if other != s.id {
                        errors.push(format!("alias {alias} is used by {other} and {}", s.id));
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Finds a system by canonical id, ignoring case and surrounding spaces.
    pub fn get(&self, id: &str) -> Option<&System> {
        let key = id.trim().to_lowercase();
        self.systems.iter().find(|s| s.id == key)
    }

    /// All systems, in file order (overrides with new ids come last).
    pub fn all(&self) -> &[System] {
        &self.systems
    }
}

/// Extensions that wrap a ROM: never evidence by themselves.
pub const CONTAINER_EXTS: [&str; 2] = ["zip", "7z"];
/// Extensions many non-ROM files use: no system candidate without
/// content evidence (spec §6.4).
pub const GENERIC_EXTS: [&str; 6] = ["bin", "iso", "img", "rom", "dat", "chd"];
/// Disc/list formats that are ROM-like even when no system lists them.
const LIST_EXTS: [&str; 3] = ["cue", "gdi", "m3u"];
/// Everyday file types that a system lists only so existing libraries
/// scan (`pico8` carts are `.p8.png`, `dos` games ship `.exe`/`.bat`/
/// `.conf`). Loose, they are screenshots, installers and scripts: no
/// evidence and no organise candidate (plan O-D7).
const EVERYDAY_EXTS: [&str; 4] = ["png", "exe", "bat", "conf"];
/// Weak-only extensions whose own header decides (systems batch 2 D7):
/// a loose `.pkg` is an organise candidate so the PKG header (PS3, Vita)
/// or the `xar!` magic of a macOS installer can name it. Its class stays
/// [`ExtClass::Unknown`]: the extension alone is no evidence.
const HEADER_DECIDED_EXTS: [&str; 1] = ["pkg"];

/// How much an extension says about the system (spec §6.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtClass {
    /// `zip`, `7z`: look inside.
    Container,
    /// `bin`, `iso`, `img`, `rom`, `dat`, `chd`: says nothing alone.
    Generic,
    /// Listed by exactly one system.
    Unique,
    /// Listed by several systems.
    Shared,
    /// Listed by none, or an everyday file type (`png`, `exe`…).
    Unknown,
}

impl SystemDb {
    /// Systems listing `ext` (any case, leading dot optional); empty for
    /// containers.
    pub fn systems_for_extension(&self, ext: &str) -> Vec<&System> {
        let ext = normalize_extension(ext);
        if ext.is_empty() || CONTAINER_EXTS.contains(&ext.as_str()) {
            return Vec::new();
        }
        self.systems
            .iter()
            .filter(|s| s.extensions.contains(&ext))
            .collect()
    }

    /// Strong and weak owners of `ext` (folder, header and hash paths);
    /// empty for containers.
    pub fn systems_accepting(&self, ext: &str) -> Vec<&System> {
        let ext = normalize_extension(ext);
        if ext.is_empty() || CONTAINER_EXTS.contains(&ext.as_str()) {
            return Vec::new();
        }
        self.systems
            .iter()
            .filter(|s| s.accepts_extension(&ext))
            .collect()
    }

    /// What `ext` says about the system.
    pub fn extension_class(&self, ext: &str) -> ExtClass {
        let lower = normalize_extension(ext);
        if EVERYDAY_EXTS.contains(&lower.as_str()) {
            return ExtClass::Unknown;
        }
        if CONTAINER_EXTS.contains(&lower.as_str()) {
            return ExtClass::Container;
        }
        if GENERIC_EXTS.contains(&lower.as_str()) {
            return ExtClass::Generic;
        }
        match self.systems_for_extension(&lower).len() {
            0 => ExtClass::Unknown,
            1 => ExtClass::Unique,
            _ => ExtClass::Shared,
        }
    }

    /// The system whose DAT list (see `dat_sources`) contains `dat`.
    ///
    /// Arcade DATs are shared (`MAME.dat` is listed by `cps1`, `cps2`,
    /// `neogeo`… as a fallback): the system with the fewest sources wins,
    /// so `MAME.dat` maps to `mame` and the FBNeo DAT to `fbneo`; ties
    /// keep database order.
    pub fn system_for_dat(&self, dat: &str) -> Option<&System> {
        self.systems
            .iter()
            .filter_map(|s| {
                let sources = crate::identify::dat::dat_sources(s);
                sources
                    .iter()
                    .any(|d| d == dat)
                    .then_some((sources.len(), s))
            })
            .min_by_key(|(n, _)| *n)
            .map(|(_, s)| s)
    }

    /// Whether a file with extension `ext` may be a ROM, a disc image or
    /// an archive of one (organise candidates).
    pub fn is_rom_like_extension(&self, ext: &str) -> bool {
        let lower = normalize_extension(ext);
        !lower.is_empty()
            && (LIST_EXTS.contains(&lower.as_str())
                || HEADER_DECIDED_EXTS.contains(&lower.as_str())
                || self.extension_class(&lower) != ExtClass::Unknown)
    }
}

/// A relative `/`-separated path with no empty, `.` or `..` segment, no
/// `\` and no control characters, so it stays under the DAT base URL.
pub(crate) fn is_safe_repo_path(path: &str) -> bool {
    !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

/// Lowercases an extension and strips a leading dot: `.SFC` -> `sfc`.
fn normalize_extension(ext: &str) -> String {
    ext.trim().trim_start_matches('.').to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every system folder on the reference SD card (Anbernic stock / Onion style).
    const SD_FOLDERS: &[&str] = &[
        "A2600",
        "A5200",
        "A7800",
        "A800",
        "AMIGA",
        "ATARIST",
        "ATOMISWAVE",
        "C64",
        "CPS1",
        "CPS2",
        "CPS3",
        "DOS",
        "DREAMCAST",
        "EASYRPG",
        "FBNEO",
        "FC",
        "FDS",
        "GB",
        "GBA",
        "GBC",
        "GG",
        "GW",
        "HBMAME",
        "LYNX",
        "MAME",
        "MD",
        "MDCD",
        "MSX",
        "N64",
        "NAOMI",
        "NDS",
        "NEOCD",
        "NEOGEO",
        "NGP",
        "ONS",
        "OPENBOR",
        "PCE",
        "PCECD",
        "PGM2",
        "PICO",
        "POKE",
        "PS",
        "PSP",
        "SATURN",
        "SCUMMVM",
        "SEGA32X",
        "SFC",
        "SMS",
        "VARCADE",
        "VB",
        "VIC20",
        "WS",
    ];

    /// Common folder names from other firmwares and their expected system.
    const COMMON_ALIASES: &[(&str, &str)] = &[
        ("nes", "nes"),
        ("snes", "snes"),
        ("genesis", "megadrive"),
        ("megadrive", "megadrive"),
        ("psx", "psx"),
        ("ps1", "psx"),
        ("gba", "gba"),
        ("mastersystem", "mastersystem"),
        ("gamegear", "gamegear"),
        ("pcengine", "pcengine"),
        ("tg16", "pcengine"),
        ("segacd", "segacd"),
        ("neogeo", "neogeo"),
        ("arcade", "mame"),
        ("fbneo", "fbneo"),
        ("n64", "n64"),
        ("nds", "nds"),
    ];

    #[test]
    fn every_sd_folder_resolves() {
        let db = SystemDb::builtin();
        let missing: Vec<_> = SD_FOLDERS
            .iter()
            .filter(|f| db.resolve_folder(f).is_none())
            .collect();
        assert!(missing.is_empty(), "unresolved SD folders: {missing:?}");
    }

    #[test]
    fn sd_folders_map_to_expected_ids() {
        let db = SystemDb::builtin();
        for (folder, id) in [
            ("FC", "nes"),
            ("SFC", "snes"),
            ("MD", "megadrive"),
            ("MDCD", "segacd"),
            ("PS", "psx"),
            ("PCE", "pcengine"),
            ("PCECD", "pcenginecd"),
            ("SMS", "mastersystem"),
            ("GG", "gamegear"),
        ] {
            assert_eq!(
                db.resolve_folder(folder).map(|s| s.id.as_str()),
                Some(id),
                "{folder}"
            );
        }
    }

    #[test]
    fn common_aliases_resolve() {
        let db = SystemDb::builtin();
        for (folder, id) in COMMON_ALIASES {
            assert_eq!(
                db.resolve_folder(folder).map(|s| s.id.as_str()),
                Some(*id),
                "folder {folder}"
            );
        }
    }

    #[test]
    fn resolution_ignores_case() {
        let db = SystemDb::builtin();
        for folder in ["sfc", "SFC", "Sfc", "Super Nintendo", "SUPER NINTENDO"] {
            assert_eq!(
                db.resolve_folder(folder).map(|s| s.id.as_str()),
                Some("snes"),
                "folder {folder}"
            );
        }
    }

    #[test]
    fn non_system_folders_do_not_resolve() {
        let db = SystemDb::builtin();
        for folder in ["APPS", "PORTS", "JAVA", "bios", "Imgs", ""] {
            assert!(db.resolve_folder(folder).is_none(), "folder {folder}");
        }
    }

    #[test]
    fn builtin_entries_are_well_formed() {
        let db = SystemDb::builtin();
        assert!(db.all().len() >= SD_FOLDERS.len());
        let mut ids = std::collections::HashSet::new();
        let mut aliases = std::collections::HashMap::new();
        for s in db.all() {
            assert!(ids.insert(s.id.clone()), "duplicate id {}", s.id);
            assert!(!s.name.is_empty(), "{} has no name", s.id);
            assert!(!s.aliases.is_empty(), "{} has no aliases", s.id);
            // WASM-4 lists only a weak `.wasm` (D4): its folder still
            // accepts exactly that. Data-folder ports (Cave Story,
            // Wolfenstein 3D, Mr. Boom, Flashback, Rick Dangerous) list
            // none: they are claimed by folder alias or DAT only (D15).
            assert!(
                !s.extensions.is_empty()
                    || !s.weak_extensions.is_empty()
                    || s.family == Family::Engine,
                "{} has no extensions",
                s.id
            );
            for ext in s.extensions.iter().chain(&s.weak_extensions) {
                assert_eq!(ext, &ext.to_lowercase(), "{}: {ext} not lowercase", s.id);
                assert!(!ext.starts_with('.'), "{}: {ext} has a dot", s.id);
            }
            for alias in &s.aliases {
                if let Some(other) = aliases.insert(alias.to_lowercase(), s.id.clone()) {
                    panic!("alias {alias} used by {other} and {}", s.id);
                }
            }
        }
    }

    #[test]
    fn dats_are_explicit_for_arcade_and_replaced_by_overrides() {
        let db = SystemDb::builtin();
        assert_eq!(db.get("mame").unwrap().dats, ["metadat/mame/MAME.dat"]);
        assert_eq!(
            db.get("neogeo").unwrap().dats,
            [
                "metadat/fbneo-split/FBNeo - Arcade Games.dat",
                "metadat/mame/MAME.dat"
            ]
        );
        assert!(db.get("gba").unwrap().dats.is_empty(), "derived");
        let db = db
            .with_overrides("[[system]]\nid = \"neogeo\"\ndats = [\"dat/One.dat\"]\n")
            .unwrap();
        assert_eq!(db.get("neogeo").unwrap().dats, ["dat/One.dat"]);
    }

    #[test]
    fn screenscraper_ids_are_built_in_and_overridable() {
        let db = SystemDb::builtin();
        assert_eq!(db.get("gba").unwrap().screenscraper, Some(12));
        assert_eq!(db.get("snes").unwrap().screenscraper, Some(4));
        assert_eq!(db.get("neogeo").unwrap().screenscraper, Some(142));
        assert_eq!(db.get("pico8").unwrap().screenscraper, None);
        let db = db
            .with_overrides("[[system]]\nid = \"pico8\"\nscreenscraper = 234\n")
            .unwrap();
        assert_eq!(db.get("pico8").unwrap().screenscraper, Some(234));
        assert_eq!(db.get("gba").unwrap().screenscraper, Some(12), "kept");
    }

    #[test]
    fn unsafe_dat_paths_are_rejected() {
        for bad in [
            "metadat/../../../x.dat",
            "..",
            "/etc/x.dat",
            "metadat//x.dat",
            "metadat/x.dat/",
            "metadat\\x.dat",
            "metadat/x\u{0}.dat",
            "metadat/x\n.dat",
            "",
        ] {
            let value = toml::Value::String(bad.into());
            let toml = format!("[[system]]\nid = \"gba\"\ndats = [{value}]\n");
            let err = SystemDb::builtin().with_overrides(&toml).unwrap_err();
            assert!(
                matches!(err, SystemsError::BadDat { ref id, .. } if id == "gba"),
                "{bad:?}: {err}"
            );
        }
        let ok = SystemDb::builtin()
            .with_overrides("[[system]]\nid = \"gba\"\ndats = [\"dat/My Set (v1.2).dat\"]\n")
            .unwrap();
        assert_eq!(ok.get("gba").unwrap().dats, ["dat/My Set (v1.2).dat"]);
    }

    #[test]
    fn get_by_id_and_fields() {
        let db = SystemDb::builtin();
        let snes = db.get("snes").expect("snes");
        assert_eq!(snes.name, "Super Nintendo");
        assert!(snes.extensions.iter().any(|e| e == "sfc"));
        assert_eq!(
            snes.libretro.as_deref(),
            Some("Nintendo - Super Nintendo Entertainment System")
        );
        assert!(!snes.arcade && !snes.multi_file);

        let lr = |id: &str| db.get(id).and_then(|s| s.libretro.clone());
        assert_eq!(
            lr("nes").as_deref(),
            Some("Nintendo - Nintendo Entertainment System")
        );
        assert_eq!(
            lr("megadrive").as_deref(),
            Some("Sega - Mega Drive - Genesis")
        );
        assert_eq!(lr("psx").as_deref(), Some("Sony - PlayStation"));
        assert_eq!(lr("nds").as_deref(), Some("Nintendo - Nintendo DS"));
        assert_eq!(lr("a2600").as_deref(), Some("Atari - 2600"));
        assert_eq!(lr("a800").as_deref(), Some("Atari - 8-bit"));

        assert!(db.get("psx").unwrap().multi_file);
        assert!(db.get("mame").unwrap().arcade);
        assert!(db.get("fbneo").unwrap().arcade);
        assert_eq!(lr("fbneo").as_deref(), Some("FBNeo - Arcade Games"));
        assert!(db.get("nope").is_none());
    }

    #[test]
    fn override_adds_alias_to_existing_system() {
        let db = SystemDb::builtin()
            .with_overrides(
                r#"
                [[system]]
                id = "snes"
                aliases = ["SUPERFAMICOM"]
                "#,
            )
            .unwrap();
        assert_eq!(
            db.resolve_folder("superfamicom").map(|s| s.id.as_str()),
            Some("snes")
        );
        // Built-in aliases and fields are kept.
        assert_eq!(
            db.resolve_folder("SFC").map(|s| s.id.as_str()),
            Some("snes")
        );
        assert_eq!(db.get("snes").unwrap().name, "Super Nintendo");
    }

    #[test]
    fn override_replaces_scalar_fields() {
        let db = SystemDb::builtin()
            .with_overrides(
                r#"
                [[system]]
                id = "snes"
                name = "Super Famicom"
                extensions = [".BS"]
                "#,
            )
            .unwrap();
        let snes = db.get("snes").unwrap();
        assert_eq!(snes.name, "Super Famicom");
        assert!(snes.extensions.iter().any(|e| e == "bs"));
        assert!(snes.extensions.iter().any(|e| e == "sfc"));
    }

    #[test]
    fn override_appends_new_system() {
        let before = SystemDb::builtin().all().len();
        let db = SystemDb::builtin()
            .with_overrides(
                r#"
                [[system]]
                id = "zz-tic"
                name = "TIC-80"
                aliases = ["ZZTIC", "zztic"]
                extensions = ["tic"]
                libretro = "TIC-80"
                "#,
            )
            .unwrap();
        assert_eq!(db.all().len(), before + 1);
        let tic = db.resolve_folder("ZZTIC").unwrap();
        assert_eq!(tic.id, "zz-tic");
        assert_eq!(tic.libretro.as_deref(), Some("TIC-80"));
        assert!(!tic.arcade && !tic.multi_file);
        assert_eq!(db.all().last().unwrap().id, "zz-tic");
    }

    #[test]
    fn override_alias_moves_from_other_system() {
        let db = SystemDb::builtin()
            .with_overrides(
                r#"
                [[system]]
                id = "fbneo"
                aliases = ["ARCADE"]
                "#,
            )
            .unwrap();
        assert_eq!(
            db.resolve_folder("arcade").map(|s| s.id.as_str()),
            Some("fbneo")
        );
    }

    #[test]
    fn override_new_system_without_name_is_an_error() {
        let err = SystemDb::builtin()
            .with_overrides("[[system]]\nid = \"x\"\naliases = [\"X\"]\n")
            .unwrap_err();
        assert!(err.to_string().contains("x"), "{err}");
    }

    /// Every ROM extension seen per folder on the reference SD card
    /// (read-only scan of /Volumes/System/ROMS; saves, BIOS and scripts left out).
    const SD_EXTENSIONS: &[(&str, &str)] = &[
        ("A2600", "a26"),
        ("ATOMISWAVE", "zip"),
        ("CPS1", "zip"),
        ("CPS2", "zip"),
        ("CPS3", "zip"),
        ("DREAMCAST", "cdi"),
        ("DREAMCAST", "chd"),
        ("FBNEO", "zip"),
        ("FC", "nes"),
        ("GB", "gb"),
        ("GBA", "gba"),
        ("GBC", "gbc"),
        ("GG", "gg"),
        ("GW", "mgw"),
        ("HBMAME", "zip"),
        ("MAME", "zip"),
        ("MD", "bin"),
        ("MD", "md"),
        ("MDCD", "chd"),
        ("MSX", "rom"),
        ("N64", "n64"),
        ("N64", "v64"),
        ("N64", "z64"),
        ("NAOMI", "zip"),
        ("NDS", "nds"),
        ("NDS", "zip"),
        ("OPENBOR", "pak"),
        ("PCE", "pce"),
        ("PCE", "chd"),
        ("PCECD", "chd"),
        ("PICO", "p8"),
        ("PS", "bin"),
        ("PS", "img"),
        ("PS", "iso"),
        ("PS", "pbp"),
        ("PSP", "iso"),
        ("SATURN", "chd"),
        ("SEGA32X", "zip"),
        ("SFC", "sfc"),
        ("SFC", "smc"),
        ("SMS", "sms"),
        ("VARCADE", "zip"),
        ("VB", "vb"),
        ("WS", "wsc"),
    ];

    #[test]
    fn reference_sd_extensions_are_accepted() {
        let db = SystemDb::builtin();
        for (folder, ext) in SD_EXTENSIONS {
            let system = db.resolve_folder(folder).expect(folder);
            assert!(
                system.extensions.iter().any(|e| e == ext),
                "{folder} ({}) does not accept .{ext}",
                system.id
            );
        }
    }

    #[test]
    fn wonderswan_folder_accepts_mono_and_color_roms() {
        let ws = SystemDb::builtin().resolve_folder("WS").unwrap().clone();
        for ext in ["ws", "wsc"] {
            assert!(ws.extensions.iter().any(|e| e == ext), "{ext}");
        }
    }

    #[test]
    fn override_aliases_are_trimmed() {
        let db = SystemDb::builtin()
            .with_overrides(
                r#"
                [[system]]
                id = "snes"
                aliases = [" SuperFamicom ", "   "]
                "#,
            )
            .unwrap();
        assert_eq!(
            db.resolve_folder("superfamicom").map(|s| s.id.as_str()),
            Some("snes")
        );
        let snes = db.get("snes").unwrap();
        assert!(snes.aliases.iter().all(|a| a == a.trim() && !a.is_empty()));
    }

    #[test]
    fn override_ids_are_case_insensitive() {
        let before = SystemDb::builtin().all().len();
        let db = SystemDb::builtin()
            .with_overrides(
                r#"
                [[system]]
                id = " SNES "
                aliases = ["superfamicom"]
                "#,
            )
            .expect("an existing id in another case is an override");
        assert_eq!(db.all().len(), before);
        assert_eq!(
            db.resolve_folder("superfamicom").map(|s| s.id.as_str()),
            Some("snes")
        );
        assert_eq!(db.get("SNES").map(|s| s.id.as_str()), Some("snes"));

        let db = SystemDb::builtin()
            .with_overrides("[[system]]\nid = \"ZZ80\"\nname = \"ZZ-80\"\n")
            .unwrap();
        assert_eq!(db.all().last().unwrap().id, "zz80");
    }

    #[test]
    fn builtin_header_rules() {
        use crate::identify::hash::HeaderRule;
        let db = SystemDb::builtin();
        for (id, rule) in [
            ("nes", HeaderRule::Nes),
            ("fds", HeaderRule::Fds),
            ("snes", HeaderRule::SnesCopier),
            ("lynx", HeaderRule::Lynx),
            ("a7800", HeaderRule::A7800),
            ("mastersystem", HeaderRule::SmsCopier),
            ("gamegear", HeaderRule::SmsCopier),
            ("n64", HeaderRule::N64),
            ("gba", HeaderRule::Plain),
        ] {
            assert_eq!(db.get(id).unwrap().header, rule, "{id}");
        }
    }

    #[test]
    fn unknown_header_rule_is_an_error() {
        let err = SystemDb::builtin()
            .with_overrides("[[system]]\nid = \"gba\"\nheader = \"bogus\"\n")
            .unwrap_err();
        assert!(matches!(err, SystemsError::BadHeader { .. }), "{err}");
    }

    #[test]
    fn invalid_override_toml_is_an_error() {
        assert!(SystemDb::builtin().with_overrides("[[system]\n").is_err());
    }

    #[test]
    fn extension_classes_follow_the_database() {
        let db = SystemDb::builtin();
        assert_eq!(db.extension_class("GBA"), ExtClass::Unique);
        assert_eq!(db.extension_class("sfc"), ExtClass::Unique);
        assert_eq!(
            db.extension_class("gb"),
            ExtClass::Shared,
            "gb and gbc list it"
        );
        assert_eq!(db.extension_class("wsc"), ExtClass::Shared);
        assert_eq!(db.extension_class("cue"), ExtClass::Shared);
        assert_eq!(db.extension_class("zip"), ExtClass::Container);
        assert_eq!(db.extension_class("7z"), ExtClass::Container);
        for generic in ["bin", "iso", "img", "rom", "dat", "chd"] {
            assert_eq!(db.extension_class(generic), ExtClass::Generic, "{generic}");
        }
        assert_eq!(db.extension_class("docx"), ExtClass::Unknown);
        assert_eq!(db.extension_class(""), ExtClass::Unknown);
    }

    #[test]
    fn systems_for_an_extension() {
        let db = SystemDb::builtin();
        let ids = |ext: &str| -> Vec<String> {
            db.systems_for_extension(ext)
                .iter()
                .map(|s| s.id.clone())
                .collect()
        };
        assert_eq!(ids("gba"), ["gba"]);
        let mut gb = ids("gb");
        gb.sort();
        assert_eq!(gb, ["gb", "gbc"]);
        assert!(ids("zip").is_empty(), "containers name no system");
    }

    #[test]
    fn the_system_of_a_dat() {
        let db = SystemDb::builtin();
        let gba = db.get("gba").unwrap();
        let dat = crate::identify::dat::dat_sources(gba)
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(db.system_for_dat(&dat).map(|s| s.id.as_str()), Some("gba"));
        assert!(db.system_for_dat("metadat/no-intro/Nope.dat").is_none());
    }

    #[test]
    fn everyday_file_types_are_not_rom_like() {
        // `pico8` lists `png` and `dos` lists `exe`/`bat`/`conf` so that
        // existing libraries scan, but a loose screenshot or installer is
        // never evidence for those systems nor an organise candidate.
        let db = SystemDb::builtin();
        for ext in ["png", "PNG", "exe", "bat", "conf"] {
            assert_eq!(db.extension_class(ext), ExtClass::Unknown, "{ext}");
            assert!(!db.is_rom_like_extension(ext), "{ext}");
        }
        assert_eq!(db.extension_class("p8"), ExtClass::Unique);
        assert_eq!(db.extension_class("com"), ExtClass::Shared, "a800 and dos");
    }

    #[test]
    fn a_shared_arcade_dat_maps_to_its_own_system() {
        let db = SystemDb::builtin();
        let id = |dat: &str| db.system_for_dat(dat).map(|s| s.id.clone());
        assert_eq!(id("metadat/mame/MAME.dat").as_deref(), Some("mame"));
        assert_eq!(
            id("metadat/fbneo-split/FBNeo - Arcade Games.dat").as_deref(),
            Some("fbneo")
        );
    }

    #[test]
    fn extensions_with_a_leading_dot_are_accepted() {
        let db = SystemDb::builtin();
        assert_eq!(db.extension_class(".GBA"), ExtClass::Unique);
        assert_eq!(db.extension_class(".zip"), ExtClass::Container);
        assert!(db.is_rom_like_extension(".cue"));
    }

    #[test]
    fn rom_like_extensions() {
        let db = SystemDb::builtin();
        for ext in [
            "gba", "zip", "7z", "bin", "cue", "m3u", "gdi", "chd", "NDS", "pkg",
        ] {
            assert!(db.is_rom_like_extension(ext), "{ext}");
        }
        // A weak-only `.pkg` is a candidate, but no evidence by itself.
        assert_eq!(db.extension_class("pkg"), ExtClass::Unknown);
        for ext in ["txt", "jpg", "docx", "dmg", ""] {
            assert!(!db.is_rom_like_extension(ext), "{ext}");
        }
    }

    #[test]
    fn every_builtin_system_has_a_family() {
        let db = SystemDb::builtin();
        let get = |id: &str| db.get(id).unwrap().family;
        assert_eq!(get("snes"), Family::Console);
        assert_eq!(get("gba"), Family::Handheld);
        assert_eq!(get("c64"), Family::Computer);
        assert_eq!(get("mame"), Family::Arcade);
        assert_eq!(get("scummvm"), Family::Engine);
        db.check_builtin_invariants().unwrap();
    }

    #[test]
    fn weak_extensions_are_accepted_but_not_classed() {
        let db = SystemDb::builtin()
            .with_overrides(
                r#"
                [[system]]
                id = "zz"
                name = "ZZ"
                aliases = ["ZZ"]
                extensions = ["zzq"]
                weak_extensions = ["st"]
                "#,
            )
            .unwrap();
        let zz = db.get("zz").unwrap();
        assert!(zz.accepts_extension("ST") && zz.accepts_extension(".zzq"));
        assert_eq!(db.extension_class("st"), ExtClass::Unique); // still atarist only
        let ids: Vec<_> = db
            .systems_accepting("st")
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(ids, ["atarist", "sufami", "zz"]);
        assert_eq!(db.systems_for_extension("st").len(), 1);
    }

    #[test]
    fn user_files_are_lenient_about_the_new_fields() {
        // A strong extension in a user file removes the same weak one; an
        // unknown family falls back to the default; neither is an error.
        let db = SystemDb::builtin()
            .with_overrides(
                r#"
                [[system]]
                id = "zz"
                name = "ZZ"
                family = "spaceship"
                weak_extensions = ["zzq"]
                [[system]]
                id = "zz"
                extensions = ["zzq"]
                "#,
            )
            .unwrap();
        let zz = db.get("zz").unwrap();
        assert_eq!(zz.family, Family::Console);
        assert!(zz.weak_extensions.is_empty() && zz.extensions == ["zzq"]);
        let arcade = SystemDb::builtin()
            .with_overrides("[[system]]\nid = \"yy\"\nname = \"YY\"\narcade = true\n")
            .unwrap();
        assert_eq!(arcade.get("yy").unwrap().family, Family::Arcade);
    }

    #[test]
    fn libretro_also_is_parsed_and_lenient() {
        // A user file appends extra thumbnail folders; repeating one does
        // not duplicate it, and the primary is untouched.
        let db = SystemDb::builtin()
            .with_overrides(
                r#"
                [[system]]
                id = "snes"
                libretro_also = ["X - Y", "X - Y"]
                [[system]]
                id = "snes"
                libretro_also = ["X - Y", "X - Z"]
                "#,
            )
            .unwrap();
        let snes = db.get("snes").unwrap();
        assert_eq!(snes.libretro_also, ["X - Y", "X - Z"]);
        assert_eq!(
            snes.libretro.as_deref(),
            Some("Nintendo - Super Nintendo Entertainment System")
        );
        assert!(SystemDb::builtin().all().iter().all(|s| s
            .libretro_also
            .iter()
            .all(|a| Some(a) != s.libretro.as_ref())));

        // `libretro_also` without `libretro` is a built-in invariant
        // violation only: a user file may still say it.
        let lone = SystemDb::builtin()
            .with_overrides("[[system]]\nid = \"zz\"\nname = \"ZZ\"\nlibretro_also = [\"X - Y\"]\n")
            .unwrap();
        assert_eq!(lone.get("zz").unwrap().libretro_also, ["X - Y"]);
        let errors = lone.check_builtin_invariants().unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("zz") && e.contains("libretro_also")),
            "{errors:?}"
        );
    }

    #[test]
    fn scoped_aliases_do_not_resolve_without_a_profile() {
        let db = SystemDb::builtin()
            .with_overrides(
                r#"
                [[system]]
                id = "zz"
                name = "ZZ"
                aliases = ["ZZBOX", "Dragon"]
                scoped_aliases = ["dragon"]
                "#,
            )
            .unwrap();
        assert_eq!(
            db.resolve_folder("zzbox").map(|s| s.id.as_str()),
            Some("zz")
        );
        assert!(db.resolve_folder("DRAGON").is_none());
    }

    #[test]
    fn scoped_aliases_match_any_case_not_only_ascii() {
        // Aliases resolve with Unicode lowercasing, so the scoped check
        // must too, or a non-ASCII scoped alias leaks into global lookup.
        let db = SystemDb::builtin()
            .with_overrides(
                r#"
                [[system]]
                id = "zz"
                name = "ZZ"
                aliases = ["ZZBOX", "Ödyssey"]
                scoped_aliases = ["ÖDYSSEY"]
                "#,
            )
            .unwrap();
        assert!(db.get("zz").unwrap().is_scoped_alias("ödyssey"));
        assert!(db.resolve_folder("ödyssey").is_none());
        assert!(db.resolve_folder("Ödyssey").is_none());
    }

    #[test]
    fn builtin_file_names_a_known_family_for_every_entry() {
        // The merge ignores an unknown family (user files stay lenient),
        // so the built-in file is checked on its raw text.
        let file: RawFile = toml::from_str(BUILTIN_TOML).unwrap();
        for raw in &file.system {
            let family = raw.family.as_deref();
            assert!(
                family.and_then(Family::parse).is_some(),
                "{}: family {family:?} is missing or unknown",
                raw.id
            );
            if raw.arcade == Some(true) {
                assert_eq!(family, Some("arcade"), "{}", raw.id);
            }
        }
    }
}
