//! Xenia (xbox360, D15): `patch.toml` files in `patches/`, plugins in
//! `plugins/<TITLEID>/` (8 hex digits).

use super::{
    addon_layout, file_name, in_emulator, EmulatorId, MasterAddon, MasterGame, Rule,
    TranslateTarget, TranslatedGame, STAYS,
};
use crate::addons::{AddonFormat, AddonKey};

const E: EmulatorId = EmulatorId::Xenia;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    addon_layout(game, to, |a| addon(game, a))
}

/// An Xbox 360 title id: 8 hex digits, upper-cased.
fn title_id8(s: &str) -> Option<String> {
    (s.len() == 8 && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| s.to_ascii_uppercase())
}

fn addon(game: &MasterGame, a: &MasterAddon) -> Rule {
    match (a.format, a.is_dir) {
        (AddonFormat::XeniaPatch, false) => match file_name(&a.path) {
            Some(name) => in_emulator(game, E, &["patches", name]),
            None => STAYS,
        },
        (AddonFormat::XeniaPlugin, true) => {
            let id = match &a.key {
                AddonKey::TitleId { id } => title_id8(id),
                _ => game.ids.title_id.as_deref().and_then(title_id8),
            };
            match id {
                Some(id) => in_emulator(game, E, &["plugins", &id]),
                None => STAYS,
            }
        }
        _ => STAYS,
    }
}
