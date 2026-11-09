//! The game as a folder (game folders spec D3, §8.1): the fixed child
//! folder names a game folder may hold and what a child of a game is.
//!
//! The names on disk are English and fixed (`Updates/`, `DLC/`, `Mods/`,
//! `Cheats/`, `Extras/`, `Saves/`); the UI translates them.

use serde::{Deserialize, Serialize};

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
