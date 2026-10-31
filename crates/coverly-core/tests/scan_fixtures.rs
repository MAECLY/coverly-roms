//! Library walker and ROM classification on fixture trees.

mod common;

use std::path::{Path, PathBuf};

use coverly_core::scan::{scan_library, LibraryScan, ScanOptions, SystemScan};
use coverly_core::systems::SystemDb;

use common::{mkdir, profile, realistic_library, touch, write};

fn scan(root: &Path, profile_id: &str) -> LibraryScan {
    scan_library(
        root,
        &profile(profile_id),
        &SystemDb::builtin(),
        &ScanOptions::default(),
    )
}

fn system<'a>(scan: &'a LibraryScan, folder: &str) -> &'a SystemScan {
    scan.systems
        .iter()
        .find(|s| s.folder_name == folder)
        .unwrap_or_else(|| panic!("system folder `{folder}` not listed"))
}

fn stems(system: &SystemScan) -> Vec<&str> {
    system.roms.iter().map(|r| r.stem.as_str()).collect()
}

#[test]
fn realistic_library_is_classified() {
    let lib = realistic_library();
    let scan = scan(lib.path(), "anbernic-stock");

    assert_eq!(scan.root, lib.path());
    assert_eq!(scan.profile_id, "anbernic-stock");
    assert!(scan.warnings.is_empty(), "{:?}", scan.warnings);

    let folders: Vec<&str> = scan
        .systems
        .iter()
        .map(|s| s.folder_name.as_str())
        .collect();
    assert_eq!(folders, ["APPS", "MD", "PS", "SFC"], "no hidden folder");

    let sfc = system(&scan, "SFC");
    assert_eq!(sfc.system_id.as_deref(), Some("snes"));
    assert_eq!(sfc.dir, lib.path().join("ROMS/SFC"));
    assert_eq!(stems(sfc), ["MARIO", "Zelda (USA)"], "no AppleDouble twin");
    let mario = &sfc.roms[0];
    assert_eq!(mario.ext, "smc");
    assert_eq!(mario.path, lib.path().join("ROMS/SFC/MARIO.SMC"));
    assert_eq!(mario.rel_dir, PathBuf::new());
    assert_eq!(mario.size, 5);
    assert!(mario.companions.is_empty());

    let ps = system(&scan, "PS");
    assert_eq!(ps.system_id.as_deref(), Some("psx"));
    assert_eq!(stems(ps), ["FF7"]);
    let ff7 = &ps.roms[0];
    assert_eq!(ff7.ext, "m3u");
    let ps_dir = lib.path().join("ROMS/PS");
    assert_eq!(
        ff7.companions,
        [
            ps_dir.join("FF7 (Disc 1).cue"),
            ps_dir.join("FF7 (Disc 1).bin"),
            ps_dir.join("FF7 (Disc 2).cue"),
            ps_dir.join("FF7 (Disc 2).bin"),
        ]
    );
    let cue_len = std::fs::metadata(ps_dir.join("FF7 (Disc 1).cue"))
        .unwrap()
        .len();
    let m3u_len = std::fs::metadata(ps_dir.join("FF7.m3u")).unwrap().len();
    assert_eq!(ff7.size, m3u_len + 2 * cue_len + 8 + 8);

    let md = system(&scan, "MD");
    assert_eq!(md.system_id.as_deref(), Some("megadrive"));
    assert_eq!(stems(md), ["Sonic Hack"]);
    assert_eq!(md.roms[0].rel_dir, PathBuf::from("Hacks"));

    let apps = system(&scan, "APPS");
    assert_eq!(apps.system_id, None);
    assert!(
        apps.roms.is_empty(),
        "unknown folders are listed, not scanned"
    );
}

#[test]
fn unknown_and_ignored_folders_are_never_roms() {
    let lib = tempfile::TempDir::new().unwrap();
    let root = lib.path();
    touch(root, "ROMS/bios/scph1001.bin");
    touch(root, "ROMS/PORTS/Shortcuts/game.sh");
    touch(root, "ROMS/SFC/bios/snes.sfc");
    touch(root, "ROMS/SFC/saves/Zelda.sfc");
    touch(root, "ROMS/SFC/Shortcuts/Zelda.sfc");
    touch(root, "ROMS/SFC/IMGS/Zelda.sfc");
    touch(root, "ROMS/SFC/Sub/Imgs/Zelda.sfc");
    touch(root, "ROMS/SFC/Sub/.DS_Store");
    touch(root, "ROMS/SFC/Thumbs.db");
    touch(root, "ROMS/SFC/Sub/Real.sfc");
    touch(root, "ROMS/SFC/no_extension");
    touch(root, "ROMS/loose-file.sfc");

    let scan = scan(root, "anbernic-stock");
    assert!(scan.warnings.is_empty(), "{:?}", scan.warnings);
    let folders: Vec<&str> = scan
        .systems
        .iter()
        .map(|s| s.folder_name.as_str())
        .collect();
    assert_eq!(folders, ["bios", "PORTS", "SFC"]);
    assert_eq!(system(&scan, "bios").system_id, None);
    assert_eq!(system(&scan, "PORTS").system_id, None);
    let sfc = system(&scan, "SFC");
    assert_eq!(stems(sfc), ["Real"]);
    assert_eq!(sfc.roms[0].rel_dir, PathBuf::from("Sub"));
}

#[test]
fn roms_root_is_found_case_insensitively() {
    let lib = tempfile::TempDir::new().unwrap();
    touch(lib.path(), "roms/sfc/Game.SFC");
    // GarlicOS expects `Roms`; the card has `roms`.
    let scan = scan(lib.path(), "garlicos");
    assert!(scan.warnings.is_empty(), "{:?}", scan.warnings);
    let sfc = system(&scan, "sfc");
    assert_eq!(sfc.system_id.as_deref(), Some("snes"));
    assert_eq!(stems(sfc), ["Game"]);
    assert_eq!(sfc.roms[0].ext, "sfc");
}

#[test]
fn missing_roms_root_is_a_warning() {
    let lib = tempfile::TempDir::new().unwrap();
    mkdir(lib.path(), "Other");
    let scan = scan(lib.path(), "anbernic-stock");
    assert!(scan.systems.is_empty());
    assert_eq!(scan.warnings.len(), 1);
    assert_eq!(scan.warnings[0].path, lib.path().join("ROMS"));
}

#[test]
fn gdi_and_cue_tracks_are_companions() {
    let lib = tempfile::TempDir::new().unwrap();
    let root = lib.path();
    write(
        root,
        "ROMS/DREAMCAST/Crazy Taxi.gdi",
        "3\n1 0 4 2352 track01.bin 0\n2 756 0 2352 \"track 02.raw\" 0\n3 45000 4 2352 TRACK03.BIN 0\n",
    );
    write(root, "ROMS/DREAMCAST/track01.bin", "1");
    write(root, "ROMS/DREAMCAST/track 02.raw", "22");
    write(root, "ROMS/DREAMCAST/track03.bin", "333");
    // Sega CD does not list `bin` as a ROM extension, yet the track is a
    // companion of the cue.
    write(
        root,
        "ROMS/MDCD/Sonic CD.cue",
        "FILE \"Sonic CD (Track 1).bin\" BINARY\nFILE \"Sonic CD (Track 2).bin\" BINARY\n",
    );
    write(root, "ROMS/MDCD/Sonic CD (Track 1).bin", "a");
    write(root, "ROMS/MDCD/Sonic CD (Track 2).bin", "b");

    let scan = scan(root, "anbernic-stock");
    assert!(scan.warnings.is_empty(), "{:?}", scan.warnings);

    let dc = system(&scan, "DREAMCAST");
    assert_eq!(stems(dc), ["Crazy Taxi"]);
    assert_eq!(dc.roms[0].companions.len(), 3);
    assert!(dc.roms[0]
        .companions
        .iter()
        .any(|c| c.file_name().unwrap().eq_ignore_ascii_case("track03.bin")));

    let cd = system(&scan, "MDCD");
    assert_eq!(cd.system_id.as_deref(), Some("segacd"));
    assert_eq!(stems(cd), ["Sonic CD"]);
    assert_eq!(cd.roms[0].companions.len(), 2);
}

#[test]
fn missing_companion_is_a_warning() {
    let lib = tempfile::TempDir::new().unwrap();
    write(lib.path(), "ROMS/PS/Game.cue", "FILE \"Game.bin\" BINARY\n");
    let scan = scan(lib.path(), "anbernic-stock");
    let ps = system(&scan, "PS");
    assert_eq!(stems(ps), ["Game"]);
    assert!(ps.roms[0].companions.is_empty());
    assert_eq!(scan.warnings.len(), 1);
    assert_eq!(scan.warnings[0].path, lib.path().join("ROMS/PS/Game.cue"));
}

#[cfg(unix)]
#[test]
fn unreadable_folder_is_a_warning_and_scan_continues() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let lib = realistic_library();
    let locked = lib.path().join("ROMS/SFC/Locked");
    touch(lib.path(), "ROMS/SFC/Locked/Secret.sfc");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        // Running as root: permissions are not enforced.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let scan = scan(lib.path(), "anbernic-stock");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(scan.warnings.len(), 1, "{:?}", scan.warnings);
    assert_eq!(scan.warnings[0].path, locked);
    assert_eq!(stems(system(&scan, "SFC")), ["MARIO", "Zelda (USA)"]);
    assert_eq!(stems(system(&scan, "PS")), ["FF7"]);
}

#[cfg(unix)]
#[test]
fn broken_symlink_is_a_warning() {
    let lib = realistic_library();
    let link = lib.path().join("ROMS/SFC/Ghost.sfc");
    std::os::unix::fs::symlink(lib.path().join("nowhere.sfc"), &link).unwrap();

    let scan = scan(lib.path(), "anbernic-stock");
    assert_eq!(scan.warnings.len(), 1, "{:?}", scan.warnings);
    assert_eq!(scan.warnings[0].path, link);
    assert_eq!(stems(system(&scan, "SFC")), ["MARIO", "Zelda (USA)"]);
}

#[test]
fn partial_downloads_are_not_roms() {
    let tmp = tempfile::tempdir().unwrap();
    let gba = tmp.path().join("ROMS/GBA");
    std::fs::create_dir_all(&gba).unwrap();
    std::fs::write(gba.join("Done.gba"), b"x").unwrap();
    std::fs::write(gba.join("Half.gba.crdownload"), b"x").unwrap();
    // A colon is not a valid file-name character on Windows.
    #[cfg(unix)]
    std::fs::write(gba.join("Done.gba:Zone.Identifier"), b"x").unwrap();
    let scan = scan(tmp.path(), "anbernic-stock");
    assert_eq!(stems(system(&scan, "GBA")), ["Done"]);
}

#[test]
fn a_profile_named_folder_is_scanned_as_its_system() {
    use coverly_core::profiles::ProfileFolder;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    touch(root, "roms/SuperFam/Zelda (USA).sfc");
    let mut p = profile("batocera");
    p.folders
        .insert("snes".into(), ProfileFolder::Named("SuperFam".into()));
    let named = scan_library(root, &p, &SystemDb::builtin(), &ScanOptions::default());
    let s = system(&named, "SuperFam");
    assert_eq!(s.system_id.as_deref(), Some("snes"));
    assert_eq!(stems(s), ["Zelda (USA)"]);
    // Without the table the folder is unknown.
    let plain = scan(root, "batocera");
    assert_eq!(system(&plain, "SuperFam").system_id, None);
}

#[test]
fn a_data_folder_port_lists_no_file_as_a_game() {
    // D15: Cave Story, Wolfenstein 3D, Mr. Boom, Flashback and Rick
    // Dangerous are game folders claimed by their folder alias only. Their
    // data files are not games: listing each one would fill the library
    // with hundreds of entries and hand them to dedupe.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    touch(root, "roms/cavestory/Cave Story/Doukutsu.exe");
    touch(root, "roms/cavestory/Cave Story/data/Stage/Cave.pxm");
    touch(root, "roms/cavestory/Cave Story/data/Org/access.org");
    touch(root, "roms/snes/Zelda (USA).sfc");
    let scan = scan(root, "batocera");
    let cave = system(&scan, "cavestory");
    assert_eq!(cave.system_id.as_deref(), Some("cavestory"));
    assert!(cave.roms.is_empty(), "{:?}", stems(cave));
    assert_eq!(stems(system(&scan, "snes")), ["Zelda (USA)"]);
}
