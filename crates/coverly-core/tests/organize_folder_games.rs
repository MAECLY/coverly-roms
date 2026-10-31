//! Folder games, plan side (systems batch 2, D12, D13, D25, D27): a folder
//! a probe proves is one `Folder` item, measured from metadata only, gated
//! per contained file, named with the layout's suffix and never merged
//! into a taken folder. The probe is the test hook of plan P-7: a folder
//! holding `GAME.PROBE` is a `gb` game.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use coverly_core::cache::Cache;
use coverly_core::consoles::{
    add_extra_probe, set_extra_probe, ConsoleId, DirListing, DirProbe, FolderMatch, IdSource,
};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::organize::target::{set_fat32_override, set_windows_paths_override};
use coverly_core::organize::{
    apply_organize, plan_organize, ApplyEnv, ItemKind, ItemNote, ItemStatus, LeftReason, MetaProbe,
    OrganizeAction, OrganizeEnv, OrganizeItem, OrganizeOptions, OrganizePlan, RenameReason,
    SkipReason,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::FakeTrash;
use tempfile::TempDir;

/// Answers `cloud_only` for files named `cloud.bin` and a 5 GiB length
/// for files named `big.bin` (never a real 4 GiB file).
struct Probe;

impl MetaProbe for Probe {
    fn cloud_only(&self, path: &Path, _: &fs::Metadata) -> bool {
        path.file_name().is_some_and(|n| n == "cloud.bin")
    }

    fn file_len(&self, path: &Path, meta: &fs::Metadata) -> u64 {
        if path.file_name().is_some_and(|n| n == "big.bin") {
            5 << 30
        } else {
            meta.len()
        }
    }
}

fn has(listing: &DirListing, name: &str) -> bool {
    listing.names.iter().any(|n| n.eq_ignore_ascii_case(name))
}

fn id(system: &str) -> ConsoleId {
    let mut id = ConsoleId::proven(system, "test probe");
    id.serial = Some("TEST01".into());
    id.title_id = Some("TEST01".into());
    id.id_source = Some(IdSource::Header);
    id
}

fn test_probe(listing: &DirListing) -> Option<FolderMatch> {
    has(listing, "GAME.PROBE").then(|| FolderMatch {
        id: id("gb"),
        probe: PathBuf::from("GAME.PROBE"),
    })
}

fn gba_probe(listing: &DirListing) -> Option<FolderMatch> {
    has(listing, "GAME.PROBE").then(|| FolderMatch {
        id: id("gba"),
        probe: PathBuf::from("GAME.PROBE"),
    })
}

/// Sets the test probe on this thread and clears every hook on drop.
struct ProbeGuard;

impl ProbeGuard {
    fn new() -> ProbeGuard {
        set_extra_probe(Some(test_probe as DirProbe));
        ProbeGuard
    }
}

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        set_extra_probe(None);
        set_fat32_override(None);
        set_windows_paths_override(None);
    }
}

struct World {
    db: SystemDb,
    profiles: Vec<Profile>,
    cache: Cache,
}

impl World {
    fn new() -> World {
        World {
            db: SystemDb::builtin(),
            profiles: builtin_profiles(),
            cache: Cache::in_memory().unwrap(),
        }
    }

    fn env(&self) -> OrganizeEnv<'_> {
        OrganizeEnv {
            db: &self.db,
            profiles: &self.profiles,
            cache: &self.cache,
            dats: None,
            probe: &Probe,
            online: None,
            now: SystemTime::now(),
        }
    }
}

fn setup() -> (World, TempDir, TempDir) {
    (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    )
}

/// Writes each `(relative path, length)` as zeros, mtimes pinned.
fn put_tree(root: &Path, files: &[(&str, u64)]) {
    for (i, (rel, len)) in files.iter().enumerate() {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![0u8; *len as usize]).unwrap();
        pin_mtime(&path, i as u64);
    }
}

fn options(target: &Path, layout: &str) -> OrganizeOptions {
    OrganizeOptions {
        layout: layout.to_string(),
        target: Some(target.to_path_buf()),
        offline: true,
        ..OrganizeOptions::default()
    }
}

fn plan(world: &World, pile: &Path, target: &Path, layout: &str) -> OrganizePlan {
    plan_organize(
        pile,
        &options(target, layout),
        &world.env(),
        &|_| {},
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn folders(p: &OrganizePlan) -> Vec<&OrganizeItem> {
    p.items
        .iter()
        .filter(|i| i.kind == ItemKind::Folder)
        .collect()
}

fn only_folder(p: &OrganizePlan) -> &OrganizeItem {
    let all = folders(p);
    assert_eq!(all.len(), 1, "{:#?}", p.items);
    all[0]
}

fn last(path: &Path) -> String {
    path.components()
        .next_back()
        .unwrap()
        .as_os_str()
        .to_string_lossy()
        .into_owned()
}

fn left_reason(item: &OrganizeItem) -> Option<&LeftReason> {
    match &item.action {
        OrganizeAction::Left { reason } => Some(reason),
        _ => None,
    }
}

#[test]
fn probed_folder_is_one_item_and_its_contents_are_not_walked() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(
        pile.path(),
        &[
            ("Zelda/GAME.PROBE", 16),
            ("Zelda/data/a.gb", 32_768),
            ("Zelda/data/b.bin", 10),
        ],
    );
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    assert_eq!(item.sources, [PathBuf::from("Zelda")]);
    assert_eq!(item.size, 16 + 32_768 + 10);
    assert_eq!(item.fps.len(), 1);
    assert_eq!(item.fps[0].size, item.size);
    assert_eq!(item.system_id.as_deref(), Some("gb"));
    assert_eq!(item.action, OrganizeAction::Auto);
    assert!(item.why.iter().any(|e| e.detail == "test probe"));
    let t = item.target.as_ref().unwrap();
    assert_eq!(last(t), "Zelda");
    assert_eq!(item.folder.as_deref(), Some("gb"));
    assert_eq!(
        t.parent().map(last).as_deref(),
        Some("gb"),
        "the folder lands in the system folder: {}",
        t.display()
    );
    assert!(!p
        .items
        .iter()
        .any(|i| i.sources[0].file_name().is_some_and(|n| n == "a.gb")));
    assert!(p.other_files.is_empty(), "{:?}", p.other_files);
    assert_eq!(p.counts.roms, 1);
}

#[test]
fn junk_inside_a_folder_is_not_counted() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(
        pile.path(),
        &[
            ("Zelda/GAME.PROBE", 16),
            ("Zelda/.DS_Store", 100),
            ("Zelda/._GAME.PROBE", 100),
            ("Zelda/Thumbs.db", 100),
        ],
    );
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    assert_eq!(item.size, 16);
    assert_eq!(item.action, OrganizeAction::Auto);
}

#[test]
fn a_folder_without_the_probe_file_is_walked_as_before() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(pile.path(), &[("Stuff/notes.txt", 5)]);
    let p = plan(&world, pile.path(), target.path(), "esde");
    assert!(folders(&p).is_empty());
}

/// Plans a pile holding `files` (plus whatever `extra` adds) and returns
/// the folder item.
fn gate(files: &[(&str, u64)], extra: impl FnOnce(&Path)) -> OrganizeItem {
    let (world, pile, target) = setup();
    put_tree(pile.path(), files);
    extra(pile.path());
    let p = plan(&world, pile.path(), target.path(), "esde");
    only_folder(&p).clone()
}

/// A pile name, its files and the reason it must be left with.
type GateCase = (
    &'static str,
    &'static [(&'static str, u64)],
    fn(&LeftReason) -> bool,
);

#[test]
fn folder_gates() {
    let _g = ProbeGuard::new();
    let cases: &[GateCase] = &[
        ("partial", &[("G/GAME.PROBE", 16), ("G/x.part", 5)], |r| {
            matches!(r, LeftReason::PartialDownload)
        }),
        ("zero", &[("G/GAME.PROBE", 16), ("G/empty", 0)], |r| {
            matches!(r, LeftReason::PartialDownload)
        }),
        (
            "crdownload",
            &[("G/GAME.PROBE", 16), ("G/d/x.iso.crdownload", 5)],
            |r| matches!(r, LeftReason::PartialDownload),
        ),
        (
            "cloud",
            &[("G/GAME.PROBE", 16), ("G/d/cloud.bin", 5)],
            |r| matches!(r, LeftReason::CloudOnly),
        ),
    ];
    for (name, files, want) in cases {
        let item = gate(files, |_| {});
        let reason = left_reason(&item);
        assert!(reason.is_some_and(want), "{name}: {:?}", item.action);
        assert!(item.target.is_none(), "{name}");
    }

    // A file written just now: the extraction may still be running.
    let item = gate(&[("G/GAME.PROBE", 16)], |pile| {
        fs::write(pile.join("G/fresh.bin"), b"x").unwrap();
    });
    assert_eq!(left_reason(&item), Some(&LeftReason::RecentlyModified));

    // One member over 4 GiB on a FAT32 target; the total does not matter.
    set_fat32_override(Some(true));
    let item = gate(&[("G/GAME.PROBE", 16), ("G/big.bin", 5)], |_| {});
    assert_eq!(left_reason(&item), Some(&LeftReason::TooBigForFat));
    let item = gate(&[("G/GAME.PROBE", 16), ("G/small.bin", 5)], |_| {});
    assert_eq!(item.action, OrganizeAction::Auto);
    set_fat32_override(None);

    // A nested path past Windows' limit once below the target.
    set_windows_paths_override(Some(true));
    let deep = format!(
        "G/{}/{}/{}/f.bin",
        "a".repeat(90),
        "b".repeat(90),
        "c".repeat(90)
    );
    let item = gate(&[("G/GAME.PROBE", 16), (deep.as_str(), 4)], |_| {});
    assert_eq!(left_reason(&item), Some(&LeftReason::PathTooLong));
    let item = gate(&[("G/GAME.PROBE", 16), ("G/d/f.bin", 4)], |_| {});
    assert_eq!(item.action, OrganizeAction::Auto);
    set_windows_paths_override(None);
}

#[cfg(unix)]
#[test]
fn a_symlink_inside_leaves_the_folder() {
    let _g = ProbeGuard::new();
    let item = gate(&[("G/GAME.PROBE", 16), ("G/real.bin", 4)], |pile| {
        std::os::unix::fs::symlink(pile.join("G/real.bin"), pile.join("G/link.bin")).unwrap();
    });
    assert_eq!(left_reason(&item), Some(&LeftReason::Symlink));
}

#[test]
fn folder_suffix_is_appended_once() {
    let _g = ProbeGuard::new();
    let (mut world, pile, target) = setup();
    let esde = world.profiles.iter_mut().find(|p| p.id == "esde").unwrap();
    esde.folder_game_suffix
        .insert("gb".to_string(), ".gbdir".to_string());
    let layout = esde.name.clone();
    put_tree(
        pile.path(),
        &[("Zelda/GAME.PROBE", 16), ("Mario.GBDIR/GAME.PROBE", 16)],
    );
    let p = plan(&world, pile.path(), target.path(), "esde");
    let all = folders(&p);
    assert_eq!(all.len(), 2);
    let zelda = all
        .iter()
        .find(|i| i.sources[0] == Path::new("Zelda"))
        .unwrap();
    assert_eq!(last(zelda.target.as_ref().unwrap()), "Zelda.gbdir");
    assert!(zelda.notes.contains(&ItemNote::FolderSuffix {
        name: "Zelda.gbdir".into(),
        layout: layout.clone(),
    }));
    let mario = all
        .iter()
        .find(|i| i.sources[0] == Path::new("Mario.GBDIR"))
        .unwrap();
    assert_eq!(last(mario.target.as_ref().unwrap()), "Mario.GBDIR");
    assert!(!mario
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::FolderSuffix { .. })));

    // A taken `Zelda.gbdir` numbers before the suffix.
    let taken = target.path().join(zelda.target.as_ref().unwrap());
    fs::create_dir_all(&taken).unwrap();
    let p = plan(&world, pile.path(), target.path(), "esde");
    let zelda = folders(&p)
        .into_iter()
        .find(|i| i.sources[0] == Path::new("Zelda"))
        .unwrap();
    assert_eq!(last(zelda.target.as_ref().unwrap()), "Zelda (2).gbdir");
    assert_eq!(
        zelda.rename,
        Some(RenameReason::Collision {
            name: "Zelda.gbdir".into()
        })
    );
}

#[test]
fn two_probes_naming_different_systems_go_to_review() {
    let _g = ProbeGuard::new();
    add_extra_probe(gba_probe as DirProbe);
    let (world, pile, target) = setup();
    put_tree(pile.path(), &[("Zelda/GAME.PROBE", 16)]);
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    assert_eq!(item.action, OrganizeAction::Review);
    assert_eq!(item.system_id, None);
    let alts: Vec<&str> = item.alternatives.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(alts, ["gb", "gba"]);
}

#[test]
fn taken_target_folder_gets_a_number_never_a_merge() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(pile.path(), &[("Zelda/GAME.PROBE", 16)]);
    let first = plan(&world, pile.path(), target.path(), "esde");
    let t = only_folder(&first).target.clone().unwrap();
    // The same game already there: still a number, never «already present».
    put_tree(&target.path().join(&t), &[("GAME.PROBE", 16)]);
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    assert_eq!(item.action, OrganizeAction::Auto);
    let t2 = item.target.as_ref().unwrap();
    assert_eq!(last(t2), "Zelda (2)");
    assert_eq!(t2.parent(), t.parent());
    assert_eq!(
        item.rename,
        Some(RenameReason::Collision {
            name: "Zelda".into()
        })
    );
}

#[test]
fn a_folder_changed_since_the_plan_is_skipped_at_apply() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(
        pile.path(),
        &[("Zelda/GAME.PROBE", 16), ("Zelda/d/a.bin", 8)],
    );
    let p = plan(&world, pile.path(), target.path(), "esde");
    let id = only_folder(&p).id;
    fs::write(pile.path().join("Zelda/d/a.bin"), [1u8; 9]).unwrap();
    pin_mtime(&pile.path().join("Zelda/d/a.bin"), 1);
    let data = TempDir::new().unwrap();
    let report = apply_organize(
        &p,
        &ApplyEnv {
            data_dir: data.path(),
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
            trash: &FakeTrash::unavailable("no-trash"),
            tools: Some(&[]),
        },
    )
    .unwrap();
    let outcome = report.items.iter().find(|o| o.item == id).unwrap();
    assert_eq!(
        outcome.status,
        ItemStatus::Skipped {
            reason: SkipReason::ChangedSincePlan
        }
    );
    assert!(pile.path().join("Zelda/d/a.bin").is_file());
}

#[test]
fn apply_never_treats_a_folder_game_as_a_file() {
    // Until the directory transfer lands (D25) an unchanged folder item is
    // skipped whole: nothing below it moves.
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(
        pile.path(),
        &[("Zelda/GAME.PROBE", 16), ("Zelda/d/a.bin", 8)],
    );
    let p = plan(&world, pile.path(), target.path(), "esde");
    let id = only_folder(&p).id;
    let data = TempDir::new().unwrap();
    let report = apply_organize(
        &p,
        &ApplyEnv {
            data_dir: data.path(),
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
            trash: &FakeTrash::unavailable("no-trash"),
            tools: Some(&[]),
        },
    )
    .unwrap();
    let outcome = report.items.iter().find(|o| o.item == id).unwrap();
    assert!(
        matches!(outcome.status, ItemStatus::Skipped { .. }),
        "{:?}",
        outcome.status
    );
    assert!(pile.path().join("Zelda/GAME.PROBE").is_file());
    assert!(pile.path().join("Zelda/d/a.bin").is_file());
}
