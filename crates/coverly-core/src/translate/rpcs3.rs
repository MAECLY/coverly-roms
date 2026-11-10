//! RPCS3 (ps3, D15): the base where the profile puts it, updates and DLC
//! installed in the emulator, per-title patch files in its `patches/`.
//! RPCS3 has no mods folder (mods replace the game's files).

use super::{
    addon_sony_title_id, file_name, id_named, in_emulator, installed_layout, Destination,
    EmulatorId, MasterAddon, MasterGame, NotSupportedWhy, Rule, TranslateNote, TranslateTarget,
    TranslatedGame, STAYS,
};
use crate::addons::{AddonFormat, AddonKind};

const E: EmulatorId = EmulatorId::Rpcs3;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    installed_layout(game, to, E, |a| addon(game, a))
}

fn addon(game: &MasterGame, addon: &MasterAddon) -> Rule {
    if addon.kind == AddonKind::Mod {
        return (
            Destination::Library,
            Some(TranslateNote::NotSupported {
                emulator: E,
                why: NotSupportedWhy::Rpcs3Mods,
            }),
        );
    }
    // `<TITLEID>_patch.yml`, never RPCS3's own `patch.yml`,
    // `imported_patch.yml` or `cheats.yml` (they start with no id).
    let name = file_name(&addon.path)
        .filter(|_| addon.format == AddonFormat::Rpcs3Patch && !addon.is_dir)
        .zip(addon_sony_title_id(game, addon))
        .and_then(|(name, id)| id_named(name, &id, "_patch.yml"));
    match name {
        Some(name) => in_emulator(game, E, &["patches", &name]),
        None => STAYS,
    }
}
