//! shadPS4 (ps4, D15): an extracted game folder with its newest update
//! beside it as `<base>-UPDATE`, DLC under `addcont/<TITLE_ID>/`, mods in
//! the `<base>-mods` overlay, cheats and patches in the emulator's
//! `cheats/` and `patches/`. Packages are never extracted.

use super::{
    addon_sony_title_id, base_parent, ends_with_ci, file_name, game_sony_title_id, id_named,
    in_emulator, on_system, placed, safe_join, Destination, EmulatorId, EntryForm, MasterAddon,
    MasterEntry, MasterGame, NotSupportedWhy, Rule, TranslateNote, TranslateTarget, TranslatedGame,
    STAYS,
};
use crate::addons::{AddonFormat, AddonKind};

const E: EmulatorId = EmulatorId::Shadps4;

const EXTRACT_FIRST: Rule = (Destination::Library, Some(TranslateNote::ExtractFirst));

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    let parent = base_parent(game, to);
    // The extracted game folder's name (`CUSA00900`); a package has none.
    let base_dir = (game.base.form == EntryForm::Extracted && game.base.is_dir)
        .then(|| file_name(&game.base.path))
        .flatten();

    let base = match base_dir {
        Some(name) => (on_system(&parent, name), None),
        None => EXTRACT_FIRST,
    };
    let mut entries = vec![placed(&game.base.path, base)];

    // shadPS4 reads one update overlay: the newest extracted one.
    let newest = game
        .updates
        .iter()
        .position(|u| u.newest && u.form == EntryForm::Extracted && u.is_dir);
    entries.extend(game.updates.iter().enumerate().map(|(i, u)| {
        let rule = if u.form == EntryForm::Package {
            EXTRACT_FIRST
        } else if !u.is_dir {
            STAYS
        } else if let Some(base) = base_dir {
            if Some(i) == newest {
                overlay(&parent, base, u)
            } else {
                older()
            }
        } else {
            // The update has no folder to sit beside until the base is
            // extracted.
            EXTRACT_FIRST
        };
        placed(&u.path, rule)
    }));

    let title_id = game_sony_title_id(game);
    entries.extend(game.dlc.iter().map(|d| {
        let rule = if d.form == EntryForm::Package {
            EXTRACT_FIRST
        } else if !d.is_dir {
            STAYS
        } else {
            match (title_id.as_deref(), file_name(&d.path)) {
                (Some(id), Some(name)) => addcont(id, name),
                _ => STAYS,
            }
        };
        placed(&d.path, rule)
    }));

    // One overlay folder only: two mods would fight over the name.
    let overlays: Vec<&MasterAddon> = game
        .mods
        .iter()
        .filter(|a| a.kind == AddonKind::Mod && a.is_dir)
        .collect();
    entries.extend(game.mods.iter().map(|a| {
        let rule = match base_dir {
            Some(base) if overlays.len() == 1 && overlays[0] == a => {
                (on_system(&parent, &format!("{base}-mods")), None)
            }
            _ => STAYS,
        };
        placed(&a.path, rule)
    }));
    entries.extend(game.cheats.iter().map(|a| placed(&a.path, cheat(game, a))));
    TranslatedGame { entries }
}

/// The newest update beside its base: `<base>-UPDATE`, or `<base>-patch`
/// when the update already used that name.
fn overlay(parent: &[&str], base: &str, update: &MasterEntry) -> Rule {
    let suffix = if file_name(&update.path).is_some_and(|n| ends_with_ci(n, "-patch")) {
        "-patch"
    } else {
        "-UPDATE"
    };
    (on_system(parent, &format!("{base}{suffix}")), None)
}

fn older() -> Rule {
    (
        Destination::Library,
        Some(TranslateNote::NotSupported {
            emulator: E,
            why: NotSupportedWhy::OlderUpdate,
        }),
    )
}

/// `addcont/<TITLE_ID>/<dlc>` under the system folder; shadPS4 has to be
/// pointed at `addcont`.
fn addcont(title_id: &str, name: &str) -> Rule {
    match safe_join(&["addcont", title_id, name]) {
        Some(path) => (
            Destination::System(path),
            Some(TranslateNote::PointEmulatorHere {
                emulator: E,
                folder: "addcont".into(),
            }),
        ),
        None => STAYS,
    }
}

/// Cheats (`SERIAL_VERSION.json`) to `cheats/`, XML patches to
/// `patches/`.
fn cheat(game: &MasterGame, addon: &MasterAddon) -> Rule {
    let Some(name) = file_name(&addon.path).filter(|_| !addon.is_dir) else {
        return STAYS;
    };
    match addon.format {
        // shadPS4 opens `<SERIAL>_<VERSION>.json` as written.
        AddonFormat::Shadps4Cheat => {
            match addon_sony_title_id(game, addon).and_then(|id| id_named(name, &id, ".json")) {
                Some(name) => in_emulator(game, E, &["cheats", &name]),
                None => STAYS,
            }
        }
        AddonFormat::Shadps4Patch => in_emulator(game, E, &["patches", name]),
        _ => STAYS,
    }
}
