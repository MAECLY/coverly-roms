//! Units: what a duplicate is made of (duplicates spec §5.1). A unit is one
//! playable thing (a ROM, a zip, an arcade set, a disc set) and every file
//! that belongs to it. Building units from a scan comes in a later step.

use serde::{Deserialize, Serialize};

/// The shape of a unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum UnitKind {
    /// One loose ROM file.
    Single,
    /// A zip with one ROM inside.
    Zip,
    /// An arcade set (a zip named after the set).
    Arcade,
    /// A cue/gdi sheet and its tracks.
    DiscSet,
    /// A CHD image.
    Chd,
    /// A single-file disc image: pbp, cso, rvz, iso.
    Image,
    /// An m3u playlist over several discs.
    MultiDisc,
}
