use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::scan::ScanOptions;
use coverly_core::settings::{LibrarySettings, RecentLibrary, LIBRARY_SETTINGS_FILE, MAX_RECENT};
use tempfile::TempDir;

fn recent(path: &str, opened_at: i64) -> RecentLibrary {
    RecentLibrary {
        path: PathBuf::from(path),
        name: path.rsplit('/').next().unwrap().to_string(),
        profile_id: "anbernic-stock".into(),
        roms: 10,
        opened_at,
    }
}

#[test]
fn missing_file_gives_defaults() {
    let dir = TempDir::new().unwrap();
    let s = LibrarySettings::load(&dir.path().join(LIBRARY_SETTINGS_FILE));
    assert_eq!(s, LibrarySettings::default());
    assert_eq!(s.version, 1);
    assert_eq!(s.ignored_dirs, ScanOptions::default().ignored_dirs);
    assert!(s.recent.is_empty());
}

#[test]
fn save_and_load_round_trip_creating_folders() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested/config").join(LIBRARY_SETTINGS_FILE);
    let mut s = LibrarySettings::default();
    s.remember(recent("/Volumes/SD", 5));
    s.save(&path).unwrap();
    assert_eq!(LibrarySettings::load(&path), s);
    assert!(
        !path.with_extension("json.tmp").exists(),
        "temp file left behind"
    );
}

#[test]
fn corrupt_file_falls_back_to_defaults_and_is_kept_aside() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(LIBRARY_SETTINGS_FILE);
    fs::write(&path, "{ not json").unwrap();
    assert_eq!(LibrarySettings::load(&path), LibrarySettings::default());
    assert_eq!(
        fs::read_to_string(path.with_extension("json.bad")).unwrap(),
        "{ not json"
    );
}

#[test]
fn unknown_fields_and_missing_fields_are_tolerated() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(LIBRARY_SETTINGS_FILE);
    fs::write(&path, r#"{"future_field": true, "ignored_dirs": ["bios"]}"#).unwrap();
    let s = LibrarySettings::load(&path);
    assert_eq!(s.ignored_dirs, ["bios"]);
    assert!(s.recent.is_empty());
}

#[test]
fn remember_moves_to_front_dedupes_and_caps() {
    let mut s = LibrarySettings::default();
    for i in 0..12 {
        s.remember(recent(&format!("/lib/{i}"), i));
    }
    assert_eq!(s.recent.len(), MAX_RECENT);
    assert_eq!(s.recent[0].path, PathBuf::from("/lib/11"));
    s.remember(recent("/lib/5", 99));
    assert_eq!(s.recent[0].path, PathBuf::from("/lib/5"));
    assert_eq!(s.recent[0].opened_at, 99);
    assert_eq!(
        s.recent
            .iter()
            .filter(|r| r.path == Path::new("/lib/5"))
            .count(),
        1
    );
}

#[test]
fn forget_removes_one_library() {
    let mut s = LibrarySettings::default();
    s.remember(recent("/a", 1));
    s.remember(recent("/b", 2));
    s.forget(&PathBuf::from("/a"));
    assert_eq!(s.recent.len(), 1);
    assert_eq!(s.recent[0].path, PathBuf::from("/b"));
}

#[test]
fn ignored_dirs_are_normalised() {
    let mut s = LibrarySettings::default();
    s.set_ignored_dirs(vec![
        "  bios ".into(),
        String::new(),
        "BIOS".into(),
        "a/b".into(),
        "..".into(),
        "c\\d".into(),
        "Saves".into(),
    ]);
    assert_eq!(s.ignored_dirs, ["bios", "Saves"]);
    let opts = s.scan_options();
    assert_eq!(opts.ignored_dirs, ["bios", "Saves"]);
    assert_eq!(opts.ignored_exts, ScanOptions::default().ignored_exts);
}
