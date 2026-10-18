//! Saves of a unit (duplicates spec §8.2, §8.3). Detection, the blank-card
//! check and the conflict test come in a later step.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// How a save was tied to its unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum SaveKey {
    /// Same file stem as the ROM.
    Stem,
    /// Named after the disc serial (DuckStation memory cards).
    Serial,
    /// Named after the arcade set.
    Set,
}

/// A real save or state of a unit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveInfo {
    pub path: PathBuf,
    pub size: u64,
    pub mtime_ns: i64,
    /// Lowercase hex.
    pub sha1: String,
    pub keyed_by: SaveKey,
}
