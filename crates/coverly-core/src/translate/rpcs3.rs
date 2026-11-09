//! RPCS3 (ps3, D15): the base where the profile puts it, updates and DLC
//! installed in the emulator, per-title patch files in its `patches/`.
//! RPCS3 has no mods folder (mods replace the game's files).

use super::{
    ends_with_ci, file_name, in_emulator, installed_layout, Destination, EmulatorId, MasterAddon,
    MasterGame, NotSupportedWhy, Rule, TranslateNote, TranslateTarget, TranslatedGame, STAYS,
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
    match file_name(&addon.path) {
        Some(name)
            if addon.format == AddonFormat::Rpcs3Patch && !addon.is_dir && per_title(name) =>
        {
            in_emulator(game, E, &["patches", name])
        }
        _ => STAYS,
    }
}

/// `<TITLEID>_patch.yml`, never RPCS3's own `patch.yml` or
/// `imported_patch.yml`.
fn per_title(name: &str) -> bool {
    ends_with_ci(name, "_patch.yml") && !name.eq_ignore_ascii_case("imported_patch.yml")
}
