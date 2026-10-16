//! Art status per ROM and media, and orphan art, on fixture trees.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::art::{resolve_art, ArtState, RomArt, SystemArt};
use coverly_core::profiles::Media;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::systems::SystemDb;

use common::{profile, realistic_library, touch, write};

fn resolve(root: &Path, profile_id: &str, media: &[Media]) -> Vec<SystemArt> {
    let profile = profile(profile_id);
    let systems = SystemDb::builtin();
    let scan = scan_library(root, &profile, &systems, &ScanOptions::default());
    resolve_art(&scan, &profile, &systems, media)
}

fn system<'a>(art: &'a [SystemArt], folder: &str) -> &'a SystemArt {
    art.iter()
        .find(|s| s.scan.folder_name == folder)
        .unwrap_or_else(|| panic!("system folder `{folder}` not listed"))
}

fn rom<'a>(system: &'a SystemArt, stem: &str) -> &'a RomArt {
    system
        .roms
        .iter()
        .find(|r| r.rom.stem == stem)
        .unwrap_or_else(|| panic!("ROM `{stem}` not listed"))
}

fn state<'a>(system: &'a SystemArt, stem: &str, media: Media) -> &'a ArtState {
    rom(system, stem)
        .art
        .get(&media)
        .unwrap_or_else(|| panic!("no {media:?} state for `{stem}`"))
}

/// File name of a present art file, as on disk.
fn present_name(state: &ArtState) -> String {
    match state {
        ArtState::Present { path } => path
            .file_name()
            .expect("art path has a file name")
            .to_string_lossy()
            .into_owned(),
        other => panic!("expected present art, got {other:?}"),
    }
}

fn orphan_names(system: &SystemArt) -> Vec<String> {
    system
        .orphans
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn anbernic_cover_present_missing_unsupported_and_orphans() {
    let lib = realistic_library();
    let root = lib.path();
    let imgs = root.join("ROMS/SFC/Imgs");
    // Replace the fixture cover with a case-different twin (remove first so
    // a case-insensitive file system does not keep the old spelling).
    fs::remove_file(imgs.join("Zelda (USA).png")).unwrap();
    write(root, "ROMS/SFC/Imgs/zelda (usa).PNG", "png");
    write(root, "ROMS/SFC/Imgs/Old Game.png", "png");
    write(root, "ROMS/SFC/Imgs/._Zelda (USA).png", "appledouble");
    write(root, "ROMS/SFC/Imgs/._Old Game.png", "appledouble");
    write(root, "ROMS/SFC/Imgs/readme.txt", "not an image");

    let art = resolve(root, "anbernic-stock", &Media::ALL);

    let sfc = system(&art, "SFC");
    assert_eq!(sfc.scan.system_id.as_deref(), Some("snes"));
    assert!(sfc.scan.roms.is_empty(), "the scan is kept without ROMs");
    assert_eq!(sfc.roms.len(), 2);

    let zelda = state(sfc, "Zelda (USA)", Media::Cover);
    assert_eq!(present_name(zelda), "zelda (usa).PNG");
    assert_eq!(
        zelda,
        &ArtState::Present {
            path: imgs.join("zelda (usa).PNG")
        }
    );
    assert_eq!(state(sfc, "MARIO", Media::Cover), &ArtState::Missing);
    assert_eq!(
        state(sfc, "Zelda (USA)", Media::Screenshot),
        &ArtState::Unsupported
    );
    assert_eq!(state(sfc, "MARIO", Media::Title), &ArtState::Unsupported);

    assert_eq!(orphan_names(sfc), ["Old Game.png"]);
    assert_eq!(sfc.orphans, [imgs.join("Old Game.png")]);

    // Systems without art have no orphans; unknown folders have no ROMs.
    assert!(system(&art, "PS").orphans.is_empty());
    assert_eq!(
        state(system(&art, "PS"), "FF7", Media::Cover),
        &ArtState::Missing
    );
    let apps = system(&art, "APPS");
    assert_eq!(apps.scan.system_id, None);
    assert!(apps.roms.is_empty() && apps.orphans.is_empty());
}

#[test]
fn only_requested_media_are_resolved() {
    let lib = realistic_library();
    let art = resolve(lib.path(), "anbernic-stock", &[Media::Cover]);
    let sfc = system(&art, "SFC");
    let keys: Vec<Media> = rom(sfc, "MARIO").art.keys().copied().collect();
    assert_eq!(keys, [Media::Cover]);
}

#[test]
fn other_extension_and_upper_case_art_folder_count_as_present() {
    let lib = realistic_library();
    let root = lib.path();
    // `.jpg`/`.webp` twins when the profile writes `.png`; an `IMGS` folder.
    fs::remove_dir_all(root.join("ROMS/SFC/Imgs")).unwrap();
    write(root, "ROMS/SFC/IMGS/Zelda (USA).jpg", "jpg");
    write(root, "ROMS/SFC/IMGS/mario.WEBP", "webp");

    let art = resolve(root, "anbernic-stock", &[Media::Cover]);
    let sfc = system(&art, "SFC");
    assert_eq!(
        state(sfc, "Zelda (USA)", Media::Cover),
        &ArtState::Present {
            path: root.join("ROMS/SFC/IMGS/Zelda (USA).jpg")
        },
        "the path is spelled as on disk"
    );
    assert_eq!(
        present_name(state(sfc, "MARIO", Media::Cover)),
        "mario.WEBP"
    );
    assert!(sfc.orphans.is_empty());
}

#[test]
fn png_is_preferred_when_several_twins_exist() {
    let lib = realistic_library();
    let root = lib.path();
    write(root, "ROMS/SFC/Imgs/Zelda (USA).jpg", "jpg");
    let art = resolve(root, "anbernic-stock", &[Media::Cover]);
    let sfc = system(&art, "SFC");
    assert_eq!(
        present_name(state(sfc, "Zelda (USA)", Media::Cover)),
        "Zelda (USA).png"
    );
    assert!(sfc.orphans.is_empty(), "a twin is not an orphan");
}

#[test]
fn mirrored_subfolders_look_for_art_next_to_the_rom() {
    let lib = realistic_library();
    let root = lib.path();
    // Art at the system level does not count for a ROM in `Hacks/` when the
    // profile mirrors subfolders; it is an orphan there.
    write(root, "ROMS/MD/Imgs/Sonic Hack.png", "png");
    let art = resolve(root, "anbernic-stock", &[Media::Cover]);
    let md = system(&art, "MD");
    assert_eq!(state(md, "Sonic Hack", Media::Cover), &ArtState::Missing);
    assert_eq!(md.orphans, [root.join("ROMS/MD/Imgs/Sonic Hack.png")]);

    write(root, "ROMS/MD/Hacks/Imgs/Sonic Hack.png", "png");
    write(root, "ROMS/MD/Hacks/Imgs/Unused.png", "png");
    let art = resolve(root, "anbernic-stock", &[Media::Cover]);
    let md = system(&art, "MD");
    assert_eq!(
        state(md, "Sonic Hack", Media::Cover),
        &ArtState::Present {
            path: root.join("ROMS/MD/Hacks/Imgs/Sonic Hack.png")
        }
    );
    assert_eq!(
        md.orphans,
        [
            root.join("ROMS/MD/Hacks/Imgs/Unused.png"),
            root.join("ROMS/MD/Imgs/Sonic Hack.png"),
        ]
    );
}

#[test]
fn flat_layouts_ignore_the_rom_subfolder() {
    let lib = tempfile::TempDir::new().unwrap();
    let root = lib.path();
    touch(root, "roms/megadrive/Hacks/Sonic Hack.md");
    touch(root, "roms/megadrive/images/Sonic Hack-image.png");
    // Batocera's `-thumb` postfix also counts as a cover (§16.8).
    touch(root, "roms/snes/Zelda (USA).sfc");
    touch(root, "roms/snes/images/Zelda (USA)-thumb.png");
    touch(root, "roms/snes/images/Zelda (USA)-screenshot.JPG");
    touch(root, "roms/snes/images/Gone-image.png");

    let art = resolve(root, "batocera", &Media::ALL);
    let md = system(&art, "megadrive");
    assert_eq!(
        present_name(state(md, "Sonic Hack", Media::Cover)),
        "Sonic Hack-image.png"
    );
    assert_eq!(state(md, "Sonic Hack", Media::Title), &ArtState::Missing);

    let snes = system(&art, "snes");
    assert_eq!(
        present_name(state(snes, "Zelda (USA)", Media::Cover)),
        "Zelda (USA)-thumb.png"
    );
    assert_eq!(
        present_name(state(snes, "Zelda (USA)", Media::Screenshot)),
        "Zelda (USA)-screenshot.JPG"
    );
    assert_eq!(orphan_names(snes), ["Gone-image.png"]);
}

#[test]
fn esde_art_under_downloaded_media() {
    let lib = tempfile::TempDir::new().unwrap();
    let root = lib.path();
    touch(root, "ROMs/snes/Zelda (USA).sfc");
    touch(root, "ROMs/snes/Hacks/Kaizo.sfc");
    touch(root, "downloaded_media/snes/covers/Zelda (USA).jpg");
    touch(root, "downloaded_media/snes/covers/Hacks/Kaizo.png");
    touch(root, "downloaded_media/snes/screenshots/Stray.png");

    let art = resolve(root, "esde", &Media::ALL);
    let snes = system(&art, "snes");
    assert_eq!(
        state(snes, "Zelda (USA)", Media::Cover),
        &ArtState::Present {
            path: root.join("downloaded_media/snes/covers/Zelda (USA).jpg")
        }
    );
    assert_eq!(
        present_name(state(snes, "Kaizo", Media::Cover)),
        "Kaizo.png"
    );
    assert_eq!(
        state(snes, "Zelda (USA)", Media::Screenshot),
        &ArtState::Missing
    );
    assert_eq!(state(snes, "Zelda (USA)", Media::Title), &ArtState::Missing);
    assert_eq!(
        snes.orphans,
        [root.join("downloaded_media/snes/screenshots/Stray.png")]
    );
}

#[test]
fn retroarch_without_a_libretro_name_is_unsupported() {
    let lib = tempfile::TempDir::new().unwrap();
    let root = lib.path();
    touch(root, "snes/Zelda (USA).sfc");
    touch(
        root,
        "thumbnails/Nintendo - Super Nintendo Entertainment System/Named_Boxarts/Zelda (USA).png",
    );
    let profile = profile("retroarch");
    let systems = SystemDb::builtin()
        .with_overrides(
            "[[system]]\nid = \"homebrew\"\nname = \"Homebrew\"\naliases = [\"homebrew\"]\n",
        )
        .unwrap();
    touch(root, "homebrew/Demo.bin");
    let scan = scan_library(root, &profile, &systems, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &systems, &[Media::Cover]);

    let snes = system(&art, "snes");
    assert_eq!(
        present_name(state(snes, "Zelda (USA)", Media::Cover)),
        "Zelda (USA).png"
    );
    let homebrew = system(&art, "homebrew");
    assert_eq!(
        state(homebrew, "Demo", Media::Cover),
        &ArtState::Unsupported
    );
}

#[test]
fn art_state_serializes_with_a_state_tag() {
    let present = ArtState::Present {
        path: PathBuf::from("Imgs/Zelda.png"),
    };
    assert_eq!(
        serde_json::to_string(&present).unwrap(),
        r#"{"state":"present","path":"Imgs/Zelda.png"}"#
    );
    assert_eq!(
        serde_json::to_string(&ArtState::Missing).unwrap(),
        r#"{"state":"missing"}"#
    );
    assert_eq!(
        serde_json::to_string(&ArtState::Unsupported).unwrap(),
        r#"{"state":"unsupported"}"#
    );
}
