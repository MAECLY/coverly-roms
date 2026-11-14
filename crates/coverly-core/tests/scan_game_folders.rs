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
use coverly_core::scan::{
    scan_library, FixedFolderCounts, LibraryScan, RomEntry, ScanOptions, SystemScan,
};
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

#[test]
fn demos_betas_and_protos_are_not_revisions_of_the_game() {
    let lib = TempDir::new().unwrap();
    for (i, name) in [
        "Sonic the Hedgehog (USA, Europe).md",
        "Sonic the Hedgehog (USA, Europe) (Rev 1).md",
        "Sonic the Hedgehog (USA, Europe) (Demo).md",
        "Sonic the Hedgehog (USA, Europe) (Beta).md",
        "Sonic the Hedgehog (USA, Europe) (Proto).md",
        "Sonic the Hedgehog (USA, Europe) (Proto) (Rev 1).md",
    ]
    .into_iter()
    .enumerate()
    {
        put(lib.path(), &format!("ROMS/MD/{name}"), &[i as u8; 16]);
    }
    let s = scan(lib.path(), "coverly-library");
    let md = system(&s, "MD");
    let group = |name: &str| {
        md.roms
            .iter()
            .find(|r| r.path.file_name().unwrap() == name)
            .unwrap_or_else(|| panic!("{name} in {:?}", md.roms))
            .version_group
            .clone()
            .unwrap_or_else(|| panic!("{name} has no version group"))
    };
    let retail = group("Sonic the Hedgehog (USA, Europe).md");
    assert_eq!(retail, group("Sonic the Hedgehog (USA, Europe) (Rev 1).md"));
    let demo = group("Sonic the Hedgehog (USA, Europe) (Demo).md");
    let beta = group("Sonic the Hedgehog (USA, Europe) (Beta).md");
    let proto = group("Sonic the Hedgehog (USA, Europe) (Proto).md");
    // A demo is its own game (D12) and a beta or prototype is not a
    // revision of the release: none folds under the retail game.
    for other in [&demo, &beta, &proto] {
        assert_ne!(&retail, other);
    }
    assert_ne!(demo, beta);
    assert_ne!(beta, proto);
    // Revisions of the prototype still group with it.
    assert_eq!(
        proto,
        group("Sonic the Hedgehog (USA, Europe) (Proto) (Rev 1).md")
    );
}

/// Part C D9: a game folder's `Mods/`, `Cheats/`, `Extras/` and `Saves/`
/// are counted from metadata only (regular files, depth ≤ 4, junk and
/// AppleDouble twins left out); nothing in them is opened.
#[test]
fn fixed_folder_counts_are_metadata_only() {
    let lib = TempDir::new().unwrap();
    let g = "ROMS/3DS/Lantern Harbor (Europe)";
    put(
        lib.path(),
        &format!("{g}/Lantern Harbor (Europe).cia"),
        &ct::cia(BASE, true, false).head_bytes(),
    );
    let counted = [
        put(lib.path(), &format!("{g}/Mods/a.bps"), &[1u8; 8]),
        put(lib.path(), &format!("{g}/Mods/pack/x.png"), &[1u8; 8]),
        put(lib.path(), &format!("{g}/Cheats/c.cht"), &[2u8; 8]),
        put(lib.path(), &format!("{g}/Extras/Manuals/m.pdf"), &[3u8; 8]),
        put(lib.path(), &format!("{g}/Saves/s.sav"), &[4u8; 8]),
    ];
    for junk in ["._a.bps", ".DS_Store", "Thumbs.db", "desktop.ini"] {
        put(lib.path(), &format!("{g}/Mods/{junk}"), &[0u8; 8]);
    }
    coverly_core::consoles::start_open_log();
    let s = scan(lib.path(), "coverly-library");
    let opened = coverly_core::consoles::take_open_log();
    let ds = system(&s, "3DS");
    assert_eq!(names(&ds.roms), ["Lantern Harbor (Europe).cia"]);
    assert_eq!(
        ds.roms[0].fixed_counts,
        Some(FixedFolderCounts {
            mods: 2,
            cheats: 1,
            extras: 1,
            saves: 1,
            capped: false,
        })
    );
    for path in &counted {
        assert!(!opened.contains(path), "{path:?} was opened: {opened:?}");
    }
}

/// Part C D10 (a): an art folder under `DLC/` is never walked for
/// children, even when hidden entries are scanned.
#[test]
fn a_media_dir_under_dlc_is_not_a_child_even_with_hidden_on() {
    let lib = TempDir::new().unwrap();
    let g = "ROMS/3DS/Pokemon Y (World)";
    put(
        lib.path(),
        &format!("{g}/Pokemon Y (World).cia"),
        &ct::cia(BASE, true, false).head_bytes(),
    );
    put(
        lib.path(),
        &format!("{g}/DLC/Pack.cia"),
        &ct::cia(DLC, true, false).head_bytes(),
    );
    put(lib.path(), &format!("{g}/DLC/.media/x-icon.png"), &[9u8; 8]);
    put(
        lib.path(),
        &format!("{g}/DLC/.media/Stray.cia"),
        &ct::cia(DLC, true, false).head_bytes(),
    );
    let opts = ScanOptions {
        include_hidden: true,
        ..ScanOptions::default()
    };
    let s = scan_library(
        lib.path(),
        &profile("coverly-library"),
        &SystemDb::builtin(),
        &opts,
    );
    let ds = system(&s, "3DS");
    assert_eq!(names(&ds.roms), ["Pokemon Y (World).cia"], "{:?}", ds.roms);
    assert_eq!(
        child_names(&ds.roms[0]),
        [("Pack.cia".to_string(), ChildRole::Dlc)]
    );
}

/// Part C D9: only a base at a game folder's root carries counts; a loose
/// ROM, a ROM in a plain folder and an orphan carry none.
#[test]
fn fixed_counts_only_on_a_game_folder_base() {
    let lib = TempDir::new().unwrap();
    pokemon_library(lib.path());
    put(
        lib.path(),
        "ROMS/3DS/Plain/Tidewalker (USA).cia",
        &[6u8; 64],
    );
    put(
        lib.path(),
        "ROMS/3DS/Loose Pile/Updates/Neon Rhythm (v1.2).cia",
        &ct::cia(UPDATE, true, false).head_bytes(),
    );
    let s = scan(lib.path(), "coverly-library");
    let ds = system(&s, "3DS");
    let by_name = |n: &str| {
        ds.roms
            .iter()
            .find(|r| r.path.file_name().is_some_and(|f| f == n))
            .unwrap_or_else(|| panic!("no `{n}` in {:?}", names(&ds.roms)))
    };
    assert_eq!(by_name("Zelda (USA).cia").fixed_counts, None);
    assert_eq!(by_name("Tidewalker (USA).cia").fixed_counts, None);
    let orphan = by_name("Neon Rhythm (v1.2).cia");
    assert_eq!(orphan.role, Some(ChildRole::Update));
    assert_eq!(orphan.fixed_counts, None);
    assert_eq!(
        by_name("Pokemon Y (World).cia").fixed_counts,
        Some(FixedFolderCounts {
            mods: 1,
            cheats: 1,
            extras: 1,
            saves: 1,
            capped: false,
        })
    );
}

/// Part C D6, D8: a demo links to its full game and an orphan update to
/// the game its folder names, by index into the same system's entries;
/// a game of the same name in another system is never a target.
#[test]
fn demos_and_orphans_link_to_their_game_in_the_same_system() {
    let lib = TempDir::new().unwrap();
    let root = lib.path();
    let base = ct::cia(BASE, true, false).head_bytes();
    put(root, "ROMS/3DS/Tidewalker (USA).cia", &base);
    put(root, "ROMS/3DS/Tidewalker (USA) (Demo).cia", &base);
    put(
        root,
        "ROMS/3DS/Downloads/Tidewalker/Updates/Tidewalker (v1.3).cia",
        &ct::cia(UPDATE, true, false).head_bytes(),
    );
    put(root, "ROMS/GB/Lantern Harbor (Europe).gb", &[1u8; 64]);
    put(root, "ROMS/3DS/Lantern Harbor (Europe) (Demo).cia", &base);
    let s = scan(root, "coverly-library");
    let ds = system(&s, "3DS");
    let index = |n: &str| {
        ds.roms
            .iter()
            .position(|r| r.path.file_name().is_some_and(|f| f == n))
            .unwrap_or_else(|| panic!("no `{n}` in {:?}", names(&ds.roms)))
    };
    let game = index("Tidewalker (USA).cia");
    let demo = &ds.roms[index("Tidewalker (USA) (Demo).cia")];
    assert!(demo.is_demo);
    assert_eq!(demo.demo_of, Some(game));
    let orphan = &ds.roms[index("Tidewalker (v1.3).cia")];
    assert_eq!(orphan.role, Some(ChildRole::Update));
    assert_eq!(orphan.orphan_of, Some(game));
    assert!(!orphan.is_demo);
    let lone = &ds.roms[index("Lantern Harbor (Europe) (Demo).cia")];
    assert!(lone.is_demo);
    assert_eq!(lone.demo_of, None, "never linked across systems");
    let full = &ds.roms[game];
    assert!(!full.is_demo);
    assert_eq!((full.demo_of, full.orphan_of), (None, None));
}

/// Extras D9: `Extras/extras.json` and its malformed-file backups are the
/// index, not extras: left out of the count at the root of `Extras/`
/// only. A json filed in a category is the user's and still counts.
#[test]
fn extras_count_leaves_out_the_index_and_its_backups_at_the_root_only() {
    let lib = TempDir::new().unwrap();
    let g = "ROMS/3DS/Lantern Harbor (Europe)";
    put(
        lib.path(),
        &format!("{g}/Lantern Harbor (Europe).cia"),
        &ct::cia(BASE, true, false).head_bytes(),
    );
    put(lib.path(), &format!("{g}/Extras/extras.json"), b"{}");
    put(
        lib.path(),
        &format!("{g}/Extras/extras.broken-20261113-120000.json"),
        b"{",
    );
    put(
        lib.path(),
        &format!("{g}/Extras/Guides/extras-notes.json"),
        b"{}",
    );
    put(
        lib.path(),
        &format!("{g}/Extras/Soundtrack/a.flac"),
        &[3u8; 8],
    );
    let s = scan(lib.path(), "coverly-library");
    let ds = system(&s, "3DS");
    assert_eq!(names(&ds.roms), ["Lantern Harbor (Europe).cia"]);
    let counts = ds.roms[0].fixed_counts.expect("a game folder base");
    assert_eq!(counts.extras, 2, "{counts:?}");
    assert!(!counts.capped);
}

/// Extras D8: what an Extras job set aside in `<library>/.coverly-trash/`
/// is never a game again, whatever the hidden-entries option says and
/// wherever the folder sits (library root or inside a system folder).
#[test]
fn coverly_trash_is_never_scanned_even_with_include_hidden() {
    let lib = TempDir::new().unwrap();
    let root = lib.path();
    put(root, "GB/Kept (Europe).gb", &[1u8; 64]);
    put(
        root,
        ".coverly-trash/job-1/GB/Set Aside (Europe).gb",
        &[2u8; 64],
    );
    put(
        root,
        "GB/.coverly-trash/job-2/Inner (Europe).gb",
        &[3u8; 64],
    );
    let flat = Profile {
        roms_root: String::new(),
        ..profile("coverly-library")
    };
    let opts = ScanOptions {
        include_hidden: true,
        ..ScanOptions::default()
    };
    let s = scan_library(root, &flat, &SystemDb::builtin(), &opts);
    assert!(
        s.systems
            .iter()
            .all(|sys| !sys.folder_name.eq_ignore_ascii_case(".coverly-trash")),
        "{:?}",
        s.systems
    );
    let all: Vec<&RomEntry> = s.systems.iter().flat_map(|sys| &sys.roms).collect();
    for rom in &all {
        assert!(
            !rom.path
                .components()
                .any(|c| c.as_os_str() == ".coverly-trash"),
            "{:?} was scanned",
            rom.path
        );
    }
    assert_eq!(names(&system(&s, "GB").roms), ["Kept (Europe).gb"]);
}
