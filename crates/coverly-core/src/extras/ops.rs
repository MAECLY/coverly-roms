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

use super::index::{list, read_index_bytes, serialize, IndexState, Listing};
use super::{normalize_tags, ExtrasFile, NameRefusal, OriginHow, TagRefusal, EXTRAS_JSON};
use crate::game_folder::FixedFolder;
use crate::time::rfc3339_utc;
use crate::write::trash::TrashBackend;
use crate::write::{
    ensure_within, JobKind, JobMeta, OverwritePolicy, WriteError, WriteOutcome, Writer,
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
/// yet. Anything else is [`ExtrasError::OutsideExtras`].
pub fn confine(dir: &Path, target: &Path) -> Result<PathBuf, ExtrasError> {
    let outside = || ExtrasError::OutsideExtras(target.to_path_buf());
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
                if r.origin.how == OriginHow::Found {
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
