//! The game as a folder (game folders spec D3, §8.1): the fixed child
//! folder names a game folder may hold and what a child of a game is.
//!
//! The names on disk are English and fixed (`Updates/`, `DLC/`, `Mods/`,
//! `Cheats/`, `Extras/`, `Saves/`); the UI translates them.

use serde::{Deserialize, Serialize};

use crate::organize::discsets::disc_tag;

/// A fixed folder inside a game folder (spec D3, D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FixedFolder {
    Updates,
    Dlc,
    Mods,
    Cheats,
    Extras,
    Saves,
}

/// What a file inside a game folder is to its base game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChildRole {
    Update,
    Dlc,
}

impl FixedFolder {
    /// Every fixed folder, in the order the spec lists them.
    pub const ALL: [FixedFolder; 6] = [
        Self::Updates,
        Self::Dlc,
        Self::Mods,
        Self::Cheats,
        Self::Extras,
        Self::Saves,
    ];

    /// Exact on-disk name: `Updates`, `DLC`, `Mods`, `Cheats`, `Extras`, `Saves`.
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Updates => "Updates",
            Self::Dlc => "DLC",
            Self::Mods => "Mods",
            Self::Cheats => "Cheats",
            Self::Extras => "Extras",
            Self::Saves => "Saves",
        }
    }

    /// Case-insensitive and exact: `updates`, `dlc` match; `Update`,
    /// `DLCs`, `Updates (old)` and `Saves ` do not.
    pub fn from_dir_name(name: &str) -> Option<FixedFolder> {
        Self::ALL
            .into_iter()
            .find(|f| f.dir_name().eq_ignore_ascii_case(name))
    }

    /// The role a file inside this folder has to its base game:
    /// `Updates` → update, `DLC` → DLC, the others none.
    pub fn child_role(self) -> Option<ChildRole> {
        match self {
            Self::Updates => Some(ChildRole::Update),
            Self::Dlc => Some(ChildRole::Dlc),
            Self::Mods | Self::Cheats | Self::Extras | Self::Saves => None,
        }
    }
}

/// One directory's shape from one listing (no file opened).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirShape {
    /// Exactly one base at the root. `fixed` holds every fixed folder found,
    /// with its on-disk spelling(s), sorted by bytes (D3: `DLC/` and `dlc/`
    /// both count).
    GameFolder {
        base: GameBase,
        fixed: Vec<(FixedFolder, String)>,
    },
    /// Fixed folders but zero or several bases: not a game folder (D3).
    Loose { fixed: Vec<(FixedFolder, String)> },
    /// No fixed folder at all.
    Plain,
}

/// The one base game at a game folder's root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameBase {
    /// The directory itself is a folder game a probe recognised (PS3 JB,
    /// Vita, Wii U, PS4/PS5, Xbox).
    SelfIsFolderGame,
    /// One ROM-like file at the root.
    File(String),
    /// One sheet set (a `.cue`/`.gdi`/`.ccd` with its tracks, or one `.m3u`
    /// with its discs); holds the sheet's file name.
    Sheet(String),
}

/// Classify one directory from one listing (spec D3, §8.1). Nothing is opened.
///
/// - `entries`: `(name, is_dir)` of the directory's own listing.
/// - `rom_like(name)`: whether a root file is a ROM-like candidate (extension
///   known to the system db, not a companion, not key material). When a
///   sheet is present the caller passes a `rom_like` that already excludes
///   the track extensions (`.bin`, `.img`, `.raw`, `.wav`…), so the files a
///   sheet references are not counted again.
/// - `sheet(name)`: whether a root file is a cue/gdi/ccd/m3u sheet.
/// - `folder_game`: a probe already recognised the directory itself.
///
/// A sheet set counts as one base: one sheet, or one `.m3u` beside the
/// per-disc sheets it lists and the disc images named after it
/// (`Game (Disc 1).chd` beside `Game.m3u`). Only directories count as
/// fixed folders; a file named `DLC` does not.
pub fn classify_listing(
    entries: &[(String, bool)],
    rom_like: &dyn Fn(&str) -> bool,
    sheet: &dyn Fn(&str) -> bool,
    folder_game: bool,
) -> DirShape {
    let mut fixed: Vec<(FixedFolder, String)> = entries
        .iter()
        .filter(|(_, is_dir)| *is_dir)
        .filter_map(|(name, _)| FixedFolder::from_dir_name(name).map(|f| (f, name.clone())))
        .collect();
    if fixed.is_empty() {
        return DirShape::Plain;
    }
    fixed.sort_by(|a, b| a.1.as_bytes().cmp(b.1.as_bytes()));

    match root_base(entries, rom_like, sheet, folder_game) {
        Some(base) => DirShape::GameFolder { base, fixed },
        None => DirShape::Loose { fixed },
    }
}

/// The single base at the root, or `None` when there are zero or several.
fn root_base(
    entries: &[(String, bool)],
    rom_like: &dyn Fn(&str) -> bool,
    sheet: &dyn Fn(&str) -> bool,
    folder_game: bool,
) -> Option<GameBase> {
    if folder_game {
        return Some(GameBase::SelfIsFolderGame);
    }
    let files = entries
        .iter()
        .filter(|(_, is_dir)| !*is_dir)
        .map(|(n, _)| n);
    let mut sheets: Vec<&String> = Vec::new();
    let mut roms: Vec<&String> = Vec::new();
    for name in files {
        if sheet(name) {
            sheets.push(name);
        } else if rom_like(name) {
            roms.push(name);
        }
    }

    let sheet_base = match sheets.as_slice() {
        [] => None,
        [one] => Some((*one).clone()),
        many => {
            // One playlist over its per-disc sheets is one set.
            let mut lists = many.iter().filter(|n| is_m3u(n));
            match (lists.next(), lists.next()) {
                (Some(list), None) => Some((*list).clone()),
                _ => return None,
            }
        }
    };

    // A playlist's disc images (`Game (Disc 1).chd` beside `Game.m3u`, the
    // disc layout's shape) are its discs, not further games. Matched by name
    // only: the playlist is never opened.
    if let Some(list) = sheet_base.as_deref().filter(|n| is_m3u(n)) {
        let title = stem(list);
        roms.retain(|rom| {
            !disc_tag(stem(rom)).is_some_and(|tag| tag.title.eq_ignore_ascii_case(title.trim()))
        });
    }

    match (sheet_base, roms.as_slice()) {
        (Some(s), []) => Some(GameBase::Sheet(s)),
        (None, [one]) => Some(GameBase::File((*one).clone())),
        _ => None,
    }
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn is_m3u(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("m3u"))
}

/// True when `dir`'s parent is a system folder and `dir` itself is named
/// like a fixed folder (Batocera `ps4/DLC/`): never a game's (D3).
pub fn is_system_level_fixed(dir_name: &str, parent_is_system: bool) -> bool {
    parent_is_system && FixedFolder::from_dir_name(dir_name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_folders_serialise_kebab_case() {
        let all = [
            (FixedFolder::Updates, "\"updates\""),
            (FixedFolder::Dlc, "\"dlc\""),
            (FixedFolder::Mods, "\"mods\""),
            (FixedFolder::Cheats, "\"cheats\""),
            (FixedFolder::Extras, "\"extras\""),
            (FixedFolder::Saves, "\"saves\""),
        ];
        for (folder, json) in all {
            assert_eq!(serde_json::to_string(&folder).unwrap(), json);
            assert_eq!(serde_json::from_str::<FixedFolder>(json).unwrap(), folder);
        }
    }

    #[test]
    fn child_roles_serialise_kebab_case() {
        assert_eq!(
            serde_json::to_string(&ChildRole::Update).unwrap(),
            "\"update\""
        );
        assert_eq!(serde_json::to_string(&ChildRole::Dlc).unwrap(), "\"dlc\"");
        assert_eq!(
            serde_json::from_str::<ChildRole>("\"dlc\"").unwrap(),
            ChildRole::Dlc
        );
    }
}
