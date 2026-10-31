//! Folder games (systems batch 2, D12, D13, D25, D27): a folder a probe
//! proves is one `Folder` item, measured from metadata only, gated per
//! contained file, named with the layout's suffix and never merged into a
//! taken folder; apply moves it with one rename on a volume, or copies it
//! file by file (verified, journaled, undoable, resumable) across volumes.
//! The probe is the test hook of plan P-7: a folder holding `GAME.PROBE`
//! is a `gb` game.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use coverly_core::cache::Cache;
use coverly_core::consoles::{
    add_extra_probe, set_extra_probe, ConsoleId, DirListing, DirProbe, FolderMatch, IdSource,
};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::organize::target::{set_fat32_override, set_windows_paths_override};
use coverly_core::organize::{
    apply_organize, plan_organize, resume_organize, ApplyEnv, ItemKind, ItemNote, ItemStatus,
    LeftReason, MetaProbe, OrganizeAction, OrganizeEnv, OrganizeItem, OrganizeOptions,
    OrganizePhase, OrganizePlan, OrganizeProgress, OrganizeReport, RenameReason, SkipReason,
    TransferMode,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::FakeTrash;
use coverly_core::write::{job_transfers, set_copy_fault, undo_job, CopyStage};
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

/// Applies `p` with the data folder `data`; `progress` sees every event.
fn apply_with(
    p: &OrganizePlan,
    data: &Path,
    progress: &(dyn Fn(OrganizeProgress) + Sync),
) -> OrganizeReport {
    apply_organize(
        p,
        &ApplyEnv {
            data_dir: data,
            progress,
            cancel: &AtomicBool::new(false),
            trash: &FakeTrash::unavailable("no-trash"),
            tools: Some(&[]),
        },
    )
    .unwrap()
}

fn apply(p: &OrganizePlan, data: &Path) -> OrganizeReport {
    apply_with(p, data, &|_| {})
}

fn resume(job: &str, data: &Path) -> OrganizeReport {
    resume_organize(
        job,
        &ApplyEnv {
            data_dir: data,
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
            trash: &FakeTrash::unavailable("no-trash"),
            tools: Some(&[]),
        },
    )
    .unwrap()
}

fn status(report: &OrganizeReport, id: usize) -> &ItemStatus {
    &report.items.iter().find(|o| o.item == id).unwrap().status
}

/// The journal of `job`, as text.
fn journal(data: &Path, job: &str) -> String {
    fs::read_to_string(data.join("jobs").join(format!("{job}.jsonl"))).unwrap()
}

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

/// Every file below `dir` (relative, by component), with its size and
/// modification time.
fn files_below(dir: &Path) -> Vec<(Vec<String>, u64, SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap().flatten() {
            let meta = entry.metadata().unwrap();
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                let rel = entry
                    .path()
                    .strip_prefix(dir)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push((rel, meta.len(), meta.modified().unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// The cross-device error a rename across volumes gives.
fn exdev() -> io::Error {
    io::Error::from_raw_os_error(if cfg!(windows) { 17 } else { 18 })
}

/// Clears the copy fault on drop (a failed assert must not leak it).
struct FaultGuard;

impl Drop for FaultGuard {
    fn drop(&mut self) {
        set_copy_fault(None);
    }
}

fn cross_volume(stage: CopyStage, _: &Path) -> Option<io::Error> {
    (stage == CopyStage::Rename).then(exdev)
}

const ZELDA: &[(&str, u64)] = &[
    ("Zelda/GAME.PROBE", 16),
    ("Zelda/d/a.bin", 3000),
    ("Zelda/d/b.bin", 5000),
    ("Zelda/d/deep/c.bin", 7),
];

#[test]
fn same_volume_folder_move_is_one_rename_and_undoes() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let before = files_below(&pile.path().join("Zelda"));
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let data = TempDir::new().unwrap();
    let report = apply(&p, data.path());
    assert_eq!(status(&report, item.id), &ItemStatus::Moved);
    let at = target.path().join(item.target.as_ref().unwrap());
    let outcome = report.items.iter().find(|o| o.item == item.id).unwrap();
    assert_eq!(outcome.targets, std::slice::from_ref(&at));
    assert!(at.join("d").join("deep").join("c.bin").is_file());
    assert!(!pile.path().join("Zelda").exists());
    assert_eq!(files_below(&at), before);
    let job = report.job.clone().unwrap();
    let text = journal(data.path(), &job);
    assert_eq!(count(&text, "\"kind\":\"dir-moved\""), 1, "{text}");
    assert_eq!(count(&text, "\"via\":\"copy\""), 0, "{text}");

    let undo = undo_job(data.path(), &job).unwrap();
    assert_eq!(undo.restored, [pile.path().join("Zelda")]);
    assert_eq!(files_below(&pile.path().join("Zelda")), before);
    assert!(!at.exists());
    assert!(!at.parent().unwrap().exists(), "the system folder it made");
}

#[test]
fn undo_leaves_a_moved_folder_that_changed_since() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let data = TempDir::new().unwrap();
    let report = apply(&p, data.path());
    let at = target.path().join(item.target.as_ref().unwrap());
    fs::write(at.join("save.sav"), b"SAVE").unwrap();
    let undo = undo_job(data.path(), report.job.as_deref().unwrap()).unwrap();
    assert!(undo.restored.is_empty(), "{undo:?}");
    assert_eq!(undo.skipped_modified, std::slice::from_ref(&at));
    assert!(at.join("save.sav").is_file());
    assert!(!pile.path().join("Zelda").exists());
}

#[test]
fn undo_never_renames_onto_a_taken_source() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let data = TempDir::new().unwrap();
    let report = apply(&p, data.path());
    let at = target.path().join(item.target.as_ref().unwrap());
    fs::create_dir(pile.path().join("Zelda")).unwrap();
    let undo = undo_job(data.path(), report.job.as_deref().unwrap()).unwrap();
    assert_eq!(undo.skipped_occupied, std::slice::from_ref(&at));
    assert!(at.join("GAME.PROBE").is_file());
    assert_eq!(fs::read_dir(pile.path().join("Zelda")).unwrap().count(), 0);
}

#[test]
fn cross_volume_copy_is_verified_per_file() {
    let _g = ProbeGuard::new();
    let _f = FaultGuard;
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let before = files_below(&pile.path().join("Zelda"));
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let data = TempDir::new().unwrap();
    set_copy_fault(Some(cross_volume));
    let report = apply(&p, data.path());
    assert_eq!(status(&report, item.id), &ItemStatus::Moved);
    let at = target.path().join(item.target.as_ref().unwrap());
    // Every file, with its size and modification time; sources gone.
    assert_eq!(files_below(&at), before);
    assert!(!pile.path().join("Zelda").exists());
    let job = report.job.clone().unwrap();
    let records = job_transfers(data.path(), &job).unwrap();
    let finished: Vec<_> = records
        .iter()
        .filter(|r| r.finished && !r.copied && r.fp.is_some_and(|fp| fp.crc32.is_some()))
        .collect();
    assert_eq!(finished.len(), 4, "{records:#?}");
    for r in &finished {
        assert!(r.from.starts_with(pile.path().join("Zelda")));
        assert!(r.target.starts_with(&at));
    }
    let text = journal(data.path(), &job);
    // `ROMs/`, the system folder, the game folder and its two sub-folders.
    assert_eq!(count(&text, "\"kind\":\"dir-created\""), 5, "{text}");

    set_copy_fault(Some(cross_volume));
    let undo = undo_job(data.path(), &job).unwrap();
    set_copy_fault(None);
    assert_eq!(undo.restored.len(), 4, "{undo:?}");
    assert_eq!(files_below(&pile.path().join("Zelda")), before);
    assert!(!at.exists());
}

fn fail_verify_of_b(stage: CopyStage, path: &Path) -> Option<io::Error> {
    match stage {
        CopyStage::Rename => Some(exdev()),
        CopyStage::Verify if path.ends_with("b.bin") => Some(io::Error::new(
            io::ErrorKind::InvalidData,
            "copy verification failed",
        )),
        _ => None,
    }
}

#[test]
fn verify_fault_mid_tree_deletes_nothing() {
    let _g = ProbeGuard::new();
    let _f = FaultGuard;
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let before = files_below(&pile.path().join("Zelda"));
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let data = TempDir::new().unwrap();
    set_copy_fault(Some(fail_verify_of_b));
    let report = apply(&p, data.path());
    set_copy_fault(None);
    assert!(
        matches!(status(&report, item.id), ItemStatus::Failed { .. }),
        "{report:?}"
    );
    assert_eq!(files_below(&pile.path().join("Zelda")), before);
    let at = target.path().join(item.target.as_ref().unwrap());
    // The copies made so far are rolled back: no half game in the library.
    assert!(!at.exists());
    let undo = undo_job(data.path(), report.job.as_deref().unwrap()).unwrap();
    assert!(undo.restored.is_empty() && undo.skipped_modified.is_empty());
    assert_eq!(files_below(&pile.path().join("Zelda")), before);
}

#[test]
fn junk_only_leftover_is_removed() {
    let _g = ProbeGuard::new();
    let _f = FaultGuard;
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    put_tree(
        pile.path(),
        &[
            ("Zelda/.DS_Store", 6),
            ("Zelda/d/._a.bin", 4),
            ("Zelda/d/Thumbs.db", 2),
        ],
    );
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let data = TempDir::new().unwrap();
    set_copy_fault(Some(cross_volume));
    let report = apply(&p, data.path());
    assert_eq!(status(&report, item.id), &ItemStatus::Moved);
    assert!(!pile.path().join("Zelda").exists());
    let at = target.path().join(item.target.as_ref().unwrap());
    let names: Vec<_> = files_below(&at).into_iter().map(|f| f.0).collect();
    assert_eq!(names.len(), 4, "junk is never copied: {names:?}");
}

/// Creates `Zelda/d/new.sav` while `a.bin`'s source is deleted: a file the
/// user put in the folder during the copy.
fn a_file_appears(stage: CopyStage, path: &Path) -> Option<io::Error> {
    match stage {
        CopyStage::Rename => Some(exdev()),
        CopyStage::Delete if path.ends_with("a.bin") => {
            fs::write(path.with_file_name("new.sav"), b"NEW").unwrap();
            None
        }
        _ => None,
    }
}

#[test]
fn another_leftover_keeps_the_source_folder() {
    let _g = ProbeGuard::new();
    let _f = FaultGuard;
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let data = TempDir::new().unwrap();
    set_copy_fault(Some(a_file_appears));
    let report = apply(&p, data.path());
    set_copy_fault(None);
    assert_eq!(status(&report, item.id), &ItemStatus::CopiedNotRemoved);
    let at = target.path().join(item.target.as_ref().unwrap());
    assert_eq!(files_below(&at).len(), 4);
    assert_eq!(
        fs::read(pile.path().join("Zelda").join("d").join("new.sav")).unwrap(),
        b"NEW"
    );
    assert!(!pile.path().join("Zelda").join("d").join("a.bin").exists());
    assert!(!pile.path().join("Zelda").join("GAME.PROBE").exists());
}

/// Like [`a_file_appears`], with the new file's AppleDouble sidecar (its
/// Finder metadata on a FAT card).
fn a_file_and_its_sidecar_appear(stage: CopyStage, path: &Path) -> Option<io::Error> {
    if stage == CopyStage::Delete && path.ends_with("a.bin") {
        fs::write(path.with_file_name("._new.sav"), b"META").unwrap();
    }
    a_file_appears(stage, path)
}

#[test]
fn a_kept_source_folder_keeps_the_sidecars_of_what_stays() {
    let _g = ProbeGuard::new();
    let _f = FaultGuard;
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let data = TempDir::new().unwrap();
    set_copy_fault(Some(a_file_and_its_sidecar_appear));
    let report = apply(&p, data.path());
    set_copy_fault(None);
    assert_eq!(status(&report, item.id), &ItemStatus::CopiedNotRemoved);
    let d = pile.path().join("Zelda").join("d");
    assert_eq!(fs::read(d.join("new.sav")).unwrap(), b"NEW");
    // The folder stays, so its litter is not cleaned: the sidecar of the
    // file that stays is that file's metadata.
    assert_eq!(fs::read(d.join("._new.sav")).unwrap(), b"META");
}

#[test]
fn copy_mode_keeps_the_source() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let before = files_below(&pile.path().join("Zelda"));
    let mut opts = options(target.path(), "esde");
    opts.mode = Some(TransferMode::Copy);
    let p = plan_organize(
        pile.path(),
        &opts,
        &world.env(),
        &|_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    let item = only_folder(&p);
    let data = TempDir::new().unwrap();
    let report = apply(&p, data.path());
    assert_eq!(status(&report, item.id), &ItemStatus::Copied);
    let at = target.path().join(item.target.as_ref().unwrap());
    assert_eq!(files_below(&at), before);
    assert_eq!(files_below(&pile.path().join("Zelda")), before);
    let text = journal(data.path(), report.job.as_deref().unwrap());
    assert_eq!(count(&text, "\"kind\":\"dir-moved\""), 0, "{text}");

    let undo = undo_job(data.path(), report.job.as_deref().unwrap()).unwrap();
    assert_eq!(undo.removed.len(), 4, "{undo:?}");
    assert!(!at.exists());
    assert_eq!(files_below(&pile.path().join("Zelda")), before);
}

static FULL: AtomicBool = AtomicBool::new(false);

/// Across volumes; while `FULL` is set, the copy of `b.bin` fails as a full
/// card would (the run pauses, as after a crash between files).
fn full_on_b(stage: CopyStage, path: &Path) -> Option<io::Error> {
    match stage {
        CopyStage::Rename => Some(exdev()),
        CopyStage::Verify if path.ends_with("b.bin") && FULL.load(Ordering::SeqCst) => {
            Some(io::Error::from(io::ErrorKind::StorageFull))
        }
        _ => None,
    }
}

#[test]
fn resume_after_a_stop_between_files() {
    let _g = ProbeGuard::new();
    let _f = FaultGuard;
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let before = files_below(&pile.path().join("Zelda"));
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let at = target.path().join(item.target.as_ref().unwrap());
    let data = TempDir::new().unwrap();
    FULL.store(true, Ordering::SeqCst);
    set_copy_fault(Some(full_on_b));
    let first = apply(&p, data.path());
    FULL.store(false, Ordering::SeqCst);
    assert!(first.paused.is_some(), "{first:?}");
    assert_eq!(first.resumable_items, [item.id]);
    // Nothing deleted yet; what was copied stays for the resume.
    assert_eq!(files_below(&pile.path().join("Zelda")), before);
    let a = at.join("d").join("a.bin");
    assert!(a.is_file());
    let job = first.job.clone().unwrap();

    let second = resume(&job, data.path());
    set_copy_fault(None);
    assert_eq!(status(&second, item.id), &ItemStatus::Moved, "{second:?}");
    assert_eq!(files_below(&at), before);
    assert!(!pile.path().join("Zelda").exists());
    // `a.bin` was copied once: one finished record for it.
    let done_a = job_transfers(data.path(), &job)
        .unwrap()
        .into_iter()
        .filter(|r| r.target == a && r.finished)
        .count();
    assert_eq!(done_a, 1);
    assert!(!at.parent().unwrap().join("Zelda (2)").exists());

    // One undo reverts both runs.
    set_copy_fault(Some(cross_volume));
    let undo = undo_job(data.path(), &job).unwrap();
    set_copy_fault(None);
    assert_eq!(undo.restored.len(), 4, "{undo:?}");
    assert_eq!(files_below(&pile.path().join("Zelda")), before);
}

#[test]
fn a_finished_folder_is_not_moved_again_on_resume() {
    let _g = ProbeGuard::new();
    let _f = FaultGuard;
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    put_tree(pile.path(), &[("Tetris.gb", 32_768)]);
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let data = TempDir::new().unwrap();
    set_copy_fault(Some(cross_volume));
    let first = apply(&p, data.path());
    assert_eq!(status(&first, item.id), &ItemStatus::Moved);
    let job = first.job.clone().unwrap();
    let second = resume(&job, data.path());
    assert_eq!(status(&second, item.id), &ItemStatus::Moved, "{second:?}");
    let at = target.path().join(item.target.as_ref().unwrap());
    let outcome = second.items.iter().find(|o| o.item == item.id).unwrap();
    assert_eq!(outcome.targets, [at]);
}

#[test]
fn progress_reports_bytes_inside_the_item() {
    let _g = ProbeGuard::new();
    let _f = FaultGuard;
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let data = TempDir::new().unwrap();
    let seen: Mutex<Vec<(usize, u64, u64)>> = Mutex::new(Vec::new());
    set_copy_fault(Some(cross_volume));
    let progress = |e: OrganizeProgress| {
        if let OrganizePhase::Folder {
            item,
            bytes_done,
            bytes_total,
        } = e.phase
        {
            seen.lock().unwrap().push((item, bytes_done, bytes_total));
        }
    };
    let report = apply_with(&p, data.path(), &progress);
    assert_eq!(status(&report, item.id), &ItemStatus::Moved);
    let seen = seen.into_inner().unwrap();
    assert!(seen.len() >= 2, "{seen:?}");
    assert!(seen.iter().all(|s| s.0 == item.id && s.2 == item.size));
    assert_eq!(seen.first().unwrap().1, 0);
    assert_eq!(seen.last().unwrap().1, item.size);
    assert!(seen.windows(2).all(|w| w[0].1 <= w[1].1), "{seen:?}");
}

#[test]
fn a_target_taken_at_apply_time_gets_the_next_number() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    let at = target.path().join(item.target.as_ref().unwrap());
    // Someone else's empty folder appears after the plan: never merged,
    // never replaced.
    fs::create_dir_all(&at).unwrap();
    let data = TempDir::new().unwrap();
    let report = apply(&p, data.path());
    assert_eq!(status(&report, item.id), &ItemStatus::Moved);
    let outcome = report.items.iter().find(|o| o.item == item.id).unwrap();
    assert_eq!(last(&outcome.targets[0]), "Zelda (2)");
    assert_eq!(fs::read_dir(&at).unwrap().count(), 0);
    let undo = undo_job(data.path(), report.job.as_deref().unwrap()).unwrap();
    assert_eq!(undo.restored, [pile.path().join("Zelda")]);
    assert!(at.is_dir(), "the other folder stays");
}
