//! Per-emulator translators (mods spec D14, D15): the master library
//! layout of a game in, the layout one emulator or frontend expects out.
//!
//! The module is pure: no file system, no clock, no settings. Organizar
//! uses it when an emulator is chosen for a system, and sync will later
//! call the same functions. A translator never extracts, decrypts or
//! installs anything: one that would need it says so with a
//! [`TranslateNote`].
//!
//! This file holds the types the plan carries; the translators fill it in.

use std::fmt;
use std::path::PathBuf;

use serde::de::{self, Deserializer, IntoDeserializer};
use serde::{Deserialize, Serialize, Serializer};

use crate::addons::HeaderFit;

/// An emulator a translator knows (D14). The wire spelling is its product
/// name in lower case (`shadps4`, `retroarch`, `duckstation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmulatorId {
    Shadps4,
    Rpcs3,
    Azahar,
    Cemu,
    Vita3k,
    Ryujinx,
    YuzuFamily,
    #[serde(rename = "retroarch")]
    RetroArch,
    #[serde(rename = "duckstation")]
    DuckStation,
    Pcsx2,
    Dolphin,
    Ppsspp,
    Xenia,
    Flycast,
    Mesen,
}

impl EmulatorId {
    /// Every emulator, in declaration order.
    pub const ALL: [EmulatorId; 15] = [
        EmulatorId::Shadps4,
        EmulatorId::Rpcs3,
        EmulatorId::Azahar,
        EmulatorId::Cemu,
        EmulatorId::Vita3k,
        EmulatorId::Ryujinx,
        EmulatorId::YuzuFamily,
        EmulatorId::RetroArch,
        EmulatorId::DuckStation,
        EmulatorId::Pcsx2,
        EmulatorId::Dolphin,
        EmulatorId::Ppsspp,
        EmulatorId::Xenia,
        EmulatorId::Flycast,
        EmulatorId::Mesen,
    ];

    /// The wire and command-line spelling (`shadps4`, `yuzu-family`).
    pub fn id(self) -> &'static str {
        match self {
            EmulatorId::Shadps4 => "shadps4",
            EmulatorId::Rpcs3 => "rpcs3",
            EmulatorId::Azahar => "azahar",
            EmulatorId::Cemu => "cemu",
            EmulatorId::Vita3k => "vita3k",
            EmulatorId::Ryujinx => "ryujinx",
            EmulatorId::YuzuFamily => "yuzu-family",
            EmulatorId::RetroArch => "retroarch",
            EmulatorId::DuckStation => "duckstation",
            EmulatorId::Pcsx2 => "pcsx2",
            EmulatorId::Dolphin => "dolphin",
            EmulatorId::Ppsspp => "ppsspp",
            EmulatorId::Xenia => "xenia",
            EmulatorId::Flycast => "flycast",
            EmulatorId::Mesen => "mesen",
        }
    }

    /// The emulator of a wire spelling, case-insensitive.
    pub fn from_id(id: &str) -> Option<EmulatorId> {
        EmulatorId::ALL
            .into_iter()
            .find(|e| e.id().eq_ignore_ascii_case(id.trim()))
    }

    /// The product name, never translated (it fills `InstallFirst` and
    /// the other notes that name an emulator).
    pub fn display_name(self) -> &'static str {
        match self {
            EmulatorId::Shadps4 => "shadPS4",
            EmulatorId::Rpcs3 => "RPCS3",
            EmulatorId::Azahar => "Azahar",
            EmulatorId::Cemu => "Cemu",
            EmulatorId::Vita3k => "Vita3K",
            EmulatorId::Ryujinx => "Ryujinx",
            EmulatorId::YuzuFamily => "yuzu",
            EmulatorId::RetroArch => "RetroArch",
            EmulatorId::DuckStation => "DuckStation",
            EmulatorId::Pcsx2 => "PCSX2",
            EmulatorId::Dolphin => "Dolphin",
            EmulatorId::Ppsspp => "PPSSPP",
            EmulatorId::Xenia => "Xenia",
            EmulatorId::Flycast => "Flycast",
            EmulatorId::Mesen => "Mesen",
        }
    }
}

impl fmt::Display for EmulatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// The emulator a run chose for one system (D16): one emulator, or
/// `none` for the library layout. On the wire a plain string: an
/// emulator id or `"none"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EmulatorChoice {
    Emulator(EmulatorId),
    None,
}

impl Serialize for EmulatorChoice {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            EmulatorChoice::Emulator(e) => e.serialize(serializer),
            EmulatorChoice::None => serializer.serialize_str("none"),
        }
    }
}

impl<'de> Deserialize<'de> for EmulatorChoice {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == "none" {
            return Ok(EmulatorChoice::None);
        }
        let de: de::value::StrDeserializer<'_, D::Error> = text.as_str().into_deserializer();
        EmulatorId::deserialize(de).map(EmulatorChoice::Emulator)
    }
}

/// Why an emulator does not use an item the way the library keeps it
/// (D15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotSupportedWhy {
    /// RPCS3 has no mods folder: mods replace the game's files.
    Rpcs3Mods,
    /// Vita3K does not read rePatch: the files have to be merged in.
    VitaRepatch,
    /// Several patches for one ROM: RetroArch applies only one.
    SeveralPatches,
    /// The emulator only uses the newest update.
    OlderUpdate,
    /// A settings file with that name already exists: never merged.
    ConfigExists,
}

/// Something a translator says about one placement instead of doing it
/// (D14): data, never prose.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TranslateNote {
    /// The emulator installs it; it stays in the library.
    InstallInEmulator { emulator: EmulatorId },
    /// A package that has to be extracted first; Coverly never does.
    ExtractFirst,
    NotSupported {
        emulator: EmulatorId,
        why: NotSupportedWhy,
    },
    /// The patch does not fit the ROM as it is on disk.
    HeaderMismatch { needs: HeaderFit },
    /// The emulator has to be pointed at `folder`.
    PointEmulatorHere {
        emulator: EmulatorId,
        folder: PathBuf,
    },
    /// The emulator would soft-patch it, but only a chosen emulator
    /// renames a patch for that.
    PatchNotApplied { emulator: EmulatorId },
}

/// Where a placement goes, as the plan shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PlacementTo {
    /// Relative to the device's system folder.
    System { path: PathBuf },
    /// Relative to the emulator's own data folder.
    EmulatorData { path: PathBuf },
    /// Stays in the library: nothing is written on the device.
    Library,
}

/// One placement of a translated game, for the preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementView {
    /// Relative to the master system folder.
    pub from: PathBuf,
    pub to: PlacementTo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<TranslateNote>,
}

/// A game of the plan laid out for one emulator (D16), for the preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslatedGameView {
    pub system: String,
    /// The base item's id in the plan, when the base is an item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<usize>,
    pub emulator: EmulatorId,
    pub placements: Vec<PlacementView>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, to_value};

    #[test]
    fn emulator_ids_round_trip_by_their_wire_spelling() {
        for e in EmulatorId::ALL {
            assert_eq!(to_value(e).unwrap(), json!(e.id()), "{e:?}");
            assert_eq!(
                serde_json::from_value::<EmulatorId>(json!(e.id())).unwrap(),
                e
            );
            assert_eq!(EmulatorId::from_id(&e.id().to_uppercase()), Some(e));
        }
        assert_eq!(EmulatorId::from_id("none"), None);
    }

    #[test]
    fn an_emulator_choice_is_a_plain_string() {
        let shad = EmulatorChoice::Emulator(EmulatorId::Shadps4);
        assert_eq!(to_value(shad).unwrap(), json!("shadps4"));
        assert_eq!(to_value(EmulatorChoice::None).unwrap(), json!("none"));
        assert_eq!(
            serde_json::from_value::<EmulatorChoice>(json!("shadps4")).unwrap(),
            shad
        );
        assert_eq!(
            serde_json::from_value::<EmulatorChoice>(json!("none")).unwrap(),
            EmulatorChoice::None
        );
        assert!(serde_json::from_value::<EmulatorChoice>(json!("snes9x")).is_err());
        assert!(serde_json::from_value::<EmulatorChoice>(json!(3)).is_err());
    }

    #[test]
    fn a_translated_game_has_a_pinned_shape() {
        let view = TranslatedGameView {
            system: "ps4".into(),
            base: Some(0),
            emulator: EmulatorId::Shadps4,
            placements: vec![
                PlacementView {
                    from: PathBuf::from("Bloodborne"),
                    to: PlacementTo::System {
                        path: PathBuf::from("CUSA00900"),
                    },
                    note: None,
                },
                PlacementView {
                    from: PathBuf::from("dlc.pkg"),
                    to: PlacementTo::Library,
                    note: Some(TranslateNote::ExtractFirst),
                },
                PlacementView {
                    from: PathBuf::from("CUSA00900_01.09.json"),
                    to: PlacementTo::EmulatorData {
                        path: PathBuf::from("cheats"),
                    },
                    note: Some(TranslateNote::NotSupported {
                        emulator: EmulatorId::Shadps4,
                        why: NotSupportedWhy::OlderUpdate,
                    }),
                },
            ],
        };
        let wire = json!({
            "system": "ps4", "base": 0, "emulator": "shadps4",
            "placements": [
                {"from": "Bloodborne", "to": {"kind": "system", "path": "CUSA00900"}},
                {"from": "dlc.pkg", "to": {"kind": "library"},
                 "note": {"kind": "extract-first"}},
                {"from": "CUSA00900_01.09.json",
                 "to": {"kind": "emulator-data", "path": "cheats"},
                 "note": {"kind": "not-supported", "emulator": "shadps4", "why": "older-update"}}
            ]
        });
        assert_eq!(to_value(&view).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<TranslatedGameView>(wire).unwrap(),
            view
        );
        for (note, wire) in [
            (
                TranslateNote::InstallInEmulator {
                    emulator: EmulatorId::Rpcs3,
                },
                json!({"kind": "install-in-emulator", "emulator": "rpcs3"}),
            ),
            (
                TranslateNote::HeaderMismatch {
                    needs: HeaderFit::NeedsHeadered,
                },
                json!({"kind": "header-mismatch", "needs": "needs-headered"}),
            ),
            (
                TranslateNote::PointEmulatorHere {
                    emulator: EmulatorId::Ryujinx,
                    folder: PathBuf::from("DLC"),
                },
                json!({"kind": "point-emulator-here", "emulator": "ryujinx", "folder": "DLC"}),
            ),
            (
                TranslateNote::PatchNotApplied {
                    emulator: EmulatorId::RetroArch,
                },
                json!({"kind": "patch-not-applied", "emulator": "retroarch"}),
            ),
        ] {
            assert_eq!(to_value(&note).unwrap(), wire);
            assert_eq!(serde_json::from_value::<TranslateNote>(wire).unwrap(), note);
        }
    }
}
