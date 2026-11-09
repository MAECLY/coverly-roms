//! Dolphin (gamecube, wii, D15): texture packs in `Load/Textures/<GAMEID>/`,
//! graphics mods in `Load/GraphicMods/<name>/`, a Riivolution pack root in
//! `Load/Riivolution/`, cheats in `GameSettings/<GAMEID>.ini` only when no
//! file of that name is there (Dolphin keeps user settings in it: never
//! merged, Q5).

use super::{
    addon_game_id, addon_layout, file_name, in_emulator, EmulatorId, MasterAddon, MasterGame, Rule,
    TranslateTarget, TranslatedGame, STAYS,
};
use crate::addons::AddonFormat;

const E: EmulatorId = EmulatorId::Dolphin;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    addon_layout(game, to, |a| addon(game, a))
}

fn addon(game: &MasterGame, a: &MasterAddon) -> Rule {
    match (a.format, a.is_dir) {
        (AddonFormat::TexturePack, true) => match addon_game_id(game, a) {
            Some(id) => in_emulator(game, E, &["Load", "Textures", &id]),
            None => STAYS,
        },
        (AddonFormat::DolphinGraphicMod, true) => match file_name(&a.path) {
            Some(name) => in_emulator(game, E, &["Load", "GraphicMods", name]),
            None => STAYS,
        },
        (AddonFormat::Riivolution, true) => in_emulator(game, E, &["Load", "Riivolution"]),
        (AddonFormat::DolphinIni, false) => match addon_game_id(game, a) {
            Some(id) => in_emulator(game, E, &["GameSettings", &format!("{id}.ini")]),
            None => STAYS,
        },
        _ => STAYS,
    }
}
