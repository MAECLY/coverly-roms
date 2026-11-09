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
