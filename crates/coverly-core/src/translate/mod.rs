//! Per-emulator translators (mods spec D14, D15): the master library
//! layout of a game in, the layout one emulator or frontend expects out.
//!
//! The module is pure: no file system, no clock, no settings. Organizar
//! uses it when an emulator is chosen for a system, and sync will later
//! call the same functions. A translator never extracts, decrypts or
//! installs anything: one that would need it says so with a
//! [`TranslateNote`].
//!
//! [`translate`] takes one [`MasterGame`] (paths relative to the master
//! system folder) and a [`TranslateTarget`] and returns where each entry
//! goes. Every key that becomes a path component goes through
//! [`safe_join`]: a key that is not one plain component is never
//! sanitised into a path.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::de::{self, Deserializer, IntoDeserializer};
use serde::{Deserialize, Serialize, Serializer};

use crate::addons::{
    one_component, valid_game_id, valid_sony_serial, valid_title_id16, AddonFormat, AddonKey,
    AddonKind, HeaderFit,
};
use crate::game_folder::FixedFolder;

mod azahar;
mod cemu;
mod dolphin;
mod duckstation;
mod flycast;
mod frontend;
mod mesen;
mod pcsx2;
mod ppsspp;
mod retroarch;
mod rpcs3;
mod shadps4;
mod switch;
mod vita3k;
mod xenia;

/// Systems a dedicated emulator runs that RetroArch is never the default
/// for (D16): the sixth generation on and every modern console.
pub const MODERN_SYSTEMS: &[&str] = &[
    "ps2",
    "gamecube",
    "wii",
    "3ds",
    "wiiu",
    "switch",
    "psvita",
    "ps3",
    "xbox",
    "xbox360",
    "ps4",
    "ps5",
    "xboxone",
    "xboxseries",
];

/// Every built-in system with a libretro name that is not in
/// [`MODERN_SYSTEMS`] (D16 «every other retro system»). A test keeps it
/// in step with `systems.toml`.
pub const RETROARCH_SYSTEMS: &[&str] = &[
    "a2600",
    "a5200",
    "a7800",
    "a800",
    "amiga",
    "atarist",
    "atomiswave",
    "c64",
    "cps1",
    "cps2",
    "cps3",
    "dos",
    "dreamcast",
    "easyrpg",
    "fbneo",
    "nes",
    "fds",
    "gb",
    "gba",
    "gbc",
    "gamegear",
    "gw",
    "hbmame",
    "lynx",
    "mame",
    "megadrive",
    "segacd",
    "msx",
    "n64",
    "naomi",
    "nds",
    "neocd",
    "neogeo",
    "ngp",
    "ngpc",
    "pcengine",
    "pcenginecd",
    "pgm2",
    "pokemini",
    "psx",
    "psp",
    "saturn",
    "scummvm",
    "sega32x",
    "snes",
    "mastersystem",
    "varcade",
    "virtualboy",
    "vic20",
    "wonderswan",
    "wonderswancolor",
    "3do",
    "pcfx",
    "cdi",
    "amigacd32",
    "cdtv",
    "colecovision",
    "intellivision",
    "vectrex",
    "odyssey2",
    "channelf",
    "arcadia",
    "scv",
    "jaguar",
    "sg1000",
    "pico",
    "supergrafx",
    "n64dd",
    "satellaview",
    "sufami",
    "supervision",
    "gamecom",
    "zxspectrum",
    "zx81",
    "amstradcpc",
    "thomson",
    "x68000",
    "x1",
    "pc98",
    "pc88",
    "plus4",
    "pet",
    "atarixe",
    "msx2",
    "tic80",
    "wasm4",
    "arduboy",
    "lowresnx",
    "vircon32",
    "doom",
    "quake",
    "cavestory",
    "wolf3d",
    "mrboom",
    "dinothawr",
    "lutro",
    "flashback",
    "xrick",
    "model2",
    "model3",
    "naomi2",
];

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

    /// The system ids this emulator's translator lays out (D16).
    pub fn systems(self) -> &'static [&'static str] {
        match self {
            EmulatorId::Shadps4 => &["ps4"],
            EmulatorId::Rpcs3 => &["ps3"],
            EmulatorId::Azahar => &["3ds"],
            EmulatorId::Cemu => &["wiiu"],
            EmulatorId::Vita3k => &["psvita"],
            EmulatorId::Ryujinx | EmulatorId::YuzuFamily => &["switch"],
            EmulatorId::RetroArch => RETROARCH_SYSTEMS,
            EmulatorId::DuckStation => &["psx"],
            EmulatorId::Pcsx2 => &["ps2"],
            EmulatorId::Dolphin => &["gamecube", "wii"],
            EmulatorId::Ppsspp => &["psp"],
            EmulatorId::Xenia => &["xbox360"],
            EmulatorId::Flycast => &["dreamcast"],
            EmulatorId::Mesen => &["nes"],
        }
    }

    /// Every emulator that runs `system`, in declaration order.
    pub fn for_system(system: &str) -> Vec<EmulatorId> {
        EmulatorId::ALL
            .into_iter()
            .filter(|e| e.systems().contains(&system))
            .collect()
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

/// What a master entry is on disk (D14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryForm {
    /// An installable package (`.pkg`, `.cia`, `.nsp`, `.wua` …).
    Package,
    /// A folder game (`CUSA00900/`, an extracted PS3 game).
    Extracted,
    /// A disc or cartridge image.
    Image,
    /// Any other single file.
    File,
}

/// The ids a game is known by; each one already validated.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameIds {
    pub title_id: Option<String>,
    pub serial: Option<String>,
    pub game_id: Option<String>,
}

/// A base, update or DLC of the master library.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterEntry {
    /// Relative to the master system folder.
    pub path: PathBuf,
    pub is_dir: bool,
    pub form: EntryForm,
    pub title_id: Option<String>,
    pub version: Option<String>,
    /// The newest update of its game (shadPS4 reads one overlay).
    pub newest: bool,
}

/// A mod, patch or cheat of the master library.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterAddon {
    /// Relative to the master system folder.
    pub path: PathBuf,
    pub is_dir: bool,
    pub kind: AddonKind,
    pub format: AddonFormat,
    pub key: AddonKey,
    pub header: Option<HeaderFit>,
}

/// One game of the master library with everything linked to it (D14).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterGame {
    pub system: String,
    /// The game folder's name in the master layout.
    pub title: String,
    pub ids: GameIds,
    pub base: MasterEntry,
    pub updates: Vec<MasterEntry>,
    pub dlc: Vec<MasterEntry>,
    pub mods: Vec<MasterAddon>,
    pub cheats: Vec<MasterAddon>,
    /// Existing names in the emulator data folder that must not be
    /// overwritten (Dolphin/PPSSPP ini, D15).
    pub taken_in_emulator: Vec<PathBuf>,
}

/// How a frontend shows a game folder (D15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrontendMode {
    /// Folders are folders.
    Plain,
    /// ES-DE «directories interpreted as files»: `Game.pkg/` holding
    /// `Game.pkg` is one game.
    DirAsGame,
}

/// How far a translator may go (D16, Q11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranslateScope {
    /// A profile default: never moves a base, update or DLC, never
    /// renames a patch, never applies [`FrontendMode::DirAsGame`].
    Conservative,
    /// Every rule of D15: an emulator chosen in the run, or sync.
    Full,
}

/// The layout a game is translated to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TranslateTarget {
    /// `None`: the master layout.
    pub emulator: Option<EmulatorId>,
    pub frontend: FrontendMode,
    pub game_folders: bool,
    pub scope: TranslateScope,
}

/// Where one entry goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Destination {
    /// Relative to the device's system folder.
    System(PathBuf),
    /// Relative to the emulator's own data folder.
    EmulatorData { emulator: EmulatorId, path: PathBuf },
    /// Stays in the library: nothing is written on the device.
    Library,
}

/// One entry of a translated game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    /// Relative to the master system folder.
    pub from: PathBuf,
    pub to: Destination,
    pub note: Option<TranslateNote>,
}

/// A game laid out for one target: one placement per master entry, base
/// first, then updates, DLC, mods and cheats.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslatedGame {
    pub entries: Vec<Placement>,
}

/// Lays `game` out for `to` (D14): the emulator's translator first, then
/// the frontend's `DirAsGame` when the run has game folders on and the
/// full scope. No emulator keeps the master layout (the caller keeps
/// today's flat placement when game folders are off).
pub fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    let laid = match to.emulator {
        Some(emulator) => emulator_layout(emulator, game, to),
        None => master_layout(game),
    };
    let laid = if to.frontend == FrontendMode::DirAsGame
        && to.game_folders
        && to.scope == TranslateScope::Full
    {
        frontend::dir_as_game(game, laid)
    } else {
        laid
    };
    never_shared(laid)
}

/// Two entries that would land on one place (the same path, ignoring
/// case, in the same folder tree) both stay in the library: nothing is
/// ever written over or merged, the user picks (spec «collisions are left
/// for review»).
fn never_shared(mut laid: TranslatedGame) -> TranslatedGame {
    fn key(to: &Destination) -> Option<(Option<EmulatorId>, Vec<String>)> {
        let (emulator, path) = match to {
            Destination::System(path) => (None, path),
            Destination::EmulatorData { emulator, path } => (Some(*emulator), path),
            Destination::Library => return None,
        };
        let parts = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect();
        Some((emulator, parts))
    }
    let keys: Vec<_> = laid.entries.iter().map(|p| key(&p.to)).collect();
    for (i, placement) in laid.entries.iter_mut().enumerate() {
        let shared = keys[i]
            .as_ref()
            .is_some_and(|k| keys.iter().filter(|o| o.as_ref() == Some(k)).count() > 1);
        if shared {
            placement.to = Destination::Library;
            placement.note = None;
        }
    }
    laid
}

/// One emulator's layout. An emulator that does not run the game's
/// system, or whose translator has no rules yet, keeps the master layout.
/// Every translator lays out the full scope; [`conservative`] then takes
/// back what a profile default may not do (D16).
fn emulator_layout(
    emulator: EmulatorId,
    game: &MasterGame,
    to: &TranslateTarget,
) -> TranslatedGame {
    if !emulator.systems().contains(&game.system.as_str()) {
        return master_layout(game);
    }
    let laid = match emulator {
        EmulatorId::Shadps4 => shadps4::translate(game, to),
        EmulatorId::Rpcs3 => rpcs3::translate(game, to),
        EmulatorId::Azahar => azahar::translate(game, to),
        EmulatorId::Cemu => cemu::translate(game, to),
        EmulatorId::Vita3k => vita3k::translate(game, to),
        EmulatorId::Ryujinx | EmulatorId::YuzuFamily => switch::translate(emulator, game, to),
        EmulatorId::RetroArch => retroarch::translate(game, to),
        EmulatorId::DuckStation => duckstation::translate(game, to),
        EmulatorId::Pcsx2 => pcsx2::translate(game, to),
        EmulatorId::Dolphin => dolphin::translate(game, to),
        EmulatorId::Ppsspp => ppsspp::translate(game, to),
        EmulatorId::Xenia => xenia::translate(game, to),
        EmulatorId::Flycast => flycast::translate(game, to),
        EmulatorId::Mesen => mesen::translate(game, to),
    };
    match to.scope {
        TranslateScope::Full => laid,
        TranslateScope::Conservative => conservative(emulator, game, laid),
    }
}

/// What a profile default may do (D16, Q11): a base, update or DLC goes
/// exactly where the master layout puts it, keeping only the notes that
/// say what the user has to do (`InstallInEmulator`, `ExtractFirst`); an
/// addon is placed only in the emulator's data folder, never beside the
/// game (that would change how the game boots). A patch the emulator
/// would soft-patch once renamed beside the game stays with
/// `PatchNotApplied` (Q11).
fn conservative(emulator: EmulatorId, game: &MasterGame, laid: TranslatedGame) -> TranslatedGame {
    let master = master_layout(game);
    let is_game = |from: &Path| {
        from == game.base.path || game.updates.iter().chain(&game.dlc).any(|e| e.path == from)
    };
    let is_patch = |from: &Path| {
        game.mods
            .iter()
            .any(|a| a.path == from && a.kind == AddonKind::Patch)
    };
    let entries = laid
        .entries
        .into_iter()
        .map(|p| {
            if is_game(&p.from) {
                let to = master
                    .entries
                    .iter()
                    .find(|m| m.from == p.from)
                    .map_or(Destination::Library, |m| m.to.clone());
                let note = p.note.filter(|n| {
                    matches!(
                        n,
                        TranslateNote::InstallInEmulator { .. } | TranslateNote::ExtractFirst
                    )
                });
                Placement {
                    from: p.from,
                    to,
                    note,
                }
            } else if matches!(p.to, Destination::System(_)) {
                let note = is_patch(&p.from).then_some(TranslateNote::PatchNotApplied { emulator });
                Placement {
                    from: p.from,
                    to: Destination::Library,
                    note,
                }
            } else {
                p
            }
        })
        .collect();
    TranslatedGame { entries }
}

/// A placement and its note, as a translator rule returns them.
type Rule = (Destination, Option<TranslateNote>);

/// Stays in the library with nothing to say (no place, or a key that is
/// not a safe component: the caller reports it).
const STAYS: Rule = (Destination::Library, None);

fn placed(from: &Path, (to, note): Rule) -> Placement {
    Placement {
        from: from.to_path_buf(),
        to,
        note,
    }
}

/// The folder parts a translator puts the base in: the game folder when
/// the run has game folders on, else the system folder itself.
fn base_parent<'a>(game: &'a MasterGame, to: &TranslateTarget) -> Vec<&'a str> {
    if to.game_folders {
        vec![game.title.as_str()]
    } else {
        Vec::new()
    }
}

/// `parent` joined with `name` under the device's system folder, or the
/// library when a part is not one safe component.
fn on_system(parent: &[&str], name: &str) -> Destination {
    let mut parts = parent.to_vec();
    parts.push(name);
    safe_join(&parts).map_or(Destination::Library, Destination::System)
}

/// `parts` under `emulator`'s data folder. A name the folder already has
/// is never written over (D15, Q5): it stays with `ConfigExists`.
fn in_emulator(game: &MasterGame, emulator: EmulatorId, parts: &[&str]) -> Rule {
    let Some(path) = safe_join(parts) else {
        return STAYS;
    };
    let lower = |p: &Path| -> Vec<String> {
        p.components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect()
    };
    let want = lower(&path);
    if game.taken_in_emulator.iter().any(|t| lower(t) == want) {
        return (
            Destination::Library,
            Some(TranslateNote::NotSupported {
                emulator,
                why: NotSupportedWhy::ConfigExists,
            }),
        );
    }
    (Destination::EmulatorData { emulator, path }, None)
}

/// A Sony title id in its compact form (`CUSA00900`, `BLUS30443`,
/// `PCSE00120`), whatever separator the text had.
fn sony_title_id(s: &str) -> Option<String> {
    valid_sony_serial(s).map(|serial| serial.replace('-', ""))
}

/// The game's Sony title id: its title id, else its serial.
fn game_sony_title_id(game: &MasterGame) -> Option<String> {
    game.ids
        .title_id
        .as_deref()
        .or(game.ids.serial.as_deref())
        .and_then(sony_title_id)
}

/// The Sony title id an addon is for: its own key when it carries one
/// (never replaced when that key is not valid), else the game's.
fn addon_sony_title_id(game: &MasterGame, addon: &MasterAddon) -> Option<String> {
    match &addon.key {
        AddonKey::TitleId { id } => sony_title_id(id),
        AddonKey::SerialVersion { serial, .. } => sony_title_id(serial),
        _ => game_sony_title_id(game),
    }
}

/// The 16-hex title id an addon is for: its own key when it carries a
/// title id (never replaced when that key is not valid), else the game's.
fn addon_title_id16(game: &MasterGame, addon: &MasterAddon) -> Option<String> {
    match &addon.key {
        AddonKey::TitleId { id } => valid_title_id16(id),
        _ => game.ids.title_id.as_deref().and_then(valid_title_id16),
    }
}

/// A disc serial in its canonical `AAAA-NNNNN` form: the addon's own
/// serial key when it carries one (never replaced when that key is not
/// valid), else the game's serial.
fn addon_serial(game: &MasterGame, addon: &MasterAddon) -> Option<String> {
    match &addon.key {
        AddonKey::Serial { serial } => valid_sony_serial(serial),
        _ => game.ids.serial.as_deref().and_then(valid_sony_serial),
    }
}

/// A game id (GameCube/Wii, PSP, Dreamcast product id): the addon's own
/// game id key when it carries one (never replaced when that key is not
/// valid), else the game's.
fn addon_game_id(game: &MasterGame, addon: &MasterAddon) -> Option<String> {
    match &addon.key {
        AddonKey::GameId { id } => valid_game_id(id),
        _ => game.ids.game_id.as_deref().and_then(valid_game_id),
    }
}

/// The base file's stem (`Chrono Trigger (USA)`), when the base is a file
/// with an extension: the name an emulator looks a companion up by.
fn base_stem(game: &MasterGame) -> Option<&str> {
    if game.base.is_dir {
        return None;
    }
    let name = file_name(&game.base.path)?;
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then_some(stem)
}

/// The lower-case extension of `path`'s file name.
fn ext_lower(path: &Path) -> Option<String> {
    let (_, ext) = file_name(path)?.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}

/// `name` as the emulator looks it up: its leading `id` as `id` is
/// written (upper case) and its ending `tail` in lower case
/// (`pcse00120_60fps.TXT` → `PCSE00120_60fps.txt`). `None` when `name`
/// does not start with `id` or end with `tail`, ignoring case.
fn id_named(name: &str, id: &str, tail: &str) -> Option<String> {
    let rest = name.len().checked_sub(tail.len())?.checked_sub(id.len())?;
    let head = name.get(..id.len())?;
    let middle = name.get(id.len()..id.len() + rest)?;
    (head.eq_ignore_ascii_case(id) && ends_with_ci(name, tail))
        .then(|| format!("{id}{middle}{tail}"))
}

/// `name` ends with `suffix`, ignoring ASCII case.
fn ends_with_ci(name: &str, suffix: &str) -> bool {
    name.len() >= suffix.len()
        && name
            .get(name.len() - suffix.len()..)
            .is_some_and(|t| t.eq_ignore_ascii_case(suffix))
}

/// The layout of an emulator that installs updates and DLC itself
/// (RPCS3, Azahar, Cemu, Vita3K, D15): the base where the profile puts
/// it, updates and DLC kept in the library with `InstallInEmulator`, and
/// each addon by `addon`.
fn installed_layout(
    game: &MasterGame,
    to: &TranslateTarget,
    emulator: EmulatorId,
    addon: impl Fn(&MasterAddon) -> Rule,
) -> TranslatedGame {
    let parent = base_parent(game, to);
    let base = file_name(&game.base.path).map_or(STAYS, |name| (on_system(&parent, name), None));
    let mut entries = vec![placed(&game.base.path, base)];
    let install = || {
        (
            Destination::Library,
            Some(TranslateNote::InstallInEmulator { emulator }),
        )
    };
    entries.extend(
        game.updates
            .iter()
            .chain(&game.dlc)
            .map(|e| placed(&e.path, install())),
    );
    entries.extend(
        game.mods
            .iter()
            .chain(&game.cheats)
            .map(|a| placed(&a.path, addon(a))),
    );
    TranslatedGame { entries }
}

/// The layout of an emulator that only adds files of its own (the
/// disc-era emulators and Mesen, D15): the base where the profile puts
/// it, updates and DLC where the master keeps them, and each addon by
/// `addon`.
fn addon_layout(
    game: &MasterGame,
    to: &TranslateTarget,
    addon: impl Fn(&MasterAddon) -> Rule,
) -> TranslatedGame {
    let parent = base_parent(game, to);
    let base = file_name(&game.base.path).map_or(STAYS, |name| (on_system(&parent, name), None));
    let master = master_layout(game);
    let mut entries = vec![placed(&game.base.path, base)];
    entries.extend(master.entries.into_iter().filter(|p| {
        game.updates
            .iter()
            .chain(&game.dlc)
            .any(|e| e.path == p.from)
    }));
    entries.extend(
        game.mods
            .iter()
            .chain(&game.cheats)
            .map(|a| placed(&a.path, addon(a))),
    );
    TranslatedGame { entries }
}

/// The master layout (identity for `coverly-library`): base →
/// `<title>/<base name>`, updates → `<title>/Updates/<name>`, DLC →
/// `<title>/DLC/<name>`, mods and patches → `<title>/Mods/<name>`,
/// cheats → `<title>/Cheats/<name>`. An entry whose title or name is not
/// one safe component stays in the library.
pub fn master_layout(game: &MasterGame) -> TranslatedGame {
    let title = game.title.as_str();
    let place = |from: &Path, folder: Option<FixedFolder>| {
        let to = file_name(from)
            .and_then(|name| match folder {
                Some(f) => safe_join(&[title, f.dir_name(), name]),
                None => safe_join(&[title, name]),
            })
            .map_or(Destination::Library, Destination::System);
        Placement {
            from: from.to_path_buf(),
            to,
            note: None,
        }
    };
    let mut entries = vec![place(&game.base.path, None)];
    entries.extend(
        game.updates
            .iter()
            .map(|e| place(&e.path, Some(FixedFolder::Updates))),
    );
    entries.extend(
        game.dlc
            .iter()
            .map(|e| place(&e.path, Some(FixedFolder::Dlc))),
    );
    entries.extend(
        game.mods
            .iter()
            .chain(&game.cheats)
            .map(|a| place(&a.path, Some(a.kind.fixed_folder()))),
    );
    TranslatedGame { entries }
}

/// The final component of `path` as text, if it has one.
fn file_name(path: &Path) -> Option<&str> {
    match path.components().next_back()? {
        Component::Normal(name) => name.to_str(),
        _ => None,
    }
}

/// The longest name one component may have, in bytes.
const MAX_COMPONENT: usize = 255;

/// Joins validated components only (D14): `None` when `parts` is empty or
/// any part is not exactly one normal component — empty, `.`, `..`, a
/// separator, a drive, a NUL, a reserved or unsafe name, or longer than
/// 255 bytes. Nothing is ever rewritten into a path.
pub fn safe_join(parts: &[&str]) -> Option<PathBuf> {
    if parts.is_empty() {
        return None;
    }
    let mut out = PathBuf::new();
    for part in parts {
        if part.len() > MAX_COMPONENT || part.contains('\0') {
            return None;
        }
        out.push(one_component(part)?);
    }
    Some(out)
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
