//! Addons (mods spec D1): a file or a folder that belongs to one game
//! without being playable — a translation patch or hack, a cheat or an
//! emulator patch, a mod or a texture pack.
//!
//! Detection is pure and bounded: patches are told by their magic and a
//! few header bytes ([`patch`]), never by reading the whole file, and no
//! key or licence file is ever opened (callers check
//! [`crate::consoles::is_key_material`] first). Each addon carries the key
//! its format holds ([`AddonKey`]); the link to a game is made from that
//! key elsewhere.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::game_folder::FixedFolder;
use crate::write::safe_file_name;

pub mod cheat;
pub mod mods;
pub mod patch;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;

/// What an addon is to its game (D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AddonKind {
    /// A translation patch or a hack (`.bps`, `.ips`, `.xdelta` …).
    Patch,
    /// A cheat file or an emulator patch.
    Cheat,
    /// A mod folder or a texture pack.
    Mod,
}

impl AddonKind {
    /// The fixed folder of a game folder it goes in: patches and mods in
    /// `Mods/` (the owner's choice), cheats and emulator patches in
    /// `Cheats/`.
    pub fn fixed_folder(self) -> FixedFolder {
        match self {
            Self::Patch | Self::Mod => FixedFolder::Mods,
            Self::Cheat => FixedFolder::Cheats,
        }
    }
}

/// The format of an addon, one variant per format of D2–D4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AddonFormat {
    // Patches (D2).
    Bps,
    Ups,
    Ips,
    Xdelta,
    Ppf,
    Aps,
    // Cheats and emulator patches (D3).
    RetroArchCht,
    FceuxCht,
    DuckStationCht,
    Pnach,
    DolphinIni,
    PpssppIni,
    CtrCheats,
    SwitchCheats,
    Pchtxt,
    Rpcs3Patch,
    Shadps4Cheat,
    Shadps4Patch,
    XeniaPatch,
    Vita3kPatch,
    MelonDsCheats,
    MgbaCheats,
    DesmumeCheats,
    CodeText,
    // Mods (D4).
    TexturePack,
    DolphinGraphicMod,
    Riivolution,
    LayeredFs,
    SwitchMod,
    ExefsPatches,
    CemuGraphicPack,
    VitaRepatch,
    MesenHdPack,
    Msu1,
    XeniaPlugin,
    N64HiresTextures,
    Shadps4Mods,
}

impl AddonFormat {
    /// The kind every addon of this format is.
    pub fn kind(self) -> AddonKind {
        use AddonFormat as F;
        match self {
            F::Bps | F::Ups | F::Ips | F::Xdelta | F::Ppf | F::Aps => AddonKind::Patch,
            F::RetroArchCht
            | F::FceuxCht
            | F::DuckStationCht
            | F::Pnach
            | F::DolphinIni
            | F::PpssppIni
            | F::CtrCheats
            | F::SwitchCheats
            | F::Pchtxt
            | F::Rpcs3Patch
            | F::Shadps4Cheat
            | F::Shadps4Patch
            | F::XeniaPatch
            | F::Vita3kPatch
            | F::MelonDsCheats
            | F::MgbaCheats
            | F::DesmumeCheats
            | F::CodeText => AddonKind::Cheat,
            F::TexturePack
            | F::DolphinGraphicMod
            | F::Riivolution
            | F::LayeredFs
            | F::SwitchMod
            | F::ExefsPatches
            | F::CemuGraphicPack
            | F::VitaRepatch
            | F::MesenHdPack
            | F::Msu1
            | F::XeniaPlugin
            | F::N64HiresTextures
            | F::Shadps4Mods => AddonKind::Mod,
        }
    }
}

/// The key an addon's format carries, which links it to one game (D5).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AddonKey {
    /// BPS/UPS: CRC32 and size of the ROM the patch applies to.
    SourceCrc { crc32: u32, size: u64 },
    /// xdelta app header: the file name of the source ROM.
    SourceName { name: String },
    /// A cheat or patch named like its ROM (stem).
    RomName { name: String },
    /// A disc or product serial (`SLUS-00594`).
    Serial { serial: String },
    /// A GameCube/Wii id (6 or 3 characters) or a PSP disc id.
    GameId { id: String },
    /// A 16-hex title id (3DS, Switch) or a console title id.
    TitleId { id: String },
    /// Every title id a mod lists (Cemu `rules.txt`).
    TitleIds { ids: Vec<String> },
    /// shadPS4 `<SERIAL>_<VERSION>.json`.
    SerialVersion { serial: String, version: String },
    /// A Switch build id, with the title id of its parent folder if any.
    BuildId {
        build_id: String,
        title_id: Option<String>,
    },
    /// N64 cart id (APS header, `hires.txt`).
    N64Id { id: String },
    /// N64 internal name (Rice texture pack folders).
    N64Name { name: String },
    /// A file holding patches or cheats for several games: never linked.
    Many { count: usize },
    /// The format carries no key.
    None,
}

/// How an addon was linked to its game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkVia {
    SourceCrc,
    Dat,
    Name,
    Readme,
    Key,
    Folder,
}

/// Whether a patch fits the ROM as it is on disk (iNES/SNES copier
/// headers).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeaderFit {
    Matches,
    NeedsHeaderless,
    NeedsHeadered,
}

/// Why an addon stays where it is (D6).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UnlinkedWhy {
    NoMatch,
    Several { count: usize },
    Many { count: usize },
    NoKey,
    Budget,
    Partial,
}

/// An addon linked to its game (D10).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonLink {
    pub kind: AddonKind,
    pub format: AddonFormat,
    /// «SLUS-00594», «0004000000055D00», «CRC32 1A2B3C4D»: data, rendered
    /// in mono.
    pub key_text: String,
    pub via: LinkVia,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<HeaderFit>,
}

// Key validators (D14): a key read from a file name, a listing or a
// manifest becomes a path component only through these.

fn all_hex(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A 16-hex title id (3DS, Switch), upper-cased.
pub fn valid_title_id16(s: &str) -> Option<String> {
    (s.len() == 16 && all_hex(s)).then(|| s.to_ascii_uppercase())
}

/// A Sony-style serial (four letters, five digits) in the canonical
/// `AAAA-NNNNN` form, whatever the separator: `SLUS-00594`,
/// `SLUS_005.94`, `slus00594`, `BLUS30001`, `CUSA07023`, `NPUA80001`,
/// `PCSE00001`.
pub fn valid_sony_serial(s: &str) -> Option<String> {
    if s.is_empty()
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return None;
    }
    let compact: String = s
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let b = compact.as_bytes();
    (b.len() == 9
        && b[..4].iter().all(u8::is_ascii_alphabetic)
        && b[4..].iter().all(u8::is_ascii_digit))
    .then(|| format!("{}-{}", &compact[..4], &compact[4..]))
}

/// A game id, upper-cased: a GameCube/Wii id of 6 or 3 characters
/// (`GALE01`, `RMC`), or a dash-free PSP disc id (`ULUS10041`, PPSSPP).
pub fn valid_game_id(s: &str) -> Option<String> {
    if !s.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let up = s.to_ascii_uppercase();
    let b = up.as_bytes();
    let psp = b.len() == 9
        && b[..4].iter().all(u8::is_ascii_alphabetic)
        && b[4..].iter().all(u8::is_ascii_digit);
    (matches!(b.len(), 3 | 6) || psp).then_some(up)
}

/// A Switch build id: 16 to 64 hex digits, even length, upper-cased.
pub fn valid_build_id(s: &str) -> Option<String> {
    ((16..=64).contains(&s.len()) && s.len().is_multiple_of(2) && all_hex(s))
        .then(|| s.to_ascii_uppercase())
}

/// `name` when it is already a safe file name ([`safe_file_name`] leaves
/// it as is) and exactly one normal path component: never `..`, `.`, an
/// absolute path, a drive or a name with a separator.
pub fn one_component(name: &str) -> Option<&str> {
    if safe_file_name(name).ok()? != name {
        return None;
    }
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(n)), None) if n == name => Some(name),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_go_to_their_fixed_folder() {
        assert_eq!(AddonKind::Patch.fixed_folder(), FixedFolder::Mods);
        assert_eq!(AddonKind::Mod.fixed_folder(), FixedFolder::Mods);
        assert_eq!(AddonKind::Cheat.fixed_folder(), FixedFolder::Cheats);
        assert_eq!(AddonFormat::Bps.kind(), AddonKind::Patch);
        assert_eq!(AddonFormat::Pnach.kind(), AddonKind::Cheat);
        assert_eq!(AddonFormat::TexturePack.kind(), AddonKind::Mod);
    }

    #[test]
    fn model_serialises_kebab_case() {
        let j = |v: &dyn erased::Ser| v.json();
        assert_eq!(j(&AddonFormat::RetroArchCht), "\"retro-arch-cht\"");
        assert_eq!(j(&AddonKind::Cheat), "\"cheat\"");
        assert_eq!(
            j(&AddonKey::SourceCrc { crc32: 1, size: 2 }),
            r#"{"kind":"source-crc","crc32":1,"size":2}"#
        );
        assert_eq!(j(&AddonKey::None), r#"{"kind":"none"}"#);
        assert_eq!(
            j(&UnlinkedWhy::Several { count: 2 }),
            r#"{"kind":"several","count":2}"#
        );
        let link = AddonLink {
            kind: AddonKind::Patch,
            format: AddonFormat::Bps,
            key_text: "CRC32 1A2B3C4D".into(),
            via: LinkVia::SourceCrc,
            header: None,
        };
        assert_eq!(
            j(&link),
            r#"{"kind":"patch","format":"bps","key_text":"CRC32 1A2B3C4D","via":"source-crc"}"#
        );
    }

    /// Serialises any of the model's types in the test above.
    mod erased {
        pub trait Ser {
            fn json(&self) -> String;
        }
        impl<T: serde::Serialize> Ser for T {
            fn json(&self) -> String {
                serde_json::to_string(self).unwrap()
            }
        }
    }

    #[test]
    fn title_and_build_ids() {
        assert_eq!(
            valid_title_id16("0004000000055d00").as_deref(),
            Some("0004000000055D00")
        );
        for bad in [
            "0004000000055D0",
            "0004000000055D000",
            "000400000005GD00",
            "",
        ] {
            assert_eq!(valid_title_id16(bad), None, "{bad}");
        }
        assert_eq!(
            valid_build_id("3ca12dfaaf9c82da").as_deref(),
            Some("3CA12DFAAF9C82DA")
        );
        assert!(valid_build_id(&"a".repeat(64)).is_some());
        for bad in [
            "3CA12DFAAF9C82D",
            "3CA12DFAAF9C82DA0",
            &"a".repeat(66),
            "../../x",
        ] {
            assert_eq!(valid_build_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn sony_serials_normalise() {
        for (raw, want) in [
            ("SLUS-00594", "SLUS-00594"),
            ("SLUS_005.94", "SLUS-00594"),
            ("slus00594", "SLUS-00594"),
            ("BLUS30001", "BLUS-30001"),
            ("CUSA07023", "CUSA-07023"),
            ("NPUA80001", "NPUA-80001"),
            ("PCSE00001", "PCSE-00001"),
        ] {
            assert_eq!(valid_sony_serial(raw).as_deref(), Some(want), "{raw}");
        }
        for bad in ["SLUS-0059", "SLU-000594", "SLUS/00594", "..", "", "T-5704H"] {
            assert_eq!(valid_sony_serial(bad), None, "{bad}");
        }
    }

    #[test]
    fn game_ids() {
        assert_eq!(valid_game_id("GALE01").as_deref(), Some("GALE01"));
        assert_eq!(valid_game_id("rmc").as_deref(), Some("RMC"));
        assert_eq!(valid_game_id("ULUS10041").as_deref(), Some("ULUS10041"));
        for bad in ["GALE0", "GALE011", "GA/E01", "..", "", "ULUSX0041"] {
            assert_eq!(valid_game_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn crafted_names_are_never_one_component() {
        assert_eq!(one_component("Game (USA).iso"), Some("Game (USA).iso"));
        assert_eq!(one_component("0004000000055D00"), Some("0004000000055D00"));
        let long = "a".repeat(300);
        for bad in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "/abs",
            "C:",
            "C:\\x",
            "a\0b",
            "name.",
            "name ",
            "CON",
            long.as_str(),
        ] {
            assert_eq!(one_component(bad), None, "{bad:?}");
        }
    }
}
