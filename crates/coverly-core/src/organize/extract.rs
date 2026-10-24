//! Extracting an archive's entries into the library (archives spec §9.1).
//!
//! An `Extract` item is committed in phases so that no final file appears
//! until the whole item is known good ([`extract_and_commit`] runs A to C,
//! [`dispose_archive`] is D, once the caller has closed the archive).
//!
//! **Phase A** ([`extract_to_temps`]): decode every wanted entry to a job
//! temp file in its target folder.
//!
//! 1. Every target folder is checked component by component below the
//!    root: each existing one must be a real directory (not a symlink, a
//!    Windows junction or another reparse point), else
//!    [`ExtractError::UnsafeTarget`] and nothing is written. Missing ones
//!    are created one level at a time, each journaled `DirCreated`.
//! 2. Leftover temps of this job in those folders are deleted.
//! 3. One [`ArchiveReader::stream_entries`] pass decodes the entries.
//! 4. Per entry, write-ahead: the `extracted` intent is journaled (and
//!    synced), the temp is created with `create_new` (plus `O_NOFOLLOW` on
//!    Unix, so a planted symlink is refused, never followed), the stream is
//!    copied through a [`VerifyingReader`] (size and CRC against the plan)
//!    with a 1 MiB buffer, the entry's modification time is set (a failure
//!    is ignored: FAT cannot store every date), the file is synced, then
//!    re-read from the medium where the OS allows it and its CRC compared.
//! 5. Sheets (≤ 64 KiB) are kept in memory for the phase B checks.
//!
//! A full volume (`ENOSPC`, `EDQUOT`, `ERROR_DISK_FULL`,
//! `ERROR_HANDLE_DISK_FULL`), on the target or on the journal's volume, is
//! [`ExtractError::DiskFull`]: the item is rolled back and the job pauses
//! ([`ExtractError::pause`]). Any other failure rolls the item back and
//! fails only that item. Cancellation is checked between entries and every
//! 16 MiB inside one. A rollback deletes this item's temps (only files this
//! run created with `create_new`) and journals `rolled-back` for every
//! intent; the archive is never touched.
//!
//! **Phase B** ([`validate`]), still nothing renamed: every sheet parses
//! and each file it names resolves, exactly (case and Unicode form), to a
//! file of the item (a deferred set must also name every track); no final
//! name, nor a case/NFC twin of it, exists yet; the archive still has the
//! plan's size and modification time. A failure rolls the item back.
//!
//! **Phase C** ([`commit`]): each temp is renamed to its final name without
//! ever replacing a file, and its completion line (with the CRC computed
//! while decoding) is journaled; then the generated `.m3u`, journaled
//! `created`. A rename that fails (a file appeared since phase B) rolls
//! the item back: finals renamed so far are deleted when they still match
//! their completion line, temps deleted.
//!
//! **Phase D** ([`dispose_archive`]), move mode with nothing left behind
//! only: the `archive-trash` intent (with the archive's fingerprint), a
//! last check against the plan, the Trash call, then `archive-trashed`
//! with the location, or `archive-kept` when the Trash refused it.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use crate::archive::names::name_key;
use crate::archive::{ArchiveError, ArchiveReader, Budget, Entry, EntryKind, VerifyingReader};
use crate::disc::sheets::{parse_sheet, SheetKind, MAX_SHEET_BYTES};
use crate::organize::apply::user_available;
use crate::organize::plan::{ExtractEntry, SheetCheck, SourceFp};
use crate::progress::PauseKind;
use crate::write::trash::TrashBackend;
use crate::write::{
    mtime_ns, remove_apple_doubles, rename_noclobber_indexed, ExtractedFile, Fingerprint,
    WriteError, Writer,
};

/// Copy and re-read buffer.
pub const CHUNK: usize = 1 << 20;
/// Cancellation is checked at least this often inside one entry.
pub const CANCEL_EVERY: u64 = 16 << 20;
/// Space a journal needs to go on: what a full journal volume reports as
/// needed.
pub const JOURNAL_HEADROOM: u64 = 1 << 20;

/// Sheet extensions kept in memory as they pass (phase B parses them).
const SHEET_EXTS: [&str; 4] = ["cue", "gdi", "ccd", "m3u"];

/// One `Extract` item's phase A.
pub struct ExtractRequest<'a> {
    /// The plan item id (journaled with every line).
    pub item: usize,
    /// The archive (absolute, inside the job's source root).
    pub archive: &'a Path,
    /// The plan's entries; each `target` is relative to the writer's root.
    pub entries: &'a [ExtractEntry],
    /// Checked between entries and every [`CANCEL_EVERY`] bytes, and
    /// handed to the decode budget so the backend stops too while it
    /// decodes bytes this item does not want (a solid block's other
    /// entries) or waits for decoder memory.
    pub cancel: &'a Arc<AtomicBool>,
    /// Called with the bytes decoded for this item so far: the entries
    /// written to temps plus every entry the decoder had to go through
    /// to reach them (the rest of a solid block), out of [`decode_total`].
    pub progress: &'a dyn Fn(u64),
}

/// One entry decoded to its temp and verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedEntry {
    /// The plan entry.
    pub entry: ExtractEntry,
    /// The final path (absolute).
    pub target: PathBuf,
    /// The temp file next to it (absolute).
    pub temp: PathBuf,
    /// What the intent line journaled.
    pub file: ExtractedFile,
    /// CRC-32 of the decoded bytes, equal to the re-read temp's.
    pub crc32: u32,
    /// The bytes of a sheet (≤ 64 KiB), kept for phase B.
    pub bytes: Option<Vec<u8>>,
}

/// What phase A left: every entry of the item in a verified temp, in the
/// order they were decoded. Nothing final exists yet.
#[derive(Debug, Default)]
pub struct PhaseA {
    pub staged: Vec<StagedEntry>,
}

/// Why phase A stopped. The item was rolled back in every case.
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    /// A target folder (or the temp name) is a symlink, a junction, a
    /// reparse point or not a folder: nothing is written through it.
    #[error("unsafe target: {}", path.display())]
    UnsafeTarget { path: PathBuf },
    /// A volume filled up: `needed` bytes to write the item again, `free`
    /// on that volume. The job pauses.
    #[error("disk full: {needed} bytes needed, {free} free")]
    DiskFull { needed: u64, free: u64 },
    #[error("cancelled")]
    Cancelled,
    /// The archive no longer lists an entry as the plan does.
    #[error("archive entry changed since the plan: {entry}")]
    ListingChanged { entry: String },
    /// The decode pass ended without reaching this entry.
    #[error("archive entry not decoded: {entry}")]
    EntryMissing { entry: String },
    /// The temp read back differs from the decoded bytes.
    #[error("the extracted file reads back differently: {entry}")]
    RereadMismatch { entry: String },
    /// Damaged data, a CRC or size mismatch, a tool failure…
    #[error(transparent)]
    Archive(ArchiveError),
    #[error(transparent)]
    Write(WriteError),
    /// The archive's size or modification time differs from the plan.
    #[error("the archive changed since the plan")]
    ChangedSincePlan,
    /// A sheet does not parse, or what it names is not exactly the
    /// extracted set (phase B).
    #[error("the disc set does not check out: {detail}")]
    SetIncomplete { detail: String },
    /// A final name (or a case/NFC twin of it) is taken.
    #[error("taken at the target: {}", path.display())]
    Collision { path: PathBuf },
}

impl ExtractError {
    /// The pause this failure puts the job in: only a full volume (the
    /// next items would fail the same way).
    pub fn pause(&self) -> Option<PauseKind> {
        match *self {
            ExtractError::DiskFull { needed, free } => Some(PauseKind::DiskFull { needed, free }),
            _ => None,
        }
    }
}

/// Test fault points of an extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// Temp writes of the calling thread fail with `StorageFull` once
    /// this many bytes were written to temps (counted per item).
    StorageFullAfter(u64),
    /// The re-read of every temp sees other bytes.
    RereadMismatch,
    /// Setting a temp's modification time fails.
    SetModified,
}

#[cfg(feature = "test-util")]
thread_local! {
    static WRITE_FAULT: std::cell::Cell<Option<Fault>> = const { std::cell::Cell::new(None) };
}

/// Test hook: injects `f` into the extractions of the calling thread.
/// `None` clears it.
#[doc(hidden)]
#[cfg(feature = "test-util")]
pub fn set_write_fault(f: Option<Fault>) {
    WRITE_FAULT.with(|c| c.set(f));
}

fn fault() -> Option<Fault> {
    #[cfg(feature = "test-util")]
    {
        WRITE_FAULT.with(std::cell::Cell::get)
    }
    #[cfg(not(feature = "test-util"))]
    {
        None
    }
}

/// An intent journaled by this item, and whether its temp was created.
struct Pending {
    /// The archive entry index.
    index: usize,
    target: PathBuf,
    temp: PathBuf,
    name: String,
    created: bool,
}

/// Phase A of one `Extract` item (see the module docs). On error the item
/// is already rolled back: no temp of it is left and every intent has its
/// `rolled-back` line (best effort when the journal itself is full).
pub fn extract_to_temps(
    writer: &mut Writer,
    reader: &mut dyn ArchiveReader,
    req: &ExtractRequest,
) -> Result<PhaseA, ExtractError> {
    let root = writer.root().to_path_buf();
    let total: u64 = req.entries.iter().map(|e| e.size).sum();
    let mut targets = Vec::with_capacity(req.entries.len());
    for e in req.entries {
        targets.push(target_path(&root, &e.target)?);
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    for t in &targets {
        let dir = t
            .parent()
            .ok_or_else(|| ExtractError::UnsafeTarget { path: t.clone() })?;
        if !dirs.iter().any(|d| d == dir) {
            dirs.push(dir.to_path_buf());
        }
    }
    for dir in &dirs {
        prepare_dir(writer, &root, dir, total)?;
    }
    writer.clean_temps(&dirs);
    check_listing(reader.entries(), req.entries)?;
    if req.cancel.load(Ordering::Relaxed) {
        return Err(ExtractError::Cancelled);
    }

    let mut wanted: Vec<usize> = req.entries.iter().map(|e| e.index).collect();
    wanted.sort_unstable();
    let decode = decode_set(reader.entries(), req.entries);
    let mut state = Run {
        writer,
        req,
        targets: &targets,
        total,
        decode: &decode,
        written: 0,
        pending: Vec::new(),
        staged: Vec::new(),
        failure: None,
    };
    let mut budget = Budget::apply().with_cancel(Arc::clone(req.cancel));
    let streamed = reader.stream_entries(&wanted, &mut budget, &mut |entry, stream| {
        match state.entry(entry, stream) {
            Ok(()) => Ok(()),
            Err(e) => {
                state.failure = Some(e);
                // A stop signal only: the failure above is reported.
                Err(ArchiveError::Cancelled)
            }
        }
    });
    let result = match (state.failure.take(), streamed) {
        (Some(e), _) => Err(e),
        (None, Err(ArchiveError::Cancelled)) => Err(ExtractError::Cancelled),
        (None, Err(e)) => Err(ExtractError::Archive(e)),
        (None, Ok(())) => match req
            .entries
            .iter()
            .find(|e| !state.staged.iter().any(|s| s.entry.index == e.index))
        {
            Some(missing) => Err(ExtractError::EntryMissing {
                entry: missing.name.clone(),
            }),
            None => Ok(()),
        },
    };
    match result {
        Ok(()) => Ok(PhaseA {
            staged: state.staged,
        }),
        Err(e) => {
            roll_back_pending(state.writer, req.item, &state.pending);
            Err(e)
        }
    }
}

/// Rolls back staged entries (phase B failure, or a cancel after phase
/// A): deletes their temps and journals `rolled-back` for each, best
/// effort. Nothing final exists yet, so nothing else is touched.
pub fn roll_back(writer: &mut Writer, item: usize, staged: &[StagedEntry]) {
    let pending: Vec<Pending> = staged
        .iter()
        .map(|s| Pending {
            index: s.entry.index,
            target: s.target.clone(),
            temp: s.temp.clone(),
            name: s.file.temp.clone(),
            created: true,
        })
        .collect();
    roll_back_pending(writer, item, &pending);
}

fn roll_back_pending(writer: &mut Writer, item: usize, pending: &[Pending]) {
    for p in pending {
        if p.created && fs::symlink_metadata(&p.temp).is_ok_and(|m| m.file_type().is_file()) {
            let _ = fs::remove_file(&p.temp);
            remove_apple_doubles(std::slice::from_ref(&p.temp));
        }
        let _ = writer.rolled_back(&p.target, &p.name, Some(item));
    }
}

/// `(index, size)` of every file entry one decode pass for `wanted` goes
/// through: the wanted ones and every other file of a solid block holding
/// one (it is decoded to reach them), in archive order.
fn decode_set(listing: &[Entry], wanted: &[ExtractEntry]) -> Vec<(usize, u64)> {
    let wants = |i: usize| wanted.iter().any(|w| w.index == i);
    let blocks: Vec<u32> = listing
        .iter()
        .filter(|e| wants(e.index))
        .filter_map(|e| e.solid_block)
        .collect();
    listing
        .iter()
        .filter(|e| {
            wants(e.index)
                || (e.kind == EntryKind::File && e.solid_block.is_some_and(|b| blocks.contains(&b)))
        })
        .map(|e| (e.index, e.size))
        .collect()
}

/// The bytes one decode pass for `wanted` goes through (what
/// [`ExtractRequest::progress`] counts up to): the wanted entries plus
/// the rest of every solid block holding one.
pub fn decode_total(listing: &[Entry], wanted: &[ExtractEntry]) -> u64 {
    decode_set(listing, wanted)
        .iter()
        .map(|(_, size)| size)
        .fold(0u64, |a, b| a.saturating_add(*b))
}

/// Test points between the phases of [`extract_and_commit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookPoint {
    /// Every entry is in its verified temp; nothing is checked yet.
    AfterPhaseA,
    /// The item checked out; nothing is renamed yet.
    AfterPhaseB,
}

#[cfg(feature = "test-util")]
type PhaseHook = Box<dyn Fn(HookPoint)>;

#[cfg(feature = "test-util")]
thread_local! {
    static PHASE_HOOK: std::cell::RefCell<Option<PhaseHook>> = const { std::cell::RefCell::new(None) };
}

/// Test hook: `hook` runs at each [`HookPoint`] of the extractions of the
/// calling thread. `None` clears it.
#[doc(hidden)]
#[cfg(feature = "test-util")]
pub fn set_phase_hook(hook: Option<Box<dyn Fn(HookPoint)>>) {
    PHASE_HOOK.with(|c| *c.borrow_mut() = hook);
}

fn phase_hook(_at: HookPoint) {
    #[cfg(feature = "test-util")]
    PHASE_HOOK.with(|c| {
        if let Some(hook) = c.borrow().as_ref() {
            hook(_at);
        }
    });
}

/// What phases B and C need besides phase A's request.
pub struct CommitRequest<'a> {
    pub extract: ExtractRequest<'a>,
    /// The archive's size and modification time when it was planned.
    pub archive_fp: Option<SourceFp>,
    /// Whether the set's sheet was read at plan time or is checked here.
    pub sheet_check: SheetCheck,
    /// The playlist to generate, relative to the writer's root.
    pub m3u: Option<&'a Path>,
}

/// Phases A, B and C of one `Extract` item: every entry decoded to a
/// verified temp, the whole item checked, then renamed into place (see
/// the module docs). Returns the final paths (the entries in plan order,
/// then the playlist). On error nothing of the item is left in the
/// library and the archive is untouched. Cancellation is honoured until
/// phase C starts; phase C runs to the end.
pub fn extract_and_commit(
    writer: &mut Writer,
    reader: &mut dyn ArchiveReader,
    req: &CommitRequest,
) -> Result<Vec<PathBuf>, ExtractError> {
    let item = req.extract.item;
    // Resolved before phase A: a refusal must not leave temps behind.
    let m3u = match req.m3u {
        Some(rel) => Some(target_path(writer.root(), rel)?),
        None => None,
    };
    let a = extract_to_temps(writer, reader, &req.extract)?;
    phase_hook(HookPoint::AfterPhaseA);
    let checked = validate(&a, req, m3u.as_deref()).and_then(|()| {
        if req.extract.cancel.load(Ordering::Relaxed) {
            Err(ExtractError::Cancelled)
        } else {
            Ok(())
        }
    });
    if let Err(e) = checked {
        roll_back(writer, item, &a.staged);
        return Err(e);
    }
    phase_hook(HookPoint::AfterPhaseB);
    commit(writer, &a, req.extract.entries, item, m3u.as_deref())
}

/// Phase B: the whole item checks out (nothing is renamed yet).
pub fn validate(a: &PhaseA, req: &CommitRequest, m3u: Option<&Path>) -> Result<(), ExtractError> {
    check_sheets(&a.staged, req.sheet_check)?;
    check_names_free(&a.staged, m3u)?;
    if !archive_unchanged(req.extract.archive, req.archive_fp) {
        return Err(ExtractError::ChangedSincePlan);
    }
    Ok(())
}

/// The archive still has the size and modification time of `fp`.
fn archive_unchanged(archive: &Path, fp: Option<SourceFp>) -> bool {
    let Some(fp) = fp else {
        return true;
    };
    fs::symlink_metadata(archive).is_ok_and(|m| {
        m.file_type().is_file()
            && m.len() == fp.size
            && m.modified().ok().map(mtime_ns) == Some(fp.mtime_ns)
    })
}

fn sheet_kind(name: &str) -> Option<SheetKind> {
    let (_, ext) = name.rsplit_once('.')?;
    SheetKind::from_extension(ext)
}

/// Every sheet kept in memory parses, and each file it names resolves,
/// relative to the sheet's own folder and byte for byte, to a file of the
/// item. A deferred set must hold a sheet, and its sheets must name every
/// other file of the item.
fn check_sheets(staged: &[StagedEntry], check: SheetCheck) -> Result<(), ExtractError> {
    let incomplete = |detail: String| ExtractError::SetIncomplete { detail };
    let mut named: Vec<&Path> = Vec::new();
    let mut sheets = 0usize;
    for s in staged {
        let (Some(kind), Some(bytes)) = (sheet_kind(&s.entry.name), s.bytes.as_deref()) else {
            continue;
        };
        sheets += 1;
        let sheet =
            parse_sheet(kind, bytes).map_err(|e| incomplete(format!("{}: {e}", s.entry.name)))?;
        let dir = s.target.parent().unwrap_or(&s.target);
        for file in &sheet.files {
            let resolved = resolve_ref(dir, file)
                .ok_or_else(|| incomplete(format!("{} names {file:?}", s.entry.name)))?;
            let Some(hit) = staged.iter().find(|o| o.target == resolved) else {
                return Err(incomplete(format!(
                    "{} names {file:?}, not in the archive",
                    s.entry.name
                )));
            };
            named.push(&hit.target);
        }
    }
    if check == SheetCheck::Deferred {
        if sheets == 0 {
            return Err(incomplete("no sheet in the archive".into()));
        }
        if let Some(stray) = staged
            .iter()
            .find(|s| sheet_kind(&s.entry.name).is_none() && !named.contains(&s.target.as_path()))
        {
            return Err(incomplete(format!(
                "{} is named by no sheet",
                stray.entry.name
            )));
        }
    }
    Ok(())
}

/// `dir/file` for a file a sheet names: plain components only (`/` or
/// `\` separated); `None` for an absolute, empty or `..` reference.
fn resolve_ref(dir: &Path, file: &str) -> Option<PathBuf> {
    let mut out = dir.to_path_buf();
    let mut any = false;
    for part in file.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            p if p.contains(':') => return None,
            p => {
                out.push(p);
                any = true;
            }
        }
    }
    (any && !file.starts_with(['/', '\\'])).then_some(out)
}

/// No final name of the item (nor the playlist) is taken in its folder,
/// ignoring case and Unicode form, and no two of them share a key. One
/// listing per folder.
fn check_names_free(staged: &[StagedEntry], m3u: Option<&Path>) -> Result<(), ExtractError> {
    let mut listed: Vec<(PathBuf, Vec<String>)> = Vec::new();
    let mut ours: Vec<(PathBuf, String)> = Vec::new();
    let finals = staged.iter().map(|s| s.target.as_path()).chain(m3u);
    for target in finals {
        let (Some(dir), Some(name)) = (target.parent(), target.file_name()) else {
            return Err(ExtractError::Collision {
                path: target.to_path_buf(),
            });
        };
        let key = name_key(&name.to_string_lossy());
        if !listed.iter().any(|(d, _)| d == dir) {
            let keys = match fs::read_dir(dir) {
                Ok(entries) => entries
                    .flatten()
                    .map(|e| name_key(&e.file_name().to_string_lossy()))
                    .collect(),
                Err(e) if e.kind() == ErrorKind::NotFound => Vec::new(),
                Err(e) => return Err(ExtractError::Write(WriteError::Io(e))),
            };
            listed.push((dir.to_path_buf(), keys));
        }
        let on_disk = listed
            .iter()
            .any(|(d, keys)| d == dir && keys.contains(&key));
        let twice = ours.iter().any(|(d, k)| d == dir && *k == key);
        if on_disk || twice {
            return Err(ExtractError::Collision {
                path: target.to_path_buf(),
            });
        }
        ours.push((dir.to_path_buf(), key));
    }
    Ok(())
}

/// The playlist of an item: one line per disc sheet (a `.gdi` wins over
/// a `.cue` of the same stem), relative to the playlist's folder with `/`,
/// in name order; UTF-8 without BOM, `\n` line ends.
pub fn m3u_text(sheets: &[&Path], m3u: &Path) -> Option<String> {
    let dir = m3u.parent()?;
    let mut lines: Vec<(String, bool)> = Vec::new();
    for sheet in sheets {
        let rel = sheet.strip_prefix(dir).ok()?;
        let line = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let gdi = sheet_kind(&line) == Some(SheetKind::Gdi);
        let stem = line
            .rsplit_once('.')
            .map_or(line.as_str(), |(s, _)| s)
            .to_string();
        match lines
            .iter_mut()
            .find(|(l, _)| l.rsplit_once('.').map_or(l.as_str(), |(s, _)| s) == stem)
        {
            Some(existing) if gdi && !existing.1 => *existing = (line, true),
            Some(_) => {}
            None => lines.push((line, gdi)),
        }
    }
    if lines.is_empty() {
        return None;
    }
    lines.sort();
    let mut text = String::new();
    for (line, _) in lines {
        text.push_str(&line);
        text.push('\n');
    }
    Some(text)
}

/// Phase C: every temp renamed to its final name (never replacing a
/// file) and journaled complete, then the playlist. A failure rolls the
/// whole item back. Returns the finals in plan order, then the playlist.
pub fn commit(
    writer: &mut Writer,
    a: &PhaseA,
    plan: &[ExtractEntry],
    item: usize,
    m3u: Option<&Path>,
) -> Result<Vec<PathBuf>, ExtractError> {
    let total: u64 = a.staged.iter().map(|s| s.entry.size).sum();
    let mut renamed: Vec<(&StagedEntry, Option<Fingerprint>)> = Vec::new();
    for (n, s) in a.staged.iter().enumerate() {
        let dir = s.target.parent().unwrap_or(&s.target);
        if let Err(e) = rename_noclobber_indexed(&s.temp, &s.target) {
            undo_commit(writer, item, &renamed, &a.staged[n..]);
            return Err(if e.kind() == ErrorKind::AlreadyExists {
                ExtractError::Collision {
                    path: s.target.clone(),
                }
            } else {
                classify_io(e, dir, total)
            });
        }
        remove_apple_doubles(std::slice::from_ref(&s.temp));
        match writer.extract_done(&s.target, &s.file, s.crc32, Some(item)) {
            Ok(fp) => renamed.push((s, Some(fp))),
            Err(e) => {
                // Renamed but not journaled complete: ours (just renamed),
                // so it goes too.
                renamed.push((s, None));
                undo_commit(writer, item, &renamed, &a.staged[n + 1..]);
                return Err(classify_journal(e, writer.journal_path()));
            }
        }
    }
    let mut finals: Vec<PathBuf> = plan
        .iter()
        .filter_map(|p| a.staged.iter().find(|s| s.entry.index == p.index))
        .map(|s| s.target.clone())
        .collect();
    if let Some(m3u) = m3u {
        let sheets: Vec<&Path> = a
            .staged
            .iter()
            .filter(|s| {
                matches!(
                    sheet_kind(&s.entry.name),
                    Some(SheetKind::Cue | SheetKind::Gdi)
                )
            })
            .map(|s| s.target.as_path())
            .collect();
        let written = match m3u_text(&sheets, m3u) {
            Some(text) => writer.create_new_file(m3u, text.as_bytes(), Some(item)),
            None => Err(WriteError::NameInvalid(m3u.display().to_string())),
        };
        if let Err(e) = written {
            undo_commit(writer, item, &renamed, &[]);
            let dir = m3u.parent().unwrap_or(m3u);
            return Err(match e {
                WriteError::Io(e) if e.kind() == ErrorKind::AlreadyExists => {
                    ExtractError::Collision {
                        path: m3u.to_path_buf(),
                    }
                }
                WriteError::NameInvalid(detail) => ExtractError::SetIncomplete { detail },
                other => classify_write(other, dir, total),
            });
        }
        finals.push(m3u.to_path_buf());
    }
    Ok(finals)
}

/// Rolls back a phase C that stopped: finals renamed so far are deleted
/// when they still match their completion line (or were renamed this
/// very moment, with no line yet), temps not renamed yet are deleted, and
/// every intent gets its `rolled-back` line. Best effort.
fn undo_commit(
    writer: &mut Writer,
    item: usize,
    renamed: &[(&StagedEntry, Option<Fingerprint>)],
    not_renamed: &[StagedEntry],
) {
    for (s, fp) in renamed {
        let ours = match fp {
            Some(fp) => fs::symlink_metadata(&s.target).is_ok_and(|m| {
                m.file_type().is_file()
                    && m.len() == fp.size
                    && m.modified().ok().map(mtime_ns) == Some(fp.mtime_ns)
            }),
            None => fs::symlink_metadata(&s.target).is_ok_and(|m| m.file_type().is_file()),
        };
        if ours {
            let _ = fs::remove_file(&s.target);
            remove_apple_doubles(std::slice::from_ref(&s.target));
        }
        let _ = writer.rolled_back(&s.target, &s.file.temp, Some(item));
    }
    roll_back(writer, item, not_renamed);
}

/// What phase D did with the archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchiveFate {
    /// In the Trash, at `trash` when the OS said where.
    Trashed { trash: Option<PathBuf> },
    /// Still where it was: the Trash refused it, or it changed since the
    /// plan (`detail` says which).
    Kept { detail: String },
}

/// Phase D for a move-mode item with nothing left in its archive: called
/// after phase C, once the caller closed the archive. The intent line
/// carries the archive's fingerprint; the archive goes to the Trash only
/// if it still is the file the plan saw. Never deletes it.
pub fn dispose_archive(
    writer: &mut Writer,
    item: usize,
    archive: &Path,
    plan_fp: Option<SourceFp>,
    trash: &dyn TrashBackend,
) -> ArchiveFate {
    let kept = |writer: &mut Writer, detail: String| {
        let _ = writer.archive_kept(archive, Some(item));
        ArchiveFate::Kept { detail }
    };
    let fp = match writer.archive_trash_intent(archive, Some(item)) {
        Ok(fp) => fp,
        // Not journaled: never trashed (undo could not find it again).
        Err(e) => {
            return ArchiveFate::Kept {
                detail: e.to_string(),
            }
        }
    };
    let same = plan_fp.is_none_or(|p| p.size == fp.size && p.mtime_ns == fp.mtime_ns);
    if !same {
        return kept(writer, "the archive changed since the plan".into());
    }
    match trash.trash(archive) {
        Ok(location) => {
            let _ = writer.archive_trashed(archive, location.as_deref(), Some(item));
            ArchiveFate::Trashed { trash: location }
        }
        Err(e) => kept(writer, e.to_string()),
    }
}

/// `root/rel` for a plan target: relative, with plain components only.
fn target_path(root: &Path, rel: &Path) -> Result<PathBuf, ExtractError> {
    let plain = rel.components().all(|c| matches!(c, Component::Normal(_)));
    if !plain || rel.as_os_str().is_empty() {
        return Err(ExtractError::UnsafeTarget {
            path: root.join(rel),
        });
    }
    Ok(root.join(rel))
}

/// Checks every existing folder from below `root` down to `dir`, creates
/// the missing ones (journaled, one level at a time), then checks again:
/// a folder swapped for a link meanwhile is still refused.
fn prepare_dir(
    writer: &mut Writer,
    root: &Path,
    dir: &Path,
    needed: u64,
) -> Result<(), ExtractError> {
    check_components(root, dir)?;
    writer.ensure_dir(dir).map_err(|e| {
        let journal = writer.journal_path().to_path_buf();
        match classify_write(e, dir, needed) {
            // `ensure_dir` journals each folder before making it: a full
            // volume is the journal's when the library still has room.
            ExtractError::DiskFull { free, .. } if free >= needed => {
                classify_journal(WriteError::DiskFull, &journal)
            }
            other => other,
        }
    })?;
    check_components(root, dir)
}

fn check_components(root: &Path, dir: &Path) -> Result<(), ExtractError> {
    let rel = dir
        .strip_prefix(root)
        .map_err(|_| ExtractError::UnsafeTarget {
            path: dir.to_path_buf(),
        })?;
    let mut at = root.to_path_buf();
    for c in rel.components() {
        at.push(c);
        match fs::symlink_metadata(&at) {
            Ok(meta) if is_real_dir(&meta) => {}
            Ok(_) => return Err(ExtractError::UnsafeTarget { path: at }),
            // Missing from here on: created by `ensure_dir`.
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(ExtractError::Write(WriteError::Io(e))),
        }
    }
    Ok(())
}

/// A directory that is not a symlink, a junction or another reparse point.
fn is_real_dir(meta: &fs::Metadata) -> bool {
    let ft = meta.file_type();
    if ft.is_symlink() || !ft.is_dir() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    true
}

/// The reader still lists every plan entry as a file of the same name and
/// size.
fn check_listing(listing: &[Entry], wanted: &[ExtractEntry]) -> Result<(), ExtractError> {
    for (i, w) in wanted.iter().enumerate() {
        let same = listing
            .iter()
            .find(|e| e.index == w.index)
            .is_some_and(|e| {
                e.kind == EntryKind::File
                    && e.unsafe_name.is_none()
                    && e.size == w.size
                    && e.name.as_str() == w.name
                    && (w.crc32.is_none() || e.crc32.is_none() || e.crc32 == w.crc32)
            });
        let twice = wanted[..i].iter().any(|o| o.index == w.index);
        if !same || twice {
            return Err(ExtractError::ListingChanged {
                entry: w.name.clone(),
            });
        }
    }
    Ok(())
}

/// The state of one decode pass.
struct Run<'r, 'a> {
    writer: &'r mut Writer,
    req: &'r ExtractRequest<'a>,
    targets: &'r [PathBuf],
    total: u64,
    /// `(index, size)` of every entry the decode pass goes through.
    decode: &'r [(usize, u64)],
    /// Bytes of this item written to temps so far.
    written: u64,
    pending: Vec<Pending>,
    staged: Vec<StagedEntry>,
    failure: Option<ExtractError>,
}

impl Run<'_, '_> {
    fn entry(&mut self, entry: &Entry, stream: &mut dyn Read) -> Result<(), ExtractError> {
        if self.req.cancel.load(Ordering::Relaxed) {
            return Err(ExtractError::Cancelled);
        }
        let Some(pos) = self.req.entries.iter().position(|e| e.index == entry.index) else {
            // Not ours (a backend handing out more than asked): skipped.
            return Ok(());
        };
        if self.pending.iter().any(|p| p.index == entry.index) {
            // A backend handing one entry out twice: never staged twice.
            return Err(ExtractError::Archive(ArchiveError::Damaged {
                detail: format!("entry {} streamed twice", entry.index),
            }));
        }
        let want = self.req.entries[pos].clone();
        let target = self.targets[pos].clone();
        let dir = target.parent().unwrap_or(&target).to_path_buf();
        let name = self.writer.extract_temp();
        let temp = dir.join(&name);
        let file = ExtractedFile {
            from: self.req.archive.to_path_buf(),
            entry: want.name.clone(),
            size: want.size,
            crc32: want.crc32,
            temp: name.clone(),
        };
        self.writer
            .extract_intent(&target, &file, Some(self.req.item))
            .map_err(|e| classify_journal(e, self.writer.journal_path()))?;
        self.pending.push(Pending {
            index: want.index,
            target: target.clone(),
            temp: temp.clone(),
            name,
            created: false,
        });
        let mut out = match create_temp(&temp) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // Our own leftovers were cleaned: anything here is foreign
                // (a planted symlink), never written through or deleted.
                return Err(ExtractError::UnsafeTarget { path: temp });
            }
            Err(e) => return Err(classify_io(e, &dir, self.total)),
        };
        if let Some(p) = self.pending.last_mut() {
            p.created = true;
        }
        let keep = is_sheet(&want.name) && want.size <= MAX_SHEET_BYTES as u64;
        let mut kept = keep.then(|| Vec::with_capacity(want.size as usize));
        let mut verify = VerifyingReader::new(stream, want.size, want.crc32);
        let mut buf = vec![0u8; CHUNK];
        let mut since_check = 0u64;
        let before: u64 = self
            .decode
            .iter()
            .filter(|(i, _)| *i < entry.index)
            .map(|(_, size)| size)
            .sum();
        let mut in_entry = 0u64;
        loop {
            let n = match verify.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => {
                    return Err(match ArchiveError::from(e) {
                        ArchiveError::Cancelled => ExtractError::Cancelled,
                        other => ExtractError::Archive(other),
                    })
                }
            };
            self.write_chunk(&mut out, &buf[..n], &dir)?;
            if let Some(k) = kept.as_mut() {
                k.extend_from_slice(&buf[..n]);
            }
            in_entry += n as u64;
            (self.req.progress)(before.saturating_add(in_entry));
            since_check += n as u64;
            if since_check >= CANCEL_EVERY {
                since_check = 0;
                if self.req.cancel.load(Ordering::Relaxed) {
                    return Err(ExtractError::Cancelled);
                }
            }
        }
        let Some(crc32) = verify.computed_crc32() else {
            // Not reached: EOF without a verified size reports an error.
            return Err(ExtractError::Archive(ArchiveError::Damaged {
                detail: "size".into(),
            }));
        };
        if let Some(mtime) = entry.mtime {
            // FAT cannot store years before 1980 or after 2107, and a
            // read-only attribute refuses it: never fails the item.
            let _ = set_modified(&out, mtime);
        }
        out.sync_all()
            .map_err(|e| classify_io(e, &dir, self.total))?;
        drop(out);
        let again = reread_crc(&temp).map_err(|e| classify_io(e, &dir, self.total))?;
        if again != Some((want.size, crc32)) {
            return Err(ExtractError::RereadMismatch { entry: want.name });
        }
        self.staged.push(StagedEntry {
            entry: want,
            target,
            temp,
            file,
            crc32,
            bytes: kept,
        });
        Ok(())
    }

    fn write_chunk(
        &mut self,
        out: &mut File,
        chunk: &[u8],
        dir: &Path,
    ) -> Result<(), ExtractError> {
        let allowed = match fault() {
            Some(Fault::StorageFullAfter(limit)) => {
                let room = limit.saturating_sub(self.written);
                usize::try_from(room).unwrap_or(usize::MAX).min(chunk.len())
            }
            _ => chunk.len(),
        };
        let result = out.write_all(&chunk[..allowed]).and_then(|()| {
            if allowed < chunk.len() {
                Err(ErrorKind::StorageFull.into())
            } else {
                Ok(())
            }
        });
        self.written += allowed as u64;
        result.map_err(|e| classify_io(e, dir, self.total))
    }
}

fn is_sheet(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| SHEET_EXTS.iter().any(|s| s.eq_ignore_ascii_case(ext)))
}

/// `create_new` (`O_CREAT | O_EXCL`, which refuses any existing name, a
/// dangling symlink included), plus `O_NOFOLLOW` on Unix.
fn create_temp(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    opts.open(path)
}

fn set_modified(file: &File, mtime: SystemTime) -> io::Result<()> {
    if fault() == Some(Fault::SetModified) {
        return Err(io::Error::other("set_modified fault"));
    }
    file.set_modified(mtime)
}

/// Size and CRC-32 of the temp read back, past the page cache where the
/// OS allows it (`F_NOCACHE` on macOS, `POSIX_FADV_DONTNEED` on Linux; a
/// normal read elsewhere, which may hit the cache).
fn reread_crc(path: &Path) -> io::Result<Option<(u64, u32)>> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = opts.open(path)?;
    bypass_cache(&file);
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; CHUNK];
    let mut size = 0u64;
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                hasher.update(&buf[..n]);
                size += n as u64;
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let mut crc = hasher.finalize();
    if fault() == Some(Fault::RereadMismatch) {
        crc ^= 1;
    }
    Ok(Some((size, crc)))
}

#[cfg(target_os = "macos")]
fn bypass_cache(file: &File) {
    use std::os::unix::io::AsRawFd;
    // SAFETY: `fcntl` on an open descriptor with an integer argument.
    unsafe {
        libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1);
    }
}

#[cfg(target_os = "linux")]
fn bypass_cache(file: &File) {
    use std::os::unix::io::AsRawFd;
    // SAFETY: `posix_fadvise` on an open descriptor; the data was synced,
    // so dropping its clean pages makes the read go to the medium.
    unsafe {
        libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn bypass_cache(_: &File) {}

/// Whether `e` says the volume (or the user's quota) is full.
pub fn is_disk_full(e: &io::Error) -> bool {
    if matches!(e.kind(), ErrorKind::StorageFull | ErrorKind::QuotaExceeded) {
        // A spent decode budget is `QuotaExceeded` too (AP-8): never a
        // full disk.
        return !crate::archive::is_budget_spent(e);
    }
    match e.raw_os_error() {
        #[cfg(unix)]
        Some(code) => code == libc::ENOSPC || code == libc::EDQUOT,
        // ERROR_HANDLE_DISK_FULL, ERROR_DISK_FULL.
        #[cfg(windows)]
        Some(code) => code == 39 || code == 112,
        _ => false,
    }
}

fn disk_full(dir: &Path, needed: u64) -> ExtractError {
    ExtractError::DiskFull {
        needed,
        free: user_available(dir).unwrap_or(0),
    }
}

fn classify_io(e: io::Error, dir: &Path, needed: u64) -> ExtractError {
    if is_disk_full(&e) {
        disk_full(dir, needed)
    } else {
        ExtractError::Write(WriteError::Io(e))
    }
}

fn classify_write(e: WriteError, dir: &Path, needed: u64) -> ExtractError {
    match e {
        WriteError::DiskFull => disk_full(dir, needed),
        WriteError::Io(e) => classify_io(e, dir, needed),
        other => ExtractError::Write(other),
    }
}

/// A journal append failed: a full journal volume pauses like a full
/// target, reporting that volume.
fn classify_journal(e: WriteError, journal: &Path) -> ExtractError {
    let dir = journal.parent().unwrap_or(journal);
    classify_write(e, dir, JOURNAL_HEADROOM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheets_are_kept_by_extension() {
        assert!(is_sheet("Game.cue"));
        assert!(is_sheet("disc/Game.GDI"));
        assert!(!is_sheet("Game (Track 1).bin"));
        assert!(!is_sheet("cue"));
    }

    #[test]
    fn plan_targets_must_be_plain_relative_paths() {
        let root = Path::new("/lib");
        assert!(target_path(root, Path::new("PS/Game.cue")).is_ok());
        assert!(target_path(root, Path::new("../x.bin")).is_err());
        assert!(target_path(root, Path::new("PS/./x.bin")).is_ok());
        assert!(target_path(root, Path::new("")).is_err());
    }

    #[test]
    fn full_volumes_are_recognised_and_budgets_are_not() {
        assert!(is_disk_full(&io::Error::from(ErrorKind::StorageFull)));
        assert!(is_disk_full(&io::Error::from(ErrorKind::QuotaExceeded)));
        let spent: io::Error = ArchiveError::BudgetSpent {
            what: crate::archive::BudgetKind::DecodedBytes,
        }
        .into();
        assert!(!is_disk_full(&spent));
        assert!(!is_disk_full(&io::Error::from(ErrorKind::PermissionDenied)));
    }
}
