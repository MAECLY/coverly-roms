//! Journaled writes inside a game's `Extras/` (extras spec D3, D5, D8, D10,
//! D13, D14, D20; plan P-1, P-3).
//!
//! One panel action is one [`JobKind::Extras`] job rooted at the library
//! root (P-1), whose every target is first confined to the game's
//! `Extras/` (or, for the no-Trash fallback, the job's own folder in
//! `<library root>/.coverly-trash/`). Every job ends by writing
//! `extras.json` through [`Writer::write`] with a backup of the previous
//! file, so undo restores it byte for byte (D3); an unreadable index is
//! copied aside first (D5), a newer one refuses every write.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

use super::index::{
    list, read_index_bytes, rel_components, serialize, IndexState, ListedFile, Listing, Match,
};
use super::{
    is_reserved_key, normalize_tags, validate_category_name, validate_file_stem, CategoryMeta,
    CategoryRef, ExtrasCategory, ExtrasFile, NameRefusal, TagRefusal, EXTRAS_JSON,
};
use crate::dedupe::apply::{trash_file, TrashStep};
use crate::game_folder::FixedFolder;
use crate::organize::target::WINDOWS_MAX_PATH;
use crate::time::rfc3339_utc;
use crate::write::trash::TrashBackend;
use crate::write::{
    ensure_within, name_key, Collision, DirOutcome, JobKind, JobMeta, MoveOutcome, OverwritePolicy,
    WriteError, WriteOutcome, Writer,
};

/// The folder below the library root where files are set aside when a
/// drive has no Trash (D8).
pub const SET_ASIDE_DIR: &str = ".coverly-trash";

/// Why an Extras write was refused or stopped. Nothing is written for a
/// refusal; the UI and the CLI render the reason.
#[derive(Debug, thiserror::Error)]
pub enum ExtrasError {
    #[error("extras.json is from a newer Coverly")]
    IndexNewer,
    /// `extras.json` is a symlink, a folder, too large or unreadable: it
    /// cannot be copied aside, so it is never replaced (D5).
    #[error("extras.json cannot be read, so it is not replaced")]
    IndexUnreadable,
    #[error("the files changed since they were listed")]
    Changed,
    #[error("outside Extras/: {0}")]
    OutsideExtras(PathBuf),
    #[error("name refused: {0:?}")]
    Name(NameRefusal),
    #[error("tags refused: {0:?}")]
    Tags(TagRefusal),
    #[error("no Trash on this drive and no place to set it aside")]
    NoTrash,
    #[error("too many files or bytes for one action")]
    TooMany,
    #[error("not enough space: {short} bytes short")]
    NoSpace { short: u64 },
    #[error("a symlink is not followed: {0}")]
    Link(PathBuf),
    #[error("cancelled")]
    Cancelled,
    #[error(transparent)]
    Write(#[from] WriteError),
}

/// Something the user should know about a finished Extras job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExtrasNote {
    /// The unreadable `extras.json` was copied to `file` (in `Extras/`)
    /// before a fresh one was written (D5).
    MalformedSaved {
        file: String,
    },
    RenamedDup {
        name: String,
    },
    Sanitised {
        names: Vec<String>,
    },
    KeysSkipped {
        n: usize,
    },
    HiddenSkipped {
        n: usize,
    },
    SetAside {
        folder: String,
    },
    StaysNoTrash {
        name: String,
    },
    TrashFailed {
        name: String,
    },
    /// The removed category's folder still holds something the app does
    /// not list (system litter, a hidden file): it stays, its listed files
    /// went to the Trash all the same (D8).
    FolderKept {
        folder: String,
    },
    Cancelled {
        copied: usize,
    },
}

/// Where an Extras job writes and what it needs.
pub struct ExtrasCtx<'a> {
    /// The library root (absolute): the job's root (P-1).
    pub library_root: &'a Path,
    /// The game folder, inside the library root.
    pub game_dir: &'a Path,
    /// The app data folder (journals and backups).
    pub data_dir: &'a Path,
    pub trash: &'a dyn TrashBackend,
    /// Injected for backup names and `added_at`.
    pub now: SystemTime,
    /// Whether two paths are on one volume: [`crate::write::same_volume`]
    /// outside tests. Setting a file aside in `.coverly-trash` is only
    /// ever a rename (D8).
    pub same_volume: fn(&Path, &Path) -> bool,
}

/// A finished Extras job: its id (undo), notes and the new listing.
#[derive(Debug)]
pub struct ExtrasOutcome {
    pub job: Option<String>,
    pub notes: Vec<ExtrasNote>,
    pub listing: Listing,
}

/// `target` when it lies strictly inside `dir`: lexically (plain names
/// only) and on disk (its deepest existing ancestor resolves inside `dir`
/// resolved, and the target itself is no symlink). `dir` need not exist
/// yet, but is never a symlink itself (it would redirect every write).
/// Anything else is [`ExtrasError::OutsideExtras`].
pub fn confine(dir: &Path, target: &Path) -> Result<PathBuf, ExtrasError> {
    let outside = || ExtrasError::OutsideExtras(target.to_path_buf());
    if fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(outside());
    }
    let rel = target.strip_prefix(dir).map_err(|_| outside())?;
    if rel.as_os_str().is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(outside());
    }
    let base = resolve(dir).ok_or_else(outside)?;
    let parent = target.parent().and_then(resolve).ok_or_else(outside)?;
    if !parent.starts_with(&base) {
        return Err(outside());
    }
    if fs::symlink_metadata(target).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(outside());
    }
    Ok(target.to_path_buf())
}

/// `path` with its deepest existing ancestor resolved by the file system
/// and the missing rest appended as it is.
fn resolve(path: &Path) -> Option<PathBuf> {
    let mut probe = path;
    let mut rest = Vec::new();
    while fs::symlink_metadata(probe).is_err() {
        rest.push(probe.file_name()?);
        probe = probe.parent()?;
    }
    let mut out = probe.canonicalize().ok()?;
    out.extend(rest.iter().rev());
    Some(out)
}

/// The game's `Extras/` folder: an existing folder of that name in any
/// case (part C counts it so), else `Extras`.
pub fn extras_dir(game_dir: &Path) -> PathBuf {
    let name = FixedFolder::Extras.dir_name();
    let existing = fs::read_dir(game_dir).ok().and_then(|rd| {
        rd.flatten()
            .find(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
                    && e.file_type().is_ok_and(|t| t.is_dir())
            })
            .map(|e| e.file_name())
    });
    game_dir.join(existing.unwrap_or_else(|| name.into()))
}

/// The game folder when it is a real folder strictly inside the library.
fn check_game_dir(ctx: &ExtrasCtx) -> Result<(), ExtrasError> {
    let outside = || ExtrasError::OutsideExtras(ctx.game_dir.to_path_buf());
    if !ctx.library_root.is_absolute() {
        return Err(outside());
    }
    ensure_within(ctx.library_root, ctx.game_dir).map_err(|_| outside())?;
    if !fs::symlink_metadata(ctx.game_dir).is_ok_and(|m| m.is_dir()) {
        return Err(outside());
    }
    Ok(())
}

/// One Extras job in progress: the writer, the index model it will write
/// and what it has to report.
struct ExtrasJob<'a> {
    ctx: &'a ExtrasCtx<'a>,
    w: Writer,
    extras: PathBuf,
    /// The reconciled index (D4): listed files first, in listing order,
    /// then the missing records, then the unreached ones.
    file: ExtrasFile,
    /// How many of `file.files` are listed files.
    listed: usize,
    /// The bytes of an unreadable index, copied aside before the write.
    malformed: Option<Vec<u8>>,
    notes: Vec<ExtrasNote>,
}

impl<'a> ExtrasJob<'a> {
    /// Checks everything a write needs before a journal exists, so a
    /// refusal leaves nothing behind: the game folder is in the library,
    /// the index is neither newer nor unreadable, and `listing` is still
    /// what is on disk (D22). Then starts the job.
    fn begin(ctx: &'a ExtrasCtx<'a>, listing: &Listing) -> Result<Self, ExtrasError> {
        check_game_dir(ctx)?;
        if matches!(listing.index, IndexState::Newer { .. }) {
            return Err(ExtrasError::IndexNewer);
        }
        let extras = extras_dir(ctx.game_dir);
        let fresh = list(&extras);
        match fresh.index {
            IndexState::Newer { .. } => return Err(ExtrasError::IndexNewer),
            IndexState::Malformed {
                unreadable: Some(_),
                ..
            } => return Err(ExtrasError::IndexUnreadable),
            _ => {}
        }
        if fresh != *listing {
            return Err(ExtrasError::Changed);
        }
        let malformed = match fresh.index {
            IndexState::Malformed { .. } => {
                Some(read_index_bytes(&extras).ok_or(ExtrasError::IndexUnreadable)?)
            }
            _ => None,
        };
        let added_at = rfc3339_utc(ctx.now);
        let mut file = fresh.base.clone().unwrap_or_else(ExtrasFile::empty);
        file.version = ExtrasFile::VERSION;
        file.files = fresh
            .files
            .iter()
            .map(|f| {
                let mut r = f.record.clone();
                // Only an implicit record is new: a written `found` one
                // keeps the `added_at` it got on its first write.
                if f.matched == Match::Found {
                    r.added_at.clone_from(&added_at);
                }
                r
            })
            .chain(fresh.missing.iter().cloned())
            .chain(fresh.unreached.iter().cloned())
            .collect();
        let w = Writer::begin_with(
            ctx.library_root,
            ctx.data_dir,
            JobMeta {
                kind: JobKind::Extras,
                ..Default::default()
            },
        )?;
        Ok(Self {
            ctx,
            w,
            extras,
            file,
            listed: fresh.files.len(),
            malformed,
            notes: Vec::new(),
        })
    }

    /// `target` inside `Extras/`, or inside this job's set-aside folder
    /// (D8, P-1).
    fn confine(&self, target: &Path) -> Result<PathBuf, ExtrasError> {
        confine(&self.extras, target).or_else(|e| {
            let aside = self
                .ctx
                .library_root
                .join(SET_ASIDE_DIR)
                .join(self.w.job_id());
            confine(&aside, target).map_err(|_| e)
        })
    }

    /// Creates `Extras/` when it is missing (journaled `DirCreated`); the
    /// only write ever made at the game-folder level.
    fn ensure_extras_dir(&mut self) -> Result<(), ExtrasError> {
        if self.extras.parent() != Some(self.ctx.game_dir) {
            return Err(ExtrasError::OutsideExtras(self.extras.clone()));
        }
        self.w.ensure_dir(&self.extras)?;
        Ok(())
    }

    /// Writes the index (P-3): an unreadable one is first copied to a new
    /// `extras.broken-<YYYYMMDD-HHMMSS>[-n].json`, then `extras.json` is
    /// written with a backup of what it replaces.
    fn write_index(&mut self) -> Result<(), ExtrasError> {
        self.ensure_extras_dir()?;
        if let Some(bad) = self.malformed.take() {
            let name = self.save_malformed(&bad)?;
            self.notes.push(ExtrasNote::MalformedSaved { file: name });
        }
        let target = self.confine(&self.extras.join(EXTRAS_JSON))?;
        match self
            .w
            .write(&target, &serialize(&self.file), OverwritePolicy::Backup)?
        {
            WriteOutcome::Created { .. } | WriteOutcome::Replaced { .. } => Ok(()),
            // A folder or symlink took its place meanwhile: never replaced.
            WriteOutcome::SkippedExists { .. } => Err(ExtrasError::Changed),
        }
    }

    /// Copies the unreadable index to a name nothing holds, journaled
    /// `Created`; never overwrites (a numeric suffix on collision).
    fn save_malformed(&mut self, bytes: &[u8]) -> Result<String, ExtrasError> {
        let stamp = backup_stamp(self.ctx.now);
        for n in 1..=1000u32 {
            let name = if n == 1 {
                format!("extras.broken-{stamp}.json")
            } else {
                format!("extras.broken-{stamp}-{n}.json")
            };
            let target = self.confine(&self.extras.join(&name))?;
            if fs::symlink_metadata(&target).is_ok() {
                continue;
            }
            match self.w.create_new_file(&target, bytes, None) {
                Ok(()) => return Ok(name),
                Err(WriteError::Io(e)) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        Err(ExtrasError::Changed)
    }

    /// Writes the index, closes the journal and lists `Extras/` again.
    fn finish(mut self) -> Result<ExtrasOutcome, ExtrasError> {
        if let Err(e) = self.write_index() {
            self.abandon();
            return Err(e);
        }
        let ExtrasJob {
            w, extras, notes, ..
        } = self;
        let job = w.finish()?;
        Ok(ExtrasOutcome {
            job: Some(job),
            notes,
            listing: list(&extras),
        })
    }

    /// Stops after an error: a job that changed nothing leaves no journal;
    /// one that did is closed, so undo reverts what it did.
    fn abandon(self) {
        if self.w.has_entries() {
            let _ = self.w.finish();
        } else {
            let _ = self.w.discard();
        }
    }
}

/// `20261113-120000` (UTC) for a malformed-file backup name (D5).
fn backup_stamp(now: SystemTime) -> String {
    let t = rfc3339_utc(now); // 2026-11-13T12:00:00Z
    let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
    format!("{}-{}", digits(&t[..10]), digits(&t[11..19]))
}

/// Replaces the tags of listed file `file` (D20). One job writing only
/// the index; refused with [`ExtrasError::Tags`] before anything is
/// journaled when the tags break the rules.
pub fn set_tags(
    ctx: &ExtrasCtx,
    listing: &Listing,
    file: usize,
    tags: &[String],
) -> Result<ExtrasOutcome, ExtrasError> {
    let tags = normalize_tags(tags).map_err(ExtrasError::Tags)?;
    if file >= listing.files.len() {
        return Err(ExtrasError::Changed);
    }
    let mut job = ExtrasJob::begin(ctx, listing)?;
    job.file.files[file].tags = tags;
    job.finish()
}

/// «Olvidar»: drops the missing record `missing` (D4). One job writing
/// only the index; undo brings the record back.
pub fn forget(
    ctx: &ExtrasCtx,
    listing: &Listing,
    missing: usize,
) -> Result<ExtrasOutcome, ExtrasError> {
    if missing >= listing.missing.len() {
        return Err(ExtrasError::Changed);
    }
    let mut job = ExtrasJob::begin(ctx, listing)?;
    let at = job.listed + missing;
    job.file.files.remove(at);
    job.finish()
}

/// The absolute path of listed file `f`.
fn path_of(extras: &Path, f: &ListedFile) -> Result<PathBuf, ExtrasError> {
    let parts = rel_components(&f.path).ok_or(ExtrasError::Changed)?;
    Ok(parts.iter().fold(extras.to_path_buf(), |p, c| p.join(c)))
}

/// `path` relative to `Extras/`, `/`-joined as `extras.json` stores it.
fn rel_of(extras: &Path, path: &Path) -> Result<String, ExtrasError> {
    let rel = path
        .strip_prefix(extras)
        .map_err(|_| ExtrasError::OutsideExtras(path.to_path_buf()))?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| match c {
            Component::Normal(n) => Ok(n.to_string_lossy().into_owned()),
            _ => Err(ExtrasError::OutsideExtras(path.to_path_buf())),
        })
        .collect::<Result<_, _>>()?;
    if parts.is_empty() {
        return Err(ExtrasError::OutsideExtras(path.to_path_buf()));
    }
    Ok(parts.join("/"))
}

/// Listed file `file`, refused when the index is stale (D22) or the file
/// is a symlink (never moved or renamed: it would move the link, and a
/// listing never follows one).
fn listed_file(listing: &Listing, file: usize) -> Result<&ListedFile, ExtrasError> {
    let f = listing.files.get(file).ok_or(ExtrasError::Changed)?;
    if f.link {
        return Err(ExtrasError::Link(PathBuf::from(&f.path)));
    }
    Ok(f)
}

/// Whether another entry of `dir` than `own` takes `name`, ignoring case
/// and Unicode normalization (FAT, exFAT, APFS and NTFS would see it as
/// taken).
fn name_taken(dir: &Path, name: &str, own: &str) -> Result<bool, ExtrasError> {
    let key = name_key(name);
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(WriteError::Io(e).into()),
    };
    for entry in rd {
        let entry = entry.map_err(WriteError::Io)?;
        let other = entry.file_name();
        let other = other.to_string_lossy();
        if other != own && name_key(&other) == key {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Renames listed file `file` to the typed `stem`, keeping its extension
/// (D14). Refused, never rewritten or suffixed, when the name breaks the
/// rules of D1/D14 or another file holds it; a change of case (or of
/// Unicode normalization) only goes through a temporary name, as the
/// no-clobber rename sees the target as taken on case-insensitive disks.
/// The file's record follows it; the same name is no job at all.
pub fn rename(
    ctx: &ExtrasCtx,
    listing: &Listing,
    file: usize,
    stem: &str,
) -> Result<ExtrasOutcome, ExtrasError> {
    check_game_dir(ctx)?;
    let f = listed_file(listing, file)?;
    let extras = extras_dir(ctx.game_dir);
    let from = path_of(&extras, f)?;
    let dir = from.parent().ok_or(ExtrasError::Changed)?.to_path_buf();
    let ext = Path::new(&f.name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let name = validate_file_stem(&dir, stem, ext).map_err(ExtrasError::Name)?;
    if name == f.name {
        return Ok(ExtrasOutcome {
            job: None,
            notes: Vec::new(),
            listing: listing.clone(),
        });
    }
    if name_taken(&dir, &name, &f.name)? {
        return Err(ExtrasError::Name(NameRefusal::Taken));
    }
    let mut job = ExtrasJob::begin(ctx, listing)?;
    let result = job.rename_file(&from, &dir.join(&name), file);
    match result {
        Ok(()) => job.finish(),
        Err(e) => {
            job.abandon();
            Err(e)
        }
    }
}

/// Moves listed files `files` into `to` (D1): the category's folder in
/// its exact spelling when it exists, else its first spelling on disk,
/// else (a fixed category) its English name, created then; the root is
/// `Extras/` itself. A user category must exist (creating one is its own
/// action). Files already in `to` stay; a taken name gets the next free
/// `" (n)"` and a [`ExtrasNote::RenamedDup`], since nobody typed it.
/// One job for all of them; their records follow.
pub fn move_to(
    ctx: &ExtrasCtx,
    listing: &Listing,
    files: &[usize],
    to: &CategoryRef,
) -> Result<ExtrasOutcome, ExtrasError> {
    check_game_dir(ctx)?;
    let mut picked: Vec<usize> = Vec::new();
    for &i in files {
        let f = listed_file(listing, i)?;
        if !same_category(&f.category, to) && !picked.contains(&i) {
            picked.push(i);
        }
    }
    let extras = extras_dir(ctx.game_dir);
    let target_dir = category_dir(&extras, listing, to)?;
    for &i in &picked {
        let name = &listing.files[i].name;
        // At the root the index's names are taken by it and its backups
        // (D1, D5); a numbered name would still read as a backup.
        if target_dir == extras && is_reserved_key(&name_key(name)) {
            return Err(ExtrasError::Name(NameRefusal::Reserved));
        }
        if too_long_for_windows(&target_dir.join(name)) {
            return Err(ExtrasError::Name(NameRefusal::PathTooLong));
        }
    }
    if picked.is_empty() {
        return Ok(ExtrasOutcome {
            job: None,
            notes: Vec::new(),
            listing: listing.clone(),
        });
    }
    let mut job = ExtrasJob::begin(ctx, listing)?;
    for i in picked {
        let from = path_of(&extras, &listing.files[i])?;
        let name = listing.files[i].name.clone();
        if let Err(e) = job.move_file(&from, &target_dir.join(&name), i) {
            job.abandon();
            return Err(e);
        }
    }
    job.finish()
}

/// Whether `path` is longer than Windows opens without long-path support,
/// checked on every OS so a library copied to a PC or a FAT card still
/// opens (D14, spec §10).
fn too_long_for_windows(path: &Path) -> bool {
    path.to_string_lossy().encode_utf16().count() > WINDOWS_MAX_PATH
}

/// Whether two category refs are one category, whatever the spelling (D1).
fn same_category(a: &CategoryRef, b: &CategoryRef) -> bool {
    match (a, b) {
        (CategoryRef::Fixed(x), CategoryRef::Fixed(y)) => x == y,
        (CategoryRef::User(x), CategoryRef::User(y)) => name_key(x) == name_key(y),
        (CategoryRef::Root, CategoryRef::Root) => true,
        _ => false,
    }
}

/// The folder files of `cat` go to (D1). A symlinked category is never
/// written through; a user category that is not listed is refused.
fn category_dir(
    extras: &Path,
    listing: &Listing,
    cat: &CategoryRef,
) -> Result<PathBuf, ExtrasError> {
    let wanted = match cat {
        CategoryRef::Root => return Ok(extras.to_path_buf()),
        CategoryRef::Fixed(c) => c.dir_name(),
        CategoryRef::User(n) => n.as_str(),
    };
    let listed = listing
        .categories
        .iter()
        .find(|c| same_category(&c.category, cat));
    let Some(listed) = listed else {
        return match cat {
            CategoryRef::Fixed(_) => Ok(extras.join(wanted)),
            _ => Err(ExtrasError::Changed),
        };
    };
    if listed.link {
        return Err(ExtrasError::Link(extras.join(wanted)));
    }
    let spelling = listed
        .spellings
        .iter()
        .find(|s| s.as_str() == wanted)
        .or_else(|| listed.spellings.first())
        .map_or(wanted, String::as_str);
    Ok(extras.join(spelling))
}

impl ExtrasJob<'_> {
    /// The record of listed file `file` now lives at `path`.
    fn follow(&mut self, file: usize, path: &Path) -> Result<(), ExtrasError> {
        let rel = rel_of(&self.extras, path)?;
        self.file.files[file].path = rel;
        Ok(())
    }

    /// Renames in one folder to a name checked free; a case-only change
    /// goes through `.coverly-<job>-rename` (two journaled moves).
    fn rename_file(&mut self, from: &Path, to: &Path, file: usize) -> Result<(), ExtrasError> {
        let from = self.confine(from)?;
        let to = self.confine(to)?;
        let same_key = match (from.file_name(), to.file_name()) {
            (Some(a), Some(b)) => name_key(&a.to_string_lossy()) == name_key(&b.to_string_lossy()),
            _ => false,
        };
        let original = from.clone();
        let from = if same_key {
            let temp = from.with_file_name(format!(".coverly-{}-rename", self.w.job_id()));
            let temp = self.confine(&temp)?;
            match self.w.move_file(&from, &temp, Collision::Rename, None)? {
                MoveOutcome::Moved { to, .. } => to,
                _ => return Err(ExtrasError::Changed),
            }
        } else {
            from
        };
        let result = match self.w.move_file(&from, &to, Collision::Skip, None) {
            Ok(MoveOutcome::Moved { to: done, .. }) if done == to => return self.follow(file, &to),
            // Taken since the check: never suffixed (the user typed it).
            Ok(MoveOutcome::SkippedExists { .. }) => Err(ExtrasError::Name(NameRefusal::Taken)),
            Ok(_) => Err(ExtrasError::Changed),
            Err(e) => Err(e.into()),
        };
        // Never leave the file under the hidden temporary name.
        if from != original && fs::symlink_metadata(&from).is_ok() {
            self.w.move_back(&from, &original)?;
        }
        result
    }

    /// Moves to another folder of `Extras/`, creating a missing category
    /// folder (journaled); a taken name gets the next free number.
    fn move_file(&mut self, from: &Path, to: &Path, file: usize) -> Result<(), ExtrasError> {
        let from = self.confine(from)?;
        let to = self.confine(to)?;
        if let Some(parent) = to.parent().filter(|p| *p != self.extras) {
            self.ensure_extras_dir()?;
            self.w.ensure_dir(parent)?;
        }
        match self.w.move_file(&from, &to, Collision::Rename, None)? {
            MoveOutcome::Moved { to, renamed } => {
                // The free number made it too long: stopped here, and the
                // closed journal undoes what this job already moved.
                if too_long_for_windows(&to) {
                    self.w.move_back(&to, &from)?;
                    return Err(ExtrasError::Name(NameRefusal::PathTooLong));
                }
                if renamed {
                    let name = to
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.notes.push(ExtrasNote::RenamedDup { name });
                }
                self.follow(file, &to)
            }
            _ => Err(ExtrasError::Changed),
        }
    }
}

// ---- Trash and own categories (D1, D8, D10) ----

/// Sends listed files `files` to the Trash (D8). On a drive without one,
/// each file is set aside by a rename into
/// `<library root>/.coverly-trash/<job>/<its path in the library>`, and
/// the whole action is refused ([`ExtrasError::NoTrash`], nothing
/// written) when that folder would be on another volume. A file the
/// Trash refuses stays where it is with [`ExtrasNote::TrashFailed`] and
/// keeps its record. Never a permanent delete; one job, undone from the
/// Trash or the set-aside folder.
pub fn trash(
    ctx: &ExtrasCtx,
    listing: &Listing,
    files: &[usize],
) -> Result<ExtrasOutcome, ExtrasError> {
    check_game_dir(ctx)?;
    let mut picked: Vec<usize> = Vec::new();
    for &i in files {
        listed_file(listing, i)?;
        if !picked.contains(&i) {
            picked.push(i);
        }
    }
    if picked.is_empty() {
        return Ok(ExtrasOutcome {
            job: None,
            notes: Vec::new(),
            listing: listing.clone(),
        });
    }
    let extras = extras_dir(ctx.game_dir);
    let paths = picked
        .iter()
        .map(|&i| path_of(&extras, &listing.files[i]))
        .collect::<Result<Vec<_>, _>>()?;
    check_set_aside(ctx, &paths)?;
    let mut job = ExtrasJob::begin(ctx, listing)?;
    let mut gone = Vec::new();
    for (&i, path) in picked.iter().zip(&paths) {
        match job.trash_one(path) {
            Ok(true) => gone.push(i),
            Ok(false) => {}
            Err(e) => {
                job.abandon();
                return Err(e);
            }
        }
    }
    job.forget_listed(gone);
    job.finish()
}

/// Refuses before anything is written when a file has no Trash and its
/// set-aside folder would be on another volume or is not a real folder
/// (D8).
fn check_set_aside(ctx: &ExtrasCtx, paths: &[PathBuf]) -> Result<(), ExtrasError> {
    for path in paths {
        if !ctx.trash.available(path) && !set_aside_ok(ctx, path) {
            return Err(ExtrasError::NoTrash);
        }
    }
    Ok(())
}

/// Whether `path` can be set aside: `.coverly-trash` is absent or a real
/// folder (a symlink would lead the file out of the library, maybe onto
/// another drive by a copy), and the library root is on its volume.
///
/// On Windows its set-aside path (`.coverly-trash\<job>\` longer) must
/// also fit the path limit, so a long one is refused before the job
/// rather than failing half-way through it.
fn set_aside_ok(ctx: &ExtrasCtx, path: &Path) -> bool {
    let top = ctx.library_root.join(SET_ASIDE_DIR);
    let top_ok = match fs::symlink_metadata(&top) {
        Ok(m) => m.is_dir(),
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    };
    top_ok
        && (ctx.same_volume)(path, ctx.library_root)
        && !(cfg!(windows) && aside_too_long(ctx, path))
}

/// Whether `path` set aside would pass the Windows path limit, with room
/// for the longest job id the writer makes (`20261016T030000Z-1a2b-`
/// and a counter of up to 8 hex digits).
fn aside_too_long(ctx: &ExtrasCtx, path: &Path) -> bool {
    const JOB_ID_UNITS: usize = 30;
    let Ok(rel) = path.strip_prefix(ctx.library_root) else {
        return true;
    };
    let target = ctx
        .library_root
        .join(SET_ASIDE_DIR)
        .join("j".repeat(JOB_ID_UNITS))
        .join(rel);
    too_long_for_windows(&target)
}

/// The folder name of an existing user category, refused for a fixed
/// one ([`NameRefusal::Reserved`], D1), a symlink and one not listed.
fn user_category<'l>(
    extras: &Path,
    listing: &'l Listing,
    name: &str,
) -> Result<&'l super::index::ListedCategory, ExtrasError> {
    if ExtrasCategory::from_dir_name(name).is_some() {
        return Err(ExtrasError::Name(NameRefusal::Reserved));
    }
    let wanted = CategoryRef::User(name.to_string());
    let listed = listing
        .categories
        .iter()
        .find(|c| matches!(c.category, CategoryRef::User(_)) && same_category(&c.category, &wanted))
        .ok_or(ExtrasError::Changed)?;
    if listed.link {
        return Err(ExtrasError::Link(extras.join(name)));
    }
    Ok(listed)
}

/// The names of the user categories listed.
fn user_names(listing: &Listing) -> Vec<String> {
    listing
        .categories
        .iter()
        .filter_map(|c| match &c.category {
            CategoryRef::User(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

/// Creates the user category `name` (D1): the typed name trimmed and in
/// NFC, refused (never rewritten) by the rules of
/// [`validate_category_name`], when a loose file in `Extras/` holds it or
/// when the folder's path would be too long for Windows. One job:
/// `Extras/` and the folder created, the index given the category with
/// the default sync (`null`, D21).
pub fn create_category(
    ctx: &ExtrasCtx,
    listing: &Listing,
    name: &str,
) -> Result<ExtrasOutcome, ExtrasError> {
    check_game_dir(ctx)?;
    let extras = extras_dir(ctx.game_dir);
    let name = validate_category_name(name, &user_names(listing)).map_err(ExtrasError::Name)?;
    if name_taken(&extras, &name, "")? {
        return Err(ExtrasError::Name(NameRefusal::Taken));
    }
    let dir = extras.join(&name);
    if too_long_for_windows(&dir) {
        return Err(ExtrasError::Name(NameRefusal::PathTooLong));
    }
    let mut job = ExtrasJob::begin(ctx, listing)?;
    let made = job.ensure_extras_dir().and_then(|()| {
        let dir = job.confine(&dir)?;
        job.w.ensure_dir(&dir)?;
        Ok(())
    });
    if let Err(e) = made {
        job.abandon();
        return Err(e);
    }
    job.file.categories.insert(name, CategoryMeta::default());
    job.finish()
}

/// Renames the user category `from` to the typed `to` (D1): one rename of
/// its folder (through a temporary name when only the case or the Unicode
/// normalization changes), every record below it following, and its
/// settings in the index carried over. Fixed categories are refused
/// ([`NameRefusal::Reserved`]); so is a category with two spellings on a
/// case-sensitive disk ([`NameRefusal::Taken`]: renaming would merge or
/// split them). The same name is no job.
pub fn rename_category(
    ctx: &ExtrasCtx,
    listing: &Listing,
    from: &str,
    to: &str,
) -> Result<ExtrasOutcome, ExtrasError> {
    check_game_dir(ctx)?;
    let extras = extras_dir(ctx.game_dir);
    let listed = user_category(&extras, listing, from)?;
    let [spelling] = listed.spellings.as_slice() else {
        return Err(ExtrasError::Name(NameRefusal::Taken));
    };
    let others: Vec<String> = user_names(listing)
        .into_iter()
        .filter(|n| name_key(n) != name_key(spelling))
        .collect();
    let to = validate_category_name(to, &others).map_err(ExtrasError::Name)?;
    if to == *spelling {
        return Ok(ExtrasOutcome {
            job: None,
            notes: Vec::new(),
            listing: listing.clone(),
        });
    }
    if name_taken(&extras, &to, spelling)? {
        return Err(ExtrasError::Name(NameRefusal::Taken));
    }
    let old_dir = extras.join(spelling);
    let new_dir = extras.join(&to);
    let below = listing
        .files
        .iter()
        .filter(|f| same_category(&f.category, &listed.category));
    for f in below {
        let parts = rel_components(&f.path).ok_or(ExtrasError::Changed)?;
        let path = parts.iter().skip(1).fold(new_dir.clone(), |p, c| p.join(c));
        if too_long_for_windows(&path) {
            return Err(ExtrasError::Name(NameRefusal::PathTooLong));
        }
    }
    let mut job = ExtrasJob::begin(ctx, listing)?;
    if let Err(e) = job.rename_dir(&old_dir, &new_dir) {
        job.abandon();
        return Err(e);
    }
    let old_key = name_key(spelling);
    for r in &mut job.file.files {
        let Some(parts) = rel_components(&r.path) else {
            continue;
        };
        if parts.len() > 1 && name_key(parts[0]) == old_key {
            let rest = parts[1..].join("/");
            r.path = format!("{to}/{rest}");
        }
    }
    let keys: Vec<String> = job
        .file
        .categories
        .keys()
        .filter(|k| name_key(k) == old_key)
        .cloned()
        .collect();
    let mut meta = None;
    for k in keys {
        let m = job.file.categories.remove(&k);
        meta = meta.or(m);
    }
    job.file.categories.insert(to, meta.unwrap_or_default());
    job.finish()
}

/// Removes the user category `name` (D8): its listed files go to the
/// Trash (or are set aside, as [`trash`] does), then its emptied folders
/// are removed, each journaled `DirRemoved` so
/// undo creates them again. A folder that still holds something (system
/// litter, a hidden file, a file the Trash refused) stays, with
/// [`ExtrasNote::FolderKept`]; nothing is ever deleted for good. Fixed
/// categories are refused ([`NameRefusal::Reserved`]).
pub fn remove_category(
    ctx: &ExtrasCtx,
    listing: &Listing,
    name: &str,
) -> Result<ExtrasOutcome, ExtrasError> {
    check_game_dir(ctx)?;
    let extras = extras_dir(ctx.game_dir);
    let listed = user_category(&extras, listing, name)?;
    let mut picked = Vec::new();
    for (i, f) in listing.files.iter().enumerate() {
        if same_category(&f.category, &listed.category) {
            listed_file(listing, i)?;
            picked.push(i);
        }
    }
    let paths = picked
        .iter()
        .map(|&i| path_of(&extras, &listing.files[i]))
        .collect::<Result<Vec<_>, _>>()?;
    check_set_aside(ctx, &paths)?;
    let mut job = ExtrasJob::begin(ctx, listing)?;
    let mut gone = Vec::new();
    for (&i, path) in picked.iter().zip(&paths) {
        match job.trash_one(path) {
            Ok(true) => gone.push(i),
            Ok(false) => {}
            Err(e) => {
                job.abandon();
                return Err(e);
            }
        }
    }
    job.forget_listed(gone);
    let mut kept = false;
    for spelling in &listed.spellings {
        match job.remove_tree(&extras.join(spelling), spelling) {
            Ok(removed) => kept |= !removed,
            Err(e) => {
                job.abandon();
                return Err(e);
            }
        }
    }
    // A kept folder is still listed as the category: its settings stay.
    if !kept {
        let key = name_key(name);
        job.file.categories.retain(|k, _| name_key(k) != key);
    }
    job.finish()
}

impl ExtrasJob<'_> {
    /// Drops the records of listed files `files` (sent away by this job).
    fn forget_listed(&mut self, mut files: Vec<usize>) {
        files.sort_unstable_by(|a, b| b.cmp(a));
        files.dedup();
        for i in files {
            if i < self.listed {
                self.file.files.remove(i);
                self.listed -= 1;
            }
        }
    }

    /// Sends one file to the Trash, or sets it aside when its drive has
    /// none (D8). `Ok(false)`: it stays where it was, with a note.
    fn trash_one(&mut self, path: &Path) -> Result<bool, ExtrasError> {
        let path = self.confine(path)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !self.ctx.trash.available(&path) {
            return self.set_aside(&path);
        }
        match trash_file(&mut self.w, self.ctx.trash, &path, None)? {
            TrashStep::Trashed { .. } => Ok(true),
            TrashStep::Kept { .. } | TrashStep::Gone => {
                self.notes.push(ExtrasNote::TrashFailed { name });
                Ok(false)
            }
        }
    }

    /// Moves `path` into this job's folder in `.coverly-trash`, keeping its
    /// path in the library below it: a journaled rename, so undo puts it
    /// back. Only ever on one volume (checked before the job began).
    fn set_aside(&mut self, path: &Path) -> Result<bool, ExtrasError> {
        let rel = path
            .strip_prefix(self.ctx.library_root)
            .map_err(|_| ExtrasError::OutsideExtras(path.to_path_buf()))?;
        if !set_aside_ok(self.ctx, path) {
            return Err(ExtrasError::NoTrash);
        }
        let top = self.ctx.library_root.join(SET_ASIDE_DIR);
        let folder = top.join(self.w.job_id());
        let target = self.confine(&folder.join(rel))?;
        let parent = target.parent().ok_or(ExtrasError::Changed)?;
        let new_top = fs::symlink_metadata(&top).is_err();
        self.w.ensure_dir(parent)?;
        if new_top {
            hide(&top);
        }
        match self.w.move_file(path, &target, Collision::Skip, None)? {
            MoveOutcome::Moved { .. } => {}
            _ => return Err(ExtrasError::Changed),
        }
        let note = ExtrasNote::SetAside {
            folder: format!("{SET_ASIDE_DIR}/{}", self.w.job_id()),
        };
        if !self.notes.contains(&note) {
            self.notes.push(note);
        }
        Ok(true)
    }

    /// Renames the folder `from` to `to` in `Extras/`; a change of case
    /// only goes through `.coverly-<job>-rename` (two journaled moves),
    /// as the no-clobber rename sees `to` as taken on case-insensitive
    /// disks. A name taken meanwhile is refused, never suffixed.
    fn rename_dir(&mut self, from: &Path, to: &Path) -> Result<(), ExtrasError> {
        let from = self.confine(from)?;
        let to = self.confine(to)?;
        let same_key = match (from.file_name(), to.file_name()) {
            (Some(a), Some(b)) => name_key(&a.to_string_lossy()) == name_key(&b.to_string_lossy()),
            _ => false,
        };
        let original = from.clone();
        let from = if same_key {
            let temp = from.with_file_name(format!(".coverly-{}-rename", self.w.job_id()));
            let temp = self.confine(&temp)?;
            self.move_dir_once(&from, &temp)?
        } else {
            from
        };
        match self.move_dir_once(&from, &to) {
            Ok(_) => Ok(()),
            Err(e) => {
                // Never leave the folder under the hidden temporary name.
                if from != original && fs::symlink_metadata(&from).is_ok() {
                    self.move_dir_once(&from, &original)?;
                }
                Err(e)
            }
        }
    }

    /// One journaled no-clobber rename of a folder; a taken name is
    /// [`NameRefusal::Taken`].
    fn move_dir_once(&mut self, from: &Path, to: &Path) -> Result<PathBuf, ExtrasError> {
        let mut quiet = |_: u64| {};
        match self
            .w
            .move_dir(from, to, Collision::Skip, None, &mut quiet)?
        {
            DirOutcome::Moved { to: done, .. } if done == to => Ok(done),
            DirOutcome::SkippedExists { .. } => Err(ExtrasError::Name(NameRefusal::Taken)),
            _ => Err(ExtrasError::Changed),
        }
    }

    /// Removes the folder `dir` and its emptied sub-folders, deepest first,
    /// each journaled `DirRemoved`. A folder that is not empty stays; when
    /// `dir` itself stays, [`ExtrasNote::FolderKept`] says so. Symlinks
    /// are never entered. `Ok(false)`: `dir` stayed.
    fn remove_tree(&mut self, dir: &Path, shown: &str) -> Result<bool, ExtrasError> {
        let dir = self.confine(dir)?;
        let mut dirs: Vec<PathBuf> = walkdir::WalkDir::new(&dir)
            .follow_links(false)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_dir())
            .map(walkdir::DirEntry::into_path)
            .collect();
        dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
        let mut removed = false;
        for d in dirs {
            let d = self.confine(&d)?;
            match self.w.remove_empty_dir(&d) {
                Ok(()) => removed |= d == dir,
                Err(WriteError::NotEmpty(_)) => {
                    if d == dir {
                        self.notes.push(ExtrasNote::FolderKept {
                            folder: shown.to_string(),
                        });
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(removed)
    }
}

/// Hides the set-aside folder on Windows, where a dot name hides nothing
/// (D8); best effort. Scan, Organizar and Duplicados skip it by name.
#[cfg(windows)]
fn hide(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN, INVALID_FILE_ATTRIBUTES,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is a nul-terminated UTF-16 path that outlives both calls.
    unsafe {
        let attrs = GetFileAttributesW(wide.as_ptr());
        if attrs != INVALID_FILE_ATTRIBUTES {
            SetFileAttributesW(wide.as_ptr(), attrs | FILE_ATTRIBUTE_HIDDEN);
        }
    }
}

#[cfg(not(windows))]
fn hide(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn backup_stamps_are_utc_date_and_time() {
        let t = UNIX_EPOCH + Duration::from_secs(1_794_571_200);
        assert_eq!(backup_stamp(t), "20261113-120000");
        assert_eq!(backup_stamp(UNIX_EPOCH), "19700101-000000");
    }
}
