//! DuckStation (psx, D15): texture packs in `textures/<SERIAL>/`, cheats
//! in `cheats/` named by the serial, a lone `.ppf` beside the image named
//! like it (DuckStation applies it on the fly).

use super::{
    addon_layout, addon_serial, base_parent, base_stem, file_name, id_named, in_emulator,
    on_system, Destination, EmulatorId, MasterAddon, MasterGame, NotSupportedWhy, Rule,
    TranslateNote, TranslateTarget, TranslatedGame, STAYS,
};
use crate::addons::{valid_sony_serial, AddonFormat};

const E: EmulatorId = EmulatorId::DuckStation;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    let parent = base_parent(game, to);
    let ppfs = game
        .mods
        .iter()
        .filter(|a| a.format == AddonFormat::Ppf && !a.is_dir)
        .count();
    addon_layout(game, to, |a| addon(game, a, &parent, ppfs))
}

fn addon(game: &MasterGame, a: &MasterAddon, parent: &[&str], ppfs: usize) -> Rule {
    match (a.format, a.is_dir) {
        (AddonFormat::TexturePack, true) => match addon_serial(game, a) {
            Some(serial) => in_emulator(game, E, &["textures", &serial]),
            None => STAYS,
        },
        (AddonFormat::DuckStationCht, false) => match cheat_name(game, a) {
            Some(name) => in_emulator(game, E, &["cheats", &name]),
            None => STAYS,
        },
        (AddonFormat::Ppf, false) => {
            if ppfs > 1 {
                return (
                    Destination::Library,
                    Some(TranslateNote::NotSupported {
                        emulator: E,
                        why: NotSupportedWhy::SeveralPatches,
                    }),
                );
            }
            match base_stem(game).map(|stem| on_system(parent, &format!("{stem}.ppf"))) {
                Some(Destination::Library) | None => STAYS,
                Some(to) => (to, None),
            }
        }
        _ => STAYS,
    }
}

/// `SERIAL.cht` or `SERIAL-HASH.cht`, the serial as DuckStation writes it
/// (`SLUS-00594`); a name that is another spelling of the serial alone
/// (`slus_005.94.cht`) becomes `SLUS-00594.cht`.
fn cheat_name(game: &MasterGame, a: &MasterAddon) -> Option<String> {
    let serial = addon_serial(game, a)?;
    let name = file_name(&a.path)?;
    if let Some(named) = id_named(name, &serial, ".cht") {
        return Some(named);
    }
    let (stem, ext) = name.rsplit_once('.')?;
    (ext.eq_ignore_ascii_case("cht") && valid_sony_serial(stem).as_deref() == Some(&serial))
        .then(|| format!("{serial}.cht"))
}
