//! Organise plan types (organise spec §11) and the planner: the survey of
//! a pile (walk, disc sets, identification on a few worker threads,
//! memoised facts, the duplicate bucket), the plan built from a survey
//! with the user's decisions, and the cheap name-only folder probe.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::archive::names::UnsafeWhy;
use crate::archive::{ContainerKind, Limits, Tool, ToolsOnDemand};
use crate::cache::Cache;
use crate::dedupe::source::DiscId;
use crate::identify::dat::DatStore;
use crate::identify::evidence::{Evidence, IdLevel};
use crate::identify::sniff::{NotARom, Sniff};
use crate::organize::apply::{cluster_size, user_available};
use crate::organize::evidence::{
    identify_candidate, DatCatalogue, Facts, Identification, LadderEnv, OnlineLookup, Thresholds,
    Verdict, MAX_DAT_DOWNLOADS,
};
use crate::organize::inspect::{
    inspect_file_with, inspect_set_primary, is_metadata_entry, ArchiveIssue, Container, Inspection,
    Payload, ZipEntryInfo,
};
use crate::organize::target::{
    assign_targets_with_extracts, default_companion_exts, file_system_is_fat32, limit_fat_folders,
    mark_duplicates, DirIndex, ExtractShape, TargetLayout,
};
use crate::organize::walk::{
    is_broad_root, walk, MetaProbe, WalkCandidate, WalkOptions, Walked, MAX_DEPTH,
};
use crate::profiles::{detect, Profile};
use crate::scan::classify::{
    extension, is_appledouble, is_hidden, is_junk_file, is_partial_download, is_zone_identifier,
};
use crate::scan::multifile::{group_sets, is_list_extension};
use crate::scan::{find_dir_ignoring_case, ScanOptions, ScanWarning};
use crate::settings::{ExternalTools, OrganizeModeSetting, Settings};
use crate::systems::{System, SystemDb};

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
    /// A container this build cannot open; `need` names the external
    /// tool that would (fields absent in plans from before archives).
    UnsupportedArchive {
        #[serde(default)]
        format: Option<String>,
        #[serde(default)]
        need: Option<Tool>,
    },
    /// The archive's header or every payload is password protected.
    EncryptedArchive,
    /// Decoding needs more memory than the cap (S2, S3).
    ArchiveNeedsMemory {
        mib: u64,
    },
    /// Too many entries or too many declared bytes to open safely; never
    /// damage (D1).
    ArchiveTooBig {
        entries: Option<u64>,
        bytes: Option<u64>,
    },
    /// One part of a multi-volume archive.
    MultiVolumeArchive {
        parts: usize,
    },
    /// A needed entry has an unsafe path (zip-slip, device name, link…).
    UnsafeArchiveEntry {
        name: String,
        why: UnsafeWhy,
    },
    /// A set member's name the target cannot store unchanged (sheets
    /// reference it, so it is never renamed).
    SetUnsafeName {
        name: String,
    },
    /// A target or temp path over the Windows limit.
    PathTooLong,
    /// The only payload is another archive.
    NestedArchive,
    /// An arcade set in a container other than zip.
    RepackArcade,
    /// The FAT32 folder has no directory slots left.
    FolderFull {
        folder: String,
    },
    /// The target folder is a symlink or junction.
    UnsafeTarget,
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

/// The reason an inspection issue leaves an item in place (archives spec
/// §4.2): size caps are never damage, and a spent budget is no issue at all
/// (the item goes to Review with `ItemNote::ArchiveNotFullyRead`).
pub fn left_reason_for(issue: &ArchiveIssue) -> LeftReason {
    match issue {
        ArchiveIssue::Damaged => LeftReason::DamagedArchive,
        ArchiveIssue::Unsupported { format, need } => LeftReason::UnsupportedArchive {
            format: format.clone(),
            need: *need,
        },
        ArchiveIssue::Encrypted => LeftReason::EncryptedArchive,
        ArchiveIssue::NeedsMemory { mib } => LeftReason::ArchiveNeedsMemory { mib: *mib },
        ArchiveIssue::TooBig { entries, bytes } => LeftReason::ArchiveTooBig {
            entries: *entries,
            bytes: *bytes,
        },
        ArchiveIssue::MultiVolume { parts } => LeftReason::MultiVolumeArchive { parts: *parts },
        ArchiveIssue::Nested => LeftReason::NestedArchive,
        ArchiveIssue::NotARom { what } => LeftReason::NotARom { what: *what },
        ArchiveIssue::TooManyRoms { count } | ArchiveIssue::SeveralGames { count } => {
            LeftReason::MultiGameArchive { count: *count }
        }
        ArchiveIssue::UnsafeEntry { name, why } => LeftReason::UnsafeArchiveEntry {
            name: name.clone(),
            why: *why,
        },
        ArchiveIssue::SetIncomplete { missing } => LeftReason::SetIncomplete {
            missing: missing.clone(),
        },
        ArchiveIssue::SetOutsideFolder => LeftReason::SetOutsideFolder,
        ArchiveIssue::SharedTrack => LeftReason::SharedTrack,
    }
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
    /// The magic says another container than the extension (`actual` is
    /// the format's short name, `7z`).
    ContainerMismatch { ext: String, actual: String },
    /// A `.gdi` and a `.cue` for the same tracks: both are extracted.
    TwinSheets { kept: Vec<String> },
    /// The format stores no CRC: only the size is checked.
    NoStoredChecksum,
    /// The decode budget ran out during the dry run.
    ArchiveNotFullyRead,
    /// Move mode keeps the archive: it holds files that are not extracted.
    ArchiveKeptLeftovers { count: usize, examples: Vec<String> },
    /// The sheet sits beyond the plan-time budget: checked on apply.
    SheetNotReadYet,
    /// Several ROMs of one system are extracted from one archive.
    MultiRomExtract { count: usize },
}

/// How an item reaches its target (archives spec §8.1).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Transfer {
    /// The file moves or is copied as it is.
    #[default]
    AsIs,
    /// The listed entries are extracted; `m3u` is a generated playlist
    /// (relative to the target root).
    Extract {
        entries: Vec<ExtractEntry>,
        bytes: u64,
        archive_after: ArchiveAfter,
        #[serde(default)]
        source: ExtractSource,
        #[serde(default)]
        sheet_check: SheetCheck,
        #[serde(default)]
        m3u: Option<PathBuf>,
    },
}

/// One archive entry an `Extract` item writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractEntry {
    /// Position in the archive's own order.
    pub index: usize,
    /// The validated entry path (`RelPath`), `/`-separated.
    pub name: String,
    pub size: u64,
    /// Stored CRC32 when the format has one.
    pub crc32: Option<u32>,
    /// Relative to the target root.
    pub target: PathBuf,
}

/// What happens to the archive once every entry is verified.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArchiveAfter {
    /// Copy mode, or move mode with leftovers.
    Keep,
    /// Move mode with nothing left behind: sent to the Trash.
    Trash,
}

/// Who decodes the archive on apply: the same as at plan time.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExtractSource {
    #[default]
    Builtin,
    /// An external tool, by kind and the version it reported.
    Tool { tool: Tool, version: String },
}

/// Whether a disc set's sheet was parsed at plan time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SheetCheck {
    #[default]
    Read,
    /// Inferred from the listing; parsed and checked on apply.
    Deferred,
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
    /// As-is (default, plans from before archives) or extract.
    #[serde(default)]
    pub transfer: Transfer,
    /// The disc's serials and fingerprint, for the UI and dedupe level 3.
    #[serde(default)]
    pub disc: Option<DiscId>,
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
        self.transfer = Transfer::AsIs;
    }
}

// --- The planner (organise spec §6.7, §7, §11) ---

/// Layout used when none is chosen: the owner's Anbernic stock card.
pub const DEFAULT_LAYOUT: &str = "anbernic-stock";

/// Entries [`probe_folder`] looks at before it stops (`truncated`).
pub const PROBE_LIMIT: usize = 5_000;

/// Full-file hashes the ladder ran (test builds only): the perf gate
/// checks that headers alone decide a big clean pile.
#[cfg(any(test, feature = "test-util"))]
pub static HASH_CALLS: AtomicUsize = AtomicUsize::new(0);

/// Worker threads identifying candidates on internal disks; removable and
/// network volumes get [`SLOW_VOLUME_WORKERS`] (plan O-D2).
const WORKERS: usize = 4;
const SLOW_VOLUME_WORKERS: usize = 2;
/// Identify progress is reported at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Knobs of one plan.
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizeOptions {
    /// Profile id of the target layout.
    pub layout: String,
    /// Where the system folders go; `None` = the picked folder.
    pub target: Option<PathBuf>,
    /// *Directly here*: system folders straight in the target, without
    /// the layout's ROMs folder.
    pub flat: bool,
    /// `None` = move on the same volume, copy across volumes.
    pub mode: Option<TransferMode>,
    pub names: NameMode,
    /// Cached DATs only; no download, no online lookup.
    pub offline: bool,
    pub thresholds: Thresholds,
    /// Lower-case extensions that follow their ROM (`state*` = prefix).
    pub companion_exts: Vec<String>,
    /// System id → folder name overrides for the chosen layout.
    pub folder_names: BTreeMap<String, String>,
    /// Files modified more recently than this may still be written.
    pub recent_secs: u64,
    /// Overrides the layout's arcade preference order.
    pub arcade_order: Option<Vec<String>>,
    /// Download missing DATs (up to [`MAX_DAT_DOWNLOADS`] per plan).
    pub download_dats: bool,
    /// Bounds for every archive the plan opens (`external_tools` follows
    /// the setting `organize.external_tools`).
    pub limits: Limits,
}

impl Default for OrganizeOptions {
    fn default() -> Self {
        OrganizeOptions {
            layout: DEFAULT_LAYOUT.to_string(),
            target: None,
            flat: false,
            mode: None,
            names: NameMode::Keep,
            offline: false,
            thresholds: Thresholds::default(),
            companion_exts: default_companion_exts(),
            folder_names: BTreeMap::new(),
            recent_secs: 60,
            arcade_order: None,
            download_dats: true,
            limits: Limits::default(),
        }
    }
}

impl OrganizeOptions {
    /// The options `settings.organize` asks for; `layout` (a per-run
    /// choice) replaces `default_layout`, and the folder names and arcade
    /// order are the ones stored for the chosen layout. `target` and
    /// `offline` are per run and stay at their defaults.
    pub fn from_settings(settings: &Settings, layout: Option<&str>) -> OrganizeOptions {
        let o = &settings.organize;
        let layout = layout
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .or_else(|| Some(o.default_layout.trim()).filter(|l| !l.is_empty()))
            .unwrap_or(DEFAULT_LAYOUT)
            .to_string();
        OrganizeOptions {
            flat: !o.nest_in_roms_root,
            mode: match o.mode {
                OrganizeModeSetting::Auto => None,
                OrganizeModeSetting::Move => Some(TransferMode::Move),
                OrganizeModeSetting::Copy => Some(TransferMode::Copy),
            },
            names: o.names,
            thresholds: Thresholds {
                auto: o.auto_threshold,
                review: o.review_threshold,
            },
            companion_exts: o.companion_exts.clone(),
            folder_names: o.folder_names.get(&layout).cloned().unwrap_or_default(),
            recent_secs: u64::from(o.recent_seconds),
            arcade_order: o
                .arcade_order
                .get(&layout)
                .filter(|order| !order.is_empty())
                .cloned(),
            download_dats: o.download_dats,
            limits: Limits {
                external_tools: o.external_tools == ExternalTools::Auto,
                ..Limits::default()
            },
            layout,
            ..OrganizeOptions::default()
        }
    }
}

/// Everything the planner consults besides the pile and the options.
#[derive(Clone, Copy)]
pub struct OrganizeEnv<'a> {
    pub db: &'a SystemDb,
    /// Built-in and custom profiles the layout is chosen from.
    pub profiles: &'a [Profile],
    pub cache: &'a Cache,
    /// Downloads missing DATs; `None` = cached DATs only.
    pub dats: Option<&'a DatStore<'a>>,
    pub probe: &'a dyn MetaProbe,
    pub online: Option<&'a dyn OnlineLookup>,
    pub now: SystemTime,
}

/// Which part of planning is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrganizePhase {
    Walk,
    Identify,
    Dats,
    Hash,
    /// Moving or copying the items (`organize::apply`).
    Apply,
    /// Extracting archive item `item` (its [`OrganizeItem::id`]):
    /// `bytes_done` of the `bytes_total` its decode pass goes through (a
    /// solid block's other entries included). The first event of an item
    /// has `bytes_done == 0` and its last `bytes_done == bytes_total`.
    Extract {
        item: usize,
        bytes_done: u64,
        bytes_total: u64,
    },
}

/// Planning progress: `done` of `total` in `phase`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizeProgress {
    pub phase: OrganizePhase,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum OrganizeError {
    #[error("source not found: {0}")]
    SourceMissing(PathBuf),
    #[error("cannot read {0}: {1}")]
    Unreadable(PathBuf, String),
    #[error("unknown layout `{0}`")]
    UnknownLayout(String),
    #[error("unknown system `{0}`")]
    UnknownSystem(String),
    #[error("cancelled")]
    Cancelled,
    #[error("item {0} is not in the plan")]
    UnknownItem(usize),
}

/// The user's answer for one item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub item: usize,
    #[serde(flatten)]
    pub choice: Choice,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "choice", rename_all = "kebab-case")]
pub enum Choice {
    /// Move to the proposed system.
    Accept,
    /// Move to this system (remembered by content).
    Assign { system: String },
    /// Keep it where it is.
    Leave,
}

/// Totals of a plan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    /// Items that move (automatically or once reviewed).
    pub roms: usize,
    /// Systems those items go to.
    pub systems: usize,
    pub auto: usize,
    pub review: usize,
    pub left: usize,
    pub junk: usize,
    pub other_files: usize,
    pub deep_folders: usize,
    /// Bytes of the items that move.
    pub bytes: u64,
}

/// Something about the whole plan (data, never prose).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PlanWarning {
    /// The system's DAT could not be consulted.
    CatalogueMissing { system: String },
    /// The target already looks like another layout.
    TargetLayoutMismatch { profile: String },
    /// The source cannot be written: files are copied, not moved.
    MoveUnavailable,
    /// Nothing loose: every ROM already sits in a system folder.
    SortedOnly,
    /// The target volume lacks room for what the plan writes: `needed`
    /// (cluster-rounded bytes, 1 % and the reserve) against the bytes
    /// `free` for this user (archives spec §8.4).
    NoSpace { needed: u64, free: u64 },
}

/// A dry run: what would move where, and why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrganizePlan {
    /// Absolute.
    pub source_root: PathBuf,
    /// Absolute.
    pub target_root: PathBuf,
    /// Absolute: the folder holding the system folders.
    pub roms_dir: PathBuf,
    pub layout: String,
    pub mode: TransferMode,
    pub names: NameMode,
    pub items: Vec<OrganizeItem>,
    pub counts: Counts,
    /// Relative to the source root, with the system they resolve to.
    pub sorted_folders: Vec<(PathBuf, String)>,
    /// Relative to the source root.
    pub deep_folders: Vec<PathBuf>,
    /// Relative to the source root.
    pub other_files: Vec<PathBuf>,
    pub warnings: Vec<PlanWarning>,
    pub broad_root: bool,
    pub fat32: bool,
    pub move_allowed: bool,
    /// The layout the target already looks like, when it is not the
    /// chosen one.
    pub target_detected: Option<String>,
    pub created_at: String,
}

/// One future plan item as the survey found it.
#[derive(Clone, Debug)]
pub struct SurveyEntry {
    /// Absolute, primary file first.
    pub sources: Vec<PathBuf>,
    pub kind: ItemKind,
    pub size: u64,
    pub fps: Vec<SourceFp>,
    /// Left before identification (walk, broken disc set, unreadable).
    pub left: Option<LeftReason>,
    pub identification: Option<Identification>,
    /// What opening an archive found (archives only): the listing, the
    /// disc set and the tool an extraction plan needs.
    pub inspection: Option<Inspection>,
}

/// What a pile holds and what each candidate is: everything a plan needs
/// except the user's options for the target and decisions, so changing
/// those re-plans without reading files.
#[derive(Clone, Debug)]
pub struct Survey {
    /// Absolute.
    pub source_root: PathBuf,
    /// Absolute.
    pub target_root: PathBuf,
    pub broad_root: bool,
    pub entries: Vec<SurveyEntry>,
    /// The walk's lists (candidates were turned into `entries`).
    pub walked: Walked,
}

/// What a quick, name-only look at a folder found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderProbe {
    pub rom_like: usize,
    pub partial_downloads: usize,
    pub cloud_only: usize,
    /// Names of folders that resolve to a system.
    pub sorted_folders: Vec<String>,
    pub writable: bool,
    pub broad_root: bool,
    /// Stopped after [`PROBE_LIMIT`] entries.
    pub truncated: bool,
}

/// The layout profile `opts.layout`, the target root and the ROMs folder.
fn resolve_layout(
    source: &Path,
    opts: &OrganizeOptions,
    env: &OrganizeEnv,
) -> Result<(Profile, PathBuf, PathBuf), OrganizeError> {
    let profile = env
        .profiles
        .iter()
        .find(|p| p.id == opts.layout)
        .cloned()
        .ok_or_else(|| OrganizeError::UnknownLayout(opts.layout.clone()))?;
    let target = match &opts.target {
        Some(t) => std::path::absolute(t).unwrap_or_else(|_| t.clone()),
        None => source.to_path_buf(),
    };
    let roms_dir = roms_dir_of(&target, &profile, opts.flat);
    Ok((profile, target, roms_dir))
}

fn roms_dir_of(target: &Path, profile: &Profile, flat: bool) -> PathBuf {
    if flat || profile.roms_root.is_empty() {
        target.to_path_buf()
    } else {
        find_dir_ignoring_case(target, &profile.roms_root)
            .unwrap_or_else(|| target.join(&profile.roms_root))
    }
}

/// `inner` is `outer` or below it (lexically).
fn is_within(inner: &Path, outer: &Path) -> bool {
    inner.starts_with(outer)
}

/// `member` is a symlink or reparse point itself, or reaches its file
/// through one: a folder between `source` and `member` that is a link.
fn reached_through_link(member: &Path, source: &Path) -> bool {
    let is_link = |p: &Path| {
        fs::symlink_metadata(p)
            .is_ok_and(|m| m.file_type().is_symlink() || super::walk::is_reparse_point(&m))
    };
    member
        .ancestors()
        .take_while(|p| *p != source && p.starts_with(source))
        .any(is_link)
}

/// `member` really lives under `source` once every link and `..` is
/// resolved. Anything that cannot be resolved counts as outside.
fn really_within(member: &Path, source: &Path) -> bool {
    match (fs::canonicalize(member), fs::canonicalize(source)) {
        (Ok(m), Ok(s)) => m.starts_with(s),
        _ => false,
    }
}

fn mtime_ns_of(meta: &fs::Metadata) -> i64 {
    match meta
        .modified()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
    {
        Ok(d) => i64::try_from(d.as_nanos()).unwrap_or(i64::MAX),
        Err(e) => i64::try_from(e.duration().as_nanos()).map_or(i64::MIN, |n| -n),
    }
}

fn fp_of(path: &Path) -> SourceFp {
    fs::symlink_metadata(path).map_or(
        SourceFp {
            size: 0,
            mtime_ns: 0,
        },
        |m| SourceFp {
            size: m.len(),
            mtime_ns: mtime_ns_of(&m),
        },
    )
}

/// Walks, groups disc sets and identifies every candidate of `source`.
pub fn survey(
    source: &Path,
    opts: &OrganizeOptions,
    env: &OrganizeEnv,
    progress: &(dyn Fn(OrganizeProgress) + Sync),
    cancel: &AtomicBool,
) -> Result<Survey, OrganizeError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(OrganizeError::Cancelled);
    }
    let source_abs = std::path::absolute(source).unwrap_or_else(|_| source.to_path_buf());
    match fs::metadata(&source_abs) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(OrganizeError::SourceMissing(source_abs)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(OrganizeError::SourceMissing(source_abs))
        }
        Err(e) => return Err(OrganizeError::Unreadable(source_abs, e.to_string())),
    }
    let (profile, target_root, roms_dir) = resolve_layout(&source_abs, opts, env)?;
    let broad_root = is_broad_root(source) || is_broad_root(&source_abs);

    // 1. The read-only walk.
    progress(OrganizeProgress {
        phase: OrganizePhase::Walk,
        done: 0,
        total: 0,
    });
    let skip_dirs = if target_root != source_abs && is_within(&target_root, &source_abs) {
        vec![target_root.clone()]
    } else {
        Vec::new()
    };
    let roms_root =
        (roms_dir != source_abs && is_within(&roms_dir, &source_abs)).then(|| roms_dir.clone());
    let ignored = ScanOptions::default().ignored_dirs;
    let mut walked = walk(
        &source_abs,
        env.db,
        &WalkOptions {
            skip_dirs,
            roms_root,
            art_dirs: &profile.art_dir_names,
            ignored_dirs: &ignored,
            now: env.now,
            recent: Duration::from_secs(opts.recent_secs),
            probe: env.probe,
            max_depth: MAX_DEPTH,
        },
    )
    .map_err(|e| OrganizeError::Unreadable(source_abs.clone(), e.to_string()))?;
    progress(OrganizeProgress {
        phase: OrganizePhase::Walk,
        done: walked.candidates.len(),
        total: walked.candidates.len(),
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(OrganizeError::Cancelled);
    }

    // 2. Disc sets, then one entry per candidate or set.
    let candidates = std::mem::take(&mut walked.candidates);
    let mut entries = build_entries(&source_abs, candidates, &mut walked);
    for (path, reason) in std::mem::take(&mut walked.left) {
        entries.push(Pending {
            entry: SurveyEntry {
                fps: vec![fp_of(&path)],
                size: fs::symlink_metadata(&path).map_or(0, |m| m.len()),
                sources: vec![path],
                kind: ItemKind::File,
                left: Some(reason),
                identification: None,
                inspection: None,
            },
            candidate: None,
            set_sniff: None,
        });
    }
    entries.sort_by(|a, b| a.entry.sources[0].cmp(&b.entry.sources[0]));

    // 3. The identification ladder on a few workers.
    let store = if opts.offline || !opts.download_dats {
        None
    } else {
        env.dats
    };
    let dat_progress = |done: usize, total: usize| {
        progress(OrganizeProgress {
            phase: OrganizePhase::Dats,
            done,
            total,
        });
    };
    let catalogue = DatCatalogue::new(env.db, env.cache, store, MAX_DAT_DOWNLOADS)
        .with_download_progress(&dat_progress);
    let arcade_order = opts
        .arcade_order
        .clone()
        .unwrap_or_else(|| profile.arcade_order.clone());
    let ladder = LadderEnv {
        db: env.db,
        catalogue: &catalogue,
        cache: env.cache,
        thresholds: opts.thresholds,
        broad_root,
        need_hash: false,
        arcade_order: &arcade_order,
        online: if opts.offline { None } else { env.online },
    };
    let todo: Vec<usize> = (0..entries.len())
        .filter(|&i| entries[i].entry.left.is_none())
        .collect();
    let workers = if crate::write::on_removable_volume(&source_abs) {
        SLOW_VOLUME_WORKERS
    } else {
        WORKERS
    };
    // External tools are looked for only when an archive needs one, and
    // never in a `PATH` entry inside the source or target root (spec §5.2).
    let tools = ToolsOnDemand::new(&[&source_abs, &target_root]);
    let results = identify_all(
        &entries,
        &todo,
        workers,
        (env, &ladder, &opts.limits, &tools),
        progress,
        cancel,
    )?;
    let mut facts_of: HashMap<usize, Facts> = HashMap::new();
    let mut volume_sets: Vec<Vec<PathBuf>> = Vec::new();
    for (i, outcome) in results {
        match outcome {
            Ok((facts, id)) => {
                if !facts.inspection.volume_parts.is_empty() {
                    volume_sets.push(facts.inspection.volume_parts.clone());
                }
                let entry = &mut entries[i].entry;
                entry.identification = Some(id);
                // Any container the reader opened is an archive item (a
                // `.tar.gz` or a `game.gba.gz` too), extracted or moved.
                if facts.inspection.container.is_archive() && entry.kind == ItemKind::File {
                    entry.kind = ItemKind::Archive;
                }
                if entry.kind == ItemKind::Archive {
                    entry.inspection = Some(facts.inspection.clone());
                }
                facts_of.insert(i, facts);
            }
            Err(left) => entries[i].entry.left = Some(left),
        }
    }

    // 4. Duplicate bucket: raw files sharing size and prefix are hashed so
    //    duplicates can be told apart.
    //    A raw file as large as an archived ROM is hashed too, so a loose
    //    copy of a zipped game is seen as the same content.
    let archived_sizes: BTreeSet<u64> = entries
        .iter()
        .filter(|p| p.entry.kind == ItemKind::Archive)
        .filter_map(|p| p.entry.identification.as_ref()?.content_key.as_ref())
        .map(|(_, size)| *size)
        .collect();
    let mut buckets: HashMap<(u64, String), Vec<usize>> = HashMap::new();
    let mut lone: Vec<usize> = Vec::new();
    for (&i, facts) in &facts_of {
        let entry = &entries[i].entry;
        let Some(id) = &entry.identification else {
            continue;
        };
        if entry.kind == ItemKind::Set
            || facts.inspection.container != Container::Raw
            || id.left.is_some()
            || id.content_key.is_some()
        {
            continue;
        }
        if archived_sizes.contains(&entry.size) {
            lone.push(i);
        } else if let Some(head) = &facts.inspection.head_crc {
            buckets
                .entry((entry.size, head.clone()))
                .or_default()
                .push(i);
        }
    }
    let mut to_hash: Vec<usize> = buckets
        .into_values()
        .filter(|g| g.len() > 1)
        .flatten()
        .chain(lone)
        .collect();
    to_hash.sort_unstable();
    if !to_hash.is_empty() {
        let hashing = LadderEnv {
            need_hash: true,
            ..ladder
        };
        for (done, &i) in to_hash.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(OrganizeError::Cancelled);
            }
            let Some(mut facts) = facts_of.remove(&i) else {
                continue;
            };
            let candidate = entries[i]
                .candidate
                .clone()
                .expect("raw entries come from candidates");
            let id = identify_one(&candidate, &mut facts, &hashing);
            remember_facts(env.cache, &candidate, &facts);
            entries[i].entry.identification = Some(id);
            progress(OrganizeProgress {
                phase: OrganizePhase::Hash,
                done: done + 1,
                total: to_hash.len(),
            });
        }
    }

    let mut entries: Vec<SurveyEntry> = entries.into_iter().map(|p| p.entry).collect();
    gather_volume_sets(&mut entries, &volume_sets, &mut walked);

    Ok(Survey {
        source_root: source_abs,
        target_root,
        broad_root,
        entries,
        walked,
    })
}

/// Turns every multi-volume set into one Left entry holding all its parts
/// (archives spec §3.2 rule 6): the entry of the set's first part found
/// (in volume order) takes the others' files, which never become items,
/// companions or other files of their own, so no part moves apart.
fn gather_volume_sets(entries: &mut Vec<SurveyEntry>, sets: &[Vec<PathBuf>], walked: &mut Walked) {
    let mut gone: BTreeSet<PathBuf> = BTreeSet::new();
    for parts in sets {
        if parts.iter().any(|p| gone.contains(p)) {
            continue;
        }
        let Some(owner) = parts
            .iter()
            .find_map(|p| entries.iter().position(|e| e.sources[0] == *p))
        else {
            continue;
        };
        let present: Vec<PathBuf> = parts.iter().filter(|p| p.is_file()).cloned().collect();
        let entry = &mut entries[owner];
        entry.fps = present.iter().map(|p| fp_of(p)).collect();
        entry.size = entry.fps.iter().map(|fp| fp.size).sum();
        entry.sources = present;
        entry.kind = ItemKind::Archive;
        entry.left = Some(LeftReason::MultiVolumeArchive { parts: parts.len() });
        entry.identification = None;
        entry.inspection = None;
        gone.extend(entry.sources.iter().skip(1).cloned());
    }
    if gone.is_empty() {
        return;
    }
    entries.retain(|e| e.sources.len() > 1 || !gone.contains(&e.sources[0]));
    walked.other_files.retain(|p| !gone.contains(p));
    walked.companions.retain(|p| !gone.contains(p));
    walked.left.retain(|(p, _)| !gone.contains(p));
}

/// An entry being surveyed: the candidate behind it (single files) or the
/// header of its data track (disc sets).
struct Pending {
    entry: SurveyEntry,
    candidate: Option<WalkCandidate>,
    set_sniff: Option<Sniff>,
}

/// Turns walk candidates into entries: list files (`.cue`, `.gdi`,
/// `.m3u`) take the files they name along as one set; broken sets stay.
fn build_entries(
    source: &Path,
    candidates: Vec<WalkCandidate>,
    walked: &mut Walked,
) -> Vec<Pending> {
    let paths: Vec<PathBuf> = candidates.iter().map(|c| c.path.clone()).collect();
    let mut warnings: Vec<ScanWarning> = Vec::new();
    let sets = group_sets(paths, &mut warnings);
    let by_path: HashMap<&Path, &WalkCandidate> =
        candidates.iter().map(|c| (c.path.as_path(), c)).collect();

    // Members referenced by more than one set.
    let mut uses: HashMap<String, usize> = HashMap::new();
    for (_, members) in &sets {
        for m in members {
            *uses.entry(m.to_string_lossy().to_lowercase()).or_default() += 1;
        }
    }
    let mut taken: BTreeSet<PathBuf> = BTreeSet::new();
    let mut out = Vec::new();
    for (primary, members) in sets {
        let Some(candidate) = by_path.get(primary.as_path()).map(|c| (*c).clone()) else {
            continue;
        };
        let is_list = is_list_extension(&candidate.ext);
        let mut entry = SurveyEntry {
            sources: vec![primary.clone()],
            kind: ItemKind::File,
            size: candidate.size,
            fps: vec![SourceFp {
                size: candidate.size,
                mtime_ns: candidate.mtime_ns,
            }],
            left: None,
            identification: None,
            inspection: None,
        };
        if !is_list {
            if candidate.ext == "zip" || candidate.ext == "7z" {
                entry.kind = ItemKind::Archive;
            }
            out.push(Pending {
                entry,
                candidate: Some(candidate),
                set_sniff: None,
            });
            continue;
        }
        // The list file and every file of its closure.
        let mut lists: Vec<PathBuf> = vec![primary.clone()];
        lists.extend(
            members
                .iter()
                .filter(|m| is_list_extension(&extension(m)))
                .cloned(),
        );
        let missing: Vec<String> = warnings
            .iter()
            .filter(|w| lists.contains(&w.path))
            .filter_map(|w| missing_name(&w.message))
            .collect();
        let base = primary.parent().unwrap_or(source).to_path_buf();
        for m in &members {
            entry.sources.push(m.clone());
            let fp = fp_of(m);
            entry.size += fp.size;
            entry.fps.push(fp);
            taken.insert(m.clone());
        }
        if !members.is_empty() {
            entry.kind = ItemKind::Set;
        }
        // A track the walk left (still being written, a symlink, a
        // partial download, unreadable) holds the whole set back.
        let member_left = members
            .iter()
            .find_map(|m| walked.left.iter().find(|(p, _)| p == m))
            .map(|(_, reason)| reason.clone());
        entry.left = if !missing.is_empty() {
            Some(LeftReason::SetIncomplete { missing })
        } else if let Some(reason) = member_left {
            Some(reason)
        } else if members.iter().any(|m| reached_through_link(m, source)) {
            Some(LeftReason::Symlink)
        } else if members
            .iter()
            .any(|m| !is_within(m, &base) || !is_within(m, source) || !really_within(m, source))
        {
            Some(LeftReason::SetOutsideFolder)
        } else if members
            .iter()
            .any(|m| uses.get(&m.to_string_lossy().to_lowercase()) > Some(&1))
        {
            Some(LeftReason::SharedTrack)
        } else {
            None
        };
        let set_sniff = if entry.left.is_none() && entry.kind == ItemKind::Set {
            members
                .iter()
                .find(|m| !is_list_extension(&extension(m)))
                .and_then(|track| inspect_set_primary(track).ok())
        } else {
            None
        };
        out.push(Pending {
            entry,
            candidate: Some(candidate),
            set_sniff,
        });
    }
    // Files that travel with a set are not companions or other files.
    walked.companions.retain(|p| !taken.contains(p));
    walked.other_files.retain(|p| !taken.contains(p));
    walked.left.retain(|(p, _)| !taken.contains(p));
    out
}

/// `x` of the multifile warning "referenced file `x` not found".
fn missing_name(message: &str) -> Option<String> {
    message
        .strip_prefix("referenced file `")?
        .strip_suffix("` not found")
        .map(str::to_string)
}

type Outcome = Result<(Facts, Identification), LeftReason>;

/// Identifies `todo` (indices into `entries`) on `workers` scoped threads
/// pulling indices from a shared counter.
fn identify_all(
    entries: &[Pending],
    todo: &[usize],
    workers: usize,
    (env, ladder, limits, tools): (&OrganizeEnv, &LadderEnv, &Limits, &ToolsOnDemand),
    progress: &(dyn Fn(OrganizeProgress) + Sync),
    cancel: &AtomicBool,
) -> Result<Vec<(usize, Outcome)>, OrganizeError> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let total = todo.len();
    let last = Mutex::new(Instant::now());
    progress(OrganizeProgress {
        phase: OrganizePhase::Identify,
        done: 0,
        total,
    });
    let results: Mutex<Vec<(usize, Outcome)>> = Mutex::new(Vec::with_capacity(total));
    std::thread::scope(|scope| {
        for _ in 0..workers.clamp(1, total.max(1)) {
            scope.spawn(|| loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let k = next.fetch_add(1, Ordering::Relaxed);
                let Some(&i) = todo.get(k) else {
                    break;
                };
                let outcome = identify_entry(&entries[i], env, ladder, (limits, tools));
                results
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((i, outcome));
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                let mut at = last.lock().unwrap_or_else(|e| e.into_inner());
                if n == total || at.elapsed() >= PROGRESS_EVERY {
                    *at = Instant::now();
                    progress(OrganizeProgress {
                        phase: OrganizePhase::Identify,
                        done: n,
                        total,
                    });
                }
            });
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(OrganizeError::Cancelled);
    }
    let mut results = results.into_inner().unwrap_or_else(|e| e.into_inner());
    results.sort_by_key(|(i, _)| *i);
    Ok(results)
}

/// Facts (memoised or freshly inspected) and the ladder's verdict for one
/// entry.
fn identify_entry(
    pending: &Pending,
    env: &OrganizeEnv,
    ladder: &LadderEnv,
    (limits, tools): (&Limits, &ToolsOnDemand),
) -> Outcome {
    let candidate = pending
        .candidate
        .as_ref()
        .expect("entries to identify come from candidates");
    let mut facts = if pending.entry.kind == ItemKind::Set {
        set_facts(candidate, &pending.entry, pending.set_sniff.clone())
    } else if candidate.cloud_only {
        name_only_facts(candidate)
    } else {
        match memoised_facts(env.cache, candidate) {
            Some(facts) => facts,
            None => {
                let inspection =
                    inspect_file_with(&candidate.path, candidate.size, env.db, limits, Some(tools))
                        .map_err(|_| LeftReason::Unreadable)?;
                Facts {
                    inspection,
                    raw_crcs: Vec::new(),
                    hashed: false,
                }
            }
        }
    };
    let id = identify_one(candidate, &mut facts, ladder);
    if pending.entry.kind != ItemKind::Set && !candidate.cloud_only && memoisable(&facts) {
        remember_facts(env.cache, candidate, &facts);
    }
    Ok((facts, id))
}

fn identify_one(
    candidate: &WalkCandidate,
    facts: &mut Facts,
    ladder: &LadderEnv,
) -> Identification {
    let name = candidate
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    identify_candidate(
        &candidate.path,
        &name,
        candidate.size,
        facts,
        candidate.cloud_only,
        ladder,
    )
}

/// Version of the memoised facts: facts from before the archive reader
/// (no version) judged a 7z named `.zip` damaged, facts of version 1
/// judged a disc archive by its cartridge rules, and facts of version 2
/// lack the entry indices, the discs and the tool an extraction needs, so
/// they are read again.
const FACTS_VERSION: u32 = 4;

/// Facts as memoised, with their version.
#[derive(Serialize, Deserialize)]
struct StoredFacts {
    #[serde(default)]
    v: u32,
    #[serde(flatten)]
    facts: Facts,
}

fn memoised_facts(cache: &Cache, c: &WalkCandidate) -> Option<Facts> {
    let json = cache.organize_facts(&c.path, c.size, c.mtime_ns).ok()??;
    let stored: StoredFacts = serde_json::from_str(&json).ok()?;
    (stored.v == FACTS_VERSION).then_some(stored.facts)
}

/// Facts that depend only on the file: not those cut short by a budget,
/// nor an archive that an external tool found later could open.
fn memoisable(facts: &Facts) -> bool {
    let insp = &facts.inspection;
    // A multi-volume set's parts are the files beside it, not this file.
    !insp.budget_spent
        && !matches!(
            insp.issue,
            Some(
                ArchiveIssue::Unsupported { need: Some(_), .. } | ArchiveIssue::MultiVolume { .. }
            )
        )
}

fn remember_facts(cache: &Cache, c: &WalkCandidate, facts: &Facts) {
    let stored = StoredFacts {
        v: FACTS_VERSION,
        facts: facts.clone(),
    };
    if let Ok(json) = serde_json::to_string(&stored) {
        let _ = cache.put_organize_facts(&c.path, c.size, c.mtime_ns, &json);
    }
}

/// A placeholder is judged by its name and extension only: never read.
fn name_only_facts(c: &WalkCandidate) -> Facts {
    let name = c
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Facts {
        inspection: Inspection {
            container: Container::Raw,
            payloads: vec![Payload {
                name,
                ext: c.ext.clone(),
                size: c.size,
                crc32: None,
                sniff: Sniff::default(),
            }],
            entries: Vec::new(),
            issue: None,
            notes: Vec::new(),
            head_crc: None,
            format: None,
            mismatch: None,
            listing_complete: true,
            budget_spent: false,
            disc_set: None,
            decoded: 0,
            tool: None,
            volume_parts: Vec::new(),
        },
        raw_crcs: Vec::new(),
        hashed: true,
    }
}

/// A disc set is judged by its data track's header and extension; the
/// list file itself is never hashed.
fn set_facts(c: &WalkCandidate, entry: &SurveyEntry, sniff: Option<Sniff>) -> Facts {
    let track = entry
        .sources
        .iter()
        .skip(1)
        .find(|m| !is_list_extension(&extension(m)));
    let name = track
        .or(entry.sources.first())
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Facts {
        inspection: Inspection {
            container: Container::Raw,
            payloads: vec![Payload {
                name,
                ext: track.map_or_else(|| c.ext.clone(), |t| extension(t)),
                size: entry.size,
                crc32: None,
                sniff: sniff.unwrap_or_default(),
            }],
            entries: Vec::new(),
            issue: None,
            notes: Vec::new(),
            head_crc: None,
            format: None,
            mismatch: None,
            listing_complete: true,
            budget_spent: false,
            disc_set: None,
            decoded: 0,
            tool: None,
            volume_parts: Vec::new(),
        },
        raw_crcs: Vec::new(),
        hashed: true,
    }
}

/// `path` relative to `root` (unchanged when it is not inside).
fn rel(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
}

/// Builds the plan of `survey` under `opts` with the user's `decisions`.
/// Reads no ROM: only the target folders are listed (collisions) and
/// files compared when a name is already taken.
pub fn plan_from_survey(
    survey: &Survey,
    opts: &OrganizeOptions,
    decisions: &[Decision],
    env: &OrganizeEnv,
) -> Result<OrganizePlan, OrganizeError> {
    let (profile, _, _) = resolve_layout(&survey.source_root, opts, env)?;
    let target_root = match &opts.target {
        Some(t) => std::path::absolute(t).unwrap_or_else(|_| t.clone()),
        None => survey.target_root.clone(),
    };
    let roms_dir = roms_dir_of(&target_root, &profile, opts.flat);
    let src = &survey.source_root;

    // Items from the survey.
    let mut items: Vec<OrganizeItem> = Vec::with_capacity(survey.entries.len());
    let mut keys: Vec<Option<(String, u64)>> = Vec::with_capacity(survey.entries.len());
    for (id, entry) in survey.entries.iter().enumerate() {
        let mut item = OrganizeItem {
            id,
            sources: entry.sources.iter().map(|p| rel(src, p)).collect(),
            kind: entry.kind,
            size: entry.size,
            fps: entry.fps.clone(),
            target: None,
            set_targets: Vec::new(),
            system_id: None,
            folder: None,
            confidence: 0.0,
            method: None,
            why: Vec::new(),
            alternatives: Vec::new(),
            rename: None,
            notes: Vec::new(),
            companions: Vec::new(),
            catalogue_name: None,
            transfer: Transfer::AsIs,
            disc: None,
            action: OrganizeAction::Left {
                reason: LeftReason::Unknown,
            },
        };
        let mut key = None;
        match (&entry.left, &entry.identification) {
            (Some(reason), _) => {
                item.action = OrganizeAction::Left {
                    reason: reason.clone(),
                }
            }
            (None, Some(found)) => {
                let c = &found.combined;
                item.confidence = c.confidence;
                item.method = c.method;
                item.why = c.why.clone();
                item.alternatives = c.alternatives.clone();
                item.notes = found.notes.clone();
                item.catalogue_name = found.catalogue_name.clone();
                item.disc = found.disc.clone();
                key = found.content_key.clone();
                item.action = match (&found.left, c.verdict) {
                    (Some(reason), _) => OrganizeAction::Left {
                        reason: reason.clone(),
                    },
                    (None, Verdict::Auto) => OrganizeAction::Auto,
                    (None, Verdict::Review) => OrganizeAction::Review,
                    (None, Verdict::Unknown) => OrganizeAction::Left {
                        reason: LeftReason::Unknown,
                    },
                };
                if found.left.is_none() {
                    item.system_id = c.system.clone();
                }
            }
            (None, None) => {}
        }
        items.push(item);
        keys.push(key);
    }

    // The user's decisions.
    for decision in decisions {
        let Some(item) = items.get_mut(decision.item) else {
            return Err(OrganizeError::UnknownItem(decision.item));
        };
        match &decision.choice {
            Choice::Assign { system } => {
                if env.db.get(system).is_none() {
                    return Err(OrganizeError::UnknownSystem(system.clone()));
                }
                if !decidable(item) {
                    continue;
                }
                item.why
                    .insert(0, Evidence::new(IdLevel::User, system, 1.0, ""));
                item.alternatives.retain(|(s, _)| s != system);
                item.system_id = Some(system.clone());
                item.method = Some(IdLevel::User);
                item.confidence = 1.0;
                item.action = OrganizeAction::Auto;
                if let Some((crc, size)) = &keys[item.id] {
                    let _ = env.cache.put_organize_choice(crc, *size, system);
                }
            }
            Choice::Accept => {
                if !decidable(item) {
                    continue;
                }
                if item.system_id.is_none() {
                    if let Some((best, weight)) = item.alternatives.first().cloned() {
                        item.alternatives.remove(0);
                        item.system_id = Some(best);
                        item.confidence = weight;
                    }
                }
                if item.system_id.is_some() {
                    item.action = OrganizeAction::Auto;
                }
            }
            Choice::Leave => item.leave(LeftReason::Unknown),
        }
    }

    // Mode: archives are kept in copy mode.
    let mut warnings: Vec<PlanWarning> = Vec::new();
    let move_allowed = dir_writable(src);
    let mut mode = opts.mode.unwrap_or(if same_volume(src, &target_root) {
        TransferMode::Move
    } else {
        TransferMode::Copy
    });
    if !move_allowed && mode == TransferMode::Move {
        mode = TransferMode::Copy;
        warnings.push(PlanWarning::MoveUnavailable);
    }

    // Duplicates, extractions, then targets.
    mark_duplicates(&mut items, &keys);
    let mut shapes: Vec<Option<ExtractShape>> = vec![None; items.len()];
    for item in items.iter_mut().filter(|i| !i.is_left()) {
        let entry = &survey.entries[item.id];
        let (Some(system), Some(insp)) = (item.system_id.clone(), entry.inspection.as_ref()) else {
            continue;
        };
        let Some(system) = env.db.get(&system) else {
            continue;
        };
        match plan_transfer(item, insp, system, &profile, mode) {
            Ok(shape) => shapes[item.id] = shape,
            Err(reason) => item.leave(reason),
        }
    }
    let fat32 = file_system_is_fat32(&target_root);
    let layout = TargetLayout {
        target_root: target_root.clone(),
        roms_dir: roms_dir.clone(),
        profile,
        overrides: opts.folder_names.clone(),
        fat32,
        windows_paths: cfg!(windows),
        names: opts.names,
        companion_exts: opts.companion_exts.clone(),
    };
    let same_content = |existing: &Path, item: &OrganizeItem| -> bool {
        item.kind != ItemKind::Set
            && item
                .sources
                .first()
                .is_some_and(|s| same_bytes(existing, &src.join(s)))
    };
    let left_companions = assign_targets_with_extracts(
        &mut items,
        &shapes,
        &layout,
        env.db,
        src,
        &survey.walked.companions,
        &mut DirIndex::new(),
        &same_content,
    );
    if fat32 {
        limit_fat_folders(&mut items, &target_root);
    }
    for (path, reason) in left_companions {
        let abs = if path.is_absolute() {
            path.clone()
        } else {
            src.join(&path)
        };
        let fp = fp_of(&abs);
        items.push(OrganizeItem {
            id: items.len(),
            sources: vec![rel(src, &abs)],
            kind: ItemKind::File,
            size: fp.size,
            fps: vec![fp],
            target: None,
            set_targets: Vec::new(),
            system_id: None,
            folder: None,
            confidence: 0.0,
            method: None,
            why: Vec::new(),
            alternatives: Vec::new(),
            rename: None,
            notes: Vec::new(),
            companions: Vec::new(),
            catalogue_name: None,
            transfer: Transfer::AsIs,
            disc: None,
            action: OrganizeAction::Left { reason },
        });
    }

    // Warnings, counts.
    let missing: BTreeSet<String> = items
        .iter()
        .filter(|i| !i.is_left())
        .flat_map(|i| i.notes.iter())
        .filter_map(|n| match n {
            ItemNote::CatalogueMissing { system } => Some(system.clone()),
            _ => None,
        })
        .collect();
    warnings.extend(
        missing
            .into_iter()
            .map(|system| PlanWarning::CatalogueMissing { system }),
    );
    let target_detected = detect(&target_root)
        .into_iter()
        .next()
        .map(|d| d.profile_id)
        .filter(|id| *id != opts.layout);
    if let Some(profile) = &target_detected {
        warnings.push(PlanWarning::TargetLayoutMismatch {
            profile: profile.clone(),
        });
    }
    let candidates = survey
        .entries
        .iter()
        .filter(|e| e.identification.is_some() || e.kind == ItemKind::Set)
        .count();
    if candidates == 0 && !survey.walked.sorted_folders.is_empty() {
        warnings.push(PlanWarning::SortedOnly);
    }
    let copies = mode == TransferMode::Copy || !same_volume(src, &target_root);
    let cluster = cluster_size(&target_root).unwrap_or(DEFAULT_CLUSTER);
    let needed = space_needed(&items, src, copies, cluster);
    if let Some(free) = user_available(&target_root).filter(|&free| free < needed) {
        warnings.push(PlanWarning::NoSpace { needed, free });
    }
    let counts = count(&items, &survey.walked);

    Ok(OrganizePlan {
        source_root: src.clone(),
        target_root,
        roms_dir,
        layout: opts.layout.clone(),
        mode,
        names: opts.names,
        items,
        counts,
        sorted_folders: survey
            .walked
            .sorted_folders
            .iter()
            .map(|(p, s)| (rel(src, p), s.clone()))
            .collect(),
        deep_folders: survey
            .walked
            .deep_folders
            .iter()
            .map(|p| rel(src, p))
            .collect(),
        other_files: survey
            .walked
            .other_files
            .iter()
            .map(|p| rel(src, p))
            .collect(),
        warnings,
        broad_root: survey.broad_root,
        fat32,
        move_allowed,
        target_detected,
        created_at: crate::time::rfc3339_utc(env.now),
    })
}

/// Leftover names a note shows.
const LEFTOVER_EXAMPLES: usize = 3;

/// Kept free on the target beyond what a plan writes (archives spec §8.4).
pub const SPACE_RESERVE: u64 = 64 * 1024 * 1024;
/// The allocation unit assumed when the volume does not tell its own.
const DEFAULT_CLUSTER: u64 = 4096;

/// Bytes the target volume needs free for the items that move: every
/// extracted entry, generated playlist and, when `copies` (copy mode or
/// another volume), every source file and companion, each rounded up to
/// whole `cluster`s; plus 1 % and [`SPACE_RESERVE`]. Nothing to write
/// needs nothing. A move on one volume renames, and the Trash frees no
/// space on it, so an archive's own size is never taken off (spec §8.4).
pub fn space_needed(items: &[OrganizeItem], source_root: &Path, copies: bool, cluster: u64) -> u64 {
    let cluster = cluster.max(1);
    let round = |n: u64| n.div_ceil(cluster).saturating_mul(cluster);
    let mut sum: u64 = 0;
    for item in items.iter().filter(|i| !i.is_left() && i.target.is_some()) {
        let mut add = match &item.transfer {
            // A playlist is a few lines: one cluster.
            Transfer::Extract { entries, m3u, .. } => entries
                .iter()
                .map(|e| round(e.size))
                .chain(m3u.as_ref().map(|_| round(1)))
                .fold(0u64, u64::saturating_add),
            Transfer::AsIs if copies => item
                .fps
                .iter()
                .map(|fp| round(fp.size))
                .fold(0u64, u64::saturating_add),
            Transfer::AsIs => 0,
        };
        if copies {
            for c in &item.companions {
                let size = fs::metadata(source_root.join(&c.source)).map_or(0, |m| m.len());
                add = add.saturating_add(round(size));
            }
        }
        sum = sum.saturating_add(add);
    }
    if sum == 0 {
        return 0;
    }
    sum.saturating_add(sum.div_ceil(100))
        .saturating_add(SPACE_RESERVE)
}

/// How an archive item reaches `system`'s folder under `profile`
/// (archives spec §8.1–§8.3): `Ok(None)` moves it as it is; `Ok(Some)`
/// sets `item.transfer` to an extraction (targets assigned later) and
/// returns its layout; `Err` leaves the item.
///
/// - Arcade sets are never extracted: a zip moves as it is, any other
///   container is `RepackArcade`.
/// - A disc archive always extracts its set (sheets, tracks, images).
/// - A cartridge archive moves as it is when the profile's emulators load
///   its container for the system, else its ROM entries are extracted.
/// - An incomplete listing is never extracted: the archive moves as it is.
/// - Move mode sends the archive to the Trash only when every file entry
///   is extracted or OS junk (an unsafe name
///   never counts as junk); anything else (a save, a readme) keeps it,
///   with a note. Copy mode always keeps it.
fn plan_transfer(
    item: &mut OrganizeItem,
    insp: &Inspection,
    system: &System,
    profile: &Profile,
    mode: TransferMode,
) -> Result<Option<ExtractShape>, LeftReason> {
    let Some(format) = insp.format else {
        return Ok(None);
    };
    // Before the listing rule: an arcade set that is not a zip never
    // loads, however much of it was listed.
    if system.arcade {
        return match insp.container {
            Container::Zip => Ok(None),
            _ => Err(LeftReason::RepackArcade),
        };
    }
    // Entries past an incomplete listing (a tar cut by the listing budget)
    // are unknown: such an archive moves as it is, never extracted (spec
    // §4.2).
    if !insp.listing_complete {
        return Ok(None);
    }
    let by_name = |name: &str| insp.entries.iter().find(|e| e.name == name);
    let (chosen, shape, sheet_check): (Vec<&ZipEntryInfo>, ExtractShape, SheetCheck) =
        match &insp.disc_set {
            Some(set) => {
                let chosen = set.members.iter().filter_map(|m| by_name(m)).collect();
                let check = if set.extractable {
                    set.sheet_check
                } else {
                    SheetCheck::Deferred
                };
                let shape = ExtractShape::Disc {
                    discs: set.discs.clone(),
                    title: set.title.clone(),
                    multi_disc: set.multi_disc,
                };
                if profile.disc_layout.is_game_folder() {
                    // One folder per game: menus list it once.
                    item.notes
                        .retain(|n| !matches!(n, ItemNote::TwinSheets { .. }));
                }
                (chosen, shape, check)
            }
            None => {
                if profile.accepts_as_is(system, ContainerKind::of(format)) {
                    return Ok(None);
                }
                let chosen: Vec<&ZipEntryInfo> = insp
                    .payloads
                    .iter()
                    .filter_map(|p| by_name(&p.name))
                    .collect();
                if chosen.is_empty() {
                    return Ok(None);
                }
                if chosen.len() > 1 {
                    item.notes
                        .retain(|n| !matches!(n, ItemNote::MultiRomZip { .. }));
                    item.notes.push(ItemNote::MultiRomExtract {
                        count: chosen.len(),
                    });
                }
                (chosen, ExtractShape::Roms, SheetCheck::Read)
            }
        };
    let mut entries = Vec::with_capacity(chosen.len());
    for e in &chosen {
        if let Some(why) = e.unsafe_why {
            return Err(LeftReason::UnsafeArchiveEntry {
                name: e.name.clone(),
                why,
            });
        }
        // Facts from before extraction carry no index: never guessed.
        let Some(index) = e.index else {
            return Err(LeftReason::Unknown);
        };
        entries.push(ExtractEntry {
            index,
            name: e.name.clone(),
            size: e.size,
            crc32: e
                .crc32
                .as_deref()
                .and_then(|c| u32::from_str_radix(c, 16).ok()),
            target: PathBuf::new(),
        });
    }
    let leftovers: Vec<&ZipEntryInfo> = insp
        .entries
        .iter()
        // An unsafe name is never junk: `__MACOSX/../Game.srm` is a save
        // that is not extracted.
        .filter(|e| e.unsafe_why.is_some() || !is_metadata_entry(&e.name))
        .filter(|e| !entries.iter().any(|x| Some(x.index) == e.index))
        .collect();
    let archive_after = match mode {
        TransferMode::Copy => ArchiveAfter::Keep,
        TransferMode::Move if leftovers.is_empty() => ArchiveAfter::Trash,
        TransferMode::Move => {
            item.notes.push(ItemNote::ArchiveKeptLeftovers {
                count: leftovers.len(),
                examples: leftovers
                    .iter()
                    .take(LEFTOVER_EXAMPLES)
                    .map(|e| e.name.clone())
                    .collect(),
            });
            ArchiveAfter::Keep
        }
    };
    if entries.iter().any(|e| e.crc32.is_none())
        && !item.notes.contains(&ItemNote::NoStoredChecksum)
    {
        item.notes.push(ItemNote::NoStoredChecksum);
    }
    item.transfer = Transfer::Extract {
        bytes: entries.iter().map(|e| e.size).sum(),
        entries,
        archive_after,
        source: match &insp.tool {
            Some((tool, version)) => ExtractSource::Tool {
                tool: *tool,
                version: version.clone(),
            },
            None => ExtractSource::Builtin,
        },
        sheet_check,
        m3u: None,
    };
    Ok(Some(shape))
}

/// Whether the user may settle this item: it moves, or nothing placed it.
fn decidable(item: &OrganizeItem) -> bool {
    match &item.action {
        OrganizeAction::Auto | OrganizeAction::Review => true,
        OrganizeAction::Left { reason } => matches!(reason, LeftReason::Unknown),
    }
}

fn count(items: &[OrganizeItem], walked: &Walked) -> Counts {
    let mut counts = Counts {
        junk: walked.junk,
        other_files: walked.other_files.len(),
        deep_folders: walked.deep_folders.len(),
        ..Counts::default()
    };
    let mut systems: BTreeSet<&str> = BTreeSet::new();
    for item in items {
        match item.action {
            OrganizeAction::Auto => counts.auto += 1,
            OrganizeAction::Review => counts.review += 1,
            OrganizeAction::Left { .. } => {
                counts.left += 1;
                continue;
            }
        }
        counts.roms += 1;
        counts.bytes += item.size;
        if let Some(system) = &item.system_id {
            systems.insert(system);
        }
    }
    counts.systems = systems.len();
    counts
}

/// Same length and same bytes (CRC32 of both, streamed).
fn same_bytes(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (fs::metadata(a), fs::metadata(b)) else {
        return false;
    };
    ma.len() == mb.len() && matches!((file_crc(a), file_crc(b)), (Some(x), Some(y)) if x == y)
}

fn file_crc(path: &Path) -> Option<u32> {
    use std::io::Read;
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match file.read(&mut buf) {
            Ok(0) => return Some(hasher.finalize()),
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

/// The nearest existing ancestor of `path` (itself when it exists).
fn existing_ancestor(path: &Path) -> Option<&Path> {
    path.ancestors().find(|p| p.exists())
}

/// Whether `a` and `b` (or their nearest existing ancestors) are on one
/// volume: same device on Unix, same drive or share on Windows (O-D3).
fn same_volume(a: &Path, b: &Path) -> bool {
    let (Some(a), Some(b)) = (existing_ancestor(a), existing_ancestor(b)) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (fs::metadata(a), fs::metadata(b)) {
            (Ok(x), Ok(y)) => x.dev() == y.dev(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let prefix = |p: &Path| {
            p.components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        };
        prefix(a) == prefix(b)
    }
}

/// Whether files in `dir` can be moved away (the folder is writable).
fn dir_writable(dir: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let Ok(c_path) = CString::new(dir.as_os_str().as_bytes()) else {
            return false;
        };
        // SAFETY: `c_path` is a valid nul-terminated string.
        unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
    }
    // Windows does not honour FILE_ATTRIBUTE_READONLY on folders (Explorer
    // sets it on Documents, Desktop and customised folders), so it says
    // nothing about moving files out; a denied move pauses the apply.
    #[cfg(windows)]
    {
        fs::metadata(dir).is_ok_and(|m| m.is_dir())
    }
    #[cfg(not(any(unix, windows)))]
    {
        fs::metadata(dir).is_ok_and(|m| !m.permissions().readonly())
    }
}

/// [`survey`] and [`plan_from_survey`] without decisions.
pub fn plan_organize(
    source: &Path,
    opts: &OrganizeOptions,
    env: &OrganizeEnv,
    progress: &(dyn Fn(OrganizeProgress) + Sync),
    cancel: &AtomicBool,
) -> Result<OrganizePlan, OrganizeError> {
    let survey = survey(source, opts, env, progress, cancel)?;
    plan_from_survey(&survey, opts, &[], env)
}

/// ROM-like files directly in `dir` (not in its subfolders), by name only:
/// what a library shows as "loose files" to organise (organise spec §4.5).
/// Partial downloads, hidden and system files do not count; an unreadable
/// or missing folder counts 0.
pub fn count_loose_roms(dir: &Path, db: &SystemDb) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            !(is_appledouble(&name)
                || is_junk_file(&name)
                || is_zone_identifier(&name)
                || is_hidden(&name)
                || is_partial_download(&name))
                && db.is_rom_like_extension(&extension(&entry.path()))
        })
        .count()
}

/// [`count_loose_roms`] of a library: its root plus its `roms_root` (found
/// ignoring case), where no system folder holds them.
pub fn count_library_loose_roms(root: &Path, roms_root: &str, db: &SystemDb) -> usize {
    let in_root = count_loose_roms(root, db);
    if roms_root.trim_matches(['/', '\\']).is_empty() {
        return in_root;
    }
    in_root + find_dir_ignoring_case(root, roms_root).map_or(0, |dir| count_loose_roms(&dir, db))
}

/// A quick look at `path` by names and metadata only (no file is read):
/// what the start screen needs to offer organising. Stops after
/// [`PROBE_LIMIT`] entries.
pub fn probe_folder(path: &Path, db: &SystemDb, probe: &dyn MetaProbe) -> io::Result<FolderProbe> {
    fs::read_dir(path)?;
    let mut out = FolderProbe {
        writable: dir_writable(path),
        broad_root: is_broad_root(path),
        ..FolderProbe::default()
    };
    // Same folders the walk never enters (`bios`, `saves`, …).
    let ignored = ScanOptions::default().ignored_dirs;
    let mut sorted: BTreeSet<String> = BTreeSet::new();
    let mut partial_dirs = 0usize;
    let walker = WalkDir::new(path)
        .follow_links(false)
        .same_file_system(true)
        .min_depth(1)
        .max_depth(MAX_DEPTH)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            if is_hidden(&name) || ignored.iter().any(|d| d.eq_ignore_ascii_case(&name)) {
                return false;
            }
            if is_partial_download(&name) {
                partial_dirs += 1;
                return false;
            }
            match db.resolve_folder(&name) {
                Some(_) => {
                    sorted.insert(name.into_owned());
                    false
                }
                None => true,
            }
        });
    let mut seen = 0usize;
    for entry in walker {
        seen += 1;
        if seen > PROBE_LIMIT {
            out.truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if is_appledouble(&name)
            || is_junk_file(&name)
            || is_zone_identifier(&name)
            || is_hidden(&name)
        {
            continue;
        }
        if is_partial_download(&name) {
            out.partial_downloads += 1;
            continue;
        }
        if db.is_rom_like_extension(&extension(entry.path())) {
            out.rom_like += 1;
            if entry
                .metadata()
                .is_ok_and(|m| probe.cloud_only(entry.path(), &m))
            {
                out.cloud_only += 1;
            }
        }
    }
    out.partial_downloads += partial_dirs;
    out.sorted_folders = sorted.into_iter().collect();
    Ok(out)
}
