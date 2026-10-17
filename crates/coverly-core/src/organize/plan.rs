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

use crate::cache::Cache;
use crate::identify::dat::DatStore;
use crate::identify::evidence::{Evidence, IdLevel};
use crate::identify::sniff::{NotARom, Sniff};
use crate::organize::evidence::{
    identify_candidate, DatCatalogue, Facts, Identification, LadderEnv, OnlineLookup, Thresholds,
    Verdict, MAX_DAT_DOWNLOADS,
};
use crate::organize::inspect::{inspect_file, inspect_set_primary, Container, Inspection, Payload};
use crate::organize::target::{
    assign_targets, default_companion_exts, file_system_is_fat32, mark_duplicates, DirIndex,
    TargetLayout,
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
use crate::settings::Settings;
use crate::systems::SystemDb;

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
        }
    }
}

impl OrganizeOptions {
    /// The options the settings ask for, with `layout` when given. Until
    /// `settings.organize` exists this is the defaults plus the layout.
    pub fn from_settings(settings: &Settings, layout: Option<&str>) -> OrganizeOptions {
        let _ = settings;
        OrganizeOptions {
            layout: layout.unwrap_or(DEFAULT_LAYOUT).to_string(),
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
    let catalogue = DatCatalogue::new(env.db, env.cache, store, MAX_DAT_DOWNLOADS);
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
    let results = identify_all(&entries, &todo, workers, env, &ladder, progress, cancel)?;
    let mut facts_of: HashMap<usize, Facts> = HashMap::new();
    for (i, outcome) in results {
        match outcome {
            Ok((facts, id)) => {
                entries[i].entry.identification = Some(id);
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

    Ok(Survey {
        source_root: source_abs,
        target_root,
        broad_root,
        entries: entries.into_iter().map(|p| p.entry).collect(),
        walked,
    })
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
        } else if members
            .iter()
            .any(|m| !is_within(m, &base) || !is_within(m, source))
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
    env: &OrganizeEnv,
    ladder: &LadderEnv,
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
                let outcome = identify_entry(&entries[i], env, ladder);
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
fn identify_entry(pending: &Pending, env: &OrganizeEnv, ladder: &LadderEnv) -> Outcome {
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
                let inspection = inspect_file(&candidate.path, candidate.size, env.db)
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
    if pending.entry.kind != ItemKind::Set && !candidate.cloud_only {
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

fn memoised_facts(cache: &Cache, c: &WalkCandidate) -> Option<Facts> {
    let json = cache.organize_facts(&c.path, c.size, c.mtime_ns).ok()??;
    serde_json::from_str(&json).ok()
}

fn remember_facts(cache: &Cache, c: &WalkCandidate, facts: &Facts) {
    if let Ok(json) = serde_json::to_string(facts) {
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

    // Duplicates, then targets.
    mark_duplicates(&mut items, &keys);
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
    let left_companions = assign_targets(
        &mut items,
        &layout,
        env.db,
        src,
        &survey.walked.companions,
        &mut DirIndex::new(),
        &same_content,
    );
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
            action: OrganizeAction::Left { reason },
        });
    }

    // Mode, warnings, counts.
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
    #[cfg(not(unix))]
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
