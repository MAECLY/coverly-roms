//! Mesen (nes, D15): HD packs in `HdPacks/<rom stem>/` (Mesen looks a
//! pack up by the ROM's file name).

use super::{
    addon_layout, base_stem, in_emulator, EmulatorId, MasterAddon, MasterGame, Rule,
    TranslateTarget, TranslatedGame, STAYS,
};
use crate::addons::AddonFormat;

const E: EmulatorId = EmulatorId::Mesen;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    addon_layout(game, to, |a| addon(game, a))
}

fn addon(game: &MasterGame, a: &MasterAddon) -> Rule {
    match (a.format, a.is_dir, base_stem(game)) {
        (AddonFormat::MesenHdPack, true, Some(stem)) => in_emulator(game, E, &["HdPacks", stem]),
        _ => STAYS,
    }
}
