//! Frontend passes that run after an emulator's translator (D15).

use std::path::{Component, Path};

use super::{safe_join, Destination, MasterGame, TranslatedGame};

/// ES-DE «directories interpreted as files»: the game folder `<Game>/`
/// is named after the base file (`Game.pkg/` holding `Game.pkg`), so the
/// frontend launches the file and shows one game. Every placement under
/// the old folder follows it. A folder game (`folder_game_suffix` rule),
/// a base with no extension, or a base the emulator did not put in a game
/// folder is left as it is.
pub(super) fn dir_as_game(game: &MasterGame, mut laid: TranslatedGame) -> TranslatedGame {
    if game.base.is_dir {
        return laid;
    }
    let Some((folder, file)) = laid
        .entries
        .iter()
        .find(|p| p.from == game.base.path)
        .and_then(|p| match &p.to {
            Destination::System(at) => folder_and_file(at),
            _ => None,
        })
    else {
        return laid;
    };
    if folder == file || Path::new(&file).extension().is_none() {
        return laid;
    }
    let Some(renamed) = safe_join(&[file.as_str()]) else {
        return laid;
    };
    for placement in &mut laid.entries {
        if let Destination::System(path) = &mut placement.to {
            let mut parts = path.components();
            if matches!(parts.next(), Some(Component::Normal(first)) if first == folder.as_str()) {
                let rest = parts.as_path();
                // `join("")` would add a trailing separator.
                *path = if rest.as_os_str().is_empty() {
                    renamed.clone()
                } else {
                    renamed.join(rest)
                };
            }
        }
    }
    laid
}

/// `(folder, file)` when `at` is exactly `<folder>/<file>`.
fn folder_and_file(at: &Path) -> Option<(String, String)> {
    let mut parts = at.components();
    match (parts.next(), parts.next(), parts.next()) {
        (Some(Component::Normal(folder)), Some(Component::Normal(file)), None) => {
            Some((folder.to_str()?.to_owned(), file.to_str()?.to_owned()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::translate::{master_layout, EntryForm, GameIds, MasterEntry, Placement};

    #[test]
    fn a_placement_that_is_the_game_folder_itself_is_renamed_without_a_trailing_separator() {
        let base = MasterEntry {
            path: ["Game", "Game.pkg"].iter().collect(),
            is_dir: false,
            form: EntryForm::Package,
            title_id: None,
            version: None,
            newest: true,
        };
        let game = MasterGame {
            system: "ps3".into(),
            title: "Game".into(),
            ids: GameIds::default(),
            base,
            updates: vec![],
            dlc: vec![],
            mods: vec![],
            cheats: vec![],
            taken_in_emulator: vec![],
        };
        let mut laid = master_layout(&game);
        laid.entries.push(Placement {
            from: PathBuf::from("Game"),
            to: Destination::System(PathBuf::from("Game")),
            note: None,
        });
        let out = dir_as_game(&game, laid);
        let Destination::System(folder) = &out.entries[1].to else {
            panic!("{out:?}");
        };
        assert_eq!(folder.as_os_str(), "Game.pkg");
    }
}
