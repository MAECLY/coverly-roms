//! After a write (fill, drawer action, undo) only the touched entries are
//! resolved again (spec §7 step 9): a scan holding just those ROMs goes
//! through the core's `resolve_art`, which reads only their art folders.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use coverly_core::art::{resolve_art, ArtState};
use coverly_core::profiles::Media;
use coverly_core::scan::{LibraryScan, SystemScan};
use coverly_core::systems::SystemDb;
use serde::Serialize;

use super::view::ArtMarks;
use super::Session;

/// New art marks of one entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EntryArtUpdate {
    pub id: u32,
    pub art: ArtMarks,
}

/// The art files that exist, per media.
pub(crate) fn present_paths(states: &BTreeMap<Media, ArtState>) -> BTreeMap<Media, PathBuf> {
    states
        .iter()
        .filter_map(|(media, state)| match state {
            ArtState::Present { path } => Some((*media, path.clone())),
            _ => None,
        })
        .collect()
}

/// A scan of only `ids` (unknown ids dropped, duplicates merged), grouped
/// by system in session order, and the ids in the order of its ROMs.
pub fn subset_scan(session: &Session, ids: &[u32]) -> (LibraryScan, Vec<u32>) {
    let mut wanted: Vec<u32> = ids
        .iter()
        .copied()
        .filter(|&id| (id as usize) < session.entries.len())
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut systems: Vec<SystemScan> = Vec::new();
    let mut last: Option<u16> = None;
    for &id in &wanted {
        let entry = &session.entries[id as usize];
        if last != Some(entry.system) {
            let sys = &session.systems[entry.system as usize];
            systems.push(SystemScan {
                dir: sys.dir.clone(),
                folder_name: sys.folder.clone(),
                system_id: sys.system_id.clone(),
                roms: Vec::new(),
            });
            last = Some(entry.system);
        }
        systems
            .last_mut()
            .expect("a system was pushed above")
            .roms
            .push(entry.rom.clone());
    }
    let scan = LibraryScan {
        root: session.root.clone(),
        profile_id: session.profile.id.clone(),
        systems,
        warnings: Vec::new(),
    };
    (scan, wanted)
}

/// Resolves the art of `ids` again from disk: a new session with their art
/// paths updated (the old one is untouched) and the new marks.
pub fn refresh_entries(
    session: &Session,
    ids: &[u32],
    systems: &SystemDb,
) -> (Session, Vec<EntryArtUpdate>) {
    let (scan, order) = subset_scan(session, ids);
    let art = resolve_art(&scan, &session.profile, systems, &Media::ALL);
    let mut next = session.clone();
    let mut updates = Vec::with_capacity(order.len());
    for (id, rom_art) in order
        .into_iter()
        .zip(art.iter().flat_map(|s| s.roms.iter()))
    {
        next.entries[id as usize].art = present_paths(&rom_art.art);
        updates.push(EntryArtUpdate {
            id,
            art: ArtMarks::from_states(&rom_art.art),
        });
    }
    (next, updates)
}

/// Entries whose art could be one of `paths` (file stem equal to the ROM
/// stem, or a Batocera `<stem>-image` style name), ignoring case. Used
/// after an undo, which reports paths only; an extra entry is harmless.
pub fn ids_for_art_paths(session: &Session, paths: &[PathBuf]) -> Vec<u32> {
    let stems: Vec<String> = paths
        .iter()
        .filter_map(|p| p.file_stem())
        .map(|s| s.to_string_lossy().to_lowercase())
        .collect();
    session
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            let stem = e.rom.stem.to_lowercase();
            stems.iter().any(|s| {
                s == &stem
                    || s.strip_prefix(stem.as_str())
                        .is_some_and(|rest| rest.starts_with('-'))
            })
        })
        .map(|(i, _)| i as u32)
        .collect()
}

/// Entry id of each ROM's primary file.
pub fn entry_index(session: &Session) -> HashMap<PathBuf, u32> {
    session
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.rom.path.clone(), i as u32))
        .collect()
}

/// The primary file of a plan item (`folder` + path relative to it).
pub fn rom_path(session: &Session, folder: &str, rel_path: &Path) -> Option<PathBuf> {
    session
        .systems
        .iter()
        .find(|s| s.folder == folder)
        .map(|s| s.dir.join(rel_path))
}
