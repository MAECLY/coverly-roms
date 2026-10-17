use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use coverly_core::organize::plan::LeftReason;
use coverly_core::organize::walk::{walk, MetaProbe, WalkOptions, Walked};
use coverly_core::systems::SystemDb;

struct NoCloud;
impl MetaProbe for NoCloud {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }
}
struct CloudNamed(&'static str);
impl MetaProbe for CloudNamed {
    fn cloud_only(&self, p: &Path, _: &fs::Metadata) -> bool {
        p.file_name().unwrap() == self.0
    }
}

fn old(path: &Path) {
    let t = filetime::FileTime::from_system_time(SystemTime::now() - Duration::from_secs(3600));
    filetime::set_file_mtime(path, t).unwrap();
}

fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    old(&p);
    p
}

fn run(root: &Path, probe: &dyn MetaProbe) -> Walked {
    let ignored = ["bios".to_string(), "saves".to_string()];
    let opts = WalkOptions {
        skip_dirs: vec![],
        roms_root: None,
        art_dirs: &[],
        ignored_dirs: &ignored,
        now: SystemTime::now(),
        recent: Duration::from_secs(60),
        probe,
        max_depth: 4,
    };
    walk(root, &SystemDb::builtin(), &opts).unwrap()
}

fn reason_of(w: &Walked, name: &str) -> Option<LeftReason> {
    w.left
        .iter()
        .find(|(p, _)| p.file_name().unwrap() == name)
        .map(|(_, r)| r.clone())
}

#[test]
fn partial_downloads_are_left_with_the_reason() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Tetris.zip.crdownload", b"x");
    put(tmp.path(), "Empty.gba", b"");
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(
        reason_of(&w, "Tetris.zip.crdownload"),
        Some(LeftReason::PartialDownload)
    );
    assert_eq!(
        reason_of(&w, "Empty.gba"),
        Some(LeftReason::PartialDownload),
        "zero bytes"
    );
    assert!(w.candidates.is_empty());
}

#[test]
fn a_file_with_a_part_sibling_is_still_downloading() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "x.zip", b"PK\x03\x04");
    put(tmp.path(), "x.zip.part", b"");
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(reason_of(&w, "x.zip"), Some(LeftReason::PartialDownload));
}

#[test]
fn recently_modified_files_wait() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("fresh.gba");
    fs::write(&p, b"x").unwrap(); // mtime = now
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(
        reason_of(&w, "fresh.gba"),
        Some(LeftReason::RecentlyModified)
    );
}

#[test]
fn cloud_only_files_are_identified_by_name_only() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "far.gba", b"x");
    let w = run(tmp.path(), &CloudNamed("far.gba"));
    assert_eq!(w.candidates.len(), 1);
    assert!(w.candidates[0].cloud_only);
}

#[test]
fn junk_bios_and_symlinks() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), ".DS_Store", b"x");
    put(tmp.path(), "._Game.gba", b"x");
    put(tmp.path(), "Game.gba:Zone.Identifier", b"x");
    put(tmp.path(), "gba_bios.bin", b"x");
    #[cfg(unix)]
    std::os::unix::fs::symlink(tmp.path().join("gba_bios.bin"), tmp.path().join("link.gba"))
        .unwrap();
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(w.junk, 3);
    assert_eq!(reason_of(&w, "gba_bios.bin"), Some(LeftReason::Bios));
    #[cfg(unix)]
    assert_eq!(reason_of(&w, "link.gba"), Some(LeftReason::Symlink));
}

#[test]
fn system_folders_are_not_walked_and_deep_folders_are_reported() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "gba/Already.gba", b"x");
    put(tmp.path(), "GBA/Too.gba", b"x"); // same folder on case-insensitive volumes
    put(tmp.path(), "a/b/c/d/e/Deep.gba", b"x");
    put(tmp.path(), "a/Loose.gba", b"x");
    let w = run(tmp.path(), &NoCloud);
    assert!(w.sorted_folders.iter().all(|(_, id)| id == "gba"));
    assert!(!w.sorted_folders.is_empty());
    assert_eq!(
        w.candidates
            .iter()
            .map(|c| c.path.file_name().unwrap().to_owned())
            .collect::<Vec<_>>(),
        ["Loose.gba"]
    );
    assert_eq!(w.deep_folders, [tmp.path().join("a/b/c/d")]);
}

#[test]
fn documents_are_companions_and_personal_files_are_other() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Game.gba", b"x");
    put(tmp.path(), "Game.sav", b"x");
    put(tmp.path(), "notes.txt", b"x");
    put(tmp.path(), "setup.dmg", b"x");
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(w.candidates.len(), 1);
    assert_eq!(w.companions.len(), 2);
    assert_eq!(w.other_files.len(), 1);
}

#[test]
fn skip_dirs_and_roms_root() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Out/Moved.gba", b"x");
    put(tmp.path(), "ROMS/GBA/Sorted.gba", b"x");
    put(tmp.path(), "ROMS/Loose.gb", b"x");
    let ignored: [String; 0] = [];
    let opts = WalkOptions {
        skip_dirs: vec![tmp.path().join("Out")],
        roms_root: Some(tmp.path().join("ROMS")),
        art_dirs: &[],
        ignored_dirs: &ignored,
        now: SystemTime::now(),
        recent: Duration::from_secs(60),
        probe: &NoCloud,
        max_depth: 4,
    };
    let w = walk(tmp.path(), &SystemDb::builtin(), &opts).unwrap();
    let names: Vec<_> = w
        .candidates
        .iter()
        .map(|c| c.path.file_name().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["Loose.gb"]);
    assert_eq!(w.candidates[0].ext, "gb");
    assert_eq!(w.candidates[0].size, 1);
    assert_eq!(
        w.sorted_folders,
        [(tmp.path().join("ROMS/GBA"), "gba".to_string())]
    );
}

#[test]
fn an_unreadable_source_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let ignored: [String; 0] = [];
    let opts = WalkOptions {
        skip_dirs: vec![],
        roms_root: None,
        art_dirs: &[],
        ignored_dirs: &ignored,
        now: SystemTime::now(),
        recent: Duration::from_secs(60),
        probe: &NoCloud,
        max_depth: 4,
    };
    assert!(walk(&tmp.path().join("missing"), &SystemDb::builtin(), &opts).is_err());
}

#[test]
fn broad_roots() {
    use coverly_core::organize::walk::is_broad_root;
    let home = directories::BaseDirs::new()
        .unwrap()
        .home_dir()
        .to_path_buf();
    assert!(is_broad_root(&home));
    assert!(is_broad_root(&home.join("Downloads")));
    assert!(is_broad_root(Path::new("/")));
    assert!(!is_broad_root(&home.join("Downloads/roms")));
}
