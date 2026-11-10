//! Azahar (3ds, D15): the base where the profile puts it, updates and DLC
//! installed in the emulator, LayeredFS mods in `load/mods/<TitleID>/`,
//! texture packs in `load/textures/<TitleID>/`, cheats in
//! `cheats/<TitleID>.txt`.

use super::{
    addon_title_id16, in_emulator, installed_layout, EmulatorId, MasterAddon, MasterGame, Rule,
    TranslateTarget, TranslatedGame, STAYS,
};
use crate::addons::AddonFormat;

const E: EmulatorId = EmulatorId::Azahar;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    installed_layout(game, to, E, |a| addon(game, a))
}

fn addon(game: &MasterGame, addon: &MasterAddon) -> Rule {
    let Some(id) = addon_title_id16(game, addon) else {
        return STAYS;
    };
    match (addon.format, addon.is_dir) {
        (AddonFormat::LayeredFs, true) => in_emulator(game, E, &["load", "mods", &id]),
        (AddonFormat::TexturePack, true) => in_emulator(game, E, &["load", "textures", &id]),
        (AddonFormat::CtrCheats, false) => in_emulator(game, E, &["cheats", &format!("{id}.txt")]),
        _ => STAYS,
    }
}
