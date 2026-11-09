//! What a game folder is (game folders spec D3, §8.1): the fixed folder
//! names and the one-listing shape test shared by the walk, the scan and
//! the dedupe units.

use coverly_core::game_folder::{
    classify_listing, is_system_level_fixed, ChildRole, DirShape, FixedFolder, GameBase,
};

fn ext(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// ROM-like root files for these tables. Track files (`.bin`) are excluded,
/// as the caller does when a sheet is present.
fn rom_like(name: &str) -> bool {
    matches!(ext(name).as_str(), "sfc" | "cia" | "nsp" | "iso")
}

fn sheet(name: &str) -> bool {
    matches!(ext(name).as_str(), "cue" | "gdi" | "ccd" | "m3u")
}

fn listing(entries: &[&str]) -> Vec<(String, bool)> {
    entries
        .iter()
        .map(|e| match e.strip_suffix('/') {
            Some(dir) => (dir.to_string(), true),
            None => (e.to_string(), false),
        })
        .collect()
}

fn shape(entries: &[&str], folder_game: bool) -> DirShape {
    classify_listing(&listing(entries), &rom_like, &sheet, folder_game)
}

fn fixed(list: &[(FixedFolder, &str)]) -> Vec<(FixedFolder, String)> {
    list.iter().map(|(f, n)| (*f, n.to_string())).collect()
}

#[test]
fn dir_names_are_exact_english() {
    let expected = [
        (FixedFolder::Updates, "Updates"),
        (FixedFolder::Dlc, "DLC"),
        (FixedFolder::Mods, "Mods"),
        (FixedFolder::Cheats, "Cheats"),
        (FixedFolder::Extras, "Extras"),
        (FixedFolder::Saves, "Saves"),
    ];
    assert_eq!(FixedFolder::ALL.len(), expected.len());
    for ((folder, name), all) in expected.iter().zip(FixedFolder::ALL) {
        assert_eq!(*folder, all);
        assert_eq!(folder.dir_name(), *name);
        assert_eq!(FixedFolder::from_dir_name(name), Some(*folder));
    }
}

#[test]
fn names_match_case_insensitively_and_exactly() {
    let table: &[(&str, Option<FixedFolder>)] = &[
        ("Updates", Some(FixedFolder::Updates)),
        ("UPDATES", Some(FixedFolder::Updates)),
        ("updates", Some(FixedFolder::Updates)),
        ("dlc", Some(FixedFolder::Dlc)),
        ("Dlc", Some(FixedFolder::Dlc)),
        ("mods", Some(FixedFolder::Mods)),
        ("CHEATS", Some(FixedFolder::Cheats)),
        ("extras", Some(FixedFolder::Extras)),
        ("saves", Some(FixedFolder::Saves)),
        ("Update", None),
        ("DLCs", None),
        ("Updates (old)", None),
        ("Saves ", None),
        (" Saves", None),
        ("", None),
        ("Mod", None),
    ];
    for (name, want) in table {
        assert_eq!(FixedFolder::from_dir_name(name), *want, "{name:?}");
    }
}

#[test]
fn child_roles_of_fixed_folders() {
    assert_eq!(FixedFolder::Updates.child_role(), Some(ChildRole::Update));
    assert_eq!(FixedFolder::Dlc.child_role(), Some(ChildRole::Dlc));
    for f in [
        FixedFolder::Mods,
        FixedFolder::Cheats,
        FixedFolder::Extras,
        FixedFolder::Saves,
    ] {
        assert_eq!(f.child_role(), None, "{f:?}");
    }
}

#[test]
fn one_rom_file_with_updates_is_a_game_folder() {
    assert_eq!(
        shape(&["Pokemon.cia", "Updates/"], false),
        DirShape::GameFolder {
            base: GameBase::File("Pokemon.cia".into()),
            fixed: fixed(&[(FixedFolder::Updates, "Updates")]),
        }
    );
}

#[test]
fn one_sheet_set_with_dlc_is_a_game_folder() {
    assert_eq!(
        shape(
            &[
                "Game.cue",
                "Game (Track 1).bin",
                "Game (Track 2).bin",
                "DLC/"
            ],
            false
        ),
        DirShape::GameFolder {
            base: GameBase::Sheet("Game.cue".into()),
            fixed: fixed(&[(FixedFolder::Dlc, "DLC")]),
        }
    );
}

#[test]
fn m3u_with_its_disc_sheets_is_one_base() {
    assert_eq!(
        shape(
            &[
                "Game.m3u",
                "Game (Disc 1).cue",
                "Game (Disc 2).cue",
                "Updates/"
            ],
            false
        ),
        DirShape::GameFolder {
            base: GameBase::Sheet("Game.m3u".into()),
            fixed: fixed(&[(FixedFolder::Updates, "Updates")]),
        }
    );
}

#[test]
fn folder_game_with_updates_is_a_game_folder() {
    assert_eq!(
        shape(&["PS3_GAME/", "PS3_DISC.SFB", "Updates/"], true),
        DirShape::GameFolder {
            base: GameBase::SelfIsFolderGame,
            fixed: fixed(&[(FixedFolder::Updates, "Updates")]),
        }
    );
}

#[test]
fn several_roms_with_mods_are_loose() {
    assert_eq!(
        shape(&["a.sfc", "b.sfc", "Mods/"], false),
        DirShape::Loose {
            fixed: fixed(&[(FixedFolder::Mods, "Mods")]),
        }
    );
}

#[test]
fn several_sheets_without_m3u_are_loose() {
    assert_eq!(
        shape(&["A.cue", "B.cue", "DLC/"], false),
        DirShape::Loose {
            fixed: fixed(&[(FixedFolder::Dlc, "DLC")]),
        }
    );
}

#[test]
fn sheet_and_another_rom_are_loose() {
    assert_eq!(
        shape(&["A.cue", "B.iso", "DLC/"], false),
        DirShape::Loose {
            fixed: fixed(&[(FixedFolder::Dlc, "DLC")]),
        }
    );
}

#[test]
fn only_a_fixed_folder_is_loose() {
    assert_eq!(
        shape(&["Mods/"], false),
        DirShape::Loose {
            fixed: fixed(&[(FixedFolder::Mods, "Mods")]),
        }
    );
}

#[test]
fn no_fixed_folder_is_plain() {
    assert_eq!(shape(&["a.sfc"], false), DirShape::Plain);
    assert_eq!(shape(&[], false), DirShape::Plain);
    assert_eq!(shape(&["PS3_GAME/"], true), DirShape::Plain);
}

#[test]
fn a_file_named_like_a_fixed_folder_does_not_count() {
    assert_eq!(shape(&["Game.cia", "DLC"], false), DirShape::Plain);
}

#[test]
fn non_rom_files_do_not_count_as_bases() {
    assert_eq!(
        shape(&["Game.cia", "readme.txt", "cover.png", "Updates/"], false),
        DirShape::GameFolder {
            base: GameBase::File("Game.cia".into()),
            fixed: fixed(&[(FixedFolder::Updates, "Updates")]),
        }
    );
}

#[test]
fn both_spellings_are_listed_sorted_by_bytes() {
    assert_eq!(
        shape(&["Game.cia", "dlc/", "Updates/", "DLC/"], false),
        DirShape::GameFolder {
            base: GameBase::File("Game.cia".into()),
            fixed: fixed(&[
                (FixedFolder::Dlc, "DLC"),
                (FixedFolder::Updates, "Updates"),
                (FixedFolder::Dlc, "dlc"),
            ]),
        }
    );
    // `DLC` sorts before `dlc` by bytes, whatever the listing order.
    let DirShape::GameFolder { fixed: found, .. } = shape(&["Game.cia", "dlc/", "DLC/"], false)
    else {
        panic!("expected a game folder");
    };
    assert_eq!(
        found,
        fixed(&[(FixedFolder::Dlc, "DLC"), (FixedFolder::Dlc, "dlc")])
    );
}

#[test]
fn system_level_fixed_folders_are_never_a_games() {
    assert!(is_system_level_fixed("DLC", true));
    assert!(is_system_level_fixed("dlc", true));
    assert!(is_system_level_fixed("Updates", true));
    assert!(!is_system_level_fixed("DLC", false));
    assert!(!is_system_level_fixed("Hacks", true));
    assert!(!is_system_level_fixed("DLCs", true));
}
