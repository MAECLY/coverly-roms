mod common;

use common::{profile, write};
use coverly_core::art::{resolve_art, ArtState};
use coverly_core::profiles::{fit_to_root, Media};
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

#[test]
fn picking_the_roms_folder_itself_scans_it_directly() {
    // The user picked `…/ROMS`, not the SD root.
    let dir = TempDir::new().unwrap();
    write(dir.path(), "SFC/Zelda (USA).sfc", "z");
    write(dir.path(), "SFC/Imgs/Zelda (USA).png", "png");
    write(dir.path(), "APPS/tool.sh", "t");
    let db = SystemDb::builtin();

    let fitted = fit_to_root(&profile("anbernic-stock"), dir.path(), &db);
    assert_eq!(fitted.roms_root, "");

    let scan = scan_library(dir.path(), &fitted, &db, &ScanOptions::default());
    assert!(scan.warnings.is_empty(), "{:?}", scan.warnings);
    let sfc = scan
        .systems
        .iter()
        .find(|s| s.folder_name == "SFC")
        .unwrap();
    assert_eq!(sfc.roms.len(), 1);
    let art = resolve_art(&scan, &fitted, &db, &[Media::Cover]);
    let sfc_art = art.iter().find(|s| s.scan.folder_name == "SFC").unwrap();
    assert!(matches!(
        sfc_art.roms[0].art[&Media::Cover],
        ArtState::Present { .. }
    ));
}

#[test]
fn an_sd_root_keeps_its_roms_root() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "ROMS/SFC/Zelda (USA).sfc", "z");
    let fitted = fit_to_root(&profile("anbernic-stock"), dir.path(), &SystemDb::builtin());
    assert_eq!(fitted, profile("anbernic-stock"));
}

#[test]
fn a_folder_without_system_folders_keeps_the_profile() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "Photos/a.jpg", "j");
    let fitted = fit_to_root(&profile("anbernic-stock"), dir.path(), &SystemDb::builtin());
    assert_eq!(fitted, profile("anbernic-stock"));
}

#[test]
fn profiles_with_art_outside_system_folders_are_never_fitted() {
    // ES-DE keeps art in `{root}/downloaded_media`: scanning the ROMs
    // folder alone would look for art in the wrong place.
    let dir = TempDir::new().unwrap();
    write(dir.path(), "snes/Zelda (USA).sfc", "z");
    let fitted = fit_to_root(&profile("esde"), dir.path(), &SystemDb::builtin());
    assert_eq!(fitted, profile("esde"));
}
