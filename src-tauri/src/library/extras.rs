//! A game's `Extras/` for the panel (part D D4, D16, D17, D22, P-4):
//! listed by entry id on a blocking worker and never written by a read.
//! Each listing gets a revision and stays in the session cache, so a later
//! write or media request names a file by `(rev, index)`, never by path.
//! Writes are [`ExtrasAction`]s, each one journaled job (D10).

use std::collections::{HashMap, VecDeque};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use coverly_core::consoles::is_key_material;
use coverly_core::extras::index::{list, IndexState, ListedFile, Listing};
use coverly_core::extras::ops::{extras_dir, AddLimits, AddSuggestion, ExtrasNote};
use coverly_core::extras::{
    CategoryRef, ExtraFormat, ExtraRecord, ExtraType, ExtrasCategory, Provenance, RipTags,
};
use coverly_core::game_folder::FixedFolder;
use coverly_core::organize::plan::dir_writable;
use serde::{Deserialize, Serialize};

use super::art::inside_root;
use super::Session;
use crate::error::IpcError;
use crate::jobs::view::EntriesUpdatedEvent;

/// Systems whose electronic manual is part of the game (D16): `Manuals/`
/// shows a note instead.
pub const MANUAL_INSIDE: [&str; 4] = ["3ds", "wiiu", "switch", "psvita"];

/// Played or shown inside the app (D17).
const INLINE: &[&str] = &[
    "mp3", "m4a", "aac", "wav", "flac", "ogg", "opus", // audio
    "mp4", "m4v", "webm", "mov", // video
    "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "svg", "heic", "tif", "tiff", // image
    "pdf", "txt", "md", "nfo", // document
];

/// Not played here, but safe to hand to another app (D17).
const EXTERNAL: &[&str] = &[
    "spc", "nsf", "nsfe", "gbs", "vgm", "vgz", "psf", "minipsf", "psf2", "minipsf2", "gsf",
    "minigsf", "usf", "miniusf", "2sf", "mini2sf", "ssf", "minissf", "dsf", "minidsf", "qsf",
    "miniqsf", "cbz", "epub",
];

/// What the panel may do with a file (D17).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Playable {
    /// Played or shown in the app (and may open in another app).
    Inline,
    /// Not played here; «Abrir con otra app» is offered.
    External,
    /// Only shown in the file manager: packages, executables, scripts,
    /// shortcuts, symlinks, key material and anything unknown.
    RevealOnly,
}

/// [`Playable`] of a file named `name`; a symlink is never served.
pub fn playable_of(name: &str, link: bool) -> Playable {
    if link || is_key_material(name) {
        return Playable::RevealOnly;
    }
    let Some((_, ext)) = name.rsplit_once('.') else {
        return Playable::RevealOnly;
    };
    let ext = ext.to_ascii_lowercase();
    if INLINE.contains(&ext.as_str()) {
        Playable::Inline
    } else if EXTERNAL.contains(&ext.as_str()) {
        Playable::External
    } else {
        Playable::RevealOnly
    }
}

/// Whether the panel may write this game's extras now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Writability {
    Yes,
    /// Not a game folder, or a folder the app cannot write.
    ReadOnly,
    /// A job or another write holds the library.
    Busy,
    /// `extras.json` comes from a newer Coverly (D5).
    Newer,
}

/// One category of the section: every fixed one (a drop slot until it
/// exists), then user ones, then the files loose in `Extras/`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CategoryView {
    pub category: CategoryRef,
    /// A folder on disk (always true for [`CategoryRef::Root`]).
    pub present: bool,
    pub count: u32,
    /// A spelling is a symlink: listed, never entered.
    pub link: bool,
    /// Copy to devices (block 4, D21): `None` inherits the default.
    pub sync: Option<bool>,
}

/// One listed file; `index` is its handle within this listing's `rev`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExtraFileView {
    pub index: u32,
    pub category: CategoryRef,
    /// `/`-joined, relative to `Extras/`: shown, never sent back.
    pub path: String,
    pub name: String,
    pub kind: ExtraType,
    pub provenance: Provenance,
    pub format: Option<ExtraFormat>,
    pub tags: Vec<String>,
    pub size: u64,
    pub link: bool,
    pub playable: Playable,
}

/// A record whose file is gone («No encontrados», D4); `index` names it
/// for «Olvidar».
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MissingView {
    pub index: u32,
    pub category: CategoryRef,
    pub path: String,
    pub name: String,
    pub kind: ExtraType,
    pub provenance: Provenance,
    pub tags: Vec<String>,
    pub size: u64,
}

/// A game's `Extras/` as the panel shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExtrasView {
    pub entry_id: u32,
    pub rev: u64,
    /// False for an entry that is no game folder: nothing to list or write.
    pub game_folder: bool,
    pub index: IndexState,
    pub writable: Writability,
    pub categories: Vec<CategoryView>,
    pub files: Vec<ExtraFileView>,
    pub missing: Vec<MissingView>,
    pub capped: bool,
    pub system: String,
    /// The system keeps the e-manual inside the game (D16).
    pub manual_inside: bool,
}

/// One listing kept by the session.
#[derive(Debug)]
pub struct ListedExtras {
    pub rev: u64,
    /// The game folder as scanned (under the library root); `None` for an
    /// entry that is no game folder.
    pub game_dir: Option<PathBuf>,
    pub listing: Listing,
}

/// A file named by `(rev, index)` of the session's listing.
#[derive(Clone, Debug)]
pub struct ExtrasHandle {
    pub rev: u64,
    pub game_dir: PathBuf,
    pub file: ListedFile,
}

/// The last listing of each entry of the open library (P-4). Revisions
/// grow for the whole process, so one from a closed library never names a
/// listing of the next.
#[derive(Debug, Default)]
pub struct ExtrasCache {
    generation: u64,
    last_rev: u64,
    by_entry: HashMap<u32, Arc<ListedExtras>>,
}

impl ExtrasCache {
    /// Keeps `listing` as entry `entry`'s newest, under a new revision
    /// (generations only grow: an older one is answered but not kept).
    pub fn store(
        &mut self,
        generation: u64,
        entry: u32,
        game_dir: Option<PathBuf>,
        listing: Listing,
    ) -> Arc<ListedExtras> {
        if generation > self.generation {
            self.by_entry.clear();
            self.generation = generation;
        }
        self.last_rev += 1;
        let listed = Arc::new(ListedExtras {
            rev: self.last_rev,
            game_dir,
            listing,
        });
        // A read of a library closed meanwhile lands late: answered, never
        // kept, so it cannot evict the open library's listings.
        if generation == self.generation {
            self.by_entry.insert(entry, Arc::clone(&listed));
        }
        listed
    }

    /// Entry `entry`'s listing, if `rev` is still its newest.
    pub fn get(
        &self,
        generation: u64,
        entry: u32,
        rev: u64,
    ) -> Result<Arc<ListedExtras>, IpcError> {
        match self.by_entry.get(&entry) {
            Some(l) if self.generation == generation && l.rev == rev => Ok(Arc::clone(l)),
            _ => Err(IpcError::ExtrasChanged),
        }
    }
}

/// The game folder of entry `id`, if it is the base at a game folder's
/// root (part C counts its fixed folders); checked inside the library.
pub fn game_dir(session: &Session, id: u32) -> Result<Option<PathBuf>, IpcError> {
    let entry = session
        .entries
        .get(id as usize)
        .ok_or(IpcError::UnknownEntry(id))?;
    let rom = &entry.rom;
    if rom.role.is_some() || rom.fixed_counts.is_none() {
        return Ok(None);
    }
    let dir = if rom.is_dir {
        rom.path.clone()
    } else {
        match rom.path.parent() {
            Some(p) => p.to_path_buf(),
            None => return Ok(None),
        }
    };
    inside_root(session, &dir)?;
    Ok(Some(dir))
}

/// Lists entry `id`'s `Extras/` (blocking). Reads only.
pub fn read_listing(session: &Session, id: u32) -> Result<(Option<PathBuf>, Listing), IpcError> {
    let dir = game_dir(session, id)?;
    let listing = match &dir {
        Some(d) => list(&extras_dir(d)),
        None => empty_listing(),
    };
    Ok((dir, listing))
}

fn empty_listing() -> Listing {
    Listing {
        index: IndexState::Absent,
        categories: Vec::new(),
        files: Vec::new(),
        missing: Vec::new(),
        unreached: Vec::new(),
        capped: false,
        base: None,
    }
}

/// Whether the app cannot write `game_dir`'s extras: the deepest existing
/// of `Extras/` and the game folder is not writable for this process (a
/// read-only mount included), or unreadable. Mode bits alone would miss
/// both, and Windows ignores the read-only attribute on folders.
pub(crate) fn read_only(game_dir: &Path) -> bool {
    let extras = extras_dir(game_dir);
    let probe = if extras.is_dir() {
        extras
    } else {
        game_dir.to_path_buf()
    };
    !probe.is_dir() || !dir_writable(&probe)
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The category a record's `/`-joined path sits in.
fn category_of_path(path: &str) -> CategoryRef {
    match path.split_once('/') {
        None => CategoryRef::Root,
        Some((first, _)) => ExtrasCategory::from_dir_name(first)
            .map_or_else(|| CategoryRef::User(first.to_string()), CategoryRef::Fixed),
    }
}

fn categories(listing: &Listing, game_folder: bool) -> Vec<CategoryView> {
    if !game_folder {
        return Vec::new();
    }
    let sync = |name: &str| listing.base.as_ref().and_then(|b| b.sync_for(name));
    let mut out: Vec<CategoryView> = ExtrasCategory::ALL
        .into_iter()
        .map(|c| {
            let listed = listing
                .categories
                .iter()
                .find(|l| l.category == CategoryRef::Fixed(c));
            CategoryView {
                category: CategoryRef::Fixed(c),
                present: listed.is_some(),
                count: listed.map_or(0, |l| count(l.count)),
                link: listed.is_some_and(|l| l.link),
                sync: sync(c.dir_name()),
            }
        })
        .collect();
    for l in &listing.categories {
        let name = match &l.category {
            CategoryRef::Fixed(_) => continue,
            CategoryRef::User(n) => Some(n.as_str()),
            CategoryRef::Root => None,
        };
        out.push(CategoryView {
            category: l.category.clone(),
            present: true,
            count: count(l.count),
            link: l.link,
            sync: name.and_then(sync),
        });
    }
    out
}

fn file_view(index: usize, f: &ListedFile) -> ExtraFileView {
    ExtraFileView {
        index: count(index),
        category: f.category.clone(),
        path: f.path.clone(),
        name: f.name.clone(),
        kind: f.record.kind,
        provenance: f.record.provenance,
        format: f.record.origin.format,
        tags: f.record.tags.clone(),
        size: f.size,
        link: f.link,
        playable: playable_of(&f.name, f.link),
    }
}

fn missing_view(index: usize, r: &ExtraRecord) -> MissingView {
    MissingView {
        index: count(index),
        category: category_of_path(&r.path),
        path: r.path.clone(),
        name: r.path.rsplit('/').next().unwrap_or_default().to_string(),
        kind: r.kind,
        provenance: r.provenance,
        tags: r.tags.clone(),
        size: r.size,
    }
}

/// The view of a kept listing. `occupied`: a job or a write holds the
/// library.
pub fn view(entry_id: u32, system: &str, listed: &ListedExtras, occupied: bool) -> ExtrasView {
    let l = &listed.listing;
    let game_folder = listed.game_dir.is_some();
    let writable = match &listed.game_dir {
        None => Writability::ReadOnly,
        Some(_) if matches!(l.index, IndexState::Newer { .. }) => Writability::Newer,
        Some(d) if read_only(d) => Writability::ReadOnly,
        Some(_) if occupied => Writability::Busy,
        Some(_) => Writability::Yes,
    };
    ExtrasView {
        entry_id,
        rev: listed.rev,
        game_folder,
        index: l.index,
        writable,
        categories: categories(l, game_folder),
        files: l
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| file_view(i, f))
            .collect(),
        missing: l
            .missing
            .iter()
            .enumerate()
            .map(|(i, r)| missing_view(i, r))
            .collect(),
        capped: l.capped,
        system: system.to_string(),
        manual_inside: MANUAL_INSIDE.contains(&system),
    }
}

/// One write of the Extras section: files by their `index` in the
/// listing a `rev` names, categories by name (part D D10, D22). Each is
/// one journaled job.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExtrasAction {
    /// The typed `stem`; the extension stays.
    Rename {
        index: u32,
        stem: String,
    },
    Move {
        files: Vec<u32>,
        to: CategoryRef,
    },
    /// To the Trash, or set aside where a drive has none (D8).
    Trash {
        files: Vec<u32>,
    },
    SetTags {
        index: u32,
        tags: Vec<String>,
    },
    /// «Olvidar» a record of «No encontrados» (D4).
    Forget {
        missing: u32,
    },
    CategoryCreate {
        name: String,
    },
    CategoryRename {
        from: String,
        to: String,
    },
    /// Its files to the Trash, then its emptied folder (D8).
    CategoryRemove {
        name: String,
    },
}

/// A finished Extras write: the job to undo (none when nothing changed),
/// the new listing (its handles are the ones to use next) and the notes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExtrasWriteView {
    pub job: Option<String>,
    pub listing: ExtrasView,
    pub notes: Vec<ExtrasNote>,
}

/// [`ExtrasWriteView`] and the `library-entries-updated` payload carrying
/// the entry's new counts, which the command emits.
#[derive(Clone, Debug)]
pub struct ExtrasWritten {
    pub view: ExtrasWriteView,
    pub updated: Option<EntriesUpdatedEvent>,
}

/// An undone Extras job and the game's listing after it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExtrasUndone {
    pub job: String,
    pub removed: usize,
    pub restored: usize,
    /// Files changed after the job: left as they are.
    pub kept: usize,
    pub listing: ExtrasView,
    /// Emitted as `library-entries-updated`, not part of the answer.
    #[serde(skip)]
    pub updated: Option<EntriesUpdatedEvent>,
}

/// The file at `index` of a kept listing.
pub fn handle(listed: &ListedExtras, index: u32) -> Result<ExtrasHandle, IpcError> {
    let (Some(game_dir), Some(file)) = (
        listed.game_dir.as_ref(),
        listed.listing.files.get(index as usize),
    ) else {
        return Err(IpcError::BadRequest(format!("no extra {index}")));
    };
    Ok(ExtrasHandle {
        rev: listed.rev,
        game_dir: game_dir.clone(),
        file: file.clone(),
    })
}

/// The game folder an Extras job wrote in: the folder holding the first
/// `Extras/` among its targets (a set-aside path in `.coverly-trash` keeps
/// the game's path below the job folder).
pub fn job_game(root: &Path, targets: &[PathBuf]) -> Option<String> {
    let extras = FixedFolder::Extras.dir_name();
    targets.iter().find_map(|t| {
        let rel = t.strip_prefix(root).unwrap_or(t);
        let parts: Vec<&OsStr> = rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(n) => Some(n),
                _ => None,
            })
            .collect();
        parts
            .windows(2)
            .find(|w| {
                w[1].to_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(extras))
            })
            .and_then(|w| w[0].to_str())
            .map(str::to_string)
    })
}

// ---- Adds from a picker or an OS drop (D11, D13, P-5) ----

/// How many OS drops are kept for a later add (D11).
pub const DROP_TOKENS_KEPT: usize = 4;

/// How long a drop may wait for its add (D11).
pub const DROP_TOKEN_TTL: Duration = Duration::from_secs(120);

/// How many add plans the session keeps for the add dialog.
pub const ADD_PLANS_KEPT: usize = 4;

/// Above this many files an add runs in the background (D13).
pub const BACKGROUND_FILES: usize = 50;

/// Above this many bytes an add runs in the background (D13).
pub const BACKGROUND_BYTES: u64 = 200_000_000;

/// Where the OS reported a drop, in physical pixels of the window (the
/// webview divides by `devicePixelRatio` before hit-testing, D11).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct DropPosition {
    pub x: f64,
    pub y: f64,
}

/// One dropped path as the webview sees it: its bare name, never a path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DroppedItem {
    pub name: String,
    /// Bytes of a file; 0 for a folder.
    pub size: u64,
    pub is_dir: bool,
}

/// `extras://drop`: an OS drop recorded under `token` (D11). The paths
/// stay in Rust; an add names the token.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DropEvent {
    pub token: u64,
    pub position: DropPosition,
    pub items: Vec<DroppedItem>,
}

/// The paths of the last OS drops (D11, P-5): the only paths an add from
/// a drop ever copies. Each token is used once and expires.
#[derive(Debug, Default)]
pub struct DropTokens {
    last: u64,
    kept: VecDeque<(u64, Vec<PathBuf>, Instant)>,
}

impl DropTokens {
    /// Keeps `paths` (as the OS reported them) under a new token, the
    /// oldest kept one forgotten past [`DROP_TOKENS_KEPT`], and answers
    /// what the webview may know: names, sizes and kinds. Reads metadata
    /// only (no symlink followed), never a file.
    pub fn record(
        &mut self,
        paths: Vec<PathBuf>,
        position: DropPosition,
        now: Instant,
    ) -> DropEvent {
        self.last += 1;
        let items = paths
            .iter()
            .map(|p| {
                let meta = fs::symlink_metadata(p).ok();
                let is_dir = meta.as_ref().is_some_and(fs::Metadata::is_dir);
                DroppedItem {
                    name: p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    size: meta.filter(|m| m.is_file()).map_or(0, |m| m.len()),
                    is_dir,
                }
            })
            .collect();
        self.kept.retain(|(_, _, at)| fresh(*at, now));
        while self.kept.len() >= DROP_TOKENS_KEPT {
            self.kept.pop_front();
        }
        self.kept.push_back((self.last, paths, now));
        DropEvent {
            token: self.last,
            position,
            items,
        }
    }

    /// The paths of drop `token`, once, while it is fresh.
    pub fn take(&mut self, token: u64, now: Instant) -> Option<Vec<PathBuf>> {
        let at = self.kept.iter().position(|(t, _, _)| *t == token)?;
        let (_, paths, when) = self.kept.remove(at)?;
        fresh(when, now).then_some(paths)
    }
}

fn fresh(at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(at) <= DROP_TOKEN_TTL
}

/// Picks files for an add (D13): the native dialog, opened from Rust on
/// a blocking worker (never the main thread); `None` when cancelled.
pub trait Picker {
    fn pick_files(&self) -> Option<Vec<PathBuf>>;
}

/// The files an add dialog offers, kept by the session under `id`.
#[derive(Debug)]
pub struct AddPlan {
    pub id: u64,
    pub generation: u64,
    pub entry: u32,
    pub files: Vec<AddSuggestion>,
    pub keys: usize,
    pub hidden: usize,
}

/// One file of [`AddPlanView`]; `index` names it in the commit.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AddFileView {
    pub index: u32,
    pub name: String,
    pub size: u64,
    pub kind: ExtraType,
    /// Suggested by its type and name (D13), changeable in the dialog.
    pub category: ExtrasCategory,
    /// A music rip's own tags, offered, never added (D15).
    pub rip: Option<RipTags>,
}

/// What the add dialog shows (D13): files by index, never by path.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AddPlanView {
    pub plan: u64,
    pub entry_id: u32,
    pub files: Vec<AddFileView>,
    pub total: u64,
    /// Key files left out (never opened).
    pub keys: u32,
    /// Hidden files and system litter left out.
    pub hidden: u32,
    /// More files or bytes than one add may copy: nothing would be.
    pub too_many: bool,
}

impl AddPlan {
    pub fn view(&self) -> AddPlanView {
        let total = self
            .files
            .iter()
            .map(|f| f.size)
            .fold(0u64, u64::saturating_add);
        let limits = AddLimits::default();
        AddPlanView {
            plan: self.id,
            entry_id: self.entry,
            files: self
                .files
                .iter()
                .enumerate()
                .map(|(i, f)| AddFileView {
                    index: count(i),
                    name: f.name.clone(),
                    size: f.size,
                    kind: f.kind,
                    category: f.category,
                    rip: f.rip.clone(),
                })
                .collect(),
            total,
            keys: count(self.keys),
            hidden: count(self.hidden),
            too_many: self.files.len() > limits.files || total > limits.bytes,
        }
    }
}

/// One file of a plan to add and its category, as the dialog decided.
/// Files of the plan left out are not added.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AddChoice {
    pub index: u32,
    pub category: CategoryRef,
}

/// How an add started: done at once (small), or copying in the
/// background under `add` (above [`BACKGROUND_FILES`] or
/// [`BACKGROUND_BYTES`], D13), ending with `extras://added`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AddStarted {
    Done { write: ExtrasWriteView },
    Background { add: u64, n: u32, total: u64 },
}

/// `extras://progress`: where a background add is (D13).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AddProgressEvent {
    pub entry_id: u32,
    pub add: u64,
    pub i: u32,
    pub n: u32,
    pub done: u64,
    pub total: u64,
}

/// How a background add ended.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AddEnd {
    /// Copied (all, or what was copied before «Cancelar»: see the notes).
    Done {
        write: ExtrasWriteView,
    },
    Failed {
        error: IpcError,
    },
}

/// `extras://added`: a background add ended.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AddFinishedEvent {
    pub entry_id: u32,
    pub add: u64,
    pub end: AddEnd,
}

/// What a background add tells the webview.
pub trait ExtrasAddEvents: Send + Sync {
    fn progress(&self, event: AddProgressEvent);
    fn finished(&self, event: AddFinishedEvent);
    /// The entry's new counts (`library-entries-updated`).
    fn entries_updated(&self, event: EntriesUpdatedEvent);
}

/// The add plans of the session and the cancel flags of running adds.
#[derive(Debug, Default)]
pub struct AddPlans {
    last: u64,
    kept: VecDeque<Arc<AddPlan>>,
    running: Vec<(u64, Arc<AtomicBool>)>,
}

impl AddPlans {
    /// A new add id (plans and drops share the sequence).
    pub fn next_id(&mut self) -> u64 {
        self.last += 1;
        self.last
    }

    /// Keeps `plan`, forgetting the oldest past [`ADD_PLANS_KEPT`].
    pub fn keep(&mut self, plan: AddPlan) -> Arc<AddPlan> {
        let plan = Arc::new(plan);
        self.restore(Arc::clone(&plan));
        plan
    }

    /// Keeps a taken plan again (an add refused before it copied
    /// anything, so the dialog can try again), forgetting the oldest past
    /// [`ADD_PLANS_KEPT`].
    pub fn restore(&mut self, plan: Arc<AddPlan>) {
        if self.kept.iter().any(|p| p.id == plan.id) {
            return;
        }
        while self.kept.len() >= ADD_PLANS_KEPT {
            self.kept.pop_front();
        }
        self.kept.push_back(plan);
    }

    /// Plan `id` of entry `entry` in library `generation`, still kept.
    pub fn get(&self, id: u64, generation: u64, entry: u32) -> Option<Arc<AddPlan>> {
        self.kept
            .iter()
            .find(|p| p.id == id && p.generation == generation && p.entry == entry)
            .cloned()
    }

    /// Plan `id` of entry `entry` in library `generation`, taken (a plan
    /// is committed once).
    pub fn take(&mut self, id: u64, generation: u64, entry: u32) -> Option<Arc<AddPlan>> {
        let at = self
            .kept
            .iter()
            .position(|p| p.id == id && p.generation == generation && p.entry == entry)?;
        self.kept.remove(at)
    }

    /// Marks add `id` as running; its flag stops it between files.
    pub fn start(&mut self, id: u64) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.running.push((id, Arc::clone(&flag)));
        flag
    }

    pub fn end(&mut self, id: u64) {
        self.running.retain(|(r, _)| *r != id);
    }

    /// Asks running add `id` to stop after its current file; false when
    /// no such add runs.
    pub fn cancel(&self, id: u64) -> bool {
        self.running
            .iter()
            .find(|(r, _)| *r == id)
            .map(|(_, f)| f.store(true, Ordering::SeqCst))
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playable_follows_the_allow_lists() {
        assert_eq!(playable_of("01 Opening.FLAC", false), Playable::Inline);
        assert_eq!(playable_of("intro.webm", false), Playable::Inline);
        assert_eq!(playable_of("manual.pdf", false), Playable::Inline);
        assert_eq!(playable_of("notes.nfo", false), Playable::Inline);
        assert_eq!(playable_of("track.spc", false), Playable::External);
        assert_eq!(playable_of("artbook.cbz", false), Playable::External);
        for name in [
            "theme.p3t",
            "avatar.pkg",
            "setup.exe",
            "run.sh",
            "link.lnk",
            "movie.mkv",
            "README",
            "game.rap",
            "prod.keys",
        ] {
            assert_eq!(playable_of(name, false), Playable::RevealOnly, "{name}");
        }
        assert_eq!(playable_of("front.png", true), Playable::RevealOnly);
    }

    #[test]
    fn a_job_is_named_after_the_folder_holding_extras() {
        let root = Path::new("lib");
        let game = root.join("ROMS").join("PS3").join("Resistance (USA)");
        assert_eq!(
            job_game(root, &[game.join("Extras").join("extras.json")]).as_deref(),
            Some("Resistance (USA)")
        );
        let aside = root
            .join(".coverly-trash")
            .join("job")
            .join("ROMS")
            .join("PS3")
            .join("Resistance (USA)")
            .join("extras")
            .join("Soundtrack")
            .join("a.mp3");
        assert_eq!(
            job_game(root, &[aside]).as_deref(),
            Some("Resistance (USA)")
        );
        assert_eq!(job_game(root, &[root.join("ROMS").join("x.png")]), None);
        assert_eq!(job_game(root, &[]), None);
    }

    #[test]
    fn missing_records_keep_their_category() {
        assert_eq!(category_of_path("a.mp3"), CategoryRef::Root);
        assert_eq!(
            category_of_path("soundtrack/a.mp3"),
            CategoryRef::Fixed(ExtrasCategory::Soundtrack)
        );
        assert_eq!(
            category_of_path("Remixes/deep/a.mp3"),
            CategoryRef::User("Remixes".into())
        );
    }

    #[test]
    fn a_listing_of_a_closed_library_never_evicts_the_open_one() {
        let mut cache = ExtrasCache::default();
        let new = cache.store(3, 0, None, empty_listing());
        // A read that started before the library was reopened lands late.
        let late = cache.store(2, 0, None, empty_listing());
        assert!(late.rev > new.rev);
        assert!(
            cache.get(3, 0, new.rev).is_ok(),
            "the open library keeps its listing"
        );
        assert_eq!(
            cache.get(2, 0, late.rev).unwrap_err(),
            IpcError::ExtrasChanged
        );
    }

    #[test]
    fn a_new_generation_forgets_older_listings() {
        let mut cache = ExtrasCache::default();
        let a = cache.store(1, 0, None, empty_listing());
        assert!(cache.get(1, 0, a.rev).is_ok());
        let b = cache.store(2, 1, None, empty_listing());
        assert!(b.rev > a.rev);
        assert_eq!(cache.get(2, 0, a.rev).unwrap_err(), IpcError::ExtrasChanged);
        assert_eq!(cache.get(1, 1, b.rev).unwrap_err(), IpcError::ExtrasChanged);
        assert!(cache.get(2, 1, b.rev).is_ok());
    }

    fn drop_at(tokens: &mut DropTokens, name: &str, now: Instant) -> u64 {
        tokens
            .record(
                vec![PathBuf::from(name)],
                DropPosition { x: 1.0, y: 2.0 },
                now,
            )
            .token
    }

    #[test]
    fn drop_tokens_keep_the_last_four_for_two_minutes() {
        let mut tokens = DropTokens::default();
        let t0 = Instant::now();
        let ids: Vec<u64> = (0..5)
            .map(|i| drop_at(&mut tokens, &format!("{i}.mp3"), t0))
            .collect();
        // The first of five is forgotten; the other four are kept.
        assert_eq!(tokens.take(ids[0], t0), None);
        for (i, id) in ids.iter().enumerate().skip(1) {
            assert_eq!(
                tokens.take(*id, t0 + DROP_TOKEN_TTL),
                Some(vec![PathBuf::from(format!("{i}.mp3"))])
            );
        }
    }

    #[test]
    fn a_token_is_used_once() {
        let mut tokens = DropTokens::default();
        let now = Instant::now();
        let id = drop_at(&mut tokens, "a.mp3", now);
        assert!(tokens.take(id, now).is_some());
        assert_eq!(tokens.take(id, now), None);
        assert_eq!(tokens.take(id + 1, now), None, "never issued");
    }

    #[test]
    fn an_expired_token_is_refused() {
        let mut tokens = DropTokens::default();
        let now = Instant::now();
        let id = drop_at(&mut tokens, "a.mp3", now);
        assert_eq!(
            tokens.take(id, now + DROP_TOKEN_TTL + Duration::from_millis(1)),
            None
        );
        // An expired drop does not hold a place either.
        let old = drop_at(&mut tokens, "b.mp3", now);
        let later = now + DROP_TOKEN_TTL + Duration::from_secs(1);
        let fresh: Vec<u64> = (0..DROP_TOKENS_KEPT)
            .map(|i| drop_at(&mut tokens, &format!("{i}.png"), later))
            .collect();
        assert_eq!(tokens.take(old, later), None);
        assert!(fresh.iter().all(|t| tokens.take(*t, later).is_some()));
    }

    #[test]
    fn a_drop_event_sends_names_sizes_and_never_paths() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("01 Opening.flac");
        fs::write(&file, [7u8; 12]).unwrap();
        let folder = dir.path().join("Scans");
        fs::create_dir(&folder).unwrap();
        let mut tokens = DropTokens::default();
        let event = tokens.record(
            vec![file.clone(), folder.clone()],
            DropPosition { x: 640.0, y: 360.5 },
            Instant::now(),
        );
        assert_eq!(
            event.items,
            [
                DroppedItem {
                    name: "01 Opening.flac".into(),
                    size: 12,
                    is_dir: false
                },
                DroppedItem {
                    name: "Scans".into(),
                    size: 0,
                    is_dir: true
                },
            ]
        );
        let json = serde_json::to_string(&event).unwrap();
        let parent = dir.path().to_string_lossy();
        assert!(!json.contains(parent.as_ref()), "{json}");
        assert!(!json.contains('/') && !json.contains('\\'), "{json}");
        assert_eq!(
            serde_json::to_value(&event).unwrap()["position"],
            serde_json::json!({ "x": 640.0, "y": 360.5 })
        );
        assert_eq!(
            tokens.take(event.token, Instant::now()),
            Some(vec![file, folder])
        );
    }

    #[test]
    fn add_plans_are_taken_once_per_entry_and_library() {
        let mut plans = AddPlans::default();
        let id = plans.next_id();
        plans.keep(AddPlan {
            id,
            generation: 2,
            entry: 5,
            files: Vec::new(),
            keys: 0,
            hidden: 0,
        });
        assert!(plans.take(id, 2, 6).is_none(), "another entry");
        assert!(plans.take(id, 3, 5).is_none(), "another library");
        assert!(plans.take(id, 2, 5).is_some());
        assert!(plans.take(id, 2, 5).is_none(), "used once");
        let flag = plans.start(id);
        assert!(plans.cancel(id));
        assert!(flag.load(Ordering::SeqCst));
        plans.end(id);
        assert!(!plans.cancel(id));
    }
}
