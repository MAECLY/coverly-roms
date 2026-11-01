//! `rename_noclobber` never replaces an existing file (spec §9).

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use coverly_core::write::{
    rename_dir_noclobber, rename_dir_noclobber_using, rename_noclobber, rename_noclobber_using,
    same_volume,
};

#[cfg(unix)]
const UNSUPPORTED: i32 = libc::ENOTSUP;
#[cfg(windows)]
const UNSUPPORTED: i32 = 50; // ERROR_NOT_SUPPORTED

fn unsupported(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::Error::from_raw_os_error(UNSUPPORTED))
}

fn hard_link(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::hard_link(from, to)
}

#[test]
fn rename_noclobber_moves_when_the_target_is_free() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a.gb"), tmp.path().join("b.gb"));
    fs::write(&a, b"A").unwrap();
    rename_noclobber(&a, &b).unwrap();
    assert!(!a.exists());
    assert_eq!(fs::read(&b).unwrap(), b"A");
}

#[test]
fn rename_noclobber_refuses_an_existing_target() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a.gb"), tmp.path().join("b.gb"));
    fs::write(&a, b"A").unwrap();
    fs::write(&b, b"B").unwrap();
    let err = rename_noclobber(&a, &b).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&b).unwrap(), b"B", "never replaced");
    assert_eq!(fs::read(&a).unwrap(), b"A");
}

#[test]
fn refuses_a_case_twin() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a.gb");
    fs::write(&a, b"A").unwrap();
    fs::write(tmp.path().join("Tetris.gb"), b"T").unwrap();
    let err = rename_noclobber(&a, &tmp.path().join("TETRIS.GB")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    assert_eq!(fs::read(tmp.path().join("Tetris.gb")).unwrap(), b"T");
    assert_eq!(fs::read(&a).unwrap(), b"A");
}

#[test]
fn refuses_an_nfc_twin() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a.gb");
    fs::write(&a, b"A").unwrap();
    // Decomposed "Pokémon" on disk, composed spelling as the target.
    fs::write(tmp.path().join("Poke\u{301}mon.gb"), b"P").unwrap();
    let err = rename_noclobber(&a, &tmp.path().join("Pok\u{e9}mon.gb")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&a).unwrap(), b"A");
}

#[test]
fn a_missing_target_folder_is_an_error_not_a_free_name() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a.gb");
    fs::write(&a, b"A").unwrap();
    let err = rename_noclobber(&a, &tmp.path().join("missing/b.gb")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(fs::read(&a).unwrap(), b"A");
}

#[test]
fn falls_back_to_link_when_exclusive_rename_is_unsupported() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    fs::write(&a, b"A").unwrap();
    rename_noclobber_using(&a, &b, &unsupported, &hard_link).unwrap();
    assert!(!a.exists() && b.exists());
    fs::write(&a, b"A2").unwrap();
    let err = rename_noclobber_using(&a, &b, &unsupported, &hard_link).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&b).unwrap(), b"A", "never replaced");
    assert_eq!(fs::read(&a).unwrap(), b"A2");
}

#[test]
fn falls_back_to_a_checked_rename_without_links() {
    // FAT/exFAT: neither an exclusive rename nor hard links.
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a.gb"), tmp.path().join("b.gb"));
    fs::write(&a, b"A").unwrap();
    rename_noclobber_using(&a, &b, &unsupported, &unsupported).unwrap();
    assert!(!a.exists());
    assert_eq!(fs::read(&b).unwrap(), b"A");

    fs::write(&a, b"A2").unwrap();
    let err = rename_noclobber_using(&a, &tmp.path().join("B.GB"), &unsupported, &unsupported)
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&b).unwrap(), b"A", "never replaced");
    assert_eq!(fs::read(&a).unwrap(), b"A2");
}

#[test]
fn a_permission_denied_link_falls_back_to_the_checked_rename() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a.gb"), tmp.path().join("b.gb"));
    fs::write(&a, b"A").unwrap();
    let denied = |_: &Path, _: &Path| Err(std::io::Error::from(ErrorKind::PermissionDenied));
    rename_noclobber_using(&a, &b, &unsupported, &denied).unwrap();
    assert!(!a.exists() && b.exists());
}

#[test]
fn other_exclusive_rename_errors_are_returned_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a.gb"), tmp.path().join("b.gb"));
    fs::write(&a, b"A").unwrap();
    let full = |_: &Path, _: &Path| Err(std::io::Error::from(ErrorKind::StorageFull));
    let never = |_: &Path, _: &Path| -> std::io::Result<()> { panic!("no fallback") };
    let err = rename_noclobber_using(&a, &b, &full, &never).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::StorageFull);
    assert!(a.exists() && !b.exists());
}

#[test]
fn a_tempdir_is_on_the_same_volume_as_itself() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(same_volume(tmp.path(), &tmp.path().join("x/y")));
}

#[test]
fn a_path_without_an_existing_ancestor_is_not_the_same_volume() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(!same_volume(tmp.path(), Path::new("relative/nowhere")));
}

#[cfg(unix)]
#[test]
fn an_unlistable_target_folder_is_refused_as_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a.gb");
    fs::write(&a, b"A").unwrap();
    let dir = tmp.path().join("GB");
    fs::create_dir(&dir).unwrap();
    // Writable and searchable, but not listable: twins cannot be ruled out.
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o300)).unwrap();
    let result = rename_noclobber(&a, &dir.join("b.gb"));
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(result.unwrap_err().kind(), ErrorKind::PermissionDenied);
    assert_eq!(fs::read(&a).unwrap(), b"A");
}

/// A small game folder: `dir/GAME.PROBE` and `dir/data/a.bin`.
fn game_dir(dir: &Path) {
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::write(dir.join("GAME.PROBE"), b"P").unwrap();
    fs::write(dir.join("data/a.bin"), b"A").unwrap();
}

#[test]
fn dir_rename_moves_a_tree_to_a_free_name() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("Zelda"), tmp.path().join("GB"));
    game_dir(&a);
    fs::create_dir(&b).unwrap();
    rename_dir_noclobber(&a, &b.join("Zelda")).unwrap();
    assert!(!a.exists());
    assert_eq!(fs::read(b.join("Zelda/data/a.bin")).unwrap(), b"A");
}

#[test]
fn dir_rename_refuses_an_existing_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("Zelda"), tmp.path().join("Taken"));
    game_dir(&a);
    fs::create_dir(&b).unwrap();
    let err = rename_dir_noclobber(&a, &b).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    assert!(a.join("data/a.bin").is_file(), "the source stays whole");
    assert_eq!(fs::read_dir(&b).unwrap().count(), 0, "never merged");
}

#[test]
fn dir_rename_fallback_creates_then_renames() {
    // FAT/exFAT on macOS/Linux: no exclusive rename. Never a hard link
    // (`EPERM` on folders) and never a bare rename onto a folder this call
    // did not create (POSIX replaces an empty one).
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("Zelda");
    game_dir(&a);
    let b = tmp.path().join("Moved");
    rename_dir_noclobber_using(&a, &b, &unsupported).unwrap();
    assert!(!a.exists());
    assert_eq!(fs::read(b.join("GAME.PROBE")).unwrap(), b"P");
    assert_eq!(fs::read(b.join("data/a.bin")).unwrap(), b"A");

    // An empty folder already at the target: refused, nothing moves.
    let c = tmp.path().join("Other");
    game_dir(&c);
    let empty = tmp.path().join("Empty");
    fs::create_dir(&empty).unwrap();
    let err = rename_dir_noclobber_using(&c, &empty, &unsupported).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    assert!(c.join("data/a.bin").is_file());
    assert_eq!(fs::read_dir(&empty).unwrap().count(), 0);

    // A case twin of the target name is taken too.
    fs::create_dir(tmp.path().join("game")).unwrap();
    let err = rename_dir_noclobber_using(&c, &tmp.path().join("Game"), &unsupported).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    assert!(c.join("data/a.bin").is_file());
    assert_eq!(fs::read_dir(tmp.path().join("game")).unwrap().count(), 0);
}

#[test]
fn dir_rename_returns_other_exclusive_errors_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("Zelda");
    game_dir(&a);
    let b = tmp.path().join("Moved");
    let full = |_: &Path, _: &Path| Err(std::io::Error::from(ErrorKind::StorageFull));
    let err = rename_dir_noclobber_using(&a, &b, &full).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::StorageFull);
    assert!(a.join("data/a.bin").is_file());
    assert!(!b.exists(), "no folder made for a failed rename");
}
