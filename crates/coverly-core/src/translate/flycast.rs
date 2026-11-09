//! Flycast (dreamcast, D15): texture packs in `textures/<GAMEID>/`.

use super::{
    addon_game_id, addon_layout, in_emulator, EmulatorId, MasterAddon, MasterGame, Rule,
    TranslateTarget, TranslatedGame, STAYS,
};
use crate::addons::AddonFormat;

const E: EmulatorId = EmulatorId::Flycast;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    addon_layout(game, to, |a| addon(game, a))
}

fn addon(game: &MasterGame, a: &MasterAddon) -> Rule {
    match (a.format, a.is_dir, addon_game_id(game, a)) {
        (AddonFormat::TexturePack, true, Some(id)) => in_emulator(game, E, &["textures", &id]),
        _ => STAYS,
    }
}
