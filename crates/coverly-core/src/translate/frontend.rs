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
                *path = renamed.join(parts.as_path());
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
