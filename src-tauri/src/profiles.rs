//! Layout profiles on the wire.

use std::collections::BTreeMap;

use coverly_core::profiles::custom::ProfileFile;
use coverly_core::profiles::Profile;
use coverly_core::settings::FieldError;
use serde::Serialize;

/// A built-in (read-only, duplicable) or custom profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProfileInfoView {
    pub profile: ProfileFile,
    pub builtin: bool,
    pub mirror_subdirs: bool,
}

impl ProfileInfoView {
    pub fn of(p: &Profile, builtin: bool) -> ProfileInfoView {
        ProfileInfoView {
            profile: ProfileFile::from_profile(p),
            builtin,
            mirror_subdirs: p.mirror_subdirs,
        }
    }
}

/// Every profile after a save, or why nothing was saved.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProfileSaveView {
    pub profiles: Vec<ProfileInfoView>,
    pub errors: Vec<FieldError>,
}

/// Where a profile being edited puts the art of an example ROM.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProfilePreviewView {
    /// `cover`/`screenshot`/`title` → library-relative path.
    pub paths: BTreeMap<String, String>,
    pub errors: Vec<FieldError>,
}
