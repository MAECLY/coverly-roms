//! Setting duplicate copies aside (duplicates spec §7–§9).
//!
//! The Trash action reuses the journaled Trash backend organise built for
//! archives (`write::trash`, plan deviation AP-3): the same three journal
//! ops carry a set-aside ROM and its companions, so undo brings them back
//! from the exact place the OS put them.

use std::path::{Path, PathBuf};

use crate::write::trash::{TrashBackend, TrashError};
use crate::write::{WriteError, Writer};

/// What sending one file to the Trash did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrashStep {
    /// In the Trash, at `location` when the OS said where.
    Trashed { location: Option<PathBuf> },
    /// Still where it was.
    Kept { why: TrashKept },
    /// Nothing was there any more.
    Gone,
}

/// Why a file stayed where it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrashKept {
    /// Its volume has no Trash.
    Unavailable,
    /// The Trash refused it.
    Refused,
}

/// Sends the file `path` (inside the job's source root) to `trash`,
/// journaled for plan item `item`: `archive_trash_intent` before the
/// call (so undo can look the file up by name and fingerprint even if the
/// run dies during it), then `archive_trashed` with the location the OS
/// gave. A volume without a Trash or a refusal journals `archive_kept`
/// and leaves the file in place; a file no longer there is
/// [`TrashStep::Gone`]. A Trash I/O error and a failed journal append are
/// returned with the intent left unresolved: the journal is then kept and
/// undo looks the file up at its place and in the Trash.
#[doc(hidden)]
pub fn trash_file(
    w: &mut Writer,
    trash: &dyn TrashBackend,
    path: &Path,
    item: Option<usize>,
) -> Result<TrashStep, WriteError> {
    match w.archive_trash_intent(path, item) {
        Ok(_) => {}
        Err(WriteError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TrashStep::Gone)
        }
        Err(e) => return Err(e),
    }
    let why = match trash.trash(path) {
        Ok(location) => {
            w.archive_trashed(path, location.as_deref(), item)?;
            return Ok(TrashStep::Trashed { location });
        }
        Err(TrashError::Missing(_)) => {
            // Gone between the intent and the call: nothing left to undo.
            w.archive_kept(path, item)?;
            return Ok(TrashStep::Gone);
        }
        Err(TrashError::Unavailable(_)) => TrashKept::Unavailable,
        Err(TrashError::Refused { .. }) => TrashKept::Refused,
        // Where the file is now is not known: the intent stays the last
        // line, so undo checks its place and the Trash.
        Err(TrashError::Io(e)) => return Err(WriteError::Io(e)),
    };
    w.archive_kept(path, item)?;
    Ok(TrashStep::Kept { why })
}
