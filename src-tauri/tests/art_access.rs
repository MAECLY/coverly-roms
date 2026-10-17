mod common;

use std::fs;

use common::{anbernic_tree, open, write};
use coverly_core::profiles::Media;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::library::art::{entry_path, read_art_bytes, MAX_ART_BYTES};
#[cfg(unix)]
use tempfile::TempDir;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake";

#[test]
fn reads_present_art_bytes() {
    let tree = anbernic_tree(&["GBA"], 1, 0);
    write(tree.path(), "ROMS/GBA/Imgs/Game 00000 (USA).png", PNG);
    let (session, _) = open(tree.path(), None).unwrap();
    assert_eq!(read_art_bytes(&session, 0, Media::Cover).unwrap(), PNG);
}

#[test]
fn missing_or_unsupported_media_is_art_missing() {
    let tree = anbernic_tree(&["GBA"], 1, 0);
    let (session, _) = open(tree.path(), None).unwrap();
    assert_eq!(
        read_art_bytes(&session, 0, Media::Cover).unwrap_err(),
        IpcError::ArtMissing("cover".into())
    );
    assert_eq!(
        read_art_bytes(&session, 0, Media::Screenshot).unwrap_err(),
        IpcError::ArtMissing("screenshot".into())
    );
}

#[test]
fn unknown_entry_is_rejected() {
    let tree = anbernic_tree(&["GBA"], 1, 1);
    let (session, _) = open(tree.path(), None).unwrap();
    assert_eq!(
        read_art_bytes(&session, 999, Media::Cover).unwrap_err(),
        IpcError::UnknownEntry(999)
    );
    assert_eq!(
        entry_path(&session, 999).unwrap_err(),
        IpcError::UnknownEntry(999)
    );
}

#[test]
fn entry_path_is_the_rom_inside_the_root() {
    let tree = anbernic_tree(&["GBA"], 1, 0);
    let (session, _) = open(tree.path(), None).unwrap();
    let path = entry_path(&session, 0).unwrap();
    assert!(path.ends_with("ROMS/GBA/Game 00000 (USA).gba"), "{path:?}");
}

#[cfg(unix)]
#[test]
fn symlinked_art_outside_the_library_is_refused() {
    let outside = TempDir::new().unwrap();
    write(outside.path(), "secret.png", PNG);
    let tree = anbernic_tree(&["GBA"], 1, 0);
    fs::create_dir_all(tree.path().join("ROMS/GBA/Imgs")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.png"),
        tree.path().join("ROMS/GBA/Imgs/Game 00000 (USA).png"),
    )
    .unwrap();
    let (session, view) = open(tree.path(), None).unwrap();
    assert_eq!(
        view.entries[0].art.cover,
        coverly_roms_lib::library::view::ArtMark::Present
    );
    assert!(matches!(
        read_art_bytes(&session, 0, Media::Cover).unwrap_err(),
        IpcError::OutsideLibrary(_)
    ));
}

#[cfg(unix)]
#[test]
fn symlinked_art_folder_outside_is_refused() {
    let outside = TempDir::new().unwrap();
    write(outside.path(), "Game 00000 (USA).png", PNG);
    let tree = anbernic_tree(&["GBA"], 1, 0);
    std::os::unix::fs::symlink(outside.path(), tree.path().join("ROMS/GBA/Imgs")).unwrap();
    let (session, _) = open(tree.path(), None).unwrap();
    match read_art_bytes(&session, 0, Media::Cover) {
        Err(IpcError::OutsideLibrary(_)) | Err(IpcError::ArtMissing(_)) => {}
        other => panic!("art outside the root was served: {other:?}"),
    }
}

#[test]
fn oversized_art_is_refused() {
    let tree = anbernic_tree(&["GBA"], 1, 1);
    let art = tree.path().join("ROMS/GBA/Imgs/Game 00000 (USA).png");
    fs::File::options()
        .write(true)
        .open(&art)
        .unwrap()
        .set_len(MAX_ART_BYTES + 1)
        .unwrap();
    let (session, _) = open(tree.path(), None).unwrap();
    assert_eq!(
        read_art_bytes(&session, 0, Media::Cover).unwrap_err(),
        IpcError::ArtTooLarge(MAX_ART_BYTES + 1)
    );
}
