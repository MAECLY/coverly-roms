//! The size confirmation (systems batch 3 D23): `large_transfer` counts
//! the bytes a plan writes to the target (copy mode, a move across
//! volumes, extractions; never a same-volume rename, never a duplicate
//! set aside) and answers when they reach 10 GB; the plan carries it as
//! an informational warning; a review choice can cross the line, so the
//! gate runs on the decided plan. Big sizes are faked through the walk's
//! `MetaProbe::file_len` hook, never written: a folder holding
//! `GAME.PROBE` is a `gb` game (the test probe), `OTHER.PROBE` also makes
//! it a `gba` one (two systems: review).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use coverly_core::cache::Cache;
use coverly_core::consoles::{
    add_extra_probe, set_extra_probe, ConsoleId, DirListing, DirProbe, FolderMatch, IdSource,
};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::dedupe::{KeepReason, Relation};
use coverly_core::organize::apply::set_free_space_override;
use coverly_core::organize::{
    large_transfer, plan_from_survey, survey, ArchiveAfter, Choice, Decision, ExtractSource,
    ItemKind, LargeTransfer, MetaProbe, OrganizeAction, OrganizeEnv, OrganizeOptions, OrganizePlan,
    PlanWarning, SheetCheck, Survey, Transfer, TransferMode, LARGE_TRANSFER,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

const GB: u64 = 1_000_000_000;

/// Answers a faked length for files named `<n>gb.bin` (`n` decimal GB,
/// `9.99gb.bin` for 9.99 GB, `10-16b.gb.bin` for 10 GB less the 16 bytes
/// of the folder's probe file); the metadata length otherwise.
struct Probe;

impl MetaProbe for Probe {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }

    fn file_len(&self, path: &Path, meta: &fs::Metadata) -> u64 {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        match name.strip_suffix("gb.bin") {
            Some("9.99") => 9_990_000_000,
            Some("10-16b.") => 10 * GB - 16,
            Some(n) => n.parse::<u64>().map_or(meta.len(), |n| n * GB),
            None => meta.len(),
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

fn gb_probe(listing: &DirListing) -> Option<FolderMatch> {
    has(listing, "GAME.PROBE").then(|| FolderMatch {
        id: id("gb"),
        probe: PathBuf::from("GAME.PROBE"),
        leave: None,
    })
}

fn gba_probe(listing: &DirListing) -> Option<FolderMatch> {
    has(listing, "OTHER.PROBE").then(|| FolderMatch {
        id: id("gba"),
        probe: PathBuf::from("OTHER.PROBE"),
        leave: None,
    })
}

/// Sets both probes on this thread and clears every hook on drop.
struct Hooks;

impl Hooks {
    fn new() -> Hooks {
        set_extra_probe(Some(gb_probe as DirProbe));
        add_extra_probe(gba_probe as DirProbe);
        Hooks
    }
}

impl Drop for Hooks {
    fn drop(&mut self) {
        set_extra_probe(None);
        set_free_space_override(None);
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

/// Writes each relative path as 16 zero bytes, mtimes pinned.
fn put_tree(root: &Path, files: &[&str]) {
    for (i, rel) in files.iter().enumerate() {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, [0u8; 16]).unwrap();
        pin_mtime(&path, i as u64);
    }
}

fn options(target: &Path, mode: TransferMode) -> OrganizeOptions {
    OrganizeOptions {
        layout: "esde".into(),
        target: Some(target.to_path_buf()),
        mode: Some(mode),
        offline: true,
        ..OrganizeOptions::default()
    }
}

fn surveyed(world: &World, pile: &Path, opts: &OrganizeOptions) -> Survey {
    survey(pile, opts, &world.env(), &|_| {}, &AtomicBool::new(false)).unwrap()
}

fn plan(world: &World, pile: &Path, target: &Path, mode: TransferMode) -> OrganizePlan {
    let opts = options(target, mode);
    let found = surveyed(world, pile, &opts);
    plan_from_survey(&found, &opts, &[], &world.env()).unwrap()
}

fn folder_named<'a>(p: &'a OrganizePlan, name: &str) -> &'a coverly_core::organize::OrganizeItem {
    p.items
        .iter()
        .find(|i| i.kind == ItemKind::Folder && i.sources[0] == Path::new(name))
        .unwrap_or_else(|| panic!("no folder {name}: {:#?}", p.items))
}

fn last(path: &Path) -> String {
    path.components()
        .next_back()
        .unwrap()
        .as_os_str()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn the_threshold_is_ten_decimal_gigabytes() {
    assert_eq!(LARGE_TRANSFER, 10_000_000_000);
}

#[test]
fn a_copy_of_twelve_gigabytes_is_large() {
    let _h = Hooks::new();
    set_free_space_override(Some(|_| Some(412_100_000_000)));
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(pile.path(), &["Big/GAME.PROBE", "Big/12gb.bin"]);
    let p = plan(&world, pile.path(), target.path(), TransferMode::Copy);
    let big = folder_named(&p, "Big");
    assert_eq!(big.action, OrganizeAction::Auto);
    assert_eq!(big.size, 12 * GB + 16);
    let t = large_transfer(&p).expect("12 GB copy is large");
    assert_eq!(
        t,
        LargeTransfer {
            bytes: 12 * GB + 16,
            items: 1,
            copies: true,
            free: Some(412_100_000_000),
            volume: last(&p.target_root),
        }
    );
    assert!(p.warnings.contains(&PlanWarning::LargeTransfer {
        bytes: 12 * GB + 16,
        items: 1,
    }));
}

#[test]
fn unknown_free_space_is_none() {
    let _h = Hooks::new();
    set_free_space_override(Some(|_| None));
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(pile.path(), &["Big/GAME.PROBE", "Big/12gb.bin"]);
    let p = plan(&world, pile.path(), target.path(), TransferMode::Copy);
    assert_eq!(large_transfer(&p).unwrap().free, None);
}

#[test]
fn a_move_on_the_same_volume_is_a_rename_and_counts_nothing() {
    let _h = Hooks::new();
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(pile.path(), &["Big/GAME.PROBE", "Big/48gb.bin"]);
    let p = plan(&world, pile.path(), target.path(), TransferMode::Move);
    assert_eq!(p.mode, TransferMode::Move);
    assert_eq!(folder_named(&p, "Big").action, OrganizeAction::Auto);
    assert_eq!(large_transfer(&p), None);
    assert!(!p
        .warnings
        .iter()
        .any(|w| matches!(w, PlanWarning::LargeTransfer { .. })));
}

#[test]
fn just_under_ten_gigabytes_is_not_large() {
    let _h = Hooks::new();
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(pile.path(), &["Big/GAME.PROBE", "Big/9.99gb.bin"]);
    let p = plan(&world, pile.path(), target.path(), TransferMode::Copy);
    assert_eq!(folder_named(&p, "Big").size, 9_990_000_016);
    assert_eq!(large_transfer(&p), None);
    assert!(!p
        .warnings
        .iter()
        .any(|w| matches!(w, PlanWarning::LargeTransfer { .. })));
}

#[test]
fn exactly_ten_gigabytes_is_large() {
    let _h = Hooks::new();
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(pile.path(), &["Big/GAME.PROBE", "Big/10-16b.gb.bin"]);
    let p = plan(&world, pile.path(), target.path(), TransferMode::Copy);
    assert_eq!(folder_named(&p, "Big").size, LARGE_TRANSFER);
    let t = large_transfer(&p).expect("10 GB on the dot is large");
    assert_eq!((t.bytes, t.items), (LARGE_TRANSFER, 1));
    assert!(p.warnings.contains(&PlanWarning::LargeTransfer {
        bytes: LARGE_TRANSFER,
        items: 1,
    }));
}

#[test]
fn a_duplicate_set_aside_is_not_counted() {
    let _h = Hooks::new();
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(
        pile.path(),
        &["A/GAME.PROBE", "A/6gb.bin", "B/GAME.PROBE", "B/6gb.bin"],
    );
    let mut p = plan(&world, pile.path(), target.path(), TransferMode::Copy);
    assert_eq!(large_transfer(&p).unwrap().items, 2);
    // B set aside as a duplicate of A: it stays on the pile's volume.
    let b = p
        .items
        .iter_mut()
        .find(|i| i.sources[0] == Path::new("B"))
        .unwrap();
    b.action = OrganizeAction::SetAside {
        keeper: PathBuf::from("gb/A.gbdir"),
        relation: Relation::Exact,
        reason: KeepReason::UserChoice,
        dest: PathBuf::from("_Duplicados/B"),
    };
    assert_eq!(large_transfer(&p), None);
}

#[test]
fn an_extraction_counts_its_extracted_bytes_even_when_moving() {
    let _h = Hooks::new();
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(pile.path(), &["Small/GAME.PROBE"]);
    let mut p = plan(&world, pile.path(), target.path(), TransferMode::Move);
    assert_eq!(large_transfer(&p), None);
    // As if the item were an archive whose entries extract to 11 GB.
    let item = p
        .items
        .iter_mut()
        .find(|i| i.sources[0] == Path::new("Small"))
        .unwrap();
    item.transfer = Transfer::Extract {
        entries: Vec::new(),
        bytes: 11 * GB,
        archive_after: ArchiveAfter::Keep,
        source: ExtractSource::Builtin,
        sheet_check: SheetCheck::default(),
        m3u: None,
    };
    let t = large_transfer(&p).expect("11 GB extracted");
    assert_eq!((t.bytes, t.items, t.copies), (11 * GB, 1, false));
}

#[test]
fn a_review_choice_that_crosses_the_line_makes_the_decided_plan_large() {
    let _h = Hooks::new();
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(
        pile.path(),
        &[
            "Five/GAME.PROBE",
            "Five/5gb.bin",
            "Six/GAME.PROBE",
            "Six/OTHER.PROBE",
            "Six/6gb.bin",
        ],
    );
    let opts = options(target.path(), TransferMode::Copy);
    let found = surveyed(&world, pile.path(), &opts);
    let before = plan_from_survey(&found, &opts, &[], &world.env()).unwrap();
    let six = folder_named(&before, "Six");
    assert_eq!(six.action, OrganizeAction::Review);
    // An undecided review item is not written: 5 GB only.
    assert_eq!(large_transfer(&before), None);
    let decisions = [Decision {
        item: six.id,
        choice: Choice::Assign {
            system: "gb".into(),
        },
    }];
    let decided = plan_from_survey(&found, &opts, &decisions, &world.env()).unwrap();
    assert_eq!(folder_named(&decided, "Six").action, OrganizeAction::Auto);
    let t = large_transfer(&decided).expect("5 + 6 GB once decided");
    assert_eq!((t.bytes, t.items), (11 * GB + 48, 2));
}
