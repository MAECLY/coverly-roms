//! PCSX2 (ps2, D15): texture packs in `textures/<SERIAL>/`, `.pnach`
//! cheats and patches in `cheats/` (PCSX2 matches them by the CRC or
//! serial in their name, kept as it is).

use super::{
    addon_layout, addon_serial, file_name, in_emulator, EmulatorId, MasterAddon, MasterGame, Rule,
    TranslateTarget, TranslatedGame, STAYS,
};
use crate::addons::AddonFormat;

const E: EmulatorId = EmulatorId::Pcsx2;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    addon_layout(game, to, |a| addon(game, a))
}

fn addon(game: &MasterGame, a: &MasterAddon) -> Rule {
    match (a.format, a.is_dir) {
        (AddonFormat::TexturePack, true) => match addon_serial(game, a) {
            Some(serial) => in_emulator(game, E, &["textures", &serial]),
            None => STAYS,
        },
        (AddonFormat::Pnach, false) => match file_name(&a.path) {
            Some(name) => in_emulator(game, E, &["cheats", name]),
            None => STAYS,
        },
        _ => STAYS,
    }
}
