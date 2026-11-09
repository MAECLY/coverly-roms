//! RetroArch (every retro system, D15): a translation patch named like
//! the ROM beside it so RetroArch soft-patches it (one patch per ROM, or
//! an IPS chain `.ips`, `.ips1`… in its own order), an MSU-1 set renamed
//! to the ROM's stem beside it, cheats in
//! `cheats/<system database name>/<rom stem>.cht`. Other mods stay.

use std::sync::OnceLock;

use super::{
    addon_layout, base_parent, base_stem, ext_lower, file_name, in_emulator, on_system,
    Destination, EmulatorId, MasterAddon, MasterGame, NotSupportedWhy, Rule, TranslateNote,
    TranslateTarget, TranslatedGame, STAYS,
};
use crate::addons::{AddonFormat, AddonKind, HeaderFit};
use crate::systems::SystemDb;

const E: EmulatorId = EmulatorId::RetroArch;

pub(super) fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame {
    let parent = base_parent(game, to);
    let stem = base_stem(game);
    let patches: Vec<&MasterAddon> = game.mods.iter().filter(|a| soft_patch(a)).collect();
    let chain = ips_chain(&patches);
    addon_layout(game, to, |a| match stem {
        Some(stem) => addon(game, a, &parent, stem, &patches, chain),
        None => STAYS,
    })
}

/// A patch RetroArch soft-patches: BPS, UPS, IPS and xdelta files.
fn soft_patch(a: &MasterAddon) -> bool {
    a.kind == AddonKind::Patch
        && !a.is_dir
        && matches!(
            a.format,
            AddonFormat::Bps | AddonFormat::Ups | AddonFormat::Ips | AddonFormat::Xdelta
        )
}

/// The extension a patch takes beside the ROM: an IPS keeps its own
/// `.ipsN` only inside a chain (RetroArch tries `.ips1` only after
/// `.ips`, so a lone `.ips1` would never be applied).
fn patch_ext(a: &MasterAddon, chain: bool) -> Option<String> {
    match a.format {
        AddonFormat::Bps => Some("bps".into()),
        AddonFormat::Ups => Some("ups".into()),
        AddonFormat::Xdelta => Some("xdelta".into()),
        AddonFormat::Ips => Some(
            ext_lower(&a.path)
                .filter(|e| chain && ips_index(e).is_some())
                .unwrap_or_else(|| "ips".into()),
        ),
        _ => None,
    }
}

/// `ips` → 0, `ips1` → 1, `ips12` → 12.
fn ips_index(ext: &str) -> Option<usize> {
    let rest = ext.strip_prefix("ips")?;
    if rest.is_empty() {
        return Some(0);
    }
    (rest.bytes().all(|b| b.is_ascii_digit()) && !rest.starts_with('0'))
        .then(|| rest.parse().ok())
        .flatten()
}

/// Several IPS patches whose own extensions are exactly `.ips`, `.ips1`,
/// … `.ips<n-1>`: RetroArch applies them in that order.
fn ips_chain(patches: &[&MasterAddon]) -> bool {
    if patches.len() < 2 || patches.iter().any(|a| a.format != AddonFormat::Ips) {
        return false;
    }
    let mut seen = vec![false; patches.len()];
    for a in patches {
        match ext_lower(&a.path).as_deref().and_then(ips_index) {
            Some(i) if i < seen.len() && !seen[i] => seen[i] = true,
            _ => return false,
        }
    }
    true
}

fn addon(
    game: &MasterGame,
    a: &MasterAddon,
    parent: &[&str],
    stem: &str,
    patches: &[&MasterAddon],
    chain: bool,
) -> Rule {
    if soft_patch(a) {
        if patches.len() > 1 && !chain {
            return (
                Destination::Library,
                Some(TranslateNote::NotSupported {
                    emulator: E,
                    why: NotSupportedWhy::SeveralPatches,
                }),
            );
        }
        if let Some(needs @ (HeaderFit::NeedsHeaderless | HeaderFit::NeedsHeadered)) = a.header {
            return (
                Destination::Library,
                Some(TranslateNote::HeaderMismatch { needs }),
            );
        }
        return match patch_ext(a, chain) {
            Some(ext) => beside(parent, &format!("{stem}.{ext}")),
            None => STAYS,
        };
    }
    match (a.format, a.is_dir) {
        (AddonFormat::Msu1, false) => match msu1_name(a, stem) {
            Some(name) => beside(parent, &name),
            None => STAYS,
        },
        (AddonFormat::RetroArchCht, false) => match database_name(&game.system) {
            Some(system) => in_emulator(game, E, &["cheats", system, &format!("{stem}.cht")]),
            None => STAYS,
        },
        _ => STAYS,
    }
}

fn beside(parent: &[&str], name: &str) -> Rule {
    match on_system(parent, name) {
        Destination::Library => STAYS,
        to => (to, None),
    }
}

/// An MSU-1 piece renamed to the ROM's stem: `<x>.msu` → `<stem>.msu`,
/// `<x>-<N>.pcm` → `<stem>-<N>.pcm`, `<x>.bml` → `<stem>.bml`.
fn msu1_name(a: &MasterAddon, stem: &str) -> Option<String> {
    let name = file_name(&a.path)?;
    let (own, ext) = name.rsplit_once('.')?;
    match ext.to_ascii_lowercase().as_str() {
        "msu" => Some(format!("{stem}.msu")),
        "bml" => Some(format!("{stem}.bml")),
        "pcm" => {
            let (_, track) = own.rsplit_once('-')?;
            (!track.is_empty() && track.bytes().all(|b| b.is_ascii_digit()))
                .then(|| format!("{stem}-{track}.pcm"))
        }
        _ => None,
    }
}

/// The libretro database name of a built-in system
/// (`Nintendo - Super Nintendo Entertainment System`), the folder
/// RetroArch's cheat database files a system's cheats under.
fn database_name(system: &str) -> Option<&'static str> {
    static DB: OnceLock<SystemDb> = OnceLock::new();
    DB.get_or_init(SystemDb::builtin)
        .get(system)
        .and_then(|s| s.libretro.as_deref())
}
