//! Reading an entry's files for the webview, only inside the opened root.
//!
//! The webview never sends a path: it names an entry id and a media, and
//! the path comes from the scan. The path is canonicalised (resolving
//! symlinks) and must stay inside the canonical library root.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use coverly_core::art::children::{child_slot_path, ChildMedia};
use coverly_core::profiles::Media;

use super::{Session, SessionEntry};
use crate::error::IpcError;

/// Largest art file served (the core image pipeline's own input cap).
pub const MAX_ART_BYTES: u64 = 20 * 1024 * 1024;

pub(crate) fn media_name(media: Media) -> String {
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
pub(crate) fn inside_root(session: &Session, path: &Path) -> Result<PathBuf, IpcError> {
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
    read_inside(session, path, || IpcError::ArtMissing(media_name(media)))
}

/// The bytes of the `slot` image of child `child_index` of entry `id`
/// (part C D10): the slot path comes from the scan, never the webview, and
/// must resolve inside the library like any art.
pub fn read_child_art_bytes(
    session: &Session,
    id: u32,
    child_index: u32,
    slot: ChildMedia,
) -> Result<Vec<u8>, IpcError> {
    let entry = entry(session, id)?;
    let child = entry
        .rom
        .children
        .get(child_index as usize)
        .ok_or(IpcError::UnknownChild(child_index))?;
    let missing = || IpcError::ArtMissing(child_media_name(slot));
    let path = child_slot_path(&child.path, slot).map_err(|_| missing())?;
    read_inside(session, &path, missing)
}

fn child_media_name(slot: ChildMedia) -> String {
    match slot {
        ChildMedia::Icon => "icon",
        ChildMedia::Banner => "banner",
    }
    .to_string()
}

/// The bytes of the regular file at `path` once it resolves inside the
/// library, at most [`MAX_ART_BYTES`]; `missing()` when it is not there.
fn read_inside(
    session: &Session,
    path: &Path,
    missing: impl Fn() -> IpcError,
) -> Result<Vec<u8>, IpcError> {
    let canon = match inside_root(session, path) {
        Err(IpcError::Unreadable(_)) => return Err(missing()),
        other => other?,
    };
    let meta = fs::metadata(&canon).map_err(|_| missing())?;
    if !meta.is_file() {
        return Err(missing());
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

#[cfg(test)]
mod tests {
    use coverly_core::dedupe::testing::pin_mtime;
    use coverly_core::scan::ScanOptions;
    use coverly_core::systems::SystemDb;
    use tempfile::TempDir;

    use super::*;
    use crate::library::{open_session, OpenRequest};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake";

    /// Writes `bytes` at `rel` (`/`-separated, joined by components).
    fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, bytes).unwrap();
        pin_mtime(&p, 1);
        p
    }

    /// `ROMS/3DS/Lantern Harbor/` with one DLC; the entry id of the game
    /// and the index of the DLC.
    fn game(lib: &Path) -> (Session, u32, u32) {
        let g = "ROMS/3DS/Lantern Harbor";
        put(lib, &format!("{g}/Lantern Harbor.cia"), &[1u8; 64]);
        put(
            lib,
            &format!("{g}/DLC/Lantern Harbor - Pack.cia"),
            &[2u8; 64],
        );
        let (session, _) = open_session(
            &OpenRequest {
                path: lib,
                profile_id: Some("coverly-library"),
                scan: &ScanOptions::default(),
                generation: 1,
                custom: &[],
                default_profile: None,
                settings: None,
            },
            &SystemDb::builtin(),
            &mut |_| {},
        )
        .unwrap();
        let id = session
            .entries
            .iter()
            .position(|e| e.rom.stem == "Lantern Harbor")
            .unwrap();
        assert_eq!(session.entries[id].rom.children.len(), 1);
        (session, id as u32, 0)
    }

    #[test]
    fn reads_a_child_slot() {
        let lib = TempDir::new().unwrap();
        let (session, id, child) = game(lib.path());
        assert_eq!(
            read_child_art_bytes(&session, id, child, ChildMedia::Icon).unwrap_err(),
            IpcError::ArtMissing("icon".into())
        );
        put(
            lib.path(),
            "ROMS/3DS/Lantern Harbor/DLC/.media/Lantern Harbor - Pack-icon.png",
            PNG,
        );
        assert_eq!(
            read_child_art_bytes(&session, id, child, ChildMedia::Icon).unwrap(),
            PNG
        );
        assert_eq!(
            read_child_art_bytes(&session, id, child, ChildMedia::Banner).unwrap_err(),
            IpcError::ArtMissing("banner".into())
        );
    }

    #[test]
    fn read_child_art_bytes_refuses_unknown_ids() {
        let lib = TempDir::new().unwrap();
        let (session, id, _) = game(lib.path());
        assert_eq!(
            read_child_art_bytes(&session, id, 1, ChildMedia::Icon).unwrap_err(),
            IpcError::UnknownChild(1)
        );
        assert_eq!(
            read_child_art_bytes(&session, 999, 0, ChildMedia::Icon).unwrap_err(),
            IpcError::UnknownEntry(999)
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_child_art_bytes_refuses_a_media_folder_escaping_the_root() {
        let outside = TempDir::new().unwrap();
        put(outside.path(), "Lantern Harbor - Pack-icon.png", PNG);
        let lib = TempDir::new().unwrap();
        let (session, id, child) = game(lib.path());
        std::os::unix::fs::symlink(
            outside.path(),
            lib.path()
                .join("ROMS")
                .join("3DS")
                .join("Lantern Harbor")
                .join("DLC")
                .join(".media"),
        )
        .unwrap();
        assert!(matches!(
            read_child_art_bytes(&session, id, child, ChildMedia::Icon).unwrap_err(),
            IpcError::OutsideLibrary(_)
        ));
    }
}
