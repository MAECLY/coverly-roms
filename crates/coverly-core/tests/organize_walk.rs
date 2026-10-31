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
        profile: None,
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
    // On NTFS `Game.gba:Zone.Identifier` is a stream of `Game.gba`, not a
    // file of its own, so the walk never sees it.
    assert_eq!(w.junk, if cfg!(windows) { 2 } else { 3 });
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
        profile: None,
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
        profile: None,
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

#[cfg(unix)]
fn opts_with<'a>(
    skip_dirs: Vec<PathBuf>,
    roms_root: Option<PathBuf>,
    ignored: &'a [String],
    probe: &'a dyn MetaProbe,
) -> WalkOptions<'a> {
    WalkOptions {
        skip_dirs,
        roms_root,
        art_dirs: &[],
        ignored_dirs: ignored,
        now: SystemTime::now(),
        recent: Duration::from_secs(60),
        probe,
        max_depth: 4,
        profile: None,
    }
}

#[test]
fn a_safari_download_bundle_is_left_and_not_entered() {
    let tmp = tempfile::tempdir().unwrap();
    // Safari writes the partial file inside `<name>.download/`.
    put(tmp.path(), "Tetris.zip.download/Tetris.zip", b"PK\x03\x04");
    put(tmp.path(), "Tetris.zip.download/Info.plist", b"x");
    let w = run(tmp.path(), &NoCloud);
    assert!(w.candidates.is_empty(), "{:?}", w.candidates);
    assert_eq!(
        reason_of(&w, "Tetris.zip.download"),
        Some(LeftReason::PartialDownload)
    );
    assert!(w.other_files.is_empty());
}

#[cfg(unix)]
#[test]
fn skip_dirs_match_through_a_symlinked_source() {
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("real");
    put(&real, "Out/GB/Moved.gb", b"x");
    put(&real, "Out/Stray.gba", b"x");
    put(&real, "Pile.gba", b"x");
    let via = tmp.path().join("via");
    std::os::unix::fs::symlink(&real, &via).unwrap();
    let ignored: [String; 0] = [];
    // The planner may hand the target canonicalised while the source is not.
    let canonical_out = fs::canonicalize(real.join("Out")).unwrap();
    let opts = opts_with(vec![canonical_out], None, &ignored, &NoCloud);
    let w = walk(&via, &SystemDb::builtin(), &opts).unwrap();
    let names: Vec<_> = w
        .candidates
        .iter()
        .map(|c| c.path.file_name().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["Pile.gba"]);
}

#[test]
fn a_deep_folder_holding_only_junk_is_not_reported() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "a/b/c/d/.DS_Store", b"x");
    put(tmp.path(), "a/b/c/d/._x", b"x");
    let w = run(tmp.path(), &NoCloud);
    assert!(w.deep_folders.is_empty(), "{:?}", w.deep_folders);
}

#[cfg(unix)]
#[test]
fn a_symlinked_folder_is_left_and_not_followed() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "elsewhere/Far.gba", b"x");
    let src = tmp.path().join("src");
    fs::create_dir(&src).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("elsewhere"), src.join("linked")).unwrap();
    let w = run(&src, &NoCloud);
    assert!(w.candidates.is_empty());
    assert_eq!(reason_of(&w, "linked"), Some(LeftReason::Symlink));
}

#[test]
fn only_real_state_slots_are_companions() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Game.state.auto", b"x");
    put(tmp.path(), "bank.statements.dmg", b"x");
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(w.companions.len(), 1, "{:?}", w.companions);
    assert_eq!(w.other_files.len(), 1, "{:?}", w.other_files);
}

#[test]
fn a_folder_the_target_layout_names_counts_as_sorted() {
    use coverly_core::profiles::{builtin_profiles, ProfileFolder};
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "SuperFam/Zelda (USA).sfc", b"x");
    let mut p = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "esde")
        .unwrap();
    p.folders
        .insert("snes".into(), ProfileFolder::Named("SuperFam".into()));
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
        profile: Some(&p),
    };
    let w = walk(tmp.path(), &SystemDb::builtin(), &opts).unwrap();
    assert!(w.candidates.is_empty());
    assert_eq!(w.sorted_folders.len(), 1);
    assert_eq!(
        w.sorted_folders[0].0.file_name().unwrap(),
        std::ffi::OsStr::new("SuperFam")
    );
    assert_eq!(w.sorted_folders[0].1, "snes");
    // Without the layout it is a pile folder to organise.
    let w = run(tmp.path(), &NoCloud);
    assert!(w.sorted_folders.is_empty());
    assert_eq!(w.candidates.len(), 1);
}

#[test]
fn dragon_folder_in_downloads_is_still_walked() {
    // D22: with no profile a `Dragon/` folder is a pile folder (its game is
    // a candidate); with the target profile set to Batocera, `dragon64/`
    // (its Dragon folder) is «already sorted».
    use coverly_core::profiles::builtin_profiles;
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Dragon/game.cas", b"x");
    let w = run(tmp.path(), &NoCloud);
    assert!(w.sorted_folders.is_empty(), "{:?}", w.sorted_folders);
    assert_eq!(
        w.candidates
            .iter()
            .map(|c| c.path.file_name().unwrap().to_owned())
            .collect::<Vec<_>>(),
        ["game.cas"]
    );

    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "dragon64/game.cas", b"x");
    let p = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "batocera")
        .unwrap();
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
        profile: Some(&p),
    };
    let w = walk(tmp.path(), &SystemDb::builtin(), &opts).unwrap();
    assert!(w.candidates.is_empty());
    assert_eq!(w.sorted_folders.len(), 1);
    assert_eq!(
        w.sorted_folders[0].0.file_name().unwrap(),
        std::ffi::OsStr::new("dragon64")
    );
    assert_eq!(w.sorted_folders[0].1, "dragon");
    // Without the profile `dragon64` is a pile folder too.
    let w = run(tmp.path(), &NoCloud);
    assert!(w.sorted_folders.is_empty());
}

/// A Doom WAD head: `magic`, one lump, then zeros (binary, like a real one).
fn wad(magic: &[u8; 4]) -> Vec<u8> {
    let mut b = vec![0u8; 1024];
    b[..4].copy_from_slice(magic);
    b[4] = 1;
    b
}

fn folder_games(w: &Walked) -> Vec<(String, String, String)> {
    w.left
        .iter()
        .filter_map(|(p, r)| match r {
            LeftReason::FolderGame { system, folder } => Some((
                p.file_name().unwrap().to_string_lossy().into_owned(),
                system.clone(),
                folder.clone(),
            )),
            _ => None,
        })
        .collect()
}

fn file_names(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn freedoom_folder_is_one_item() {
    // D15: a pile folder directly holding an `IWAD` is one item left in
    // place; its other files are not organised one by one.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Freedoom Phase 1");
    put(tmp.path(), "Freedoom Phase 1/freedoom1.wad", &wad(b"IWAD"));
    put(tmp.path(), "Freedoom Phase 1/README.txt", b"Freedoom\n");
    put(
        tmp.path(),
        "Freedoom Phase 1/extra/patch.wad",
        &wad(b"PWAD"),
    );
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(
        folder_games(&w),
        [(
            "Freedoom Phase 1".to_string(),
            "doom".to_string(),
            "PRBOOM".to_string()
        )]
    );
    assert_eq!(w.left.len(), 1, "{:?}", w.left);
    assert_eq!(w.left[0].0, dir);
    assert!(w.candidates.is_empty(), "{:?}", w.candidates);
    assert!(w.companions.is_empty(), "{:?}", w.companions);
    assert!(w.other_files.is_empty(), "{:?}", w.other_files);
}

#[test]
fn folder_game_target_follows_the_layout() {
    use coverly_core::profiles::builtin_profiles;
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Freedoom/freedoom2.wad", &wad(b"IWAD"));
    let p = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "batocera")
        .unwrap();
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
        profile: Some(&p),
    };
    let w = walk(tmp.path(), &SystemDb::builtin(), &opts).unwrap();
    assert_eq!(
        folder_games(&w),
        [(
            "Freedoom".to_string(),
            "doom".to_string(),
            "prboom".to_string()
        )]
    );
}

#[test]
fn a_wad_without_magic_or_in_the_cloud_proves_no_folder_game() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Bare/game.wad", &wad(b"\0\0\0\x20"));
    put(tmp.path(), "Cloud/doom.wad", &wad(b"IWAD"));
    let w = run(tmp.path(), &CloudNamed("doom.wad"));
    assert!(folder_games(&w).is_empty(), "{:?}", w.left);
    let mut names = w
        .candidates
        .iter()
        .map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["doom.wad", "game.wad"]);
}

#[test]
fn a_loose_wad_at_the_root_is_a_candidate() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "DOOM.WAD", &wad(b"IWAD"));
    let w = run(tmp.path(), &NoCloud);
    assert!(w.left.is_empty(), "{:?}", w.left);
    assert_eq!(w.candidates.len(), 1);
}

#[test]
fn cave_story_folder_files_are_unknown() {
    // D15: Cave Story ships as data with no launcher Coverly could trust:
    // no rule is invented, its folder is walked like any other.
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Cave Story/Doukutsu.exe", b"MZ\x90\0");
    put(tmp.path(), "Cave Story/data/Stage/Cave.pxm", b"PXM\x10");
    let w = run(tmp.path(), &NoCloud);
    assert!(folder_games(&w).is_empty(), "{:?}", w.left);
    assert!(w.sorted_folders.is_empty(), "{:?}", w.sorted_folders);
    let mut seen = file_names(&w.other_files);
    seen.extend(file_names(
        &w.candidates
            .iter()
            .map(|c| c.path.clone())
            .collect::<Vec<_>>(),
    ));
    seen.sort();
    assert_eq!(seen, ["Cave.pxm", "Doukutsu.exe"]);
}

#[test]
fn scummvm_launcher_keeps_its_folder() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Monkey/monkey.scummvm", b"monkey\n");
    put(tmp.path(), "Monkey/MONKEY.000", &[0u8, 1, 2, 3]);
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(
        folder_games(&w),
        [(
            "Monkey".to_string(),
            "scummvm".to_string(),
            "SCUMMVM".to_string()
        )]
    );
    assert_eq!(w.left.len(), 1);
    assert!(w.candidates.is_empty() && w.other_files.is_empty());
}

#[test]
fn solarus_and_lutro_folders_are_one_item() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Zelda ROTH/zsdx.solarus", b"PK\x03\x04");
    put(tmp.path(), "Zelda ROTH/manual.pdf", b"%PDF");
    put(tmp.path(), "Pong/pong.lutro", b"PK\x03\x04");
    put(
        tmp.path(),
        "Platformer/main.lua",
        b"function lutro.draw() end\n",
    );
    put(
        tmp.path(),
        "Platformer/conf.lua",
        b"function lutro.conf(t) end\n",
    );
    put(tmp.path(), "Script/main.lua", b"print(1)\n");
    let mut got = folder_games(&run(tmp.path(), &NoCloud));
    got.sort();
    assert_eq!(
        got,
        [
            ("Platformer".into(), "lutro".into(), "LUTRO".into()),
            ("Pong".into(), "lutro".into(), "LUTRO".into()),
            ("Zelda ROTH".into(), "solarus".into(), "SOLARUS".into()),
        ]
    );
}

#[test]
fn a_folder_game_alias_folder_stays_already_sorted() {
    // A folder named by the system (`prboom/`, `scummvm/`) is sorted as
    // today, not a folder game.
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "prboom/freedoom1.wad", &wad(b"IWAD"));
    put(tmp.path(), "scummvm/Monkey/monkey.scummvm", b"monkey\n");
    let w = run(tmp.path(), &NoCloud);
    assert!(w.left.is_empty(), "{:?}", w.left);
    let mut ids: Vec<&str> = w.sorted_folders.iter().map(|(_, id)| id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, ["doom", "scummvm"]);
}

#[test]
fn mame_chd_folder_is_kept_with_its_set() {
    // D16: `sfiii/` holding only `.chd` beside `sfiii.zip` is the set's
    // CHD folder: its files are not candidates of their own. A folder
    // with anything else, or with no set beside it, is walked as usual.
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "sfiii.zip", b"PK\x03\x04");
    let chd = put(tmp.path(), "sfiii/sfiii.chd", b"MComprHD");
    put(tmp.path(), "sfiii/.DS_Store", b"\0\0\0\x01Bud1");
    put(tmp.path(), "kof.zip", b"PK\x03\x04");
    put(tmp.path(), "kof/kof.chd", b"MComprHD");
    put(tmp.path(), "kof/readme.txt", b"hello");
    put(tmp.path(), "lone/lone.chd", b"MComprHD");
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(
        w.chd_folders,
        [(tmp.path().join("sfiii.zip"), vec![chd.clone()])]
    );
    let names = file_names(
        &w.candidates
            .iter()
            .map(|c| c.path.clone())
            .collect::<Vec<_>>(),
    );
    assert_eq!(names, ["kof.chd", "kof.zip", "lone.chd", "sfiii.zip"]);
}
