//! Choosing the copy to keep (duplicates spec §6). The ordered rules come
//! in a later step; this holds the typed reason.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::identify::names::{Region, ReleaseClass};

/// Dump quality as weighed by the `verified` rule (spec §6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DumpQuality {
    /// The content hash is in the catalogue.
    DatMatch,
    /// Not in the catalogue, or no catalogue.
    Unknown,
    /// Tagged `[!]` or plain.
    Good,
    /// Tagged `[b]`.
    Bad,
    /// Tagged `[o]`.
    Overdump,
    /// No copier header (SNES, SMS/GG).
    Headerless,
    /// With a copier header.
    Headered,
}

/// Why the keeper of a group stays: the rule that decided and the values
/// it compared, never prose (spec §6.2). The UI and the CLI localise it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum KeepReason {
    UserChoice,
    HasSave {
        path: PathBuf,
    },
    Played {
        playcount: u32,
    },
    InLibrary,
    Verified {
        kept: DumpQuality,
        other: DumpQuality,
    },
    Retail {
        other: ReleaseClass,
    },
    Region {
        kept: Region,
        other: Region,
    },
    /// As written: `V1.1`, `Rev 1`.
    Revision {
        kept: String,
        other: String,
    },
    /// Containers as in the order (`chd`, `pbp`, `zip`, `raw`).
    Container {
        kept: String,
        other: String,
        profile: String,
    },
    HasCover,
    CatalogueName {
        name: String,
    },
    Tie,
}
