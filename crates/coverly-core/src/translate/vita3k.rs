//! Vita3K (psvita, D15): the base where the profile puts it, updates and
//! DLC installed in the emulator, `<TITLEID>*.txt` patches in its
//! `patch/`. Vita3K does not read rePatch: those files have to be merged
//! into the game, which Coverly never does.

use super::{
    addon_sony_title_id, file_name, id_named, in_emulator, installed_layout, Destination,
    EmulatorId, MasterAddon, MasterGame, NotSupportedWhy, Rule, TranslateNote, TranslateTarget,
    TranslatedGame, STAYS,
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
    // Vita3K loads `patch/*.txt` whose name holds the TITLE_ID as
    // written; the global `patchlist.txt` is the user's own and never
    // starts with an id, so it is never written over.
    let name = file_name(&addon.path)
        .filter(|_| addon.format == AddonFormat::Vita3kPatch && !addon.is_dir)
        .zip(addon_sony_title_id(game, addon))
        .and_then(|(name, id)| id_named(name, &id, ".txt"));
    match name {
        Some(name) => in_emulator(game, E, &["patch", &name]),
        None => STAYS,
    }
}
