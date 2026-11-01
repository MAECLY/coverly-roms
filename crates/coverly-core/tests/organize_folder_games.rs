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

/// Answers `cloud_only` for files named `cloud.bin`, a 5 GiB length for
/// files named `big.bin` and a FAT32-sized part (0xFFFF0000) for the
/// `00`/`01` parts of a split Switch file (never a real 4 GiB file).
struct Probe;

impl MetaProbe for Probe {
    fn cloud_only(&self, path: &Path, _: &fs::Metadata) -> bool {
        path.file_name().is_some_and(|n| n == "cloud.bin")
    }

    fn file_len(&self, path: &Path, meta: &fs::Metadata) -> u64 {
        let split_part = path.file_name().is_some_and(|n| n == "00" || n == "01")
            && path
                .parent()
                .and_then(|p| p.extension())
                .is_some_and(|e| e == "nsp");
        if path.file_name().is_some_and(|n| n == "big.bin") {
            5 << 30
        } else if split_part {
            0xFFFF_0000
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
        leave: None,
    })
}

fn gba_probe(listing: &DirListing) -> Option<FolderMatch> {
    has(listing, "GAME.PROBE").then(|| FolderMatch {
        id: id("gba"),
        probe: PathBuf::from("GAME.PROBE"),
        leave: None,
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

// ---------------------------------------------------------------------
// Folder games in the library (spec D26): one `RomEntry` per folder game,
// never descended into, never opened as a file.

mod library {
    use std::sync::atomic::AtomicBool;

    use coverly_core::art::resolve_art;
    use coverly_core::audit::build_report;
    use coverly_core::consoles::{start_open_log, take_open_log};
    use coverly_core::credentials::SourceKeys;
    use coverly_core::fill::{Endpoints, FillEnv, FillSettings};
    use coverly_core::identify::dat::DatStore;
    use coverly_core::identify::{IdMethod, Identifier, IdentifyOptions};
    use coverly_core::jobs::ApplyOptions;
    use coverly_core::net::testing::{png_bytes, MockServer};
    use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
    use coverly_core::profiles::Media;
    use coverly_core::progress::NoProgress;
    use coverly_core::scan::{scan_library, LibraryScan, RomEntry, ScanOptions};

    use super::*;

    const GB: &str = "Nintendo - Game Boy";
    const GB_DAT: &str = "metadat/no-intro/Nintendo - Game Boy.dat";

    fn profile(id: &str) -> Profile {
        let mut p = builtin_profiles().into_iter().find(|p| p.id == id).unwrap();
        p.folder_game_suffix.insert("gb".into(), ".gbdir".into());
        p
    }

    /// `<roms>/gb/`: a probed folder, a suffixed folder with no probe
    /// file, and a loose ROM.
    fn library(roms: &str) -> TempDir {
        let lib = TempDir::new().unwrap();
        let gb = format!("{roms}/gb");
        put_tree(
            lib.path(),
            &[
                (&format!("{gb}/Zelda/GAME.PROBE"), 16),
                (&format!("{gb}/Zelda/data/a.gb"), 32_768),
                (&format!("{gb}/Zelda/data/b.bin"), 10),
                (&format!("{gb}/Tetris.gb"), 64),
                (&format!("{gb}/Mario.gbdir/data/c.gb"), 100),
            ],
        );
        lib
    }

    fn scan(root: &Path, profile: &Profile) -> LibraryScan {
        scan_library(root, profile, &SystemDb::builtin(), &ScanOptions::default())
    }

    fn gb_roms(scan: &LibraryScan) -> &[RomEntry] {
        &scan
            .systems
            .iter()
            .find(|s| s.system_id.as_deref() == Some("gb"))
            .unwrap()
            .roms
    }

    fn rom<'a>(roms: &'a [RomEntry], folder: &str) -> &'a RomEntry {
        roms.iter()
            .find(|r| r.path.file_name().is_some_and(|n| n == folder))
            .unwrap_or_else(|| panic!("{folder} in {roms:?}"))
    }

    #[test]
    fn a_folder_game_is_one_library_entry_and_its_contents_are_not_scanned() {
        let _g = ProbeGuard::new();
        let lib = library("roms");
        let scan = scan(lib.path(), &profile("batocera"));
        let roms = gb_roms(&scan);
        let names: Vec<&str> = roms.iter().map(|r| r.stem.as_str()).collect();
        assert_eq!(names, ["Mario", "Tetris", "Zelda"], "{roms:?}");

        let zelda = rom(roms, "Zelda");
        assert!(zelda.is_dir);
        assert_eq!(zelda.ext, "");
        assert_eq!(zelda.size, 16 + 32_768 + 10);
        assert!(zelda.rel_dir.as_os_str().is_empty());
        assert!(zelda.companions.is_empty());
        assert!(zelda.path.is_dir());
        let newest = fs::metadata(zelda.path.join("data/b.bin"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(zelda.modified, newest, "newest file inside");

        let mario = rom(roms, "Mario.gbdir");
        assert!(
            mario.is_dir,
            "the layout's suffix alone makes a folder game"
        );
        assert_eq!(
            (mario.stem.as_str(), mario.ext.as_str()),
            ("Mario", "gbdir")
        );
        assert_eq!(mario.size, 100);

        let tetris = rom(roms, "Tetris.gb");
        assert!(!tetris.is_dir);
        assert_eq!(tetris.ext, "gb");
    }

    #[test]
    fn without_the_probe_or_the_suffix_a_folder_is_walked_as_before() {
        let lib = library("roms");
        let scan = scan(
            lib.path(),
            &builtin_profiles()
                .into_iter()
                .find(|p| p.id == "batocera")
                .unwrap(),
        );
        let roms = gb_roms(&scan);
        let mut files: Vec<&str> = roms
            .iter()
            .map(|r| r.path.file_name().unwrap().to_str().unwrap())
            .collect();
        files.sort_unstable();
        assert_eq!(files, ["Tetris.gb", "a.gb", "c.gb"]);
        assert!(roms.iter().all(|r| !r.is_dir));
    }

    /// ES-DE keeps a directory's extension in its media name
    /// (`Utils::FileSystem::getStem` returns a directory's whole name), so
    /// on ES-DE the stem of `Mario.gbdir/` is `Mario.gbdir`; Batocera's
    /// `getStem` cuts at the last dot for files and folders alike.
    #[test]
    fn es_de_names_the_media_of_a_suffixed_folder_after_its_whole_name() {
        let _g = ProbeGuard::new();
        let lib = library("ROMs");
        let scan = scan(lib.path(), &profile("esde"));
        let roms = gb_roms(&scan);
        let mario = rom(roms, "Mario.gbdir");
        assert!(mario.is_dir);
        assert_eq!(
            (mario.stem.as_str(), mario.ext.as_str()),
            ("Mario.gbdir", "gbdir")
        );
        assert_eq!(rom(roms, "Zelda").stem, "Zelda");
    }

    fn dat_text() -> String {
        let crc = format!("{:08X}", crc32fast::hash(&[0u8; 64]));
        format!("clrmamepro (\n name \"GB\"\n)\ngame (\n name \"Tetris (World)\"\n rom ( name \"Tetris (World).gb\" size 64 crc {crc} )\n)\n")
    }

    fn net() -> (PoliteHttp<UreqHttp>, Cache) {
        let cfg = NetConfig {
            retries: 0,
            ..NetConfig::default()
        };
        (
            PoliteHttp::new(UreqHttp::new(&cfg), &cfg),
            Cache::in_memory().unwrap(),
        )
    }

    fn opened_inside(log: &[PathBuf], dir: &Path) -> Vec<PathBuf> {
        log.iter().filter(|p| p.starts_with(dir)).cloned().collect()
    }

    #[test]
    fn hashing_never_opens_a_folder_entry() {
        let _g = ProbeGuard::new();
        let lib = library("roms");
        let scan = scan(lib.path(), &profile("batocera"));
        let roms = gb_roms(&scan);
        let server = MockServer::start();
        server.serve_dat(GB_DAT, &dat_text());
        let (http, cache) = net();
        let dats = DatStore::new(&http, &cache, server.base());
        let id = Identifier::new(&cache, &dats, IdentifyOptions::default());
        let db = SystemDb::builtin();
        let gb = db.get("gb").unwrap();

        start_open_log();
        let tetris = id.identify(rom(roms, "Tetris.gb"), gb, false).unwrap();
        let zelda_rom = rom(roms, "Zelda");
        let zelda = id.identify(zelda_rom, gb, false).unwrap();
        let mario_rom = rom(roms, "Mario.gbdir");
        let mario = id.identify(mario_rom, gb, false).unwrap();
        let log = take_open_log();

        assert_eq!(tetris.method, IdMethod::Hash, "files still hash");
        assert_eq!((zelda.method, zelda.hash), (IdMethod::Name, None));
        assert_eq!(zelda.parsed.title, "Zelda");
        assert_eq!((mario.method, mario.hash), (IdMethod::Name, None));
        for folder in [zelda_rom, mario_rom] {
            assert_eq!(opened_inside(&log, &folder.path), Vec::<PathBuf>::new());
            let mtime = coverly_core::cache::mtime_ns(folder.modified);
            assert_eq!(
                cache.quick_id(&folder.path, folder.size, mtime).unwrap(),
                None
            );
        }
    }

    #[test]
    fn a_gamelist_fill_writes_the_folder_as_the_game_path() {
        let _g = ProbeGuard::new();
        let lib = library("roms");
        let r = lib.path();
        let server = MockServer::start();
        server.serve_libretro_root(&[GB]);
        server.serve_libretro_index(GB, "Named_Boxarts", &["Zelda.png", "Mario.png"]);
        for name in ["Zelda.png", "Mario.png"] {
            server.serve_libretro_image(GB, "Named_Boxarts", name, png_bytes(400, 560));
        }
        let (http, cache) = net();
        let endpoints = Endpoints {
            libretro: server.base(),
            libretro_raw: server.url("/raw"),
            database: server.base(),
            ..Endpoints::default()
        };
        let cache_dir = TempDir::new().unwrap();
        let data = TempDir::new().unwrap();
        let env = FillEnv {
            cache: &cache,
            http: &http,
            endpoints: &endpoints,
            cache_dir: cache_dir.path(),
            data_dir: data.path(),
            keys: &SourceKeys::default(),
        };
        let profile = profile("batocera");
        let db = SystemDb::builtin();
        let scan = scan(r, &profile);
        let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
        let report = build_report(r, &profile, &db, &ScanOptions::default(), &[Media::Cover]);
        let gb_report = report
            .systems
            .iter()
            .find(|s| s.system_id.as_deref() == Some("gb"))
            .unwrap();
        let mut rel: Vec<PathBuf> = gb_report
            .entries
            .iter()
            .map(|e| e.rel_path.clone())
            .collect();
        rel.sort();
        let want: Vec<PathBuf> = ["Mario.gbdir", "Tetris.gb", "Zelda"]
            .map(PathBuf::from)
            .to_vec();
        assert_eq!(rel, want, "the audit lists each folder game once");
        assert_eq!(gb_report.roms, 3);
        let settings = FillSettings {
            media: vec![Media::Cover],
            ..FillSettings::default()
        };
        start_open_log();
        let plan = env.plan(
            r,
            &profile,
            &db,
            &art,
            &settings,
            &NoProgress,
            &AtomicBool::new(false),
        );
        let report = env
            .apply(
                &plan,
                &profile,
                &ApplyOptions::default(),
                &NoProgress,
                &AtomicBool::new(false),
            )
            .unwrap();
        let log = take_open_log();
        assert_eq!(report.filled, 2, "{report:?}");
        let gb = r.join("roms/gb");
        let gamelist = fs::read_to_string(gb.join("gamelist.xml")).unwrap();
        assert!(gamelist.contains("<path>./Zelda</path>"), "{gamelist}");
        assert!(
            gamelist.contains("<path>./Mario.gbdir</path>"),
            "{gamelist}"
        );
        assert!(
            gamelist.contains("<thumbnail>./images/Zelda-image.png</thumbnail>"),
            "{gamelist}"
        );
        assert!(
            gamelist.contains("<thumbnail>./images/Mario-image.png</thumbnail>"),
            "{gamelist}"
        );
        assert!(gb.join("images/Zelda-image.png").is_file());
        assert!(gb.join("images/Mario-image.png").is_file());
        assert_eq!(
            opened_inside(&log, &gb.join("Zelda")),
            Vec::<PathBuf>::new()
        );
        assert_eq!(
            opened_inside(&log, &gb.join("Mario.gbdir")),
            Vec::<PathBuf>::new()
        );
    }
}

/// ES-DE keeps the folder suffix in the media stem, but the game's title
/// is still the name without it: `Mario.gbdir/` is looked up as `Mario`.
mod library_title {
    use coverly_core::cache::Cache;
    use coverly_core::identify::dat::DatStore;
    use coverly_core::identify::{Identifier, IdentifyOptions};
    use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
    use coverly_core::scan::{scan_library, ScanOptions};

    use super::*;

    #[test]
    fn an_es_de_suffixed_folder_is_identified_by_its_name_without_the_suffix() {
        let _g = ProbeGuard::new();
        let lib = TempDir::new().unwrap();
        put_tree(lib.path(), &[("ROMs/gb/Mario (USA).gbdir/data/c.gb", 100)]);
        let mut profile = builtin_profiles()
            .into_iter()
            .find(|p| p.id == "esde")
            .unwrap();
        profile
            .folder_game_suffix
            .insert("gb".into(), ".gbdir".into());
        let db = SystemDb::builtin();
        let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
        let gb = scan
            .systems
            .iter()
            .find(|s| s.system_id.as_deref() == Some("gb"))
            .unwrap();
        let mario = &gb.roms[0];
        assert_eq!(mario.stem, "Mario (USA).gbdir", "media stem keeps it");
        assert_eq!(mario.title_stem(), "Mario (USA)");

        let cfg = NetConfig {
            retries: 0,
            ..NetConfig::default()
        };
        let http = PoliteHttp::new(UreqHttp::new(&cfg), &cfg);
        let cache = Cache::in_memory().unwrap();
        let dats = DatStore::new(&http, &cache, "http://127.0.0.1:9");
        let id = Identifier::new(&cache, &dats, IdentifyOptions::default());
        let identity = id.identify(mario, db.get("gb").unwrap(), false).unwrap();
        assert_eq!(identity.parsed.title, "Mario");
        assert_eq!(identity.parsed.regions.len(), 1, "{:?}", identity.parsed);
    }
}

/// The first real folder probes (systems batch 2, Task 9): Wii U loadiine
/// and NUS folders.
mod wiiu {
    use coverly_core::consoles::testing::{nus_folder, wiiu_folder};
    use coverly_core::scan::{scan_library, ScanOptions};

    use super::*;

    const SM3DW: &str = "Super Mario 3D World [0005000010145D00]";

    #[test]
    fn a_loadiine_folder_is_one_item_named_by_its_meta_xml() {
        let (world, pile, target) = setup();
        wiiu_folder(
            &pile.path().join(SM3DW),
            "0005000010145D00",
            "WUP-P-ARDP",
            "Super Mario 3D World",
        );
        let p = plan(&world, pile.path(), target.path(), "esde");
        let item = only_folder(&p);
        assert_eq!(item.sources, [PathBuf::from(SM3DW)]);
        assert_eq!(item.system_id.as_deref(), Some("wiiu"));
        assert_eq!(item.action, OrganizeAction::Auto);
        assert!(
            item.why.iter().any(|e| e.detail == "meta.xml"),
            "{:?}",
            item.why
        );
        // The row's «why» line names the title id and its file (§10).
        let tid = item.title_id.as_ref().expect("title id on the item");
        assert_eq!(
            (tid.id.as_str(), tid.source),
            (
                "0005000010145D00",
                coverly_core::consoles::IdSource::MetaXml
            )
        );
        let t = item.target.as_ref().unwrap();
        assert_eq!(last(t), SM3DW);
        assert_eq!(t.parent().map(last).as_deref(), Some("wiiu"));
        // `code/game.rpx` has a Wii U header but stays inside the folder.
        assert_eq!(p.items.len(), 1, "{:#?}", p.items);
        assert!(p.other_files.is_empty(), "{:?}", p.other_files);
    }

    #[test]
    fn a_wiiu_nus_folder_is_one_item_and_a_3ds_one_is_not() {
        let (world, pile, target) = setup();
        nus_folder(&pile.path().join("Wii U Title"), 0x0005_0000_1010_ED00);
        nus_folder(&pile.path().join("3DS Title"), 0x0004_0000_0003_0800);
        let p = plan(&world, pile.path(), target.path(), "esde");
        let item = only_folder(&p);
        assert_eq!(item.sources, [PathBuf::from("Wii U Title")]);
        assert_eq!(item.system_id.as_deref(), Some("wiiu"));
        assert!(item.why.iter().any(|e| e.detail == "NUS title.tmd"));
        // The 3DS download is walked file by file: its ticket is key
        // material, never opened, and nothing of it is a folder item.
        let three = pile.path().join("3DS Title");
        assert!(p
            .items
            .iter()
            .filter(|i| i.kind != ItemKind::Folder)
            .all(|i| i
                .sources
                .iter()
                .all(|s| pile.path().join(s).starts_with(&three))));
        assert!(p.items.iter().any(|i| {
            i.sources[0].file_name().is_some_and(|n| n == "title.tik")
                && left_reason(i) == Some(&LeftReason::KeyMaterial)
        }));
    }

    #[test]
    fn a_moved_wiiu_folder_is_one_library_entry() {
        let lib = TempDir::new().unwrap();
        let dir = lib.path().join("roms").join("wiiu").join(SM3DW);
        wiiu_folder(
            &dir,
            "0005000010145D00",
            "WUP-P-ARDP",
            "Super Mario 3D World",
        );
        let profile = builtin_profiles()
            .into_iter()
            .find(|p| p.id == "batocera")
            .unwrap();
        let scan = scan_library(
            lib.path(),
            &profile,
            &SystemDb::builtin(),
            &ScanOptions::default(),
        );
        let roms = &scan
            .systems
            .iter()
            .find(|s| s.system_id.as_deref() == Some("wiiu"))
            .unwrap()
            .roms;
        assert_eq!(roms.len(), 1, "{roms:?}");
        assert!(roms[0].is_dir);
        assert_eq!(roms[0].stem, SM3DW);
        assert_eq!(roms[0].ext, "");
    }
}

/// A Cemu `.wua` starts with a zstd block: it is a Wii U item, not a
/// damaged `.zst` archive (seen on a real WUA).
#[test]
fn a_wua_is_a_wiiu_file_not_a_zstd_archive() {
    let (world, pile, target) = setup();
    let path = pile.path().join("Super Mario 3D World (USA).wua");
    fs::write(&path, coverly_core::consoles::testing::wua().bytes()).unwrap();
    pin_mtime(&path, 1);
    let p = plan(&world, pile.path(), target.path(), "esde");
    assert_eq!(p.items.len(), 1, "{:#?}", p.items);
    let item = &p.items[0];
    assert_eq!(item.kind, ItemKind::File, "{item:#?}");
    assert_eq!(item.system_id.as_deref(), Some("wiiu"));
    assert!(item.why.iter().any(|e| e.detail == "WUA: base, update"));
}

/// Split «archive bit» Switch files (systems batch 2, Task 10, D12).
mod switch {
    use super::*;

    const GAME: &str = "Super Mario Odyssey [0100000000010000][v0].nsp";

    #[test]
    fn a_split_nsp_folder_is_one_item_and_fits_fat32() {
        let _g = ProbeGuard::new();
        let (world, pile, target) = setup();
        let dir = pile.path().join(GAME);
        fs::create_dir(&dir).unwrap();
        let head = coverly_core::consoles::testing::nsp(&[(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca",
            &[0x55; 64],
        )]);
        fs::write(dir.join("00"), head.bytes()).unwrap();
        fs::write(dir.join("01"), [0x55u8; 32]).unwrap();
        pin_mtime(&dir.join("00"), 1);
        pin_mtime(&dir.join("01"), 2);
        set_fat32_override(Some(true));
        let p = plan(&world, pile.path(), target.path(), "esde");
        let item = only_folder(&p);
        assert_eq!(p.items.len(), 1, "{:#?}", p.items);
        assert_eq!(item.sources, [PathBuf::from(GAME)]);
        assert_eq!(item.system_id.as_deref(), Some("switch"));
        // Each part is under 4 GiB: the folder fits although the game,
        // two parts of 0xFFFF0000 bytes, does not fit in one file.
        assert_eq!(item.size, 2 * 0xFFFF_0000);
        assert!(item.moves_in(), "{:?}", item.action);
        assert!(item.why.iter().any(|e| e.detail == "split NSP folder"));
        let t = item.target.as_ref().unwrap();
        assert_eq!(last(t), GAME);
        assert_eq!(t.parent().map(last).as_deref(), Some("switch"));
    }
}

/// PS3 JB/HDD and PS Vita app folders (systems batch 2, Task 11, D12,
/// D13).
mod sony {
    use coverly_core::consoles::testing::{ps3_hdd, ps3_jb, vita_app};
    use coverly_core::scan::{scan_library, ScanOptions};

    use super::*;

    const DEMONS: &str = "Demon's Souls (BLUS-30443)";

    #[test]
    fn a_jb_folder_moves_whole_with_the_ps3_suffix_where_the_layout_needs_it() {
        for layout in ["esde", "batocera"] {
            let (world, pile, target) = setup();
            ps3_jb(&pile.path().join(DEMONS), "BLUS30443", "DG");
            let p = plan(&world, pile.path(), target.path(), layout);
            assert_eq!(p.items.len(), 1, "{layout}: {:#?}", p.items);
            let item = only_folder(&p);
            assert_eq!(item.system_id.as_deref(), Some("ps3"), "{layout}");
            assert_eq!(item.action, OrganizeAction::Auto, "{layout}");
            assert!(item.why.iter().any(|e| e.detail == "PARAM.SFO"), "{layout}");
            let t = item.target.as_ref().unwrap();
            let suffixed = format!("{DEMONS}.ps3");
            assert_eq!(last(t), suffixed, "{layout}");
            assert_eq!(t.parent().map(last).as_deref(), Some("ps3"), "{layout}");
            let name = world
                .profiles
                .iter()
                .find(|p| p.id == layout)
                .unwrap()
                .name
                .clone();
            assert!(
                item.notes.contains(&ItemNote::FolderSuffix {
                    name: suffixed.clone(),
                    layout: name,
                }),
                "{layout}: {:?}",
                item.notes
            );
        }
        // Onion has no PS3 folder: the first alias, and no suffix.
        let (world, pile, target) = setup();
        ps3_jb(&pile.path().join(DEMONS), "BLUS30443", "DG");
        let p = plan(&world, pile.path(), target.path(), "onion");
        let item = only_folder(&p);
        let t = item.target.as_ref().unwrap();
        assert_eq!(last(t), DEMONS);
        assert_eq!(t.parent().map(last).as_deref(), Some("PS3"));
        assert!(!item
            .notes
            .iter()
            .any(|n| matches!(n, ItemNote::FolderSuffix { .. })));
    }

    #[test]
    fn hdd_and_vita_folders_are_one_item_each() {
        let (world, pile, target) = setup();
        ps3_hdd(&pile.path().join("NPUB30024"), "NPUB30024", "HG");
        vita_app(&pile.path().join("PCSE00844"), "PCSE00844");
        let p = plan(&world, pile.path(), target.path(), "batocera");
        assert_eq!(p.items.len(), 2, "{:#?}", p.items);
        let all = folders(&p);
        let vita = all
            .iter()
            .find(|i| i.sources[0] == Path::new("PCSE00844"))
            .unwrap();
        assert_eq!(vita.system_id.as_deref(), Some("psvita"));
        let t = vita.target.as_ref().unwrap();
        assert_eq!(
            (last(t), t.parent().map(last)),
            ("PCSE00844".to_string(), Some("psvita".to_string()))
        );
        // The licence inside moves with the folder: no item of its own.
        assert!(p
            .items
            .iter()
            .all(|i| i.sources[0].file_name().is_none_or(|n| n != "work.bin")));
        let hdd = all
            .iter()
            .find(|i| i.sources[0] == Path::new("NPUB30024"))
            .unwrap();
        assert_eq!(hdd.system_id.as_deref(), Some("ps3"));
        assert_eq!(last(hdd.target.as_ref().unwrap()), "NPUB30024.ps3");
    }

    #[test]
    fn a_jb_folder_another_probe_also_claims_goes_to_review() {
        // Two probes naming different systems (a PS3 JB folder that also
        // holds the test probe's file): not a real game, so review.
        let _g = ProbeGuard::new();
        let (world, pile, target) = setup();
        let dir = pile.path().join(DEMONS);
        ps3_jb(&dir, "BLUS30443", "DG");
        put_tree(&dir, &[("GAME.PROBE", 16)]);
        let p = plan(&world, pile.path(), target.path(), "esde");
        let item = only_folder(&p);
        assert_eq!(item.action, OrganizeAction::Review);
        assert_eq!(item.system_id, None);
        let alts: Vec<&str> = item.alternatives.iter().map(|(s, _)| s.as_str()).collect();
        // Probe order (D12): the PS3 probes come first.
        assert_eq!(alts, ["ps3", "gb"]);
    }

    #[test]
    fn a_suffixed_ps3_folder_is_one_library_entry() {
        let lib = TempDir::new().unwrap();
        let dir = lib.path().join("roms").join("ps3").join("Game.ps3");
        ps3_jb(&dir, "BLUS30443", "DG");
        let profile = builtin_profiles()
            .into_iter()
            .find(|p| p.id == "batocera")
            .unwrap();
        let scan = scan_library(
            lib.path(),
            &profile,
            &SystemDb::builtin(),
            &ScanOptions::default(),
        );
        let roms = &scan
            .systems
            .iter()
            .find(|s| s.system_id.as_deref() == Some("ps3"))
            .unwrap()
            .roms;
        assert_eq!(roms.len(), 1, "{roms:?}");
        assert!(roms[0].is_dir);
        assert_eq!(roms[0].stem, "Game");
        assert_eq!(roms[0].ext, "ps3");
    }
}

/// Xbox `default.xbe` folders and Xbox 360 GOD/XBLA trees (systems batch
/// 2, Task 12, D12).
mod xbox {
    use coverly_core::consoles::testing::{god_folder, xbe, xex2};

    use super::*;

    #[test]
    fn an_xbe_folder_moves_whole_to_the_xbox_folder() {
        let (world, pile, target) = setup();
        let dir = pile.path().join("Halo");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("default.xbe"), xbe(0x4D53_0004, "Halo")).unwrap();
        fs::write(dir.join("media.xmv"), [0u8; 64]).unwrap();
        pin_mtime(&dir.join("default.xbe"), 1);
        pin_mtime(&dir.join("media.xmv"), 2);
        let p = plan(&world, pile.path(), target.path(), "esde");
        assert_eq!(p.items.len(), 1, "{:#?}", p.items);
        let item = only_folder(&p);
        assert_eq!(item.system_id.as_deref(), Some("xbox"));
        assert_eq!(item.action, OrganizeAction::Auto);
        assert!(item.why.iter().any(|e| e.detail == "default.xbe"));
        let t = item.target.as_ref().unwrap();
        assert_eq!(
            (last(t), t.parent().map(last)),
            ("Halo".to_string(), Some("xbox".to_string()))
        );
    }

    #[test]
    fn an_xbla_tree_is_one_folder_item_for_xbox360() {
        let (world, pile, target) = setup();
        god_folder(pile.path(), 0x5841_0889, 0x000D_0000, b"LIVE");
        let p = plan(&world, pile.path(), target.path(), "batocera");
        assert_eq!(p.items.len(), 1, "{:#?}", p.items);
        let item = only_folder(&p);
        assert_eq!(item.sources[0], Path::new("58410889"));
        assert_eq!(item.system_id.as_deref(), Some("xbox360"));
        let t = item.target.as_ref().unwrap();
        assert_eq!(
            (last(t), t.parent().map(last)),
            ("58410889".to_string(), Some("xbox360".to_string()))
        );
    }

    #[test]
    fn a_god_tree_and_a_xex_folder_are_one_item_each() {
        let (world, pile, target) = setup();
        let god = pile.path().join("Halo 3 (GOD)");
        fs::create_dir(&god).unwrap();
        god_folder(&god, 0x4D53_07E6, 0x0000_7000, b"CON ");
        let xex = pile.path().join("Gears");
        fs::create_dir(&xex).unwrap();
        fs::write(xex.join("default.xex"), xex2(0x4D53_082D)).unwrap();
        pin_mtime(&xex.join("default.xex"), 3);
        let p = plan(&world, pile.path(), target.path(), "esde");
        let all = folders(&p);
        assert_eq!(all.len(), 2, "{:#?}", p.items);
        assert!(all
            .iter()
            .all(|i| i.system_id.as_deref() == Some("xbox360")));
        // The GOD data parts move with their folder: no item of their own.
        assert_eq!(p.items.len(), 2, "{:#?}", p.items);
    }

    #[test]
    fn an_eight_hex_cache_folder_is_not_a_game() {
        let (world, pile, target) = setup();
        put_tree(
            pile.path(),
            &[(
                "1A2B3C4D/00000001/0123456789ABCDEF0123456789ABCDEF01234567",
                64,
            )],
        );
        let p = plan(&world, pile.path(), target.path(), "esde");
        assert!(folders(&p).is_empty(), "{:#?}", p.items);
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_with_a_junk_name_still_leaves_the_folder() {
    let _g = ProbeGuard::new();
    for name in ["._link", ".DS_Store", "Thumbs.db"] {
        let item = gate(&[("G/GAME.PROBE", 16), ("G/real.bin", 4)], |pile| {
            std::os::unix::fs::symlink("/", pile.join("G").join(name)).unwrap();
        });
        assert_eq!(left_reason(&item), Some(&LeftReason::Symlink), "{name}");
    }
}

#[test]
fn a_folder_with_a_junk_name_is_counted_like_the_copy_sees_it() {
    let _g = ProbeGuard::new();
    // Only junk *files* are OS litter: a folder named so is copied with
    // its contents, so the plan measures it too.
    let item = gate(
        &[
            ("G/GAME.PROBE", 16),
            ("G/._data/a.bin", 8),
            ("G/Thumbs.db/b.bin", 4),
        ],
        |_| {},
    );
    assert_eq!(item.size, 28);
}

#[cfg(unix)]
#[test]
fn a_symlink_added_after_the_plan_is_skipped_at_apply() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(pile.path(), ZELDA);
    let p = plan(&world, pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    assert_eq!(item.action, OrganizeAction::Auto);
    std::os::unix::fs::symlink("/", pile.path().join("Zelda").join("d").join("root")).unwrap();
    let data = TempDir::new().unwrap();
    let report = apply(&p, data.path());
    assert_eq!(
        status(&report, item.id),
        &ItemStatus::Skipped {
            reason: SkipReason::ChangedSincePlan
        }
    );
    assert!(pile.path().join("Zelda").join("GAME.PROBE").is_file());
}
