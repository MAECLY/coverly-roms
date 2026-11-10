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

/// The first banned I/O, clock or environment use in `text`, if any.
/// Catches grouped and aliased imports too (`use std::{fs, path}`).
fn banned_use(text: &str) -> Option<&'static str> {
    const BANNED: &[&str] = &[
        "std::fs",
        "std::time",
        "std::env",
        "std::process",
        "std::net",
        "std::io",
        "SystemTime",
        "Instant::",
        "File::",
        "fs::",
        "env::",
        "io::",
        "time::",
        "process::",
    ];
    if let Some(b) = BANNED.iter().find(|b| text.contains(*b)) {
        return Some(b);
    }
    // `use std::{fs, …}` / `use std::{ env, … }`.
    let grouped = text
        .match_indices("std::{")
        .map(|(at, _)| &text[at..text[at..].find('}').map_or(text.len(), |end| at + end)]);
    for group in grouped {
        for name in ["fs", "env", "io", "time", "process", "net"] {
            if group
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .any(|word| word == name)
            {
                return Some(name);
            }
        }
    }
    None
}

#[test]
fn the_banned_use_check_catches_grouped_imports() {
    assert_eq!(banned_use("use std::{fs, path::Path};"), Some("fs"));
    assert_eq!(banned_use("use std::{\n    env,\n};"), Some("env"));
    assert_eq!(banned_use("use std::fs as f;"), Some("std::fs"));
    assert_eq!(banned_use("use std::{fmt, path::Path};"), None);
    assert_eq!(banned_use("use std::path::{Component, Path};"), None);
}

/// Every `.rs` file under `dir`, sub-folders included.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn translate_never_touches_the_filesystem() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("translate");
    let mut files = Vec::new();
    rust_files(&dir, &mut files);
    for path in &files {
        let text = fs::read_to_string(path).unwrap();
        assert_eq!(banned_use(&text), None, "{}", path.display());
    }
    assert!(files.len() >= 2, "mod.rs and frontend.rs at least");
}

#[test]
fn an_emulator_never_lays_out_a_system_it_does_not_run() {
    // shadPS4 does not run 3DS games: the master layout is kept.
    let g = fixtures::pokemon_y();
    for e in EmulatorId::ALL {
        if e.systems().contains(&"3ds") {
            continue;
        }
        let t = target(Some(e), FrontendMode::Plain, true, TranslateScope::Full);
        assert_eq!(translate(&g, &t), master_layout(&g), "{e:?}");
    }
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
    // The frontend pass runs after the emulator's: RPCS3 keeps the base
    // in its game folder, and that folder is named after the base file.
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

// Translators (D15): shadPS4, RPCS3, Azahar, Cemu and Vita3K. One table
// per translator; every path by component.

mod emulators {
    use super::fixtures::{addon, entry};
    use super::*;
    use coverly_core::addons::{AddonKind, HeaderFit};
    use coverly_core::translate::{NotSupportedWhy, TranslateNote};

    pub fn full(e: EmulatorId) -> TranslateTarget {
        target(Some(e), FrontendMode::Plain, false, TranslateScope::Full)
    }

    pub fn conservative(e: EmulatorId) -> TranslateTarget {
        target(
            Some(e),
            FrontendMode::Plain,
            false,
            TranslateScope::Conservative,
        )
    }

    pub fn emu(emulator: EmulatorId, parts: &[&str]) -> Destination {
        Destination::EmulatorData {
            emulator,
            path: pb(parts),
        }
    }

    pub fn tid(id: &str) -> AddonKey {
        AddonKey::TitleId { id: id.into() }
    }

    fn versioned(path: &[&str], form: EntryForm, version: &str, newest: bool) -> MasterEntry {
        MasterEntry {
            version: Some(version.into()),
            newest,
            ..entry(path, form == EntryForm::Extracted, form)
        }
    }

    /// A PS4 folder game: base `CUSA00900/`, two extracted updates
    /// (v1.09 newest), one extracted DLC, the `-mods` overlay, a cheat
    /// and an XML patch.
    pub fn bloodborne() -> MasterGame {
        let t = "Bloodborne";
        MasterGame {
            system: "ps4".into(),
            title: t.into(),
            ids: GameIds {
                title_id: Some("CUSA00900".into()),
                serial: None,
                game_id: None,
            },
            base: entry(&[t, "CUSA00900"], true, EntryForm::Extracted),
            updates: vec![
                versioned(
                    &[t, "Updates", "v1.09"],
                    EntryForm::Extracted,
                    "01.09",
                    true,
                ),
                versioned(
                    &[t, "Updates", "v1.08"],
                    EntryForm::Extracted,
                    "01.08",
                    false,
                ),
            ],
            dlc: vec![entry(&[t, "DLC", "dlc"], true, EntryForm::Extracted)],
            mods: vec![addon(
                &[t, "Mods", "CUSA00900-mods"],
                true,
                AddonFormat::Shadps4Mods,
                tid("CUSA00900"),
            )],
            cheats: vec![
                addon(
                    &[t, "Cheats", "CUSA00900_01.09.json"],
                    false,
                    AddonFormat::Shadps4Cheat,
                    AddonKey::SerialVersion {
                        serial: "CUSA00900".into(),
                        version: "01.09".into(),
                    },
                ),
                addon(
                    &[t, "Cheats", "Bloodborne.xml"],
                    false,
                    AddonFormat::Shadps4Patch,
                    tid("CUSA00900"),
                ),
            ],
            taken_in_emulator: vec![],
        }
    }

    /// The same game as packages: nothing extracted.
    pub fn bloodborne_pkg() -> MasterGame {
        let t = "Bloodborne";
        MasterGame {
            base: entry(&[t, "Bloodborne.pkg"], false, EntryForm::Package),
            updates: vec![versioned(
                &[t, "Updates", "Bloodborne (v1.09).pkg"],
                EntryForm::Package,
                "01.09",
                true,
            )],
            dlc: vec![entry(
                &[t, "DLC", "Bloodborne (The Old Hunters).pkg"],
                false,
                EntryForm::Package,
            )],
            mods: vec![],
            cheats: vec![],
            ..bloodborne()
        }
    }

    /// A PS3 game: folder base, a package update and DLC, a mod folder,
    /// a per-title patch file and an xdelta translation.
    pub fn demons_souls() -> MasterGame {
        let t = "Demon's Souls (USA)";
        MasterGame {
            system: "ps3".into(),
            title: t.into(),
            ids: GameIds {
                title_id: Some("BLUS30443".into()),
                serial: Some("BLUS-30443".into()),
                game_id: None,
            },
            base: entry(&[t, "Demon's Souls (USA)"], true, EntryForm::Extracted),
            updates: vec![versioned(
                &[t, "Updates", "BLUS30443 (v1.04).pkg"],
                EntryForm::Package,
                "01.04",
                true,
            )],
            dlc: vec![entry(
                &[t, "DLC", "BLUS30443 (Pack).pkg"],
                false,
                EntryForm::Package,
            )],
            mods: vec![
                addon(
                    &[t, "Mods", "USRDIR mod"],
                    true,
                    AddonFormat::TexturePack,
                    tid("BLUS30443"),
                ),
                addon(
                    &[t, "Mods", "eboot fix.xdelta"],
                    false,
                    AddonFormat::Xdelta,
                    AddonKey::None,
                ),
            ],
            cheats: vec![addon(
                &[t, "Cheats", "BLUS30443_patch.yml"],
                false,
                AddonFormat::Rpcs3Patch,
                tid("BLUS30443"),
            )],
            taken_in_emulator: vec![],
        }
    }

    /// A 3DS game: base, update and DLC packages, a LayeredFS mod, a
    /// texture pack and a cheat file.
    pub fn pokemon_y_full() -> MasterGame {
        let mut g = super::fixtures::pokemon_y();
        let t = g.title.clone();
        g.dlc = vec![entry(
            &[&t, "DLC", "Pokemon Y (World) (DLC).cia"],
            false,
            EntryForm::Package,
        )];
        g.mods.push(addon(
            &[&t, "Mods", "HD textures"],
            true,
            AddonFormat::TexturePack,
            tid("0004000000055D00"),
        ));
        g
    }

    /// A Wii U game: base, update and DLC packages, a graphic pack mod
    /// and a graphic pack cheat.
    pub fn botw() -> MasterGame {
        let t = "Zelda BotW (USA)";
        let mut cheat = addon(
            &[t, "Cheats", "BotW_InfiniteStamina"],
            true,
            AddonFormat::CemuGraphicPack,
            AddonKey::TitleIds {
                ids: vec!["00050000101C9400".into()],
            },
        );
        cheat.kind = AddonKind::Cheat;
        MasterGame {
            system: "wiiu".into(),
            title: t.into(),
            ids: GameIds {
                title_id: Some("00050000101C9400".into()),
                serial: None,
                game_id: None,
            },
            base: entry(&[t, "Zelda BotW (USA).wua"], false, EntryForm::Package),
            updates: vec![versioned(
                &[t, "Updates", "Zelda BotW (USA) (v208)"],
                EntryForm::Extracted,
                "208",
                true,
            )],
            dlc: vec![entry(
                &[t, "DLC", "Zelda BotW (USA) (DLC)"],
                true,
                EntryForm::Extracted,
            )],
            mods: vec![
                addon(
                    &[t, "Mods", "BotW_FPS++"],
                    true,
                    AddonFormat::CemuGraphicPack,
                    AddonKey::TitleIds {
                        ids: vec!["00050000101C9400".into()],
                    },
                ),
                addon(
                    &[t, "Mods", "Zelda (Translation).xdelta"],
                    false,
                    AddonFormat::Xdelta,
                    AddonKey::None,
                ),
            ],
            cheats: vec![cheat],
            taken_in_emulator: vec![],
        }
    }

    /// A Vita game: base, update and DLC packages, a rePatch mod and a
    /// Vita3K patch.
    pub fn persona4() -> MasterGame {
        let t = "Persona 4 Golden (USA)";
        MasterGame {
            system: "psvita".into(),
            title: t.into(),
            ids: GameIds {
                title_id: Some("PCSE00120".into()),
                serial: None,
                game_id: None,
            },
            base: entry(
                &[t, "Persona 4 Golden (USA).vpk"],
                false,
                EntryForm::Package,
            ),
            updates: vec![versioned(
                &[t, "Updates", "PCSE00120 (v1.01).pkg"],
                EntryForm::Package,
                "01.01",
                true,
            )],
            dlc: vec![entry(
                &[t, "DLC", "PCSE00120 (Costume).pkg"],
                false,
                EntryForm::Package,
            )],
            mods: vec![addon(
                &[t, "Mods", "PCSE00120"],
                true,
                AddonFormat::VitaRepatch,
                tid("PCSE00120"),
            )],
            cheats: vec![addon(
                &[t, "Cheats", "PCSE00120_60FPS.txt"],
                false,
                AddonFormat::Vita3kPatch,
                tid("PCSE00120"),
            )],
            taken_in_emulator: vec![],
        }
    }

    /// Base, update and DLC placements of `t` equal the master layout's
    /// (Conservative never moves them).
    pub fn games_as_master(g: &MasterGame, t: &TranslatedGame) {
        let master = master_layout(g);
        let paths: Vec<&PathBuf> = std::iter::once(&g.base.path)
            .chain(g.updates.iter().map(|e| &e.path))
            .chain(g.dlc.iter().map(|e| &e.path))
            .collect();
        for from in paths {
            let want = master.entries.iter().find(|p| &p.from == from).unwrap();
            let got = t.entries.iter().find(|p| &p.from == from).unwrap();
            assert_eq!(got.to, want.to, "{}", from.display());
        }
    }

    /// One placement per master entry, in the master order.
    pub fn same_entries(g: &MasterGame, t: &TranslatedGame) {
        let master = master_layout(g);
        let want: Vec<&Path> = master.entries.iter().map(|p| p.from.as_path()).collect();
        let got: Vec<&Path> = t.entries.iter().map(|p| p.from.as_path()).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn shadps4() {
        let e = EmulatorId::Shadps4;
        let g = bloodborne();
        let t = translate(&g, &full(e));
        same_entries(&g, &t);
        assert_eq!(
            dest(&t, "CUSA00900"),
            Destination::System(pb(&["CUSA00900"]))
        );
        assert_eq!(placement(&t, "CUSA00900").note, None);
        assert_eq!(
            dest(&t, "v1.09"),
            Destination::System(pb(&["CUSA00900-UPDATE"]))
        );
        assert_eq!(placement(&t, "v1.09").note, None);
        assert_eq!(placement(&t, "v1.08").to, Destination::Library);
        assert_eq!(
            placement(&t, "v1.08").note,
            Some(TranslateNote::NotSupported {
                emulator: e,
                why: NotSupportedWhy::OlderUpdate
            })
        );
        assert_eq!(
            dest(&t, "dlc"),
            Destination::System(pb(&["addcont", "CUSA00900", "dlc"]))
        );
        assert_eq!(
            placement(&t, "dlc").note,
            Some(TranslateNote::PointEmulatorHere {
                emulator: e,
                folder: pb(&["addcont"])
            })
        );
        assert_eq!(
            dest(&t, "CUSA00900-mods"),
            Destination::System(pb(&["CUSA00900-mods"]))
        );
        assert_eq!(
            dest(&t, "CUSA00900_01.09.json"),
            emu(e, &["cheats", "CUSA00900_01.09.json"])
        );
        assert_eq!(
            dest(&t, "Bloodborne.xml"),
            emu(e, &["patches", "Bloodborne.xml"])
        );

        // With game folders on, the base and its overlays stay together
        // in the game folder; DLC keeps the system-level `addcont`.
        let mut folders = full(e);
        folders.game_folders = true;
        let t = translate(&g, &folders);
        assert_eq!(
            dest(&t, "CUSA00900"),
            Destination::System(pb(&["Bloodborne", "CUSA00900"]))
        );
        assert_eq!(
            dest(&t, "v1.09"),
            Destination::System(pb(&["Bloodborne", "CUSA00900-UPDATE"]))
        );
        assert_eq!(
            dest(&t, "CUSA00900-mods"),
            Destination::System(pb(&["Bloodborne", "CUSA00900-mods"]))
        );
        assert_eq!(
            dest(&t, "dlc"),
            Destination::System(pb(&["addcont", "CUSA00900", "dlc"]))
        );

        // An update already named `-patch` keeps that name.
        let mut patch = bloodborne();
        patch.updates[0].path = pb(&["Bloodborne", "Updates", "CUSA00900-patch"]);
        let t = translate(&patch, &full(e));
        assert_eq!(
            dest(&t, "CUSA00900-patch"),
            Destination::System(pb(&["CUSA00900-patch"]))
        );

        // Packages: ExtractFirst, left in the library; never extracted.
        let g = bloodborne_pkg();
        let t = translate(&g, &full(e));
        for name in [
            "Bloodborne.pkg",
            "Bloodborne (v1.09).pkg",
            "Bloodborne (The Old Hunters).pkg",
        ] {
            assert_eq!(placement(&t, name).to, Destination::Library, "{name}");
            assert_eq!(
                placement(&t, name).note,
                Some(TranslateNote::ExtractFirst),
                "{name}"
            );
        }

        // An extracted update of a package base has no folder to sit
        // beside: the base has to be extracted first.
        let mut mixed = bloodborne();
        mixed.base = entry(&["Bloodborne", "Bloodborne.pkg"], false, EntryForm::Package);
        let t = translate(&mixed, &full(e));
        assert_eq!(placement(&t, "v1.09").to, Destination::Library);
        assert_eq!(
            placement(&t, "v1.09").note,
            Some(TranslateNote::ExtractFirst)
        );
        assert_eq!(placement(&t, "CUSA00900-mods").to, Destination::Library);
        // DLC only needs the title id.
        assert_eq!(
            dest(&t, "dlc"),
            Destination::System(pb(&["addcont", "CUSA00900", "dlc"]))
        );

        // Conservative: base, update and DLC exactly as the master; the
        // overlay is not placed; cheats and patches still are.
        let g = bloodborne();
        let t = translate(&g, &conservative(e));
        same_entries(&g, &t);
        games_as_master(&g, &t);
        assert_eq!(placement(&t, "dlc").note, None);
        assert_eq!(placement(&t, "v1.08").note, None);
        assert_eq!(placement(&t, "CUSA00900-mods").to, Destination::Library);
        assert_eq!(
            dest(&t, "CUSA00900_01.09.json"),
            emu(e, &["cheats", "CUSA00900_01.09.json"])
        );
        let t = translate(&bloodborne_pkg(), &conservative(e));
        assert_eq!(
            placement(&t, "Bloodborne.pkg").note,
            Some(TranslateNote::ExtractFirst)
        );
    }

    #[test]
    fn shadps4_without_a_valid_title_id_leaves_dlc_in_the_library() {
        let mut g = bloodborne();
        g.ids.title_id = Some("../x".into());
        let t = translate(&g, &full(EmulatorId::Shadps4));
        assert_eq!(placement(&t, "dlc").to, Destination::Library);
        assert_eq!(placement(&t, "dlc").note, None);
    }

    #[test]
    fn shadps4_places_one_overlay_only() {
        let mut g = bloodborne();
        g.mods.push(addon(
            &["Bloodborne", "Mods", "Other mod"],
            true,
            AddonFormat::Shadps4Mods,
            tid("CUSA00900"),
        ));
        let t = translate(&g, &full(EmulatorId::Shadps4));
        assert_eq!(placement(&t, "CUSA00900-mods").to, Destination::Library);
        assert_eq!(placement(&t, "Other mod").to, Destination::Library);
    }

    #[test]
    fn rpcs3() {
        let e = EmulatorId::Rpcs3;
        let g = demons_souls();
        let t = translate(&g, &full(e));
        same_entries(&g, &t);
        assert_eq!(
            dest(&t, "Demon's Souls (USA)"),
            Destination::System(pb(&["Demon's Souls (USA)"]))
        );
        assert_eq!(placement(&t, "Demon's Souls (USA)").note, None);
        for name in ["BLUS30443 (v1.04).pkg", "BLUS30443 (Pack).pkg"] {
            assert_eq!(placement(&t, name).to, Destination::Library, "{name}");
            assert_eq!(
                placement(&t, name).note,
                Some(TranslateNote::InstallInEmulator { emulator: e }),
                "{name}"
            );
        }
        assert_eq!(placement(&t, "USRDIR mod").to, Destination::Library);
        assert_eq!(
            placement(&t, "USRDIR mod").note,
            Some(TranslateNote::NotSupported {
                emulator: e,
                why: NotSupportedWhy::Rpcs3Mods
            })
        );
        assert_eq!(placement(&t, "eboot fix.xdelta").to, Destination::Library);
        assert_eq!(placement(&t, "eboot fix.xdelta").note, None);
        assert_eq!(
            dest(&t, "BLUS30443_patch.yml"),
            emu(e, &["patches", "BLUS30443_patch.yml"])
        );

        // RPCS3's own patch files are never written over.
        for own in ["patch.yml", "imported_patch.yml", "cheats.yml"] {
            let mut g = demons_souls();
            g.cheats[0].path = pb(&[&g.title.clone(), "Cheats", own]);
            let t = translate(&g, &full(e));
            assert_eq!(placement(&t, own).to, Destination::Library, "{own}");
        }

        // A patch file the emulator folder already has stays.
        let mut g = demons_souls();
        g.taken_in_emulator = vec![pb(&["patches", "BLUS30443_patch.yml"])];
        let t = translate(&g, &full(e));
        assert_eq!(
            placement(&t, "BLUS30443_patch.yml").to,
            Destination::Library
        );
        assert_eq!(
            placement(&t, "BLUS30443_patch.yml").note,
            Some(TranslateNote::NotSupported {
                emulator: e,
                why: NotSupportedWhy::ConfigExists
            })
        );

        // Game folders on: the base stays in its game folder.
        let mut folders = full(e);
        folders.game_folders = true;
        let t = translate(&demons_souls(), &folders);
        assert_eq!(
            dest(&t, "Demon's Souls (USA)"),
            Destination::System(pb(&["Demon's Souls (USA)", "Demon's Souls (USA)"]))
        );

        // Conservative: as the master, with the install note.
        let g = demons_souls();
        let t = translate(&g, &conservative(e));
        games_as_master(&g, &t);
        assert_eq!(
            placement(&t, "BLUS30443 (v1.04).pkg").note,
            Some(TranslateNote::InstallInEmulator { emulator: e })
        );
        assert_eq!(
            dest(&t, "BLUS30443_patch.yml"),
            emu(e, &["patches", "BLUS30443_patch.yml"])
        );
    }

    #[test]
    fn azahar() {
        let e = EmulatorId::Azahar;
        let g = pokemon_y_full();
        let t = translate(&g, &full(e));
        same_entries(&g, &t);
        assert_eq!(
            dest(&t, "Pokemon Y (World).cia"),
            Destination::System(pb(&["Pokemon Y (World).cia"]))
        );
        for name in [
            "Pokemon Y (World) (v1.4).cia",
            "Pokemon Y (World) (DLC).cia",
        ] {
            assert_eq!(placement(&t, name).to, Destination::Library, "{name}");
            assert_eq!(
                placement(&t, name).note,
                Some(TranslateNote::InstallInEmulator { emulator: e }),
                "{name}"
            );
        }
        assert_eq!(
            dest(&t, "0004000000055D00"),
            emu(e, &["load", "mods", "0004000000055D00"])
        );
        assert_eq!(
            dest(&t, "HD textures"),
            emu(e, &["load", "textures", "0004000000055D00"])
        );
        assert_eq!(
            dest(&t, "0004000000055D00.txt"),
            emu(e, &["cheats", "0004000000055D00.txt"])
        );

        // A mod keyed by nothing uses the game's title id; a crafted one
        // is never a path.
        let mut g = pokemon_y_full();
        g.mods[0].key = AddonKey::None;
        let t = translate(&g, &full(e));
        assert_eq!(
            dest(&t, "0004000000055D00"),
            emu(e, &["load", "mods", "0004000000055D00"])
        );
        let mut g = pokemon_y_full();
        g.mods[0].key = tid("../../x");
        g.ids.title_id = None;
        let t = translate(&g, &full(e));
        assert_eq!(placement(&t, "0004000000055D00").to, Destination::Library);
        assert_eq!(placement(&t, "0004000000055D00").note, None);

        // Conservative: games as the master, addons still placed.
        let g = pokemon_y_full();
        let t = translate(&g, &conservative(e));
        games_as_master(&g, &t);
        assert_eq!(
            placement(&t, "Pokemon Y (World) (v1.4).cia").note,
            Some(TranslateNote::InstallInEmulator { emulator: e })
        );
        assert_eq!(
            dest(&t, "HD textures"),
            emu(e, &["load", "textures", "0004000000055D00"])
        );
    }

    #[test]
    fn cemu() {
        let e = EmulatorId::Cemu;
        let g = botw();
        let t = translate(&g, &full(e));
        same_entries(&g, &t);
        assert_eq!(
            dest(&t, "Zelda BotW (USA).wua"),
            Destination::System(pb(&["Zelda BotW (USA).wua"]))
        );
        for name in ["Zelda BotW (USA) (v208)", "Zelda BotW (USA) (DLC)"] {
            assert_eq!(placement(&t, name).to, Destination::Library, "{name}");
            assert_eq!(
                placement(&t, name).note,
                Some(TranslateNote::InstallInEmulator { emulator: e }),
                "{name}"
            );
        }
        assert_eq!(
            dest(&t, "BotW_FPS++"),
            emu(e, &["graphicPacks", "BotW_FPS++"])
        );
        assert_eq!(
            dest(&t, "BotW_InfiniteStamina"),
            emu(e, &["graphicPacks", "BotW_InfiniteStamina"])
        );
        assert_eq!(
            placement(&t, "Zelda (Translation).xdelta").to,
            Destination::Library
        );

        // A graphic pack that is a file (a zip) is not a pack folder.
        let mut g = botw();
        g.mods[0].is_dir = false;
        let t = translate(&g, &full(e));
        assert_eq!(placement(&t, "BotW_FPS++").to, Destination::Library);
    }

    #[test]
    fn vita3k() {
        let e = EmulatorId::Vita3k;
        let g = persona4();
        let t = translate(&g, &full(e));
        same_entries(&g, &t);
        assert_eq!(
            dest(&t, "Persona 4 Golden (USA).vpk"),
            Destination::System(pb(&["Persona 4 Golden (USA).vpk"]))
        );
        for name in ["PCSE00120 (v1.01).pkg", "PCSE00120 (Costume).pkg"] {
            assert_eq!(placement(&t, name).to, Destination::Library, "{name}");
            assert_eq!(
                placement(&t, name).note,
                Some(TranslateNote::InstallInEmulator { emulator: e }),
                "{name}"
            );
        }
        assert_eq!(placement(&t, "PCSE00120").to, Destination::Library);
        assert_eq!(
            placement(&t, "PCSE00120").note,
            Some(TranslateNote::NotSupported {
                emulator: e,
                why: NotSupportedWhy::VitaRepatch
            })
        );
        assert_eq!(
            dest(&t, "PCSE00120_60FPS.txt"),
            emu(e, &["patch", "PCSE00120_60FPS.txt"])
        );

        // The global patch list is never written over.
        let mut g = persona4();
        g.cheats[0].path = pb(&[&g.title.clone(), "Cheats", "patchlist.txt"]);
        let t = translate(&g, &full(e));
        assert_eq!(placement(&t, "patchlist.txt").to, Destination::Library);
    }

    #[test]
    fn two_addons_never_share_one_place() {
        // Two LayeredFS mods and two cheat files for one title would land
        // on the same `load/mods/<TitleID>` and `cheats/<TitleID>.txt`:
        // nothing is written over, both stay for review.
        let e = EmulatorId::Azahar;
        let mut g = pokemon_y_full();
        let t = g.title.clone();
        g.mods.push(addon(
            &[&t, "Mods", "Other mod"],
            true,
            AddonFormat::LayeredFs,
            tid("0004000000055D00"),
        ));
        g.cheats.push(addon(
            &[&t, "Cheats", "More cheats.txt"],
            false,
            AddonFormat::CtrCheats,
            tid("0004000000055D00"),
        ));
        let tr = translate(&g, &full(e));
        same_entries(&g, &tr);
        for name in [
            "0004000000055D00",
            "Other mod",
            "0004000000055D00.txt",
            "More cheats.txt",
        ] {
            assert_eq!(placement(&tr, name).to, Destination::Library, "{name}");
        }
        // The texture pack has its own place and keeps it.
        assert_eq!(
            dest(&tr, "HD textures"),
            emu(e, &["load", "textures", "0004000000055D00"])
        );
        let tr = translate(&g, &conservative(e));
        assert_eq!(placement(&tr, "Other mod").to, Destination::Library);

        // Cemu: a pack name that differs only in case is the same folder
        // on a case-insensitive disk.
        let mut g = botw();
        g.cheats[0].path = pb(&["Zelda BotW (USA)", "Cheats", "botw_fps++"]);
        let tr = translate(&g, &full(EmulatorId::Cemu));
        assert_eq!(placement(&tr, "BotW_FPS++").to, Destination::Library);
        assert_eq!(placement(&tr, "botw_fps++").to, Destination::Library);
    }

    #[test]
    fn an_emulator_file_name_carries_the_id_as_the_emulator_reads_it() {
        // Vita3K loads `patch/*.txt` whose name contains the TITLE_ID, as
        // written; RPCS3 loads `*_patch.yml`; shadPS4 `SERIAL_VERSION.json`.
        let mut g = persona4();
        g.cheats[0].path = pb(&[&g.title.clone(), "Cheats", "pcse00120_60fps.TXT"]);
        let t = translate(&g, &full(EmulatorId::Vita3k));
        assert_eq!(
            dest(&t, "pcse00120_60fps.TXT"),
            emu(EmulatorId::Vita3k, &["patch", "PCSE00120_60fps.txt"])
        );
        let mut g = demons_souls();
        g.cheats[0].path = pb(&[&g.title.clone(), "Cheats", "blus30443_PATCH.YML"]);
        let t = translate(&g, &full(EmulatorId::Rpcs3));
        assert_eq!(
            dest(&t, "blus30443_PATCH.YML"),
            emu(EmulatorId::Rpcs3, &["patches", "BLUS30443_patch.yml"])
        );
        let mut g = bloodborne();
        g.cheats[0].path = pb(&["Bloodborne", "Cheats", "cusa00900_01.09.JSON"]);
        let t = translate(&g, &full(EmulatorId::Shadps4));
        assert_eq!(
            dest(&t, "cusa00900_01.09.JSON"),
            emu(EmulatorId::Shadps4, &["cheats", "CUSA00900_01.09.json"])
        );
        // A Vita3K patch whose name does not start with its id stays.
        let mut g = persona4();
        g.cheats[0].path = pb(&[&g.title.clone(), "Cheats", "60fps.txt"]);
        let t = translate(&g, &full(EmulatorId::Vita3k));
        assert_eq!(placement(&t, "60fps.txt").to, Destination::Library);
    }

    #[test]
    fn a_header_note_is_never_invented_by_these_translators() {
        // Header fit is a retro-ROM concern (RetroArch, Task 16).
        let mut g = demons_souls();
        g.mods[1].header = Some(HeaderFit::NeedsHeaderless);
        let t = translate(&g, &full(EmulatorId::Rpcs3));
        assert_eq!(placement(&t, "eboot fix.xdelta").note, None);
    }

    #[test]
    fn crafted_names_never_become_emulator_paths() {
        // A cheat whose name is not one safe component stays.
        let mut g = bloodborne();
        g.cheats[0].path = pb(&["Bloodborne", "Cheats", "CON"]);
        let t = translate(&g, &full(EmulatorId::Shadps4));
        assert_eq!(placement(&t, "CON").to, Destination::Library);
    }
}

// Translators (D15): Switch (Ryujinx, yuzu family), RetroArch and the
// disc-era emulators. Same helpers, every path by component.

mod more_emulators {
    use super::emulators::{conservative, emu, full, games_as_master, same_entries, tid};
    use super::fixtures::{addon, entry};
    use super::*;
    use coverly_core::addons::HeaderFit;
    use coverly_core::translate::{NotSupportedWhy, TranslateNote};

    fn serial(s: &str) -> AddonKey {
        AddonKey::Serial { serial: s.into() }
    }

    fn game_id(s: &str) -> AddonKey {
        AddonKey::GameId { id: s.into() }
    }

    fn rom_name(s: &str) -> AddonKey {
        AddonKey::RomName { name: s.into() }
    }

    fn game(system: &str, title: &str, ids: GameIds, base: MasterEntry) -> MasterGame {
        MasterGame {
            system: system.into(),
            title: title.into(),
            ids,
            base,
            updates: vec![],
            dlc: vec![],
            mods: vec![],
            cheats: vec![],
            taken_in_emulator: vec![],
        }
    }

    const ZELDA_ID: &str = "01007EF00011E000";
    const BUILD_ID: &str = "A1B2C3D4E5F60718";

    /// A Switch game: base, update and DLC files, a bare title id mod, a
    /// user-named mod folder, a cheat file and an exefs patch set.
    fn zelda() -> MasterGame {
        let t = "Zelda";
        let mut g = game(
            "switch",
            t,
            GameIds {
                title_id: Some(ZELDA_ID.into()),
                serial: None,
                game_id: None,
            },
            entry(&[t, "Zelda.nsp"], false, EntryForm::Package),
        );
        g.updates = vec![entry(
            &[t, "Updates", "Zelda (v131072).nsp"],
            false,
            EntryForm::Package,
        )];
        g.dlc = vec![entry(
            &[t, "DLC", "Zelda (Expansion Pass).nsp"],
            false,
            EntryForm::Package,
        )];
        g.mods = vec![
            addon(
                &[t, "Mods", ZELDA_ID],
                true,
                AddonFormat::SwitchMod,
                tid(ZELDA_ID),
            ),
            addon(
                &[t, "Mods", "Zelda 60fps", ZELDA_ID],
                true,
                AddonFormat::SwitchMod,
                tid(ZELDA_ID),
            ),
            addon(
                &[t, "Mods", "Better HUD"],
                true,
                AddonFormat::SwitchMod,
                tid(ZELDA_ID),
            ),
            addon(
                &[t, "Mods", "Res patch"],
                true,
                AddonFormat::ExefsPatches,
                AddonKey::BuildId {
                    build_id: BUILD_ID.into(),
                    title_id: None,
                },
            ),
        ];
        g.cheats = vec![addon(
            &[t, "Cheats", "a1b2c3d4e5f60718.txt"],
            false,
            AddonFormat::SwitchCheats,
            AddonKey::BuildId {
                build_id: BUILD_ID.into(),
                title_id: Some(ZELDA_ID.into()),
            },
        )];
        g
    }

    #[test]
    fn switch_ryujinx_and_yuzu_family() {
        let g = zelda();
        let low = ZELDA_ID.to_ascii_lowercase();
        for e in [EmulatorId::Ryujinx, EmulatorId::YuzuFamily] {
            let t = translate(&g, &full(e));
            same_entries(&g, &t);
            assert_eq!(
                dest(&t, "Zelda.nsp"),
                Destination::System(pb(&["Zelda.nsp"]))
            );
            assert_eq!(placement(&t, "Zelda.nsp").note, None);
            assert_eq!(
                dest(&t, "Zelda (v131072).nsp"),
                Destination::System(pb(&["Updates", "Zelda (v131072).nsp"]))
            );
            assert_eq!(
                placement(&t, "Zelda (v131072).nsp").note,
                Some(TranslateNote::PointEmulatorHere {
                    emulator: e,
                    folder: pb(&["Updates"])
                })
            );
            assert_eq!(
                dest(&t, "Zelda (Expansion Pass).nsp"),
                Destination::System(pb(&["DLC", "Zelda (Expansion Pass).nsp"]))
            );
            assert_eq!(
                placement(&t, "Zelda (Expansion Pass).nsp").note,
                Some(TranslateNote::PointEmulatorHere {
                    emulator: e,
                    folder: pb(&["DLC"])
                })
            );
        }

        let t = translate(&g, &full(EmulatorId::Ryujinx));
        let r = EmulatorId::Ryujinx;
        // The bare id folder becomes the `Coverly mod` level.
        assert_eq!(
            t.entries[3].to,
            emu(r, &["mods", "contents", &low, "Coverly mod"])
        );
        // A user-named folder above the id keeps its name as the level.
        assert_eq!(
            t.entries[4].to,
            emu(r, &["mods", "contents", &low, "Zelda 60fps"])
        );
        assert_eq!(
            dest(&t, "Better HUD"),
            emu(r, &["mods", "contents", &low, "Better HUD"])
        );
        assert_eq!(
            dest(&t, "Res patch"),
            emu(r, &["mods", "exefs_patches", "Res patch"])
        );
        assert_eq!(
            dest(&t, "a1b2c3d4e5f60718.txt"),
            emu(
                r,
                &["mods", "contents", &low, "cheats", "A1B2C3D4E5F60718.txt"]
            )
        );

        let y = EmulatorId::YuzuFamily;
        let t = translate(&g, &full(y));
        assert_eq!(t.entries[3].to, emu(y, &["load", ZELDA_ID, "Coverly mod"]));
        assert_eq!(t.entries[4].to, emu(y, &["load", ZELDA_ID, "Zelda 60fps"]));
        assert_eq!(
            dest(&t, "Res patch"),
            emu(y, &["load", ZELDA_ID, "Res patch", "exefs"])
        );
        // yuzu reads cheats one mod level down.
        assert_eq!(
            dest(&t, "a1b2c3d4e5f60718.txt"),
            emu(
                y,
                &[
                    "load",
                    ZELDA_ID,
                    "Coverly cheats",
                    "cheats",
                    "A1B2C3D4E5F60718.txt"
                ]
            )
        );

        // Conservative (Q8, Q11): base, update and DLC as the master,
        // mods and cheats still placed in the emulator's folder.
        for e in [EmulatorId::Ryujinx, EmulatorId::YuzuFamily] {
            let t = translate(&g, &conservative(e));
            same_entries(&g, &t);
            games_as_master(&g, &t);
            assert_eq!(placement(&t, "Zelda (v131072).nsp").note, None);
            assert!(matches!(
                dest(&t, "a1b2c3d4e5f60718.txt"),
                Destination::EmulatorData { .. }
            ));
            assert!(matches!(
                dest(&t, "Better HUD"),
                Destination::EmulatorData { .. }
            ));
        }
    }

    #[test]
    fn switch_never_builds_a_path_from_a_bad_id() {
        let mut g = zelda();
        g.ids.title_id = None;
        g.mods[2].key = tid("../../x");
        g.cheats[0].key = AddonKey::BuildId {
            build_id: "../x".into(),
            title_id: Some(ZELDA_ID.into()),
        };
        let t = translate(&g, &full(EmulatorId::Ryujinx));
        assert_eq!(placement(&t, "Better HUD").to, Destination::Library);
        assert_eq!(
            placement(&t, "a1b2c3d4e5f60718.txt").to,
            Destination::Library
        );
        // yuzu needs the game's title id for an exefs set.
        let t = translate(&g, &full(EmulatorId::YuzuFamily));
        assert_eq!(placement(&t, "Res patch").to, Destination::Library);
    }

    /// A SNES game for RetroArch: one BPS translation, a cheat and an
    /// MSU-1 set.
    fn chrono() -> MasterGame {
        let t = "Chrono Trigger (USA)";
        let mut g = game(
            "snes",
            t,
            GameIds::default(),
            entry(&[t, "Chrono Trigger (USA).sfc"], false, EntryForm::Image),
        );
        let mut bps = addon(
            &[t, "Mods", "Chrono Trigger Retranslation.bps"],
            false,
            AddonFormat::Bps,
            AddonKey::SourceCrc {
                crc32: 0x2D20_6BF7,
                size: 4_194_304,
            },
        );
        bps.header = Some(HeaderFit::Matches);
        g.mods = vec![
            bps,
            addon(
                &[t, "Mods", "ct.msu"],
                false,
                AddonFormat::Msu1,
                rom_name("ct"),
            ),
            addon(
                &[t, "Mods", "ct-1.pcm"],
                false,
                AddonFormat::Msu1,
                rom_name("ct"),
            ),
            addon(
                &[t, "Mods", "ct-12.pcm"],
                false,
                AddonFormat::Msu1,
                rom_name("ct"),
            ),
            addon(
                &[t, "Mods", "HD"],
                true,
                AddonFormat::TexturePack,
                AddonKey::None,
            ),
        ];
        g.cheats = vec![addon(
            &[t, "Cheats", "Chrono Trigger (USA).cht"],
            false,
            AddonFormat::RetroArchCht,
            rom_name("Chrono Trigger (USA)"),
        )];
        g
    }

    #[test]
    fn retroarch() {
        let e = EmulatorId::RetroArch;
        let g = chrono();
        let stem = "Chrono Trigger (USA)";
        let t = translate(&g, &full(e));
        same_entries(&g, &t);
        assert_eq!(
            dest(&t, "Chrono Trigger (USA).sfc"),
            Destination::System(pb(&["Chrono Trigger (USA).sfc"]))
        );
        assert_eq!(
            dest(&t, "Chrono Trigger Retranslation.bps"),
            Destination::System(pb(&["Chrono Trigger (USA).bps"]))
        );
        assert_eq!(placement(&t, "Chrono Trigger Retranslation.bps").note, None);
        assert_eq!(
            dest(&t, "Chrono Trigger (USA).cht"),
            emu(
                e,
                &[
                    "cheats",
                    "Nintendo - Super Nintendo Entertainment System",
                    "Chrono Trigger (USA).cht"
                ]
            )
        );
        // MSU-1: every piece renamed to the ROM stem, beside the ROM.
        assert_eq!(
            dest(&t, "ct.msu"),
            Destination::System(pb(&[&format!("{stem}.msu")]))
        );
        assert_eq!(
            dest(&t, "ct-1.pcm"),
            Destination::System(pb(&[&format!("{stem}-1.pcm")]))
        );
        assert_eq!(
            dest(&t, "ct-12.pcm"),
            Destination::System(pb(&[&format!("{stem}-12.pcm")]))
        );
        // Other mods stay.
        assert_eq!(placement(&t, "HD").to, Destination::Library);
        assert_eq!(placement(&t, "HD").note, None);

        // With game folders on, beside the ROM in its folder.
        let mut folders = full(e);
        folders.game_folders = true;
        let t = translate(&g, &folders);
        assert_eq!(
            dest(&t, "Chrono Trigger Retranslation.bps"),
            Destination::System(pb(&[stem, "Chrono Trigger (USA).bps"]))
        );

        // Conservative: never renamed, the patch says why.
        let t = translate(&g, &conservative(e));
        same_entries(&g, &t);
        games_as_master(&g, &t);
        assert_eq!(
            placement(&t, "Chrono Trigger Retranslation.bps").to,
            Destination::Library
        );
        assert_eq!(
            placement(&t, "Chrono Trigger Retranslation.bps").note,
            Some(TranslateNote::PatchNotApplied { emulator: e })
        );
        assert_eq!(placement(&t, "ct.msu").to, Destination::Library);
        assert_eq!(placement(&t, "ct.msu").note, None);
        assert!(matches!(
            dest(&t, "Chrono Trigger (USA).cht"),
            Destination::EmulatorData { .. }
        ));
    }

    #[test]
    fn retroarch_header_mismatch_and_several_patches() {
        let e = EmulatorId::RetroArch;
        let mut g = chrono();
        g.mods[0].header = Some(HeaderFit::NeedsHeaderless);
        let t = translate(&g, &full(e));
        assert_eq!(
            placement(&t, "Chrono Trigger Retranslation.bps").to,
            Destination::Library
        );
        assert_eq!(
            placement(&t, "Chrono Trigger Retranslation.bps").note,
            Some(TranslateNote::HeaderMismatch {
                needs: HeaderFit::NeedsHeaderless
            })
        );
        // The mismatch note survives the conservative scope.
        let t = translate(&g, &conservative(e));
        assert_eq!(
            placement(&t, "Chrono Trigger Retranslation.bps").note,
            Some(TranslateNote::HeaderMismatch {
                needs: HeaderFit::NeedsHeaderless
            })
        );

        // No fit to check (IPS) places it.
        let mut g = chrono();
        g.mods[0] = addon(
            &[&g.title.clone(), "Mods", "Hack.ips"],
            false,
            AddonFormat::Ips,
            AddonKey::None,
        );
        let t = translate(&g, &full(e));
        assert_eq!(
            dest(&t, "Hack.ips"),
            Destination::System(pb(&["Chrono Trigger (USA).ips"]))
        );

        // Two patches for one ROM (Q6): neither is renamed.
        let mut g = chrono();
        let title = g.title.clone();
        g.mods.push(addon(
            &[&title, "Mods", "Other.ups"],
            false,
            AddonFormat::Ups,
            AddonKey::None,
        ));
        let t = translate(&g, &full(e));
        for name in ["Chrono Trigger Retranslation.bps", "Other.ups"] {
            assert_eq!(placement(&t, name).to, Destination::Library, "{name}");
            assert_eq!(
                placement(&t, name).note,
                Some(TranslateNote::NotSupported {
                    emulator: e,
                    why: NotSupportedWhy::SeveralPatches
                }),
                "{name}"
            );
        }

        // An IPS chain keeps its order by its own extensions.
        let mut g = chrono();
        g.mods.remove(0);
        for name in ["Fix.ips1", "Base.ips", "More.IPS2"] {
            g.mods.push(addon(
                &[&title, "Mods", name],
                false,
                AddonFormat::Ips,
                AddonKey::None,
            ));
        }
        let t = translate(&g, &full(e));
        assert_eq!(
            dest(&t, "Base.ips"),
            Destination::System(pb(&["Chrono Trigger (USA).ips"]))
        );
        assert_eq!(
            dest(&t, "Fix.ips1"),
            Destination::System(pb(&["Chrono Trigger (USA).ips1"]))
        );
        assert_eq!(
            dest(&t, "More.IPS2"),
            Destination::System(pb(&["Chrono Trigger (USA).ips2"]))
        );
        // A chain with a gap is several patches.
        let mut g = chrono();
        g.mods.remove(0);
        for name in ["Base.ips", "More.ips2"] {
            g.mods.push(addon(
                &[&title, "Mods", name],
                false,
                AddonFormat::Ips,
                AddonKey::None,
            ));
        }
        let t = translate(&g, &full(e));
        assert_eq!(placement(&t, "Base.ips").to, Destination::Library);
        assert_eq!(placement(&t, "More.ips2").to, Destination::Library);
    }

    #[test]
    fn a_lone_numbered_ips_is_named_as_the_first_of_a_chain() {
        // RetroArch only tries `.ips1` after `.ips`: a lone `.ips1` beside
        // the ROM would never be applied.
        let e = EmulatorId::RetroArch;
        let mut g = chrono();
        let title = g.title.clone();
        g.mods[0] = addon(
            &[&title, "Mods", "Hack.ips1"],
            false,
            AddonFormat::Ips,
            AddonKey::None,
        );
        let t = translate(&g, &full(e));
        assert_eq!(
            dest(&t, "Hack.ips1"),
            Destination::System(pb(&["Chrono Trigger (USA).ips"]))
        );
    }

    #[test]
    fn yuzu_places_an_exefs_set_by_its_own_title_id() {
        let mut g = zelda();
        g.ids.title_id = None;
        g.mods[3].key = AddonKey::BuildId {
            build_id: BUILD_ID.into(),
            title_id: Some(ZELDA_ID.to_ascii_lowercase()),
        };
        let y = EmulatorId::YuzuFamily;
        let t = translate(&g, &full(y));
        assert_eq!(
            dest(&t, "Res patch"),
            emu(y, &["load", ZELDA_ID, "Res patch", "exefs"])
        );
    }

    #[test]
    fn ppsspp_takes_the_disc_id_from_a_serial() {
        let e = EmulatorId::Ppsspp;
        let t0 = "Monster Hunter Freedom Unite (USA)";
        let mut g = game(
            "psp",
            t0,
            GameIds {
                title_id: None,
                serial: Some("ULUS-10391".into()),
                game_id: None,
            },
            entry(
                &[t0, "Monster Hunter Freedom Unite (USA).iso"],
                false,
                EntryForm::Image,
            ),
        );
        g.mods = vec![
            addon(
                &[t0, "Mods", "HD pack"],
                true,
                AddonFormat::TexturePack,
                AddonKey::None,
            ),
            addon(
                &[t0, "Mods", "Other pack"],
                true,
                AddonFormat::TexturePack,
                serial("ULUS_103.91"),
            ),
        ];
        let other = g.mods.pop().expect("two packs");
        let t = translate(&g, &full(e));
        assert_eq!(
            dest(&t, "HD pack"),
            emu(e, &["PSP", "TEXTURES", "ULUS10391"])
        );
        // The addon's own serial key gives the same id: two packs never
        // share one place, so both stay.
        g.mods.push(other);
        let t = translate(&g, &full(e));
        assert_eq!(placement(&t, "HD pack").to, Destination::Library);
        assert_eq!(placement(&t, "Other pack").to, Destination::Library);
        // An addon's own key that is not an id is never replaced.
        g.mods.truncate(1);
        g.mods[0].key = game_id("../x");
        let t = translate(&g, &full(e));
        assert_eq!(placement(&t, "HD pack").to, Destination::Library);
    }

    #[test]
    fn retroarch_leaves_formats_it_does_not_read() {
        let e = EmulatorId::RetroArch;
        let mut g = chrono();
        let title = g.title.clone();
        g.mods = vec![addon(
            &[&title, "Mods", "Fix.ppf"],
            false,
            AddonFormat::Ppf,
            AddonKey::None,
        )];
        g.cheats = vec![addon(
            &[&title, "Cheats", "Chrono Trigger (USA).cht"],
            false,
            AddonFormat::FceuxCht,
            rom_name("Chrono Trigger (USA)"),
        )];
        let t = translate(&g, &full(e));
        assert_eq!(placement(&t, "Fix.ppf").to, Destination::Library);
        assert_eq!(
            placement(&t, "Chrono Trigger (USA).cht").to,
            Destination::Library
        );
    }

    /// A PS1 game for DuckStation: a texture pack, a cheat and a PPF.
    fn mgs() -> MasterGame {
        let t = "Metal Gear Solid (USA) (Disc 1)";
        let mut g = game(
            "psx",
            t,
            GameIds {
                title_id: None,
                serial: Some("SLUS-00594".into()),
                game_id: None,
            },
            entry(
                &[t, "Metal Gear Solid (USA) (Disc 1).cue"],
                false,
                EntryForm::Image,
            ),
        );
        g.mods = vec![
            addon(
                &[t, "Mods", "SLUS-00594"],
                true,
                AddonFormat::TexturePack,
                serial("SLUS-00594"),
            ),
            addon(
                &[t, "Mods", "Undub.ppf"],
                false,
                AddonFormat::Ppf,
                AddonKey::None,
            ),
        ];
        g.cheats = vec![addon(
            &[t, "Cheats", "SLUS-00594.cht"],
            false,
            AddonFormat::DuckStationCht,
            serial("SLUS-00594"),
        )];
        g
    }

    #[test]
    fn duckstation() {
        let e = EmulatorId::DuckStation;
        let g = mgs();
        let t = translate(&g, &full(e));
        same_entries(&g, &t);
        assert_eq!(dest(&t, "SLUS-00594"), emu(e, &["textures", "SLUS-00594"]));
        assert_eq!(
            dest(&t, "SLUS-00594.cht"),
            emu(e, &["cheats", "SLUS-00594.cht"])
        );
        assert_eq!(
            dest(&t, "Undub.ppf"),
            Destination::System(pb(&["Metal Gear Solid (USA) (Disc 1).ppf"]))
        );
        // A cheat named by another spelling of the serial is named as
        // DuckStation looks it up.
        let mut g = mgs();
        g.cheats[0].path = pb(&[&g.title.clone(), "Cheats", "slus_005.94.cht"]);
        let t = translate(&g, &full(e));
        assert_eq!(
            dest(&t, "slus_005.94.cht"),
            emu(e, &["cheats", "SLUS-00594.cht"])
        );
        // Conservative: the PPF is not renamed.
        let t = translate(&mgs(), &conservative(e));
        assert_eq!(placement(&t, "Undub.ppf").to, Destination::Library);
        assert_eq!(
            placement(&t, "Undub.ppf").note,
            Some(TranslateNote::PatchNotApplied { emulator: e })
        );
        assert_eq!(dest(&t, "SLUS-00594"), emu(e, &["textures", "SLUS-00594"]));
    }

    #[test]
    fn pcsx2() {
        let e = EmulatorId::Pcsx2;
        let t0 = "Kingdom Hearts (USA)";
        let mut g = game(
            "ps2",
            t0,
            GameIds {
                title_id: None,
                serial: Some("SLUS-20370".into()),
                game_id: None,
            },
            entry(&[t0, "Kingdom Hearts (USA).iso"], false, EntryForm::Image),
        );
        g.mods = vec![addon(
            &[t0, "Mods", "HD pack"],
            true,
            AddonFormat::TexturePack,
            AddonKey::None,
        )];
        g.cheats = vec![addon(
            &[t0, "Cheats", "SLUS-20370_AFBA4BB6.pnach"],
            false,
            AddonFormat::Pnach,
            serial("SLUS-20370"),
        )];
        let t = translate(&g, &full(e));
        same_entries(&g, &t);
        assert_eq!(dest(&t, "HD pack"), emu(e, &["textures", "SLUS-20370"]));
        assert_eq!(
            dest(&t, "SLUS-20370_AFBA4BB6.pnach"),
            emu(e, &["cheats", "SLUS-20370_AFBA4BB6.pnach"])
        );
        // No serial anywhere: the pack stays.
        g.ids.serial = None;
        let t = translate(&g, &full(e));
        assert_eq!(placement(&t, "HD pack").to, Destination::Library);
    }

    fn mario_kart() -> MasterGame {
        let t0 = "Mario Kart Wii (USA)";
        let mut g = game(
            "wii",
            t0,
            GameIds {
                title_id: None,
                serial: None,
                game_id: Some("RMCE01".into()),
            },
            entry(&[t0, "Mario Kart Wii (USA).rvz"], false, EntryForm::Image),
        );
        g.mods = vec![
            addon(
                &[t0, "Mods", "RMCE01"],
                true,
                AddonFormat::TexturePack,
                game_id("RMCE01"),
            ),
            addon(
                &[t0, "Mods", "CTGP"],
                true,
                AddonFormat::Riivolution,
                game_id("RMC"),
            ),
        ];
        g.cheats = vec![addon(
            &[t0, "Cheats", "RMCE01.ini"],
            false,
            AddonFormat::DolphinIni,
            game_id("RMCE01"),
        )];
        g
    }

    #[test]
    fn dolphin() {
        let e = EmulatorId::Dolphin;
        let g = mario_kart();
        let t = translate(&g, &full(e));
        same_entries(&g, &t);
        assert_eq!(dest(&t, "RMCE01"), emu(e, &["Load", "Textures", "RMCE01"]));
        assert_eq!(dest(&t, "CTGP"), emu(e, &["Load", "Riivolution"]));
        assert_eq!(
            dest(&t, "RMCE01.ini"),
            emu(e, &["GameSettings", "RMCE01.ini"])
        );
        // Q5: an existing settings file is never merged into.
        let mut g = mario_kart();
        g.taken_in_emulator = vec![pb(&["GameSettings", "rmce01.ini"])];
        let t = translate(&g, &full(e));
        assert_eq!(placement(&t, "RMCE01.ini").to, Destination::Library);
        assert_eq!(
            placement(&t, "RMCE01.ini").note,
            Some(TranslateNote::NotSupported {
                emulator: e,
                why: NotSupportedWhy::ConfigExists
            })
        );
        // GameCube too.
        let mut g = mario_kart();
        g.system = "gamecube".into();
        assert_eq!(
            dest(&translate(&g, &full(e)), "RMCE01"),
            emu(e, &["Load", "Textures", "RMCE01"])
        );
    }

    #[test]
    fn ppsspp() {
        let e = EmulatorId::Ppsspp;
        let t0 = "Monster Hunter Freedom Unite (USA)";
        let mut g = game(
            "psp",
            t0,
            GameIds {
                title_id: None,
                serial: None,
                game_id: Some("ULUS10391".into()),
            },
            entry(
                &[t0, "Monster Hunter Freedom Unite (USA).iso"],
                false,
                EntryForm::Image,
            ),
        );
        g.mods = vec![addon(
            &[t0, "Mods", "ULUS10391"],
            true,
            AddonFormat::TexturePack,
            game_id("ULUS10391"),
        )];
        g.cheats = vec![addon(
            &[t0, "Cheats", "ULUS10391.ini"],
            false,
            AddonFormat::PpssppIni,
            game_id("ULUS10391"),
        )];
        let t = translate(&g, &full(e));
        same_entries(&g, &t);
        assert_eq!(
            dest(&t, "ULUS10391"),
            emu(e, &["PSP", "TEXTURES", "ULUS10391"])
        );
        assert_eq!(
            dest(&t, "ULUS10391.ini"),
            emu(e, &["PSP", "Cheats", "ULUS10391.ini"])
        );
        g.taken_in_emulator = vec![pb(&["PSP", "Cheats", "ULUS10391.ini"])];
        let t = translate(&g, &full(e));
        assert_eq!(placement(&t, "ULUS10391.ini").to, Destination::Library);
    }

    #[test]
    fn xenia_flycast_and_mesen() {
        let e = EmulatorId::Xenia;
        let t0 = "Blue Dragon (USA)";
        let mut g = game(
            "xbox360",
            t0,
            GameIds {
                title_id: Some("4D5307DF".into()),
                serial: None,
                game_id: None,
            },
            entry(&[t0, "Blue Dragon (USA).iso"], false, EntryForm::Image),
        );
        g.mods = vec![addon(
            &[t0, "Mods", "4D5307DF"],
            true,
            AddonFormat::XeniaPlugin,
            tid("4D5307DF"),
        )];
        g.cheats = vec![addon(
            &[t0, "Cheats", "4D5307DF - Blue Dragon.patch.toml"],
            false,
            AddonFormat::XeniaPatch,
            tid("4D5307DF"),
        )];
        let t = translate(&g, &full(e));
        same_entries(&g, &t);
        assert_eq!(dest(&t, "4D5307DF"), emu(e, &["plugins", "4D5307DF"]));
        assert_eq!(
            dest(&t, "4D5307DF - Blue Dragon.patch.toml"),
            emu(e, &["patches", "4D5307DF - Blue Dragon.patch.toml"])
        );
        // A crafted plugin id is never a path.
        g.mods[0].key = tid("../../x");
        let t = translate(&g, &full(e));
        assert_eq!(placement(&t, "4D5307DF").to, Destination::Library);

        let e = EmulatorId::Flycast;
        let t0 = "Shenmue (USA) (Disc 1)";
        let mut g = game(
            "dreamcast",
            t0,
            GameIds {
                title_id: None,
                serial: None,
                game_id: Some("51059N".into()),
            },
            entry(&[t0, "Shenmue (USA) (Disc 1).gdi"], false, EntryForm::Image),
        );
        g.mods = vec![addon(
            &[t0, "Mods", "51059N"],
            true,
            AddonFormat::TexturePack,
            game_id("51059N"),
        )];
        let t = translate(&g, &full(e));
        assert_eq!(dest(&t, "51059N"), emu(e, &["textures", "51059N"]));

        let e = EmulatorId::Mesen;
        let t0 = "Super Mario Bros. (World)";
        let mut g = game(
            "nes",
            t0,
            GameIds::default(),
            entry(
                &[t0, "Super Mario Bros. (World).nes"],
                false,
                EntryForm::Image,
            ),
        );
        g.mods = vec![addon(
            &[t0, "Mods", "SMB HD"],
            true,
            AddonFormat::MesenHdPack,
            rom_name("SMB HD"),
        )];
        let t = translate(&g, &full(e));
        assert_eq!(
            dest(&t, "SMB HD"),
            emu(e, &["HdPacks", "Super Mario Bros. (World)"])
        );
        // Conservative keeps it: it is in the emulator's own folder.
        let t = translate(&g, &conservative(e));
        assert_eq!(
            dest(&t, "SMB HD"),
            emu(e, &["HdPacks", "Super Mario Bros. (World)"])
        );
    }
}
