//! Cemu (wiiu, D15): the base where the profile puts it, updates and DLC
//! installed in the emulator, graphic packs (mods and cheats alike) in
//! `graphicPacks/<pack folder>/`.

use super::{
    file_name, in_emulator, installed_layout, EmulatorId, MasterAddon, MasterGame, Rule,
    TranslateTarget, TranslatedGame, STAYS,
};
use crate::addons::AddonFormat;

const E: EmulatorId = EmulatorId::Cemu;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    installed_layout(game, to, E, |a| addon(game, a))
}

fn addon(game: &MasterGame, addon: &MasterAddon) -> Rule {
    match file_name(&addon.path) {
        Some(name) if addon.format == AddonFormat::CemuGraphicPack && addon.is_dir => {
            in_emulator(game, E, &["graphicPacks", name])
        }
        _ => STAYS,
    }
}
