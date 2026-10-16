//! System database: platforms, their folder aliases and ROM extensions.
//!
//! The built-in database is `systems.toml`, embedded at compile time. Users
//! can extend or override it with a TOML file in the same format.

use serde::Deserialize;
use thiserror::Error;

use crate::identify::hash::HeaderRule;

/// The embedded built-in database.
const BUILTIN_TOML: &str = include_str!("systems.toml");

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
    /// Arcade platform (ROM set names instead of game titles).
    pub arcade: bool,
    /// Disc system: cue/bin, m3u, gdi, chd.
    pub multi_file: bool,
    /// Header rule for content hashing.
    pub header: HeaderRule,
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
    arcade: Option<bool>,
    multi_file: Option<bool>,
    header: Option<String>,
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
    /// `arcade`, `multi_file` and `header` replace the current values when given;
    /// `aliases` and `extensions` are added to the current lists. An alias
    /// claimed by an override is removed from any other system, so the user
    /// file always wins. An entry with a new `id` is appended and must have a
    /// `name`.
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

        // The user file wins: an alias belongs to one system only.
        for alias in &aliases {
            let key = alias.to_lowercase();
            for other in self.systems.iter_mut().filter(|s| s.id != id) {
                other.aliases.retain(|a| a.to_lowercase() != key);
            }
        }

        let header = match raw.header {
            Some(value) => Some(HeaderRule::parse(&value).ok_or(SystemsError::BadHeader {
                id: id.clone(),
                value,
            })?),
            None => None,
        };

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
                    arcade: false,
                    multi_file: false,
                    header: HeaderRule::Plain,
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
        Ok(())
    }

    /// Finds the system whose alias matches `folder_name`, ignoring case.
    pub fn resolve_folder(&self, folder_name: &str) -> Option<&System> {
        if folder_name.is_empty() {
            return None;
        }
        let key = folder_name.to_lowercase();
        self.systems
            .iter()
            .find(|s| s.aliases.iter().any(|a| a.to_lowercase() == key))
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
            assert!(!s.extensions.is_empty(), "{} has no extensions", s.id);
            for ext in &s.extensions {
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
                id = "tic80"
                name = "TIC-80"
                aliases = ["TIC", "tic80"]
                extensions = ["tic"]
                libretro = "TIC-80"
                "#,
            )
            .unwrap();
        assert_eq!(db.all().len(), before + 1);
        let tic = db.resolve_folder("TIC").unwrap();
        assert_eq!(tic.id, "tic80");
        assert_eq!(tic.libretro.as_deref(), Some("TIC-80"));
        assert!(!tic.arcade && !tic.multi_file);
        assert_eq!(db.all().last().unwrap().id, "tic80");
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
            .with_overrides("[[system]]\nid = \"TIC80\"\nname = \"TIC-80\"\n")
            .unwrap();
        assert_eq!(db.all().last().unwrap().id, "tic80");
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
}
