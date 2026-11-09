//! The game as a folder in the library scan (game folders spec D8, D15,
//! D5): updates and DLC in a game folder are children of their base, not
//! games; orphans stay visible with their role; `Mods/`, `Cheats/`,
//! `Extras/` and `Saves/` of a game folder are never scanned; a folder
//! game's size leaves its fixed folders out; retro revisions share a
//! version group. Whatever the profile and whatever the option says.

use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::consoles::testing as ct;
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::game_folder::ChildRole;
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::scan::{scan_library, LibraryScan, RomEntry, ScanOptions, SystemScan};
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c));
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    pin_mtime(&p, 1);
    p
}

fn at(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c))
}

fn profile(id: &str) -> Profile {
    builtin_profiles()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("no built-in profile `{id}`"))
}

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
        .find(|s| s.folder_name.eq_ignore_ascii_case(folder))
        .unwrap_or_else(|| panic!("system folder `{folder}` not listed: {:?}", scan.systems))
}

fn names(roms: &[RomEntry]) -> Vec<String> {
    roms.iter()
        .map(|r| r.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

fn child_names(rom: &RomEntry) -> Vec<(String, ChildRole)> {
    rom.children
        .iter()
        .map(|c| {
            (
                c.path.file_name().unwrap().to_string_lossy().into_owned(),
                c.role,
            )
        })
        .collect()
}

const BASE: u64 = 0x0004_0000_0005_5E00;
const UPDATE: u64 = 0x0004_000E_0005_5E00;
const DLC: u64 = 0x0004_008C_0005_5E00;

/// `ROMS/3DS/Pokemon Y (World)/` with its base, an update, a DLC, and
/// `Mods/`, `Cheats/`, `Extras/`, `Saves/` content named like games.
fn pokemon_library(root: &Path) {
    let g = "ROMS/3DS/Pokemon Y (World)";
    put(
        root,
        &format!("{g}/Pokemon Y (World).cia"),
        &ct::cia(BASE, true, false).head_bytes(),
    );
    put(
        root,
        &format!("{g}/Updates/Pokemon Y (World) (v1.5).cia"),
        &ct::cia(UPDATE, true, false).head_bytes(),
    );
    put(
        root,
        &format!("{g}/DLC/Pokemon Y (World) - Pack.cia"),
        &ct::cia(DLC, true, false).head_bytes(),
    );
    put(root, &format!("{g}/Mods/romfs patch.cia"), &[1u8; 32]);
    put(root, &format!("{g}/Cheats/cheat.cia"), &[2u8; 32]);
    put(root, &format!("{g}/Extras/OST.cia"), &[3u8; 32]);
    put(root, &format!("{g}/Saves/save.cia"), &[4u8; 32]);
    put(root, "ROMS/3DS/Zelda (USA).cia", &[5u8; 64]);
}

#[test]
fn updates_and_dlc_are_children_of_their_base() {
    let lib = TempDir::new().unwrap();
    pokemon_library(lib.path());
    let s = scan(lib.path(), "coverly-library");
    let ds = system(&s, "3DS");
    assert_eq!(
        names(&ds.roms),
        ["Zelda (USA).cia", "Pokemon Y (World).cia"],
        "{:?}",
        ds.roms
    );
    let pokemon = &ds.roms[1];
    assert_eq!(pokemon.role, None);
    assert_eq!(
        child_names(pokemon),
        [
            ("Pokemon Y (World) - Pack.cia".to_string(), ChildRole::Dlc),
            (
                "Pokemon Y (World) (v1.5).cia".to_string(),
                ChildRole::Update
            ),
        ]
    );
    let update = pokemon
        .children
        .iter()
        .find(|c| c.role == ChildRole::Update)
        .unwrap();
    let path = at(
        lib.path(),
        "ROMS/3DS/Pokemon Y (World)/Updates/Pokemon Y (World) (v1.5).cia",
    );
    assert_eq!(update.path, path);
    assert_eq!(update.size, fs::metadata(&path).unwrap().len());
    assert_eq!(
        update.modified,
        fs::metadata(&path).unwrap().modified().unwrap()
    );
    // The base's own size never includes its children.
    let base_len = fs::metadata(&pokemon.path).unwrap().len();
    assert_eq!(pokemon.size, base_len);
    // The game count is the top-level entries: the update and DLC are
    // not games, `Mods/`…`Saves/` are never scanned.
    let total: usize = s.systems.iter().map(|s| s.roms.len()).sum();
    assert_eq!(total, 2);
}

#[test]
fn orphans_stay_top_level_with_their_role() {
    let lib = TempDir::new().unwrap();
    let g = "ROMS/3DS/Pokemon Y (World)";
    put(
        lib.path(),
        &format!("{g}/Updates/u.cia"),
        &ct::cia(UPDATE, true, false).head_bytes(),
    );
    put(
        lib.path(),
        &format!("{g}/DLC/d.cia"),
        &ct::cia(DLC, true, false).head_bytes(),
    );
    put(lib.path(), &format!("{g}/Mods/m.cia"), &[1u8; 16]);
    let s = scan(lib.path(), "coverly-library");
    let ds = system(&s, "3DS");
    let found: Vec<(String, Option<ChildRole>)> = ds
        .roms
        .iter()
        .map(|r| {
            (
                r.path.file_name().unwrap().to_string_lossy().into_owned(),
                r.role,
            )
        })
        .collect();
    // No base at the root: not a game folder, so `Mods/` is scanned as
    // today and nothing is hidden.
    assert_eq!(
        found,
        [
            ("d.cia".to_string(), Some(ChildRole::Dlc)),
            ("m.cia".to_string(), None),
            ("u.cia".to_string(), Some(ChildRole::Update)),
        ],
        "{:?}",
        ds.roms
    );
    assert!(ds.roms.iter().all(|r| r.children.is_empty()));
}

#[test]
fn a_folder_of_several_games_keeps_its_mods_folder() {
    let lib = TempDir::new().unwrap();
    put(lib.path(), "ROMS/SFC/Hacks/Mods/x.sfc", &[1u8; 16]);
    put(lib.path(), "ROMS/SFC/Hacks/a.sfc", &[2u8; 16]);
    put(lib.path(), "ROMS/SFC/Hacks/b.sfc", &[3u8; 16]);
    let s = scan(lib.path(), "coverly-library");
    let sfc = system(&s, "SFC");
    assert_eq!(names(&sfc.roms), ["a.sfc", "b.sfc", "x.sfc"]);
    assert!(sfc.roms.iter().all(|r| r.role.is_none()));
}

#[test]
fn batocera_dlc_folder_under_the_system_is_unchanged() {
    let lib = TempDir::new().unwrap();
    let dlc = at(lib.path(), "roms/ps4/DLC/CUSA00001");
    ct::ps4_app(
        &dlc,
        "CUSA00001",
        "ac",
        "UP0001-CUSA00001_00-DLC0000000000001",
    );
    let s = scan(lib.path(), "batocera");
    let ps4 = system(&s, "ps4");
    assert_eq!(names(&ps4.roms), ["CUSA00001"], "{:?}", ps4.roms);
    let entry = &ps4.roms[0];
    assert!(entry.is_dir);
    assert_eq!(entry.role, None);
    assert!(entry.children.is_empty());
    assert_eq!(entry.rel_dir, PathBuf::from("DLC"));
}

#[test]
fn respected_whatever_the_profile() {
    let lib = TempDir::new().unwrap();
    pokemon_library(lib.path());
    let summary = |id: &str| {
        let s = scan(lib.path(), id);
        let ds = system(&s, "3DS");
        ds.roms
            .iter()
            .map(|r| (names(std::slice::from_ref(r)), child_names(r), r.role))
            .collect::<Vec<_>>()
    };
    let library = summary("coverly-library");
    // MinUI (a library opened as MinUI because of its `.media/`) and
    // the Anbernic stock profile (game folders off) see the same games.
    assert_eq!(summary("minui"), library);
    assert_eq!(summary("anbernic-stock"), library);
}

#[test]
fn folder_game_size_leaves_its_fixed_folders_out() {
    let lib = TempDir::new().unwrap();
    let game = at(lib.path(), "ROMS/PS3/Demons Souls [BLUS30443]");
    ct::ps3_jb(&game, "BLUS30443", "DG");
    let s = scan(lib.path(), "coverly-library");
    let before = system(&s, "PS3").roms[0].size;

    put(
        lib.path(),
        "ROMS/PS3/Demons Souls [BLUS30443]/Updates/BLUS30443 v1.04.pkg",
        &[7u8; 1000],
    );
    put(
        lib.path(),
        "ROMS/PS3/Demons Souls [BLUS30443]/Extras/manual.pkg",
        &[8u8; 500],
    );
    let s = scan(lib.path(), "coverly-library");
    let ps3 = system(&s, "PS3");
    assert_eq!(names(&ps3.roms), ["Demons Souls [BLUS30443]"]);
    let entry = &ps3.roms[0];
    assert!(entry.is_dir);
    assert_eq!(entry.size, before, "Updates/ and Extras/ are not the base");
    assert_eq!(
        child_names(entry),
        [("BLUS30443 v1.04.pkg".to_string(), ChildRole::Update)]
    );
    assert_eq!(entry.children[0].size, 1000);
}

#[test]
fn a_folder_update_inside_updates_is_one_child() {
    let lib = TempDir::new().unwrap();
    let game = at(lib.path(), "ROMS/PS3/Demons Souls [BLUS30443]");
    ct::ps3_jb(&game, "BLUS30443", "DG");
    ct::ps3_hdd(
        &game.join("Updates").join("BLUS30443-patch"),
        "BLUS30443",
        "GD",
    );
    let s = scan(lib.path(), "coverly-library");
    let ps3 = system(&s, "PS3");
    assert_eq!(
        names(&ps3.roms),
        ["Demons Souls [BLUS30443]"],
        "{:?}",
        ps3.roms
    );
    let entry = &ps3.roms[0];
    assert_eq!(
        child_names(entry),
        [("BLUS30443-patch".to_string(), ChildRole::Update)]
    );
    assert!(entry.children[0].size > 0);
}

#[test]
fn retro_revisions_share_a_version_group() {
    let lib = TempDir::new().unwrap();
    put(
        lib.path(),
        "ROMS/MD/Sonic the Hedgehog (USA, Europe).md",
        &[1u8; 16],
    );
    put(
        lib.path(),
        "ROMS/MD/Sonic the Hedgehog (USA, Europe) (Rev 1).md",
        &[2u8; 16],
    );
    put(
        lib.path(),
        "ROMS/MD/Sonic the Hedgehog (Japan).md",
        &[3u8; 16],
    );
    put(lib.path(), "ROMS/MD/Streets of Rage (v1.1).md", &[4u8; 16]);
    let s = scan(lib.path(), "coverly-library");
    let md = system(&s, "MD");
    let by = |name: &str| {
        md.roms
            .iter()
            .find(|r| r.path.file_name().unwrap() == name)
            .unwrap_or_else(|| panic!("{name} in {:?}", md.roms))
    };
    let plain = by("Sonic the Hedgehog (USA, Europe).md");
    let rev = by("Sonic the Hedgehog (USA, Europe) (Rev 1).md");
    let japan = by("Sonic the Hedgehog (Japan).md");
    assert!(plain.version_group.is_some());
    assert_eq!(plain.version_group, rev.version_group);
    assert_ne!(plain.version_group, japan.version_group, "region kept");
    assert_eq!(plain.revision, None);
    assert_eq!(rev.revision.as_deref(), Some("Rev 1"));
    assert_eq!(
        by("Streets of Rage (v1.1).md").revision.as_deref(),
        Some("v1.1")
    );
    assert_eq!(md.roms.len(), 4, "revisions stay separate entries");
}
