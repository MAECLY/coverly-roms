//! Huge items (adjustments spec D15–D17): an item that alone writes
//! 10 GB or more to the target, or a folder game extracted out of an
//! archive at any size, is listed in `large_transfer().huge` with the
//! bytes of the shadPS4 overlays that follow it; `rest` counts every other
//! item; `gate` accepts a confirmation only when it names every huge item
//! of the decided plan and at least its bytes; «No» is `Choice::Decline`
//! (`LeftReason::NotConfirmed`); `check_space` counts extraction bytes in
//! every mode. Big sizes are faked through `MetaProbe::file_len`, never
//! written: a file named `<n>gb.bin` is `n` decimal GB; a folder holding
//! `GAME.PROBE` is a `gb` game (the test probe), `OTHER.PROBE` also makes
//! it a `gba` one (two systems: review).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use coverly_core::cache::Cache;
use coverly_core::consoles::testing::{param_sfo_with, ps4_app, put_file};
use coverly_core::consoles::{
    add_extra_probe, set_extra_probe, ConsoleId, DirListing, DirProbe, FolderMatch, IdSource,
};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::organize::apply::set_free_space_override;
use coverly_core::organize::{
    apply_organize, gate, large_transfer, plan_from_survey, survey, ApplyEnv, ApplyError,
    ArchiveAfter, Choice, Decision, ExtractSource, HugeItem, ItemKind, LargeConfirm, LeftReason,
    MetaProbe, OrganizeAction, OrganizeEnv, OrganizeItem, OrganizeOptions, OrganizePlan,
    PlanWarning, SheetCheck, Survey, Transfer, TransferMode,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::FakeTrash;
use tempfile::TempDir;

const GB: u64 = 1_000_000_000;
const BLOODBORNE: &str = "UP9000-CUSA00900_00-BLOODBORNE000000";

/// Answers `n` GB for files named `<n>gb.bin` or `<tag>_<n>gb.bin` (the
/// tag keeps equal trees apart), the metadata length otherwise.
struct Probe;

impl MetaProbe for Probe {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }

    fn file_len(&self, path: &Path, meta: &fs::Metadata) -> u64 {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        name.strip_suffix("gb.bin")
            .and_then(|n| n.rsplit('_').next()?.parse::<u64>().ok())
            .map_or(meta.len(), |n| n * GB)
    }
}

fn has(listing: &DirListing, name: &str) -> bool {
    listing.names.iter().any(|n| n.eq_ignore_ascii_case(name))
}

/// A distinct id per folder, so equal ids never make title duplicates.
fn id(system: &str, name: &str) -> ConsoleId {
    let mut id = ConsoleId::proven(system, "test probe");
    id.serial = Some(name.to_ascii_uppercase());
    id.title_id = Some(name.to_ascii_uppercase());
    id.id_source = Some(IdSource::Header);
    id
}

fn gb_probe(listing: &DirListing) -> Option<FolderMatch> {
    has(listing, "GAME.PROBE").then(|| FolderMatch {
        id: id("gb", listing.name),
        probe: PathBuf::from("GAME.PROBE"),
        leave: None,
    })
}

fn gba_probe(listing: &DirListing) -> Option<FolderMatch> {
    has(listing, "OTHER.PROBE").then(|| FolderMatch {
        id: id("gba", listing.name),
        probe: PathBuf::from("OTHER.PROBE"),
        leave: None,
    })
}

/// Bytes free on every target unless a test says otherwise.
const FREE: u64 = 900 * GB;

/// Sets both probes and a fixed free space on this thread and clears
/// every hook on drop.
struct Hooks;

impl Hooks {
    fn new() -> Hooks {
        set_extra_probe(Some(gb_probe as DirProbe));
        add_extra_probe(gba_probe as DirProbe);
        set_free_space_override(Some(|_| Some(FREE)));
        Hooks
    }
}

impl Drop for Hooks {
    fn drop(&mut self) {
        set_extra_probe(None);
        set_free_space_override(None);
    }
}

/// The plan's clock: well after every pinned fixture mtime.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000 + 100_000)
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
            titles: None,
            now: now(),
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

/// Pins every file below `dir` (the PS4 builders write "now").
fn pin_all(dir: &Path) {
    for e in fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        if path.is_dir() {
            pin_all(&path);
        } else {
            pin_mtime(&path, 1);
        }
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

fn by<'a>(p: &'a OrganizePlan, name: &str) -> &'a OrganizeItem {
    p.items
        .iter()
        .find(|i| i.sources[0] == Path::new(name))
        .unwrap_or_else(|| panic!("no {name}: {:#?}", p.items))
}

fn confirm(bytes: u64, items: &[usize]) -> LargeConfirm {
    LargeConfirm {
        bytes,
        items: items.to_vec(),
    }
}

#[test]
fn a_single_twelve_gigabyte_copy_is_huge() {
    let _h = Hooks::new();
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(pile.path(), &["Big/GAME.PROBE", "Big/12gb.bin"]);
    let p = plan(&world, pile.path(), target.path(), TransferMode::Copy);
    let big = by(&p, "Big");
    let t = large_transfer(&p).expect("12 GB copy is large");
    assert_eq!(
        t.huge,
        [HugeItem {
            item: big.id,
            name: "Big".into(),
            system: "gb".into(),
            bytes: 12 * GB + 16,
            extract: false,
            followers: Vec::new(),
        }]
    );
    assert_eq!((t.bytes, t.rest), (12 * GB + 16, 0));
}

#[test]
fn a_nine_gigabyte_item_in_a_fifteen_gigabyte_plan_is_not_huge() {
    let _h = Hooks::new();
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(
        pile.path(),
        &[
            "Nine/GAME.PROBE",
            "Nine/9gb.bin",
            "Six/GAME.PROBE",
            "Six/6gb.bin",
        ],
    );
    let p = plan(&world, pile.path(), target.path(), TransferMode::Copy);
    let t = large_transfer(&p).expect("15 GB in all");
    assert!(t.huge.is_empty(), "{t:#?}");
    assert_eq!((t.bytes, t.rest, t.items), (15 * GB + 32, 15 * GB + 32, 2));
    // The aggregate alone: no item to confirm by id.
    assert_eq!(gate(&p, Some(&confirm(t.bytes, &[]))), Ok(()));
    assert_eq!(gate(&p, None), Err(t));
}

#[test]
fn three_forty_gigabyte_folders_and_twenty_gigabytes_of_rest() {
    let _h = Hooks::new();
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let mut files = Vec::new();
    for name in ["A", "B", "C"] {
        files.push(format!("{name}/GAME.PROBE"));
        files.push(format!("{name}/{name}_40gb.bin"));
    }
    for name in ["w", "x", "y", "z"] {
        files.push(format!("{name}/GAME.PROBE"));
        files.push(format!("{name}/{name}_5gb.bin"));
    }
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    put_tree(pile.path(), &refs);
    let p = plan(&world, pile.path(), target.path(), TransferMode::Copy);
    let t = large_transfer(&p).unwrap();
    let names: Vec<&str> = t.huge.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["A", "B", "C"]);
    assert!(t.huge.iter().all(|h| h.bytes == 40 * GB + 16));
    assert_eq!(t.rest, 20 * GB + 4 * 16);
    assert_eq!(t.bytes, t.rest + 3 * (40 * GB + 16));
}

#[test]
fn a_folder_game_extracted_out_of_an_archive_is_huge_at_any_size() {
    let _h = Hooks::new();
    let world = World::new();
    let (pile, target) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let game = TempDir::new().unwrap();
    ps4_app(game.path(), "CUSA00900", "gd", BLOODBORNE);
    let zip_path = pile.path().join("Bloodborne.zip");
    {
        let file = fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        fn add(
            zip: &mut zip::ZipWriter<fs::File>,
            opts: zip::write::SimpleFileOptions,
            root: &Path,
            dir: &Path,
        ) {
            for e in fs::read_dir(dir).unwrap() {
                let path = e.unwrap().path();
                if path.is_dir() {
                    add(zip, opts, root, &path);
                    continue;
                }
                let rel: Vec<String> = path
                    .strip_prefix(root)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                zip.start_file(format!("CUSA00900/{}", rel.join("/")), opts)
                    .unwrap();
                zip.write_all(&fs::read(&path).unwrap()).unwrap();
            }
        }
        add(&mut zip, opts, game.path(), game.path());
        zip.finish().unwrap();
    }
    pin_mtime(&zip_path, 1);
    // A same-volume move: an as-is item would count nothing.
    let p = plan(&world, pile.path(), target.path(), TransferMode::Move);
    let item = by(&p, "Bloodborne.zip");
    let Transfer::Extract {
        bytes, folder_game, ..
    } = &item.transfer
    else {
        panic!("{item:#?}");
    };
    assert!(*folder_game);
    let t = large_transfer(&p).expect("a folder game extraction always asks");
    assert_eq!(
        t.huge,
        [HugeItem {
            item: item.id,
            name: "Bloodborne.zip".into(),
            system: "ps4".into(),
            bytes: *bytes,
            extract: true,
            followers: Vec::new(),
        }]
    );
    assert_eq!((t.bytes, t.rest), (*bytes, 0));
    // Under the threshold the plan carries no size warning.
    assert!(!p
        .warnings
        .iter()
        .any(|w| matches!(w, PlanWarning::LargeTransfer { .. })));
    assert_eq!(gate(&p, None), Err(t.clone()));
    assert_eq!(gate(&p, Some(&confirm(*bytes, &[item.id]))), Ok(()));
}

/// `CUSA00900` (6 GB) with its `-UPDATE` (3 GB, 01.09) and `-patch`
/// (2 GB, 01.10: another version, so no duplicate of the first) shadPS4
/// overlays.
fn ps4_with_overlays(pile: &Path) {
    ps4_app(&pile.join("CUSA00900"), "CUSA00900", "gd", BLOODBORNE);
    put_file(&pile.join("CUSA00900"), "6gb.bin", &[0u8; 16]);
    for (suffix, size) in [("-UPDATE", "3gb.bin"), ("-patch", "2gb.bin")] {
        let dir = pile.join(format!("CUSA00900{suffix}"));
        ps4_app(&dir, "CUSA00900", "gp", BLOODBORNE);
        put_file(&dir, size, &[0u8; 16]);
    }
    put_file(
        &pile.join("CUSA00900-patch"),
        "sce_sys/param.sfo",
        &param_sfo_with(
            &[
                ("APP_VER", "01.10"),
                ("CATEGORY", "gp"),
                ("CONTENT_ID", BLOODBORNE),
                ("TITLE", "Test Game"),
                ("TITLE_ID", "CUSA00900"),
                ("VERSION", "01.00"),
            ],
            &[("APP_TYPE", 1), ("PUBTOOLVER", 0x0289_0000)],
        ),
    );
    pin_all(pile);
}

#[test]
fn a_ps4_base_counts_the_overlays_that_follow_it() {
    let _h = Hooks::new();
    let world = World::new();
    let (pile, target) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    ps4_with_overlays(pile.path());
    let p = plan(&world, pile.path(), target.path(), TransferMode::Copy);
    let (base, update, patch) = (
        by(&p, "CUSA00900"),
        by(&p, "CUSA00900-UPDATE"),
        by(&p, "CUSA00900-patch"),
    );
    for item in [base, update, patch] {
        assert_eq!(item.action, OrganizeAction::Auto, "{item:#?}");
    }
    let t = large_transfer(&p).expect("6 + 3 + 2 GB");
    assert_eq!(t.huge.len(), 1, "{t:#?}");
    let huge = &t.huge[0];
    assert_eq!(huge.item, base.id);
    assert_eq!(huge.bytes, base.size + update.size + patch.size);
    let mut followers = huge.followers.clone();
    followers.sort_unstable();
    let mut want = vec![update.id, patch.id];
    want.sort_unstable();
    assert_eq!(followers, want);
    assert_eq!(t.rest, 0);
    assert_eq!(t.bytes, huge.bytes);
}

/// An overlay follows only a base that is written: under a base still
/// to review (apply skips it), a 12 GB overlay is asked on its own.
#[test]
fn an_overlay_under_an_undecided_base_is_asked_on_its_own() {
    let _h = Hooks::new();
    let world = World::new();
    let (pile, target) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    ps4_with_overlays(pile.path());
    let mut p = plan(&world, pile.path(), target.path(), TransferMode::Copy);
    for item in &mut p.items {
        match item.sources[0].to_str() {
            Some("CUSA00900") => item.action = OrganizeAction::Review,
            Some("CUSA00900-UPDATE") => item.size = 12 * GB,
            _ => {}
        }
    }
    let (update, patch) = (by(&p, "CUSA00900-UPDATE"), by(&p, "CUSA00900-patch"));
    let t = large_transfer(&p).expect("12 + 2 GB");
    assert_eq!(
        t.huge.iter().map(|h| (h.item, h.bytes)).collect::<Vec<_>>(),
        [(update.id, 12 * GB)],
        "{t:#?}"
    );
    assert!(t.huge[0].followers.is_empty());
    assert_eq!(t.rest, patch.size);
    assert_eq!(t.bytes, 12 * GB + patch.size);
}

#[test]
fn declining_a_base_leaves_it_not_confirmed_and_its_overlays_follow() {
    let _h = Hooks::new();
    let world = World::new();
    let (pile, target) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    ps4_with_overlays(pile.path());
    let opts = options(target.path(), TransferMode::Copy);
    let found = surveyed(&world, pile.path(), &opts);
    let first = plan_from_survey(&found, &opts, &[], &world.env()).unwrap();
    let base = by(&first, "CUSA00900").id;
    let decided = plan_from_survey(
        &found,
        &opts,
        &[Decision {
            item: base,
            choice: Choice::Decline,
        }],
        &world.env(),
    )
    .unwrap();
    assert_eq!(
        by(&decided, "CUSA00900").action,
        OrganizeAction::Left {
            reason: LeftReason::NotConfirmed
        }
    );
    for name in ["CUSA00900-UPDATE", "CUSA00900-patch"] {
        let item = by(&decided, name);
        assert!(
            matches!(
                item.action,
                OrganizeAction::Left {
                    reason: LeftReason::GameOverlay { .. }
                }
            ),
            "{item:#?}"
        );
    }
    // Nothing is written: nothing to confirm.
    assert_eq!(large_transfer(&decided), None);
    assert_eq!(gate(&decided, None), Ok(()));
}

#[test]
fn the_gate_needs_every_huge_item_and_at_least_the_bytes() {
    let _h = Hooks::new();
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(
        pile.path(),
        &[
            "Big/GAME.PROBE",
            "Big/12gb.bin",
            "Huge/GAME.PROBE",
            "Huge/20gb.bin",
            "Small/GAME.PROBE",
            "Small/3gb.bin",
        ],
    );
    let opts = options(target.path(), TransferMode::Copy);
    let found = surveyed(&world, pile.path(), &opts);
    let p = plan_from_survey(&found, &opts, &[], &world.env()).unwrap();
    let (big, huge) = (by(&p, "Big").id, by(&p, "Huge").id);
    let t = large_transfer(&p).unwrap();
    assert_eq!(t.huge.len(), 2);
    assert_eq!(t.rest, 3 * GB + 16);

    assert_eq!(gate(&p, Some(&confirm(t.bytes, &[huge, big]))), Ok(()));
    assert_eq!(gate(&p, Some(&confirm(t.bytes + 1, &[big, huge]))), Ok(()));
    assert_eq!(gate(&p, None), Err(t.clone()));
    // A huge item missing from the answers.
    assert_eq!(gate(&p, Some(&confirm(t.bytes, &[big]))), Err(t.clone()));
    // Fewer bytes than the plan writes.
    assert_eq!(
        gate(&p, Some(&confirm(t.bytes - 1, &[big, huge]))),
        Err(t.clone())
    );

    // «No» to Big: the decided plan no longer lists it, and the bytes
    // the UI sends are the rest plus the accepted item.
    let decided = plan_from_survey(
        &found,
        &opts,
        &[Decision {
            item: big,
            choice: Choice::Decline,
        }],
        &world.env(),
    )
    .unwrap();
    let after = large_transfer(&decided).unwrap();
    assert_eq!(after.huge.len(), 1);
    assert_eq!(after.huge[0].item, huge);
    let sent = t.rest + t.huge.iter().find(|h| h.item == huge).unwrap().bytes;
    assert_eq!(gate(&decided, Some(&confirm(sent, &[huge]))), Ok(()));
}

#[test]
fn a_plan_that_grew_after_the_answers_is_refused() {
    let _h = Hooks::new();
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(
        pile.path(),
        &[
            "Big/GAME.PROBE",
            "Big/12gb.bin",
            "Six/GAME.PROBE",
            "Six/OTHER.PROBE",
            "Six/6gb.bin",
        ],
    );
    let opts = options(target.path(), TransferMode::Copy);
    let found = surveyed(&world, pile.path(), &opts);
    let before = plan_from_survey(&found, &opts, &[], &world.env()).unwrap();
    let six = by(&before, "Six");
    assert_eq!(six.action, OrganizeAction::Review);
    let t = large_transfer(&before).unwrap();
    let answers = confirm(t.bytes, &[by(&before, "Big").id]);
    assert_eq!(gate(&before, Some(&answers)), Ok(()));
    let grown = plan_from_survey(
        &found,
        &opts,
        &[Decision {
            item: six.id,
            choice: Choice::Assign {
                system: "gb".into(),
            },
        }],
        &world.env(),
    )
    .unwrap();
    let fresh = large_transfer(&grown).unwrap();
    assert_eq!(fresh.bytes, t.bytes + 6 * GB + 32);
    assert_eq!(gate(&grown, Some(&answers)), Err(fresh));
}

#[test]
fn check_space_counts_a_same_volume_extraction() {
    let _h = Hooks::new();
    set_free_space_override(Some(|_| Some(GB)));
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(pile.path(), &["Game/GAME.PROBE"]);
    let mut p = plan(&world, pile.path(), target.path(), TransferMode::Move);
    assert_eq!(p.mode, TransferMode::Move);
    // As if the item were a zipped PS4 folder extracting 40 GB.
    let item = p
        .items
        .iter_mut()
        .find(|i| i.sources[0] == Path::new("Game"))
        .unwrap();
    item.transfer = Transfer::Extract {
        entries: Vec::new(),
        bytes: 40 * GB,
        archive_after: ArchiveAfter::Keep,
        source: ExtractSource::Builtin,
        sheet_check: SheetCheck::default(),
        m3u: None,
        folder_game: true,
    };
    let data = TempDir::new().unwrap();
    let err = apply_organize(
        &p,
        &ApplyEnv {
            data_dir: data.path(),
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
            trash: &FakeTrash::unavailable("no-trash"),
            tools: Some(&[]),
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, ApplyError::NoSpace { needed, free } if needed == 40 * GB + 400_000_000 && free == GB),
        "{err:?}"
    );
    // Nothing was written.
    assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
}

#[test]
fn check_space_still_skips_a_same_volume_rename() {
    let _h = Hooks::new();
    set_free_space_override(Some(|_| Some(1)));
    let (world, pile, target) = (
        World::new(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    put_tree(pile.path(), &["Game/GAME.PROBE", "Game/48gb.bin"]);
    let p = plan(&world, pile.path(), target.path(), TransferMode::Move);
    assert_eq!(by(&p, "Game").kind, ItemKind::Folder);
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
    );
    assert!(report.is_ok(), "{report:?}");
}

#[test]
fn a_saved_plan_from_before_the_adjustments_still_loads() {
    let old = serde_json::json!({
        "kind": "extract",
        "entries": [],
        "bytes": 5,
        "archive_after": "keep",
    });
    let t: Transfer = serde_json::from_value(old).unwrap();
    let Transfer::Extract { folder_game, .. } = &t else {
        panic!("{t:?}");
    };
    assert!(!folder_game);
    // And a plain extraction writes no new key.
    let back = serde_json::to_value(&t).unwrap();
    assert!(back.get("folder_game").is_none(), "{back}");
    let confirm: LargeConfirm =
        serde_json::from_value(serde_json::json!({"bytes": 7, "items": [1, 2]})).unwrap();
    assert_eq!(
        confirm,
        LargeConfirm {
            bytes: 7,
            items: vec![1, 2]
        }
    );
}
