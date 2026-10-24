//! Organise targets: folders per layout, collisions, duplicates,
//! companions, disc sets and file system limits (organise spec §7.3–§8.2).

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::organize::plan::{
    Companion, ItemKind, ItemNote, LeftReason, NameMode, OrganizeAction, OrganizeItem,
    RenameReason, SourceFp,
};
use coverly_core::organize::target::{
    assign_targets, default_companion_exts, file_system_is_fat32, mark_duplicates, DirIndex,
    TargetLayout,
};
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

fn item(id: usize, sources: &[&str], system: &str, kind: ItemKind, size: u64) -> OrganizeItem {
    OrganizeItem {
        id,
        sources: sources.iter().map(PathBuf::from).collect(),
        kind,
        size,
        fps: sources
            .iter()
            .map(|_| SourceFp { size, mtime_ns: 0 })
            .collect(),
        target: None,
        set_targets: Vec::new(),
        system_id: Some(system.to_string()),
        folder: None,
        confidence: 0.95,
        method: None,
        why: Vec::new(),
        alternatives: Vec::new(),
        rename: None,
        notes: Vec::new(),
        companions: Vec::new(),
        catalogue_name: None,
        transfer: Default::default(),
        disc: None,
        action: OrganizeAction::Auto,
    }
}

fn auto(id: usize, source: &str, system: &str) -> OrganizeItem {
    item(id, &[source], system, ItemKind::File, 4)
}

fn layout(target: &Path, nested: bool) -> TargetLayout {
    TargetLayout {
        target_root: target.to_path_buf(),
        roms_dir: if nested {
            target.join("ROMS")
        } else {
            target.to_path_buf()
        },
        profile: common::profile("anbernic-stock"),
        overrides: BTreeMap::new(),
        fat32: false,
        windows_paths: false,
        names: NameMode::Keep,
        companion_exts: default_companion_exts(),
    }
}

fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    p
}

/// Runs [`assign_targets`] with byte comparison as "same content".
fn assign(
    items: &mut [OrganizeItem],
    layout: &TargetLayout,
    source: &Path,
    companions: &[PathBuf],
) -> Vec<(PathBuf, LeftReason)> {
    let db = SystemDb::builtin();
    let mut index = DirIndex::new();
    let same = |existing: &Path, it: &OrganizeItem| {
        fs::read(existing).ok() == fs::read(source.join(&it.sources[0])).ok()
    };
    assign_targets(items, layout, &db, source, companions, &mut index, &same)
}

fn target(it: &OrganizeItem) -> PathBuf {
    it.target.clone().expect("item has a target")
}

#[test]
fn targets_follow_the_layout_and_reuse_existing_folders_in_any_case() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    fs::create_dir_all(t.path().join("ROMS/gba")).unwrap();
    put(s.path(), "a.gba", b"gba");
    put(s.path(), "b.gb", b"gb");
    let mut review = auto(2, "c.gb", "gbc");
    review.action = OrganizeAction::Review;
    let mut left = auto(3, "d.gb", "gb");
    left.action = OrganizeAction::Left {
        reason: LeftReason::Unknown,
    };
    let mut items = vec![auto(0, "a.gba", "gba"), auto(1, "b.gb", "gb"), review, left];
    let lay = layout(t.path(), true);
    assert!(assign(&mut items, &lay, s.path(), &[]).is_empty());
    assert_eq!(target(&items[0]), PathBuf::from("ROMS/gba/a.gba"));
    assert_eq!(items[0].folder.as_deref(), Some("gba"));
    assert_eq!(target(&items[1]), PathBuf::from("ROMS/GB/b.gb"));
    assert_eq!(items[1].folder.as_deref(), Some("GB"));
    assert_eq!(items[1].rename, None);
    assert!(items[1].notes.is_empty());
    // Review items get a target too, so the dry run shows where they go.
    assert_eq!(target(&items[2]), PathBuf::from("ROMS/GBC/c.gb"));
    assert_eq!(items[3].target, None);
    // Planning is read-only: no folder was created.
    assert!(!t.path().join("ROMS/GB").exists());
}

#[test]
fn flat_layout_puts_system_folders_directly_in_the_target() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(s.path(), "x.gba", b"gba");
    let mut items = vec![auto(0, "x.gba", "gba")];
    assign(&mut items, &layout(t.path(), false), s.path(), &[]);
    assert_eq!(target(&items[0]), PathBuf::from("GBA/x.gba"));
}

#[test]
fn an_existing_roms_folder_keeps_its_casing() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    fs::create_dir_all(t.path().join("roms")).unwrap();
    put(s.path(), "x.gba", b"gba");
    let mut items = vec![auto(0, "x.gba", "gba")];
    assign(&mut items, &layout(t.path(), true), s.path(), &[]);
    assert_eq!(target(&items[0]), PathBuf::from("roms/GBA/x.gba"));
}

#[test]
fn two_sources_to_one_target_get_numbered_names() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(s.path(), "a/Tetris.gb", b"one");
    put(s.path(), "b/Tetris.gb", b"two");
    let mut items = vec![auto(0, "a/Tetris.gb", "gb"), auto(1, "b/Tetris.gb", "gb")];
    assign(&mut items, &layout(t.path(), true), s.path(), &[]);
    assert_eq!(target(&items[0]), PathBuf::from("ROMS/GB/Tetris.gb"));
    assert_eq!(items[0].rename, None);
    assert_eq!(target(&items[1]), PathBuf::from("ROMS/GB/Tetris (2).gb"));
    assert_eq!(
        items[1].rename,
        Some(RenameReason::Collision {
            name: "Tetris.gb".into()
        })
    );
}

#[test]
fn an_existing_different_file_gets_a_numbered_name_and_the_same_file_is_already_present() {
    // Different bytes → numbered name.
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(s.path(), "Tetris.gb", b"new");
    put(t.path(), "ROMS/GB/Tetris.gb", b"old");
    put(t.path(), "ROMS/GB/Tetris (2).gb", b"older");
    let mut items = vec![auto(0, "Tetris.gb", "gb")];
    assign(&mut items, &layout(t.path(), true), s.path(), &[]);
    assert_eq!(target(&items[0]), PathBuf::from("ROMS/GB/Tetris (3).gb"));
    assert_eq!(items[0].action, OrganizeAction::Auto);

    // Same bytes → left in place, already present.
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(s.path(), "Tetris.gb", b"same");
    put(t.path(), "ROMS/GB/Tetris.gb", b"same");
    let mut items = vec![auto(0, "Tetris.gb", "gb")];
    assign(&mut items, &layout(t.path(), true), s.path(), &[]);
    assert_eq!(
        items[0].action,
        OrganizeAction::Left {
            reason: LeftReason::AlreadyPresent {
                at: PathBuf::from("ROMS/GB/Tetris.gb")
            }
        }
    );
    assert_eq!(items[0].target, None);
}

#[test]
fn case_twins_collide() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(s.path(), "Tetris.gb", b"new");
    put(t.path(), "ROMS/GB/tetris.GB", b"old");
    let mut items = vec![auto(0, "Tetris.gb", "gb")];
    assign(&mut items, &layout(t.path(), true), s.path(), &[]);
    assert_eq!(target(&items[0]), PathBuf::from("ROMS/GB/Tetris (2).gb"));
}

#[cfg(unix)]
#[test]
fn unsafe_names_are_made_fat_safe_with_a_reason() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(s.path(), "Zelda: Link?.gb", b"z");
    let mut items = vec![auto(0, "Zelda: Link?.gb", "gb")];
    assign(&mut items, &layout(t.path(), true), s.path(), &[]);
    assert_eq!(target(&items[0]), PathBuf::from("ROMS/GB/Zelda_ Link_.gb"));
    assert_eq!(
        items[0].rename,
        Some(RenameReason::SafeName {
            from: "Zelda: Link?.gb".into()
        })
    );
}

#[test]
fn duplicates_by_content_keep_the_shortest_name() {
    let mut items = vec![
        auto(0, "Tetris 2 (1).zip", "gb"),
        auto(1, "Tetris 2.zip", "gb"),
        auto(2, "Other.zip", "gb"),
        auto(3, "Tetris 2 - Copy.zip", "gb"),
    ];
    let key = Some(("ABCD1234".to_string(), 4));
    let keys = vec![key.clone(), key.clone(), None, key];
    mark_duplicates(&mut items, &keys);
    let dup = OrganizeAction::Left {
        reason: LeftReason::Duplicate {
            of: PathBuf::from("Tetris 2.zip"),
        },
    };
    assert_eq!(items[0].action, dup);
    assert_eq!(items[1].action, OrganizeAction::Auto);
    assert_eq!(items[2].action, OrganizeAction::Auto);
    assert_eq!(items[3].action, dup);
}

#[test]
fn companions_follow_and_are_renamed_with_catalogue_names() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(s.path(), "Game.gba", b"rom");
    let sav = put(s.path(), "Game.sav", b"save");
    let state = put(s.path(), "Game.state3", b"state");
    let other = put(s.path(), "Other.sav", b"save");
    let mut it = auto(0, "Game.gba", "gba");
    it.catalogue_name = Some("Game (USA)".into());
    let mut items = vec![it];
    let mut lay = layout(t.path(), true);
    lay.names = NameMode::Catalogue;
    let left = assign(&mut items, &lay, s.path(), &[sav, state, other]);
    assert!(left.is_empty());
    assert_eq!(target(&items[0]), PathBuf::from("ROMS/GBA/Game (USA).gba"));
    assert_eq!(
        items[0].rename,
        Some(RenameReason::Catalogue {
            from: "Game.gba".into()
        })
    );
    assert_eq!(
        items[0].companions,
        vec![
            Companion {
                source: "Game.sav".into(),
                target: "ROMS/GBA/Game (USA).sav".into()
            },
            Companion {
                source: "Game.state3".into(),
                target: "ROMS/GBA/Game (USA).state3".into()
            },
        ]
    );
}

#[test]
fn a_companion_collision_keeps_the_original_name() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(s.path(), "Game.gba", b"rom");
    let sav = put(s.path(), "Game.sav", b"save");
    put(t.path(), "ROMS/GBA/Game (USA).sav", b"someone else's save");
    let mut it = auto(0, "Game.gba", "gba");
    it.catalogue_name = Some("Game (USA)".into());
    let mut items = vec![it];
    let mut lay = layout(t.path(), true);
    lay.names = NameMode::Catalogue;
    assign(&mut items, &lay, s.path(), &[sav]);
    assert_eq!(target(&items[0]), PathBuf::from("ROMS/GBA/Game.gba"));
    assert_eq!(items[0].rename, None);
    assert_eq!(
        items[0].companions,
        vec![Companion {
            source: "Game.sav".into(),
            target: "ROMS/GBA/Game.sav".into()
        }]
    );
}

#[test]
fn a_companion_of_two_roms_in_different_folders_is_ambiguous() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(s.path(), "Tetris.gb", b"gb");
    put(s.path(), "Tetris.gbc", b"gbc");
    let sav = put(s.path(), "Tetris.sav", b"save");
    let mut items = vec![auto(0, "Tetris.gb", "gb"), auto(1, "Tetris.gbc", "gbc")];
    let left = assign(
        &mut items,
        &layout(t.path(), true),
        s.path(),
        std::slice::from_ref(&sav),
    );
    assert!(items[0].companions.is_empty());
    assert!(items[1].companions.is_empty());
    assert_eq!(left, vec![(sav, LeftReason::AmbiguousCompanion)]);
    assert_eq!(target(&items[0]), PathBuf::from("ROMS/GB/Tetris.gb"));
    assert_eq!(target(&items[1]), PathBuf::from("ROMS/GBC/Tetris.gbc"));
}

#[test]
fn a_set_moves_together_and_a_set_collision_skips_the_whole_set() {
    let s = TempDir::new().unwrap();
    put(s.path(), "Game.cue", b"FILE \"Game.bin\" BINARY");
    put(s.path(), "Game.bin", b"track");
    let set = || item(0, &["Game.cue", "Game.bin"], "psx", ItemKind::Set, 1 << 20);

    let t = TempDir::new().unwrap();
    let mut items = vec![set()];
    assign(&mut items, &layout(t.path(), true), s.path(), &[]);
    assert_eq!(target(&items[0]), PathBuf::from("ROMS/PS/Game.cue"));
    assert_eq!(
        items[0].set_targets,
        vec![PathBuf::from("ROMS/PS/Game.bin")]
    );

    let t = TempDir::new().unwrap();
    put(t.path(), "ROMS/PS/Game.bin", b"another track");
    let mut items = vec![set()];
    assign(&mut items, &layout(t.path(), true), s.path(), &[]);
    assert_eq!(
        items[0].action,
        OrganizeAction::Left {
            reason: LeftReason::CollisionSet
        }
    );
    assert_eq!(items[0].target, None);
    assert!(items[0].set_targets.is_empty());
}

#[test]
fn a_set_keeps_the_layout_of_its_members() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(
        s.path(),
        "pile/Game.cue",
        b"FILE \"Disc 1/track.bin\" BINARY",
    );
    put(s.path(), "pile/Disc 1/track.bin", b"track");
    let mut items = vec![item(
        0,
        &["pile/Game.cue", "pile/Disc 1/track.bin"],
        "psx",
        ItemKind::Set,
        10,
    )];
    assign(&mut items, &layout(t.path(), true), s.path(), &[]);
    assert_eq!(target(&items[0]), PathBuf::from("ROMS/PS/Game.cue"));
    assert_eq!(
        items[0].set_targets,
        vec![PathBuf::from("ROMS/PS/Disc 1/track.bin")]
    );
}

#[test]
fn too_big_for_fat32() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    let mut big = auto(0, "Big.iso", "psp");
    big.size = 4 << 30;
    big.fps[0].size = 4 << 30;
    let mut items = vec![big];
    let mut lay = layout(t.path(), true);
    lay.fat32 = true;
    assign(&mut items, &lay, s.path(), &[]);
    assert_eq!(
        items[0].action,
        OrganizeAction::Left {
            reason: LeftReason::TooBigForFat
        }
    );
    // The same item is fine off FAT32.
    items[0].action = OrganizeAction::Auto;
    assign(&mut items, &layout(t.path(), true), s.path(), &[]);
    assert!(items[0].target.is_some());
}

#[test]
fn long_windows_paths_are_shortened() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    let name = format!("{}.gb", "a".repeat(250));
    let mut items = vec![auto(0, &name, "gb")];
    let mut lay = layout(t.path(), true);
    lay.windows_paths = true;
    assign(&mut items, &lay, s.path(), &[]);
    let full = t.path().join(target(&items[0]));
    assert!(full.to_string_lossy().encode_utf16().count() <= 259);
    assert!(target(&items[0]).to_string_lossy().ends_with(".gb"));
    assert_eq!(items[0].rename, Some(RenameReason::PathLength));
}

#[test]
fn a_system_the_device_does_not_show_gets_a_note() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    let mut lay = layout(t.path(), true);
    // A device whose firmware shows only GB/ and GBA/.
    lay.profile.known_folders = vec!["GB".into(), "GBA".into()];
    let mut items = vec![auto(0, "x.gba", "gba"), auto(1, "y.gbc", "gbc")];
    assign(&mut items, &lay, s.path(), &[]);
    assert!(items[0].notes.is_empty());
    assert_eq!(
        items[1].notes,
        vec![ItemNote::NoFolderOnDevice {
            folder: "GBC".into()
        }]
    );
    assert_eq!(target(&items[1]), PathBuf::from("ROMS/GBC/y.gbc"));
}

#[test]
fn the_directory_index_reads_each_folder_once() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(t.path(), "ROMS/GB/existing.gb", b"x");
    let db = SystemDb::builtin();
    let mut items: Vec<_> = (0..50)
        .map(|i| auto(i, &format!("g{i}.gb"), "gb"))
        .collect();
    let mut index = DirIndex::new();
    let lay = layout(t.path(), true);
    assign_targets(&mut items, &lay, &db, s.path(), &[], &mut index, &|_, _| {
        false
    });
    assert_eq!(index.read_dirs(), 1);
}

#[test]
fn a_temp_folder_is_not_fat32() {
    let t = TempDir::new().unwrap();
    assert!(!file_system_is_fat32(t.path()));
    assert!(!file_system_is_fat32(&t.path().join("not/yet/created")));
}

#[test]
fn renamed_companions_also_fit_windows_paths() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    let stem = "a".repeat(240);
    let rom = format!("{stem}.gb");
    let sav = put(s.path(), &format!("{stem}.state10"), b"state");
    let mut items = vec![auto(0, &rom, "gb")];
    let mut lay = layout(t.path(), true);
    lay.windows_paths = true;
    assign(&mut items, &lay, s.path(), &[sav]);
    let rom_target = target(&items[0]);
    assert_eq!(items[0].companions.len(), 1);
    let comp_target = &items[0].companions[0].target;
    let units = |p: &Path| t.path().join(p).to_string_lossy().encode_utf16().count();
    assert!(units(&rom_target) <= 259);
    assert!(units(comp_target) <= 259, "{}", comp_target.display());
    // The save keeps the ROM's (shortened) stem so emulators still find it.
    assert_eq!(
        rom_target.file_stem(),
        Path::new(comp_target)
            .file_stem()
            .and_then(|s| Path::new(s).file_stem())
    );
    assert_eq!(items[0].rename, Some(RenameReason::PathLength));
}

#[test]
fn auto_save_states_with_two_extensions_follow_and_are_renamed() {
    let s = TempDir::new().unwrap();
    let t = TempDir::new().unwrap();
    put(s.path(), "Game.gba", b"rom");
    let auto_state = put(s.path(), "Game.state.auto", b"state");
    let mut it = auto(0, "Game.gba", "gba");
    it.catalogue_name = Some("Game (USA)".into());
    let mut items = vec![it];
    let mut lay = layout(t.path(), true);
    lay.names = NameMode::Catalogue;
    assign(&mut items, &lay, s.path(), &[auto_state]);
    assert_eq!(
        items[0].companions,
        vec![Companion {
            source: "Game.state.auto".into(),
            target: "ROMS/GBA/Game (USA).state.auto".into()
        }]
    );
}

/// Extract targets of disc archives (archives spec §8.2): names kept,
/// lengths and collisions checked for the whole set.
mod extract {
    use super::*;
    use coverly_core::organize::plan::{ArchiveAfter, ExtractEntry, SheetCheck, Transfer};
    use coverly_core::organize::target::{assign_targets_with_extracts, ExtractShape};

    /// A psx disc archive item extracting `names` (the first a sheet).
    fn disc_item(id: usize, archive: &str, names: &[&str]) -> OrganizeItem {
        let mut it = item(id, &[archive], "psx", ItemKind::Archive, 10);
        it.transfer = Transfer::Extract {
            entries: names
                .iter()
                .enumerate()
                .map(|(index, name)| ExtractEntry {
                    index,
                    name: (*name).to_string(),
                    size: 1,
                    crc32: Some(0),
                    target: PathBuf::new(),
                })
                .collect(),
            bytes: names.len() as u64,
            archive_after: ArchiveAfter::Trash,
            source: Default::default(),
            sheet_check: SheetCheck::Read,
            m3u: None,
        };
        it
    }

    fn shape(first: &str) -> Option<ExtractShape> {
        Some(ExtractShape::Disc {
            discs: vec![first.to_string()],
            title: String::new(),
            multi_disc: false,
        })
    }

    fn run(items: &mut [OrganizeItem], lay: &TargetLayout, s: &Path) {
        let shapes: Vec<Option<ExtractShape>> = items
            .iter()
            .map(|i| match &i.transfer {
                Transfer::Extract { entries, .. } => shape(&entries[0].name),
                Transfer::AsIs => None,
            })
            .collect();
        let db = SystemDb::builtin();
        let never = |_: &Path, _: &OrganizeItem| false;
        assign_targets_with_extracts(
            items,
            &shapes,
            lay,
            &db,
            s,
            &[],
            &mut DirIndex::new(),
            &never,
        );
    }

    fn left(it: &OrganizeItem) -> Option<&LeftReason> {
        match &it.action {
            OrganizeAction::Left { reason } => Some(reason),
            _ => None,
        }
    }

    #[test]
    fn case_twins_inside_one_archive_collision_set() {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let mut items = vec![disc_item(
            0,
            "Game.7z",
            &["Game.cue", "Track 01.bin", "track 01.bin"],
        )];
        run(&mut items, &layout(t.path(), true), s.path());
        assert_eq!(left(&items[0]), Some(&LeftReason::CollisionSet));
        assert_eq!(items[0].transfer, Transfer::AsIs);
    }

    #[test]
    fn a_loose_file_planned_first_keeps_a_contested_name() {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        put(s.path(), "Track 01.bin", b"x");
        let mut items = vec![
            auto(0, "Track 01.bin", "psx"),
            disc_item(1, "Game.7z", &["Game.cue", "Track 01.bin"]),
        ];
        run(&mut items, &layout(t.path(), true), s.path());
        assert_eq!(target(&items[0]), PathBuf::from("ROMS/PS/Track 01.bin"));
        assert_eq!(left(&items[1]), Some(&LeftReason::CollisionSet));
    }

    #[test]
    fn member_unsafe_name_set_unsafe_name() {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let mut items = vec![
            disc_item(0, "A.7z", &["A.cue", "A?.bin"]),
            disc_item(1, "B.7z", &["B.cue", "B (Track 01).bin."]),
        ];
        run(&mut items, &layout(t.path(), true), s.path());
        assert_eq!(
            left(&items[0]),
            Some(&LeftReason::SetUnsafeName {
                name: "A?.bin".into()
            })
        );
        assert_eq!(
            left(&items[1]),
            Some(&LeftReason::SetUnsafeName {
                name: "B (Track 01).bin.".into()
            })
        );
    }

    #[test]
    fn long_target_left_path_too_long() {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let long = format!("{}.bin", "t".repeat(240));
        let names = ["Game.cue", long.as_str()];
        let mut lay = layout(t.path(), true);
        let mut items = vec![disc_item(0, "Game.7z", &names)];
        run(&mut items, &lay, s.path());
        assert_eq!(left(&items[0]), None, "fits without the Windows limit");

        lay.windows_paths = true;
        let mut items = vec![disc_item(0, "Game.7z", &names)];
        run(&mut items, &lay, s.path());
        assert_eq!(left(&items[0]), Some(&LeftReason::PathTooLong));

        // A component over 255 bytes never fits, whatever the OS.
        let huge = format!("{}.bin", "é".repeat(130));
        let mut items = vec![disc_item(0, "Game.7z", &["Game.cue", huge.as_str()])];
        run(&mut items, &layout(t.path(), true), s.path());
        assert_eq!(left(&items[0]), Some(&LeftReason::PathTooLong));
    }

    #[test]
    fn long_temp_name_counts() {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let ps = t.path().join("ROMS").join("PS");
        let units = |p: &Path| p.to_string_lossy().encode_utf16().count();
        // A folder inside the archive sized so the final path is exactly
        // 259 units: only the temp file beside it is too long.
        let file = "t.bin";
        let dir_len = 259 - units(&ps) - 1 - 1 - file.len();
        assert!(dir_len > 0 && dir_len <= 255);
        let deep = format!("{}/{file}", "d".repeat(dir_len));
        let names = ["Game.cue", deep.as_str()];
        assert_eq!(units(&ps.join(&deep)), 259);
        let mut lay = layout(t.path(), true);
        lay.windows_paths = true;
        let mut items = vec![disc_item(0, "Game.7z", &names)];
        run(&mut items, &lay, s.path());
        assert_eq!(left(&items[0]), Some(&LeftReason::PathTooLong));
    }

    #[test]
    fn too_deep_a_member_is_path_too_long() {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let deep = format!("{}t.bin", "d/".repeat(16));
        let mut items = vec![disc_item(0, "Game.7z", &["Game.cue", deep.as_str()])];
        run(&mut items, &layout(t.path(), true), s.path());
        assert_eq!(left(&items[0]), Some(&LeftReason::PathTooLong));
    }

    #[test]
    fn extracted_sizes_decide_fat32_not_the_archive() {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let mut lay = layout(t.path(), true);
        lay.fat32 = true;
        let mut small = disc_item(0, "Big.7z", &["Big.cue", "Big.bin"]);
        small.size = 5 << 30;
        small.fps[0].size = 5 << 30;
        let mut big = disc_item(1, "Huge.7z", &["Huge.cue", "Huge.bin"]);
        if let Transfer::Extract { entries, .. } = &mut big.transfer {
            entries[1].size = 4 << 30;
        }
        let mut items = vec![small, big];
        run(&mut items, &lay, s.path());
        assert_eq!(left(&items[0]), None);
        assert_eq!(left(&items[1]), Some(&LeftReason::TooBigForFat));
    }

    /// A game-folder layout (MinUI).
    fn minui(target: &Path) -> TargetLayout {
        TargetLayout {
            profile: common::profile("minui"),
            ..layout(target, true)
        }
    }

    fn target_of(it: &OrganizeItem, i: usize) -> String {
        match &it.transfer {
            Transfer::Extract { entries, .. } => {
                entries[i].target.to_string_lossy().replace('\\', "/")
            }
            other => panic!("not an extraction: {other:?}"),
        }
    }

    #[test]
    fn game_folders_differing_only_in_case_collide() {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        // On FAT, exFAT and default APFS/NTFS both land in one folder.
        let mut items = vec![
            disc_item(0, "A.7z", &["Game (USA).cue", "Game (USA).bin"]),
            disc_item(1, "B.7z", &["game (usa).cue", "game (usa) (Track 1).bin"]),
        ];
        run(&mut items, &minui(t.path()), s.path());
        assert_eq!(left(&items[0]), None);
        assert_eq!(left(&items[1]), Some(&LeftReason::CollisionSet));
    }

    #[test]
    fn a_file_where_the_game_folder_goes_is_a_collision() {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let mut items = vec![disc_item(0, "A.7z", &["Game (USA).cue", "Game (USA).bin"])];
        let lay = minui(t.path());
        // Find the system folder the layout picks, then block the game
        // folder there with a plain file.
        run(&mut items, &lay, s.path());
        let cue = target_of(&items[0], 0);
        let folder = cue.rsplit_once('/').unwrap().0.to_string();
        put(t.path(), &folder, b"not a folder");
        let mut items = vec![disc_item(0, "A.7z", &["Game (USA).cue", "Game (USA).bin"])];
        run(&mut items, &lay, s.path());
        assert_eq!(left(&items[0]), Some(&LeftReason::CollisionSet));
    }

    #[test]
    fn an_existing_game_folder_in_another_case_is_reused() {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let lay = minui(t.path());
        let mut items = vec![disc_item(0, "A.7z", &["Game (USA).cue", "Game (USA).bin"])];
        run(&mut items, &lay, s.path());
        let cue = target_of(&items[0], 0);
        let system = cue
            .rsplit_once('/')
            .unwrap()
            .0
            .rsplit_once('/')
            .unwrap()
            .0
            .to_string();
        fs::create_dir_all(t.path().join(&system).join("game (usa)")).unwrap();
        let mut items = vec![disc_item(0, "A.7z", &["Game (USA).cue", "Game (USA).bin"])];
        run(&mut items, &lay, s.path());
        assert_eq!(left(&items[0]), None);
        assert_eq!(
            target_of(&items[0], 0),
            format!("{system}/game (usa)/Game (USA).cue")
        );
        assert_eq!(
            target_of(&items[0], 1),
            format!("{system}/game (usa)/Game (USA).bin")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_game_folder_is_never_written_through() {
        let (s, t, elsewhere) = (
            TempDir::new().unwrap(),
            TempDir::new().unwrap(),
            TempDir::new().unwrap(),
        );
        let lay = minui(t.path());
        let mut items = vec![disc_item(0, "A.7z", &["Game (USA).cue", "Game (USA).bin"])];
        run(&mut items, &lay, s.path());
        let cue = target_of(&items[0], 0);
        let folder = t.path().join(cue.rsplit_once('/').unwrap().0);
        fs::create_dir_all(folder.parent().unwrap()).unwrap();
        // A link where the game folder goes would send the tracks outside
        // the target.
        std::os::unix::fs::symlink(elsewhere.path(), &folder).unwrap();
        let mut items = vec![disc_item(0, "A.7z", &["Game (USA).cue", "Game (USA).bin"])];
        run(&mut items, &lay, s.path());
        assert_eq!(left(&items[0]), Some(&LeftReason::CollisionSet));
    }
}
