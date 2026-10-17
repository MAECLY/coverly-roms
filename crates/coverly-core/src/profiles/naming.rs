//! Folder names organise creates for each system (spec §8.1).

use std::collections::BTreeMap;

use super::{FolderNaming, Profile};
use crate::systems::{System, SystemDb};

/// The folder a system's ROMs go to under a layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderChoice {
    /// Folder name, e.g. `GBA`, `snes`.
    pub folder: String,
    /// The system that owns the folder when it is shared (`ngp` for NGPC on
    /// Anbernic); `None` when the folder is the system's own.
    pub host: Option<String>,
    /// Whether the device shows this folder; `None` when the layout does
    /// not list its folders.
    pub on_device: Option<bool>,
}

/// The folder for `system` under `profile`, honouring `overrides`
/// (`settings.organize.folder_names[profile.id]`).
pub fn system_folder(
    profile: &Profile,
    system: &System,
    db: &SystemDb,
    overrides: &BTreeMap<String, String>,
) -> FolderChoice {
    let host = profile
        .host_folders
        .get(&system.id)
        .and_then(|h| db.get(h))
        .filter(|h| h.id != system.id);
    let owner = host.unwrap_or(system);
    let folder = overrides
        .get(&system.id)
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| default_name(profile.system_folder, owner, db));
    let on_device = (!profile.known_folders.is_empty()).then(|| {
        profile
            .known_folders
            .iter()
            .any(|k| k.eq_ignore_ascii_case(&folder))
    });
    FolderChoice {
        folder,
        host: host.map(|h| h.id.clone()),
        on_device,
    }
}

/// `Id` falls back to the first alias when the id itself is not an alias
/// of the system (the folder must resolve back, or the scan would not see
/// it).
fn default_name(rule: FolderNaming, system: &System, db: &SystemDb) -> String {
    let first = system
        .aliases
        .first()
        .cloned()
        .unwrap_or_else(|| system.id.clone());
    match rule {
        FolderNaming::FirstAlias => first,
        FolderNaming::Id => {
            let resolves = db
                .resolve_folder(&system.id)
                .is_some_and(|s| s.id == system.id);
            if resolves {
                system.id.clone()
            } else {
                first
            }
        }
    }
}

/// A user folder name for `system_id` must resolve back to that system or
/// to its host under `profile`.
pub fn validate_folder_override(
    db: &SystemDb,
    profile: &Profile,
    system_id: &str,
    folder: &str,
) -> Result<(), String> {
    let wanted = profile
        .host_folders
        .get(system_id)
        .map_or(system_id, String::as_str);
    match db.resolve_folder(folder.trim()) {
        Some(s) if s.id == wanted => Ok(()),
        Some(s) => Err(format!("`{folder}` is the folder of {} ({})", s.name, s.id)),
        None => Err(format!("`{folder}` is not a known system folder name")),
    }
}
