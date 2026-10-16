//! Fitting a profile to the folder the user actually picked.

use std::fs;
use std::path::Path;

use super::Profile;
use crate::scan::find_dir_ignoring_case;
use crate::systems::SystemDb;

/// The profile to scan `root` with.
///
/// Users often pick the ROMs folder itself (`…/ROMS`) instead of the SD
/// root. When `root` has no `roms_root` folder, every slot of the profile
/// lives inside the system folder (templates start with `{system_dir}`)
/// and a child of `root` is a known system folder, the returned profile
/// scans `root` directly (`roms_root = ""`). Otherwise `profile` is
/// returned unchanged. Read-only.
pub fn fit_to_root(profile: &Profile, root: &Path, systems: &SystemDb) -> Profile {
    let mut fitted = profile.clone();
    if profile.roms_root.is_empty() || find_dir_ignoring_case(root, &profile.roms_root).is_some() {
        return fitted;
    }
    let art_inside_system_dirs = profile
        .slots
        .values()
        .all(|slot| slot.template.starts_with("{system_dir}"));
    if !art_inside_system_dirs {
        return fitted;
    }
    let has_system_folder = fs::read_dir(root)
        .map(|entries| {
            entries.flatten().any(|e| {
                e.path().is_dir()
                    && systems
                        .resolve_folder(&e.file_name().to_string_lossy())
                        .is_some()
            })
        })
        .unwrap_or(false);
    if has_system_folder {
        fitted.roms_root = String::new();
    }
    fitted
}
