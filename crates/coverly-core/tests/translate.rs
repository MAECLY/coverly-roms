//! The pure translate module (mods spec D14, D15): master layout in,
//! one emulator's or frontend's layout out. Paths are compared by
//! component, never by slash text.

use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::addons::{AddonFormat, AddonKey};
use coverly_core::systems::SystemDb;
use coverly_core::translate::{
    master_layout, safe_join, translate, Destination, EmulatorId, EntryForm, FrontendMode, GameIds,
    MasterAddon, MasterEntry, MasterGame, Placement, TranslateScope, TranslateTarget,
    TranslatedGame, MODERN_SYSTEMS, RETROARCH_SYSTEMS,
};

/// A relative path from its components.
fn pb(parts: &[&str]) -> PathBuf {
    parts.iter().collect()
}

mod fixtures {
    use super::*;

    pub fn entry(path: &[&str], is_dir: bool, form: EntryForm) -> MasterEntry {
        MasterEntry {
            path: pb(path),
            is_dir,
            form,
            title_id: None,
            version: None,
            newest: true,
        }
    }

    pub fn addon(path: &[&str], is_dir: bool, format: AddonFormat, key: AddonKey) -> MasterAddon {
        MasterAddon {
            path: pb(path),
            is_dir,
            kind: format.kind(),
            format,
            key,
            header: None,
        }
    }

    /// A 3DS game in the master layout: base, one update, one mod folder
    /// and one cheat file.
    pub fn pokemon_y() -> MasterGame {
        let title = "Pokemon Y (World)";
        let tid = "0004000000055D00";
        MasterGame {
            system: "3ds".into(),
            title: title.into(),
            ids: GameIds {
                title_id: Some(tid.into()),
                serial: None,
                game_id: None,
            },
            base: entry(&[title, "Pokemon Y (World).cia"], false, EntryForm::Package),
            updates: vec![entry(
                &[title, "Updates", "Pokemon Y (World) (v1.4).cia"],
                false,
                EntryForm::Package,
            )],
            dlc: vec![],
            mods: vec![addon(
                &[title, "Mods", tid],
                true,
                AddonFormat::LayeredFs,
                AddonKey::TitleId { id: tid.into() },
            )],
            cheats: vec![addon(
                &[title, "Cheats", "0004000000055D00.txt"],
                false,
                AddonFormat::CtrCheats,
                AddonKey::TitleId { id: tid.into() },
            )],
            taken_in_emulator: vec![],
        }
    }

    /// A PS3 package game for ES-DE: base `Game.pkg` with an update and
    /// a DLC.
    pub fn game_pkg() -> MasterGame {
        MasterGame {
            system: "ps3".into(),
            title: "Game".into(),
            ids: GameIds::default(),
            base: entry(&["Game", "Game.pkg"], false, EntryForm::Package),
            updates: vec![entry(
                &["Game", "Updates", "Game (v1.01).pkg"],
                false,
                EntryForm::Package,
            )],
            dlc: vec![entry(
                &["Game", "DLC", "Game (Pack).pkg"],
                false,
                EntryForm::Package,
            )],
            mods: vec![],
            cheats: vec![],
            taken_in_emulator: vec![],
        }
    }
}

fn target(
    emulator: Option<EmulatorId>,
    frontend: FrontendMode,
    game_folders: bool,
    scope: TranslateScope,
) -> TranslateTarget {
    TranslateTarget {
        emulator,
        frontend,
        game_folders,
        scope,
    }
}

fn placement<'a>(t: &'a TranslatedGame, from_name: &str) -> &'a Placement {
    t.entries
        .iter()
        .find(|p| p.from.file_name().is_some_and(|n| n == from_name))
        .unwrap_or_else(|| panic!("no placement from {from_name}: {t:?}"))
}

fn dest(t: &TranslatedGame, from_name: &str) -> Destination {
    placement(t, from_name).to.clone()
}

#[test]
fn translate_never_touches_the_filesystem() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("translate");
    let mut files = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = fs::read_to_string(&path).unwrap();
        files += 1;
        for banned in [
            "std::fs",
            "std::time",
            "SystemTime",
            "Instant::",
            "std::env",
            "File::",
            "std::process",
            "std::net",
        ] {
            assert!(!text.contains(banned), "{} uses {banned}", path.display());
        }
    }
    assert!(files >= 2, "mod.rs and frontend.rs at least");
}

#[test]
fn crafted_keys_never_become_paths() {
    let long = "A".repeat(300);
    for bad in [
        "..",
        ".",
        "a/b",
        r"a\b",
        "C:",
        r"C:\x",
        "/abs",
        "",
        "x\0y",
        "CON",
        "trailing.",
        long.as_str(),
    ] {
        assert_eq!(safe_join(&["mods", bad]), None, "{bad:?}");
        assert_eq!(safe_join(&[bad]), None, "{bad:?}");
    }
    assert_eq!(safe_join(&[]), None);
    let ok = safe_join(&["load", "mods", "0004000000055D00"]).unwrap();
    assert_eq!(ok.components().count(), 3);
    assert_eq!(ok, pb(&["load", "mods", "0004000000055D00"]));
    // 255 bytes is the longest name a component may have.
    let max = "B".repeat(255);
    assert!(safe_join(&[max.as_str()]).is_some());
}

#[test]
fn the_master_layout_puts_everything_in_the_game_folder() {
    let g = fixtures::pokemon_y();
    let t = master_layout(&g);
    let title = "Pokemon Y (World)";
    assert_eq!(t.entries.len(), 4);
    assert_eq!(
        dest(&t, "Pokemon Y (World).cia"),
        Destination::System(pb(&[title, "Pokemon Y (World).cia"]))
    );
    assert_eq!(
        dest(&t, "Pokemon Y (World) (v1.4).cia"),
        Destination::System(pb(&[title, "Updates", "Pokemon Y (World) (v1.4).cia"]))
    );
    assert_eq!(
        dest(&t, "0004000000055D00"),
        Destination::System(pb(&[title, "Mods", "0004000000055D00"]))
    );
    assert_eq!(
        dest(&t, "0004000000055D00.txt"),
        Destination::System(pb(&[title, "Cheats", "0004000000055D00.txt"]))
    );
    assert!(t.entries.iter().all(|p| p.note.is_none()));
    // Base first, then updates, DLC, mods and cheats.
    assert_eq!(t.entries[0].from, g.base.path);
}

#[test]
fn a_master_layout_never_builds_a_path_from_an_unsafe_title() {
    let mut g = fixtures::pokemon_y();
    g.title = "../escape".into();
    let t = master_layout(&g);
    assert!(
        t.entries.iter().all(|p| p.to == Destination::Library),
        "{t:?}"
    );
}

#[test]
fn no_emulator_keeps_the_master_layout() {
    let g = fixtures::pokemon_y();
    for scope in [TranslateScope::Full, TranslateScope::Conservative] {
        let t = target(None, FrontendMode::Plain, true, scope);
        assert_eq!(translate(&g, &t), master_layout(&g));
    }
}

#[test]
fn es_de_dir_as_game_names_the_folder_after_the_base_file() {
    let g = fixtures::game_pkg();
    let full = target(None, FrontendMode::DirAsGame, true, TranslateScope::Full);
    let t = translate(&g, &full);
    assert_eq!(
        dest(&t, "Game.pkg"),
        Destination::System(pb(&["Game.pkg", "Game.pkg"]))
    );
    assert_eq!(
        dest(&t, "Game (v1.01).pkg"),
        Destination::System(pb(&["Game.pkg", "Updates", "Game (v1.01).pkg"]))
    );
    assert_eq!(
        dest(&t, "Game (Pack).pkg"),
        Destination::System(pb(&["Game.pkg", "DLC", "Game (Pack).pkg"]))
    );

    // Conservative scope never applies it.
    let cons = target(
        None,
        FrontendMode::DirAsGame,
        true,
        TranslateScope::Conservative,
    );
    assert_eq!(translate(&g, &cons), master_layout(&g));

    // Nor does a run with game folders off.
    let flat = target(None, FrontendMode::DirAsGame, false, TranslateScope::Full);
    assert_eq!(translate(&g, &flat), master_layout(&g));
}

#[test]
fn dir_as_game_also_follows_an_emulator_translator() {
    // The frontend pass runs after the emulator's: with no translator
    // rules yet the emulator keeps the master layout and the folder is
    // still named after the base file.
    let g = fixtures::game_pkg();
    let t = translate(
        &g,
        &target(
            Some(EmulatorId::Rpcs3),
            FrontendMode::DirAsGame,
            true,
            TranslateScope::Full,
        ),
    );
    let Destination::System(base) = dest(&t, "Game.pkg") else {
        panic!("{t:?}");
    };
    assert_eq!(
        base.components().next().map(|c| c.as_os_str().to_owned()),
        Some("Game.pkg".into())
    );
}

#[test]
fn dir_as_game_leaves_a_folder_game_and_an_extensionless_base() {
    // A folder game keeps the existing `folder_game_suffix` rule.
    let mut g = fixtures::game_pkg();
    g.base = fixtures::entry(&["Game", "BLUS30001"], true, EntryForm::Extracted);
    let full = target(None, FrontendMode::DirAsGame, true, TranslateScope::Full);
    assert_eq!(translate(&g, &full), master_layout(&g));

    let mut g = fixtures::game_pkg();
    g.base = fixtures::entry(&["Game", "Game"], false, EntryForm::File);
    assert_eq!(translate(&g, &full), master_layout(&g));
}

#[test]
fn every_emulator_knows_its_systems() {
    let db = SystemDb::builtin();
    for e in EmulatorId::ALL {
        assert!(!e.systems().is_empty(), "{e:?}");
        assert!(!e.display_name().is_empty());
        for s in e.systems() {
            assert!(db.get(s).is_some(), "{e:?} names unknown system {s}");
            assert!(EmulatorId::for_system(s).contains(&e), "{e:?} {s}");
        }
    }
    assert_eq!(EmulatorId::for_system("ps4"), vec![EmulatorId::Shadps4]);
    assert_eq!(
        EmulatorId::for_system("switch"),
        vec![EmulatorId::Ryujinx, EmulatorId::YuzuFamily]
    );
    assert_eq!(
        EmulatorId::for_system("gamecube"),
        vec![EmulatorId::Dolphin]
    );
    assert_eq!(
        EmulatorId::for_system("psx"),
        vec![EmulatorId::RetroArch, EmulatorId::DuckStation]
    );
    assert_eq!(
        EmulatorId::for_system("nes"),
        vec![EmulatorId::RetroArch, EmulatorId::Mesen]
    );
    assert_eq!(EmulatorId::for_system("snes"), vec![EmulatorId::RetroArch]);
    assert!(EmulatorId::for_system("xboxseries").is_empty());
    assert!(EmulatorId::for_system("unknown").is_empty());
}

#[test]
fn retroarch_covers_every_retro_system_of_the_database() {
    let db = SystemDb::builtin();
    for s in RETROARCH_SYSTEMS {
        let sys = db
            .get(s)
            .unwrap_or_else(|| panic!("{s} is not a built-in system"));
        assert!(sys.libretro.is_some(), "{s} has no libretro name");
        assert!(!MODERN_SYSTEMS.contains(s), "{s} is modern");
    }
    for s in MODERN_SYSTEMS {
        assert!(db.get(s).is_some(), "{s} is not a built-in system");
    }
    // Every built-in system with a libretro name that is not modern is a
    // RetroArch system, so a new retro system is not forgotten.
    for sys in db.all() {
        let retro = sys.libretro.is_some() && !MODERN_SYSTEMS.contains(&sys.id.as_str());
        assert_eq!(
            RETROARCH_SYSTEMS.contains(&sys.id.as_str()),
            retro,
            "{}",
            sys.id
        );
    }
    assert_eq!(EmulatorId::RetroArch.systems(), RETROARCH_SYSTEMS);
}
