//! Folder names organise creates for each system (spec §8.1).

use std::collections::BTreeMap;

use super::{FolderNaming, Profile, ProfileFolder};
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
/// (`settings.organize.folder_names[profile.id]`). An override that does
/// not pass [`validate_folder_override`] is ignored: the folder must
/// resolve back to the system (or its host) and never be a path.
pub fn system_folder(
    profile: &Profile,
    system: &System,
    db: &SystemDb,
    overrides: &BTreeMap<String, String>,
) -> FolderChoice {
    // Order (spec D5, plan P-3): user override → the frontend's own folder
    // → the host's folder → the naming rule.
    let own = profile.folders.get(&system.id);
    let host = match own {
        Some(ProfileFolder::Named(_)) => None,
        _ => profile
            .host_folders
            .get(&system.id)
            .and_then(|h| db.get(h))
            .filter(|h| h.id != system.id),
    };
    let owner = host.unwrap_or(system);
    let user = overrides
        .get(&system.id)
        .map(|f| f.trim().to_string())
        .filter(|f| validate_folder_override(db, profile, &system.id, f).is_ok());
    let folder = user
        .clone()
        .or_else(|| match own {
            Some(ProfileFolder::Named(n)) => Some(n.clone()),
            _ => None,
        })
        .unwrap_or_else(|| default_name(profile, owner, db));
    let on_device = if !profile.known_folders.is_empty() {
        Some(
            profile
                .known_folders
                .iter()
                .any(|k| k.eq_ignore_ascii_case(&folder)),
        )
    } else if matches!(own, Some(ProfileFolder::Absent)) && host.is_none() && user.is_none() {
        // The frontend has no folder for it (D5). A name the user chose
        // gets no «no folder on device» note (spec §12).
        Some(false)
    } else {
        None
    };
    FolderChoice {
        folder,
        host: host.map(|h| h.id.clone()),
        on_device,
    }
}

/// `Id` falls back to the first alias when the id itself is not an alias
/// of the system (the folder must resolve back, or the scan would not see
/// it).
fn default_name(profile: &Profile, system: &System, db: &SystemDb) -> String {
    let first = system
        .aliases
        .first()
        .cloned()
        .unwrap_or_else(|| system.id.clone());
    match profile.system_folder {
        FolderNaming::FirstAlias => first,
        FolderNaming::Id => {
            let resolves = db
                .resolve_folder_in(Some(profile), &system.id)
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
/// to its host under `profile` (resolved with the profile's own folders,
/// [`SystemDb::resolve_folder_in`]). A system the profile names a folder
/// for has no host there.
pub fn validate_folder_override(
    db: &SystemDb,
    profile: &Profile,
    system_id: &str,
    folder: &str,
) -> Result<(), String> {
    let wanted = match profile.folders.get(system_id) {
        Some(ProfileFolder::Named(_)) => system_id,
        _ => profile
            .host_folders
            .get(system_id)
            .map_or(system_id, String::as_str),
    };
    match db.resolve_folder_in(Some(profile), folder.trim()) {
        Some(s) if s.id == wanted => Ok(()),
        Some(s) => Err(format!("`{folder}` is the folder of {} ({})", s.name, s.id)),
        None => Err(format!("`{folder}` is not a known system folder name")),
    }
}
