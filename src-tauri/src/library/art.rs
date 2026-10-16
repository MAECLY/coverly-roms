//! Reading an entry's files for the webview, only inside the opened root.
//!
//! The webview never sends a path: it names an entry id and a media, and
//! the path comes from the scan. The path is canonicalised (resolving
//! symlinks) and must stay inside the canonical library root.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use coverly_core::profiles::Media;

use super::{Session, SessionEntry};
use crate::error::IpcError;

/// Largest art file served (the core image pipeline's own input cap).
pub const MAX_ART_BYTES: u64 = 20 * 1024 * 1024;

fn media_name(media: Media) -> String {
    match media {
        Media::Cover => "cover",
        Media::Screenshot => "screenshot",
        Media::Title => "title",
    }
    .to_string()
}

fn entry(session: &Session, id: u32) -> Result<&SessionEntry, IpcError> {
    session
        .entries
        .get(id as usize)
        .ok_or(IpcError::UnknownEntry(id))
}

/// `path` canonicalised, if it stays inside the library.
fn inside_root(session: &Session, path: &Path) -> Result<PathBuf, IpcError> {
    let canon =
        fs::canonicalize(path).map_err(|_| IpcError::Unreadable(path.display().to_string()))?;
    if canon.starts_with(&session.root_canon) {
        Ok(canon)
    } else {
        Err(IpcError::OutsideLibrary(path.display().to_string()))
    }
}

/// The bytes of `media` of entry `id`.
pub fn read_art_bytes(session: &Session, id: u32, media: Media) -> Result<Vec<u8>, IpcError> {
    let entry = entry(session, id)?;
    let path = entry
        .art
        .get(&media)
        .ok_or_else(|| IpcError::ArtMissing(media_name(media)))?;
    let canon = match inside_root(session, path) {
        Err(IpcError::Unreadable(_)) => return Err(IpcError::ArtMissing(media_name(media))),
        other => other?,
    };
    let meta = fs::metadata(&canon).map_err(|_| IpcError::ArtMissing(media_name(media)))?;
    if !meta.is_file() {
        return Err(IpcError::ArtMissing(media_name(media)));
    }
    if meta.len() > MAX_ART_BYTES {
        return Err(IpcError::ArtTooLarge(meta.len()));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    File::open(&canon)
        .and_then(|f| f.take(MAX_ART_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| IpcError::Io(e.to_string()))?;
    if bytes.len() as u64 > MAX_ART_BYTES {
        return Err(IpcError::ArtTooLarge(bytes.len() as u64));
    }
    Ok(bytes)
}

/// The primary file of entry `id`, checked to be inside the library.
pub fn entry_path(session: &Session, id: u32) -> Result<PathBuf, IpcError> {
    let entry = entry(session, id)?;
    inside_root(session, &entry.rom.path)
}
