//! Vita3K (psvita, D15): the base where the profile puts it, updates and
//! DLC installed in the emulator, `<TITLEID>*.txt` patches in its
//! `patch/`. Vita3K does not read rePatch: those files have to be merged
//! into the game, which Coverly never does.

use super::{
    ends_with_ci, file_name, in_emulator, installed_layout, Destination, EmulatorId, MasterAddon,
    MasterGame, NotSupportedWhy, Rule, TranslateNote, TranslateTarget, TranslatedGame, STAYS,
};
use crate::addons::AddonFormat;

const E: EmulatorId = EmulatorId::Vita3k;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    installed_layout(game, to, E, |a| addon(game, a))
}

fn addon(game: &MasterGame, addon: &MasterAddon) -> Rule {
    if addon.format == AddonFormat::VitaRepatch {
        return (
            Destination::Library,
            Some(TranslateNote::NotSupported {
                emulator: E,
                why: NotSupportedWhy::VitaRepatch,
            }),
        );
    }
    match file_name(&addon.path) {
        // The global `patchlist.txt` is the user's own: never written over.
        Some(name)
            if addon.format == AddonFormat::Vita3kPatch
                && !addon.is_dir
                && ends_with_ci(name, ".txt")
                && !name.eq_ignore_ascii_case("patchlist.txt") =>
        {
            in_emulator(game, E, &["patch", name])
        }
        _ => STAYS,
    }
}
