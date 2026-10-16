//! Fixture trees for integration tests, built in a [`TempDir`].

#![allow(dead_code)] // each test binary uses a different subset

use std::fs;
use std::path::Path;

use coverly_core::profiles::{builtin_profiles, Profile};
use tempfile::TempDir;

/// Writes `contents` to `root/rel`, creating parent folders.
pub fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("fixture path has a parent")).unwrap();
    fs::write(path, contents).unwrap();
}

/// Creates an empty file at `root/rel`, creating parent folders.
pub fn touch(root: &Path, rel: &str) {
    write(root, rel, "");
}

/// Creates the folder `root/rel` (and its parents).
pub fn mkdir(root: &Path, rel: &str) {
    fs::create_dir_all(root.join(rel)).unwrap();
}

/// The built-in profile with `id`.
pub fn profile(id: &str) -> Profile {
    builtin_profiles()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("no built-in profile `{id}`"))
}

/// A realistic Anbernic-stock SD card:
///
/// ```text
/// ROMS/SFC/Zelda (USA).sfc          ROM
/// ROMS/SFC/._Zelda (USA).sfc        AppleDouble twin, ignored
/// ROMS/SFC/MARIO.SMC                ROM, upper-case extension
/// ROMS/SFC/notes.txt                ignored extension
/// ROMS/SFC/Imgs/Zelda (USA).png     art folder, never scanned
/// ROMS/PS/FF7 (Disc 1).cue + .bin   companions of the m3u
/// ROMS/PS/FF7 (Disc 2).cue + .bin   companions of the m3u
/// ROMS/PS/FF7.m3u                   the one PS ROM
/// ROMS/MD/Hacks/Sonic Hack.md       ROM in a subfolder
/// ROMS/APPS/thing.sh                unknown system folder
/// ROMS/.hidden/x.sfc                hidden folder, ignored
/// ```
pub fn realistic_library() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "ROMS/SFC/Zelda (USA).sfc", "zelda-rom");
    write(root, "ROMS/SFC/._Zelda (USA).sfc", "appledouble");
    write(root, "ROMS/SFC/MARIO.SMC", "mario");
    write(root, "ROMS/SFC/notes.txt", "notes");
    write(root, "ROMS/SFC/Imgs/Zelda (USA).png", "png");
    write(
        root,
        "ROMS/PS/FF7 (Disc 1).cue",
        "FILE \"FF7 (Disc 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
    );
    write(root, "ROMS/PS/FF7 (Disc 1).bin", "disc-one");
    write(
        root,
        "ROMS/PS/FF7 (Disc 2).cue",
        "FILE \"FF7 (Disc 2).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
    );
    write(root, "ROMS/PS/FF7 (Disc 2).bin", "disc-two");
    write(
        root,
        "ROMS/PS/FF7.m3u",
        "FF7 (Disc 1).cue\nFF7 (Disc 2).cue\n",
    );
    write(root, "ROMS/MD/Hacks/Sonic Hack.md", "sonic");
    write(root, "ROMS/APPS/thing.sh", "#!/bin/sh\n");
    write(root, "ROMS/.hidden/x.sfc", "hidden");
    dir
}
