//! Ryujinx and the yuzu family (switch, D15): the base where the profile
//! puts it, updates and DLC in their own `Updates/` and `DLC/` folders
//! under the system folder that the emulator is pointed at (Q8), mods and
//! cheats in the emulator's mod folder: Ryujinx
//! `mods/contents/<titleid>/<mod>/`, yuzu `load/<TitleID>/<mod>/`.

use super::{
    addon_title_id16, base_parent, file_name, in_emulator, on_system, placed, Destination,
    EmulatorId, MasterAddon, MasterEntry, MasterGame, Rule, TranslateNote, TranslateTarget,
    TranslatedGame, STAYS,
};
use crate::addons::{valid_build_id, valid_title_id16, AddonFormat, AddonKey};
use crate::game_folder::FixedFolder;

/// The mod level a bare `<TitleID>/` mod folder gets.
const COVERLY_MOD: &str = "Coverly mod";
/// The yuzu mod level cheats go under (yuzu reads `cheats/` one level
/// below the title id, never at the title id itself).
const COVERLY_CHEATS: &str = "Coverly cheats";

/// Folder names that are part of a mod's path, never its name.
const NOT_A_MOD_NAME: &[&str] = &["mods", "contents", "atmosphere", "titles", "load", "cheats"];

pub(super) fn translate(
    emulator: EmulatorId,
    game: &MasterGame,
    to: &TranslateTarget,
) -> TranslatedGame {
    let parent = base_parent(game, to);
    let base = file_name(&game.base.path).map_or(STAYS, |name| (on_system(&parent, name), None));
    let mut entries = vec![placed(&game.base.path, base)];
    entries.extend(
        game.updates
            .iter()
            .map(|e| placed(&e.path, separate(emulator, FixedFolder::Updates, e))),
    );
    entries.extend(
        game.dlc
            .iter()
            .map(|e| placed(&e.path, separate(emulator, FixedFolder::Dlc, e))),
    );
    entries.extend(
        game.mods
            .iter()
            .chain(&game.cheats)
            .map(|a| placed(&a.path, addon(emulator, game, a))),
    );
    TranslatedGame { entries }
}

/// An update or DLC in its own folder under the system folder, which the
/// emulator is pointed at.
fn separate(emulator: EmulatorId, folder: FixedFolder, entry: &MasterEntry) -> Rule {
    match file_name(&entry.path) {
        Some(name) => match on_system(&[folder.dir_name()], name) {
            Destination::Library => STAYS,
            to => (
                to,
                Some(TranslateNote::PointEmulatorHere {
                    emulator,
                    folder: folder.dir_name().into(),
                }),
            ),
        },
        None => STAYS,
    }
}

/// The title id folder name as the emulator writes it: Ryujinx lower
/// case, the yuzu family upper case.
fn title_dir(emulator: EmulatorId, id: &str) -> String {
    if emulator == EmulatorId::Ryujinx {
        id.to_ascii_lowercase()
    } else {
        id.to_ascii_uppercase()
    }
}

fn addon(emulator: EmulatorId, game: &MasterGame, addon: &MasterAddon) -> Rule {
    let ryujinx = emulator == EmulatorId::Ryujinx;
    match (addon.format, addon.is_dir) {
        (AddonFormat::SwitchMod, true) => {
            let (Some(id), Some(level)) = (addon_title_id16(game, addon), mod_level(game, addon))
            else {
                return STAYS;
            };
            let id = title_dir(emulator, &id);
            if ryujinx {
                in_emulator(game, emulator, &["mods", "contents", &id, level])
            } else {
                in_emulator(game, emulator, &["load", &id, level])
            }
        }
        (AddonFormat::SwitchCheats, false) => {
            let AddonKey::BuildId { build_id, title_id } = &addon.key else {
                return STAYS;
            };
            let id = match title_id {
                Some(id) => valid_title_id16(id),
                None => game.ids.title_id.as_deref().and_then(valid_title_id16),
            };
            let (Some(id), Some(build)) = (id, valid_build_id(build_id)) else {
                return STAYS;
            };
            let id = title_dir(emulator, &id);
            let name = format!("{build}.txt");
            if ryujinx {
                in_emulator(game, emulator, &["mods", "contents", &id, "cheats", &name])
            } else {
                in_emulator(
                    game,
                    emulator,
                    &["load", &id, COVERLY_CHEATS, "cheats", &name],
                )
            }
        }
        (AddonFormat::ExefsPatches, true) => {
            let Some(set) = file_name(&addon.path) else {
                return STAYS;
            };
            if ryujinx {
                in_emulator(game, emulator, &["mods", "exefs_patches", set])
            } else {
                // yuzu keeps exefs patches inside a mod of the title: the
                // set's own title id when it names one (never replaced
                // when it is not valid), else the game's.
                let id = match &addon.key {
                    AddonKey::BuildId {
                        title_id: Some(id), ..
                    } => valid_title_id16(id),
                    _ => addon_title_id16(game, addon),
                };
                match id {
                    Some(id) => in_emulator(
                        game,
                        emulator,
                        &["load", &title_dir(emulator, &id), set, "exefs"],
                    ),
                    None => STAYS,
                }
            }
        }
        _ => STAYS,
    }
}

/// The mod level of a mod folder: its own name when the user named it, the
/// name of the user's folder above a bare `<TitleID>/`, else
/// `Coverly mod`.
fn mod_level<'a>(game: &MasterGame, addon: &'a MasterAddon) -> Option<&'a str> {
    let name = file_name(&addon.path)?;
    if valid_title_id16(name).is_none() {
        return Some(name);
    }
    let parent = addon.path.parent().and_then(file_name);
    let named = parent.filter(|p| {
        !p.eq_ignore_ascii_case(&game.title)
            && FixedFolder::from_dir_name(p).is_none()
            && valid_title_id16(p).is_none()
            && !NOT_A_MOD_NAME.iter().any(|n| p.eq_ignore_ascii_case(n))
    });
    Some(named.unwrap_or(COVERLY_MOD))
}
