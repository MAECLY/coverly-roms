//! PPSSPP (psp, D15): texture packs in `PSP/TEXTURES/<GAMEID>/`, cheats in
//! `PSP/Cheats/<GAMEID>.ini` only when no file of that name is there
//! (never merged, Q5).

use super::{
    addon_game_id, addon_layout, in_emulator, EmulatorId, MasterAddon, MasterGame, Rule,
    TranslateTarget, TranslatedGame, STAYS,
};
use crate::addons::{valid_game_id, valid_sony_serial, AddonFormat, AddonKey};

const E: EmulatorId = EmulatorId::Ppsspp;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    addon_layout(game, to, |a| addon(game, a))
}

fn addon(game: &MasterGame, a: &MasterAddon) -> Rule {
    let Some(id) = disc_id(game, a) else {
        return STAYS;
    };
    match (a.format, a.is_dir) {
        (AddonFormat::TexturePack, true) => in_emulator(game, E, &["PSP", "TEXTURES", &id]),
        (AddonFormat::PpssppIni, false) => {
            in_emulator(game, E, &["PSP", "Cheats", &format!("{id}.ini")])
        }
        _ => STAYS,
    }
}

/// PPSSPP's game id, the dash-free disc id (`ULUS10391`): the addon's own
/// game id or serial key when it carries one (never replaced when that
/// key is not valid), else the game's game id, else its serial.
fn disc_id(game: &MasterGame, a: &MasterAddon) -> Option<String> {
    let psp = |id: String| (id.len() == 9).then_some(id);
    let compact = |s: &str| valid_sony_serial(s).map(|s| s.replace('-', ""));
    match &a.key {
        AddonKey::GameId { .. } => addon_game_id(game, a).and_then(psp),
        AddonKey::Serial { serial } => compact(serial),
        _ => game
            .ids
            .game_id
            .as_deref()
            .and_then(valid_game_id)
            .and_then(psp)
            .or_else(|| game.ids.serial.as_deref().and_then(compact)),
    }
}
