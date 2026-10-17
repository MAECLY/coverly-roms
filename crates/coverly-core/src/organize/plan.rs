//! Organise plan types (organise spec §11). This file holds the shared
//! vocabulary; the planner itself comes later.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::identify::evidence::{Evidence, IdLevel};
use crate::identify::sniff::NotARom;

/// How files reach their system folder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferMode {
    /// Rename in place (same volume) or copy, verify, then delete.
    #[default]
    Move,
    /// Leave the source untouched.
    Copy,
}

/// Whether organised files keep their name or take the catalogue name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NameMode {
    #[default]
    Keep,
    Catalogue,
}

/// What one plan item moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemKind {
    /// A raw ROM file.
    File,
    /// A zip/7z holding one game.
    Archive,
    /// Several files that travel together (`.cue` + `.bin`, `.m3u`, `.gdi`).
    Set,
}

/// Why a file stays where it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LeftReason {
    /// Still being downloaded (`.crdownload`, `.part` sibling, zero bytes).
    PartialDownload,
    /// Modified within the last `recent` seconds: may still be written.
    RecentlyModified,
    /// A cloud placeholder whose bytes are not on this disk.
    CloudOnly,
    /// A web page or text file with a ROM name.
    NotARom {
        what: NotARom,
    },
    /// BIOS or firmware: moving it into a game folder makes it a fake game.
    Bios,
    DamagedArchive,
    UnsupportedArchive,
    /// An archive holding several games.
    MultiGameArchive {
        count: usize,
    },
    /// A system Coverly recognises but does not organise.
    UnsupportedSystem {
        name: String,
    },
    /// ROM-like, but no level could place it.
    Unknown,
    /// Same content as another item of the plan.
    Duplicate {
        of: PathBuf,
    },
    /// The same content already sits in the target folder.
    AlreadyPresent {
        at: PathBuf,
    },
    /// Already inside a folder that resolves to a system.
    AlreadySorted {
        folder: String,
    },
    /// Symlinks, junctions and other reparse points are never followed.
    Symlink,
    /// A disc set whose listed files are missing.
    SetIncomplete {
        missing: Vec<String>,
    },
    /// A disc set that lists files outside its folder.
    SetOutsideFolder,
    /// A track referenced by more than one disc set.
    SharedTrack,
    /// A companion whose stem matches several ROMs.
    AmbiguousCompanion,
    /// Larger than 4 GiB on a FAT32 target.
    TooBigForFat,
    Unreadable,
    /// One file of a set collides at the target: the whole set stays.
    CollisionSet,
}

/// Something worth telling about an item that still moves (data for the
/// UI, never prose).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ItemNote {
    /// A zip with several ROMs of one system: most cores load the first.
    MultiRomZip { count: usize },
    /// The target folder does not exist on the chosen device layout.
    NoFolderOnDevice { folder: String },
    /// The extension says one system, the contents do not look like it.
    ExtensionContradicted { ext: String },
    /// The system's DAT could not be consulted (offline, cap, none).
    CatalogueMissing { system: String },
    /// An archive entry is encrypted: its header was not read.
    Encrypted,
}

/// Why an item's target name differs from its source name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RenameReason {
    /// `name` was taken in the target folder (on disk or by another
    /// item): the item got the next free ` (n)` name.
    Collision { name: String },
    /// Characters FAT/exFAT cannot store were replaced (`from` is the
    /// name before).
    SafeName { from: String },
    /// The stem was shortened so the full path fits Windows' 259 units.
    PathLength,
    /// Renamed to the DAT game name (*Names: catalogue*).
    Catalogue { from: String },
}

/// A save, patch or note that follows its ROM.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Companion {
    /// Relative to the source root.
    pub source: PathBuf,
    /// Relative to the target root.
    pub target: PathBuf,
}

/// Size and mtime of a source when it was planned: apply skips items
/// whose source changed since.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFp {
    pub size: u64,
    pub mtime_ns: i64,
}

/// What the plan proposes for an item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum OrganizeAction {
    /// Moved without asking.
    Auto,
    /// Moved once the user confirms the system.
    Review,
    /// Stays where it is.
    Left { reason: LeftReason },
}

/// One game of the plan: a file, an archive or a disc set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrganizeItem {
    pub id: usize,
    /// Relative to the source root; the primary file first.
    pub sources: Vec<PathBuf>,
    pub kind: ItemKind,
    pub size: u64,
    /// One per source, for re-validation at apply.
    pub fps: Vec<SourceFp>,
    /// Relative to the target root (the primary file).
    pub target: Option<PathBuf>,
    /// The other members of a set, in the order of `sources[1..]`.
    pub set_targets: Vec<PathBuf>,
    pub system_id: Option<String>,
    /// The system folder name actually used (an existing folder keeps
    /// its casing).
    pub folder: Option<String>,
    pub confidence: f32,
    pub method: Option<IdLevel>,
    pub why: Vec<Evidence>,
    pub alternatives: Vec<(String, f32)>,
    pub rename: Option<RenameReason>,
    pub notes: Vec<ItemNote>,
    pub companions: Vec<Companion>,
    pub catalogue_name: Option<String>,
    #[serde(flatten)]
    pub action: OrganizeAction,
}

impl OrganizeItem {
    /// The item stays in place.
    pub fn is_left(&self) -> bool {
        matches!(self.action, OrganizeAction::Left { .. })
    }

    /// Leaves the item in place with `reason` and drops its targets.
    pub fn leave(&mut self, reason: LeftReason) {
        self.action = OrganizeAction::Left { reason };
        self.target = None;
        self.set_targets.clear();
        self.companions.clear();
        self.rename = None;
    }
}
