//! Files that travel with a set-aside copy, and files that stay because
//! the keeper shares them (duplicates spec §9.1). Matching comes in a later
//! step.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::profiles::Media;

/// What a travelling file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "media", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum TravelKind {
    Save,
    State,
    Cheat,
    Patch,
    Text,
    Art(Media),
    /// A macOS `._` twin.
    AppleDouble,
}

/// A file that leaves with the extra copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Travel {
    pub from: PathBuf,
    pub kind: TravelKind,
}

/// A companion with a stem shared with another member; it stays in place.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedStay {
    pub path: PathBuf,
    /// The member (`Member::id`) that keeps using it.
    pub with: usize,
}
