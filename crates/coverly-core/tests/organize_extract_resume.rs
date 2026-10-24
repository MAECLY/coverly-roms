//! Resume of interrupted extractions (archives spec §9.3): a run killed
//! anywhere in phases A–D is finished by the next one without a second
//! copy of any file, without rewriting the files already in place, and
//! with the archive sent to the Trash exactly once.
//!
//! Crashes are injected with `Fault::AbortAfterLine(n)`: once `n` journal
//! lines of the extraction are written, the extractor stops at its next
//! step as if the process was killed (nothing rolled back, the job not
//! finished).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::UNIX_EPOCH;

use coverly_core::archive::testing::{sevenz, SevenZShape};
use coverly_core::archive::{file_entries, open_archive, Limits};
use coverly_core::organize::extract::{set_write_fault, Fault};
use coverly_core::organize::{
    apply_organize, resume_organize, ApplyEnv, ArchiveAfter, Counts, ExtractEntry, ExtractSource,
    ItemKind, ItemStatus, NameMode, OrganizeAction, OrganizeItem, OrganizePlan, OrganizeReport,
    SheetCheck, SkipReason, SourceFp, Transfer, TransferMode,
};
use coverly_core::write::trash::{FakeTrash, TrashBackend};
use coverly_core::write::{
    job_archive_steps, job_transfers, undo_job_with, ArchiveStep, TransferRecord,
};
use filetime::FileTime;
use tempfile::TempDir;

/// A library root, a download folder, an app data dir and a Trash.
struct Env {
    _tmp: TempDir,
    root: PathBuf,
    src: PathBuf,
    data: PathBuf,
    trash: PathBuf,
}

fn env() -> Env {
    let tmp = TempDir::new().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let (root, src, data) = (base.join("ROMS"), base.join("Downloads"), base.join("data"));
    for d in [&root, &src, &data] {
        fs::create_dir_all(d).unwrap();
    }
    Env {
        _tmp: tmp,
        root,
        src,
        data,
        trash: base.join("Trash"),
    }
}

/// Synthetic disc tracks (no copyrighted bytes) and their sheet, the
/// sheet last.
fn disc_files(stem: &str, tracks: usize) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let mut cue = String::new();
    for t in 1..=tracks {
        let name = format!("{stem} (Track {t:02}).bin");
        let kind = if t == 1 { "MODE2/2352" } else { "AUDIO" };
        cue.push_str(&format!(
            "FILE \"{name}\" BINARY\r\n  TRACK {t:02} {kind}\r\n    INDEX 01 00:00:00\r\n"
        ));
        let len = 20_000 + 3_000 * t as u32;
        let bytes = (0..len).map(|i| ((i * t as u32 + 7) % 251) as u8).collect();
        files.push((name, bytes));
    }
    files.push((format!("{stem}.cue"), cue.into_bytes()));
    files
}

/// A solid 7z of `stem`'s disc misnamed `.zip` (the real case's shape),
/// and its plan entries under `folder`.
fn disc_archive(
    env: &Env,
    stem: &str,
    tracks: usize,
    folder: &str,
) -> (PathBuf, Vec<ExtractEntry>) {
    let files = disc_files(stem, tracks);
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice()))
        .collect();
    let path = env.src.join(format!("{stem}.zip"));
    fs::write(&path, sevenz(&refs, SevenZShape::solid_lzma2_sheets_last())).unwrap();
    let reader = open_archive(&path, &Limits::default()).unwrap().unwrap();
    let entries = file_entries(reader.as_ref())
        .map(|e| ExtractEntry {
            index: e.index,
            name: e.name.as_str().to_string(),
            size: e.size,
            crc32: e.crc32,
            target: Path::new(folder).join(e.name.as_str()),
        })
        .collect();
    (path, entries)
}

fn fp_of(path: &Path) -> SourceFp {
    let meta = fs::metadata(path).unwrap();
    let ns = meta
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    SourceFp {
        size: meta.len(),
        mtime_ns: i64::try_from(ns).unwrap(),
    }
}

fn extract_item(
    env: &Env,
    archive: &Path,
    entries: Vec<ExtractEntry>,
    after: ArchiveAfter,
    m3u: Option<PathBuf>,
) -> OrganizeItem {
    let bytes = entries.iter().map(|e| e.size).sum();
    OrganizeItem {
        id: 0,
        sources: vec![archive.strip_prefix(&env.src).unwrap().to_path_buf()],
        kind: ItemKind::Archive,
        size: fs::metadata(archive).unwrap().len(),
        fps: vec![fp_of(archive)],
        target: Some(entries[0].target.clone()),
        set_targets: Vec::new(),
        system_id: Some("psx".into()),
        folder: Some("PS".into()),
        confidence: 1.0,
        method: None,
        why: Vec::new(),
        alternatives: Vec::new(),
        rename: None,
        notes: Vec::new(),
        companions: Vec::new(),
        catalogue_name: None,
        transfer: Transfer::Extract {
            entries,
            bytes,
            archive_after: after,
            source: ExtractSource::Builtin,
            sheet_check: SheetCheck::Read,
            m3u,
        },
        disc: None,
        action: OrganizeAction::Auto,
    }
}

fn plan_of(env: &Env, mut items: Vec<OrganizeItem>, mode: TransferMode) -> OrganizePlan {
    for (i, item) in items.iter_mut().enumerate() {
        item.id = i;
    }
    OrganizePlan {
        source_root: env.src.clone(),
        target_root: env.root.clone(),
        roms_dir: env.root.clone(),
        layout: "anbernic-stock".into(),
        mode,
        names: NameMode::Keep,
        items,
        counts: Counts::default(),
        sorted_folders: Vec::new(),
        deep_folders: Vec::new(),
        other_files: Vec::new(),
        warnings: Vec::new(),
        broad_root: false,
        fat32: false,
        move_allowed: true,
        target_detected: None,
        created_at: "2026-10-24T00:00:00Z".into(),
    }
}

/// Applies `plan`, killed after `lines` journal lines of its extractions.
fn apply_killed(
    env: &Env,
    plan: &OrganizePlan,
    trash: &dyn TrashBackend,
    lines: u64,
) -> OrganizeReport {
    let progress = |_| {};
    let aenv = ApplyEnv {
        data_dir: &env.data,
        progress: &progress,
        cancel: &AtomicBool::new(false),
        trash,
        tools: Some(&[]),
    };
    set_write_fault(Some(Fault::AbortAfterLine(lines)));
    let report = apply_organize(plan, &aenv);
    set_write_fault(None);
    let report = report.unwrap();
    assert!(report.job.is_some(), "{report:?}");
    report
}

fn resume(env: &Env, job: &str, trash: &dyn TrashBackend) -> OrganizeReport {
    let progress = |_| {};
    let aenv = ApplyEnv {
        data_dir: &env.data,
        progress: &progress,
        cancel: &AtomicBool::new(false),
        trash,
        tools: Some(&[]),
    };
    resume_organize(job, &aenv).unwrap()
}

/// Names directly in `dir`, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn temps_in(dir: &Path) -> Vec<String> {
    names_in(dir)
        .into_iter()
        .filter(|n| n.starts_with(".coverly-"))
        .collect()
}

/// The entries' file names, sorted (what a folder holds once extracted).
fn final_names(entries: &[ExtractEntry]) -> Vec<String> {
    let mut names: Vec<String> = entries
        .iter()
        .map(|e| e.target.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Every final holds exactly its entry's bytes.
fn assert_extracted(env: &Env, stem: &str, tracks: usize, entries: &[ExtractEntry]) {
    for (name, bytes) in disc_files(stem, tracks) {
        let e = entries.iter().find(|e| e.name == name).unwrap();
        assert_eq!(fs::read(env.root.join(&e.target)).unwrap(), bytes, "{name}");
    }
}

/// The journal's finished records (completion lines) for `target`.
fn completions<'r>(records: &'r [TransferRecord], target: &Path) -> Vec<&'r TransferRecord> {
    records
        .iter()
        .filter(|r| r.finished && r.target == target)
        .collect()
}

/// A modification time no extraction sets: a final that keeps it was
/// never rewritten.
fn marker() -> FileTime {
    FileTime::from_unix_time(1_234_567_890, 0)
}

fn mtime(path: &Path) -> FileTime {
    FileTime::from_last_modification_time(&fs::metadata(path).unwrap())
}

fn last_step(env: &Env, job: &str, item: usize) -> Option<ArchiveStep> {
    job_archive_steps(&env.data, job)
        .unwrap()
        .into_iter()
        .rev()
        .find(|s| s.item == Some(item))
        .map(|s| s.step)
}

#[test]
fn crash_in_phase_a_restarts_item_and_deletes_temps() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, entries) = disc_archive(&env, "Next Tetris, The", 2, "PS");
    let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Keep, None);
    let plan = plan_of(&env, vec![item], TransferMode::Copy);
    // Two intents written, two temps decoded; killed before the third.
    let crashed = apply_killed(&env, &plan, &trash, 2);
    let job = crashed.job.unwrap();
    assert_eq!(crashed.resumable_items, [0]);
    let ps = env.root.join("PS");
    assert_eq!(temps_in(&ps).len(), 2, "{:?}", names_in(&ps));
    assert_eq!(names_in(&ps).len(), 2, "no final before phase C");

    let report = resume(&env, &job, &trash);
    assert_eq!(report.items[0].status, ItemStatus::Extracted, "{report:?}");
    assert_eq!(names_in(&ps), final_names(&entries));
    assert_extracted(&env, "Next Tetris, The", 2, &entries);
    // The killed run's intents are closed, never left open for undo.
    let records = job_transfers(&env.data, &job).unwrap();
    let open: Vec<&TransferRecord> = records
        .iter()
        .filter(|r| {
            !r.finished
                && r.extracted.as_ref().is_some_and(|x| !x.rolled_back)
                && completions(&records, &r.target).iter().all(|c| {
                    c.extracted.as_ref().unwrap().temp != r.extracted.as_ref().unwrap().temp
                })
        })
        .collect();
    assert!(open.is_empty(), "{open:?}");
    assert_eq!(
        fs::read(&archive).unwrap().len() as u64,
        fp_of(&archive).size
    );
}

#[test]
fn crash_after_5_of_12_resume_finishes_no_double_files() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, entries) = disc_archive(&env, "Tetris Plus", 11, "PS");
    assert_eq!(entries.len(), 12);
    let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Keep, None);
    let plan = plan_of(&env, vec![item], TransferMode::Copy);
    // Twelve intents, five completions; the sixth is renamed, its line
    // never written; the last six are still temps.
    let job = apply_killed(&env, &plan, &trash, 12 + 5).job.unwrap();
    let ps = env.root.join("PS");
    assert_eq!(temps_in(&ps).len(), 6, "{:?}", names_in(&ps));
    let finals: Vec<PathBuf> = names_in(&ps)
        .into_iter()
        .filter(|n| !n.starts_with(".coverly-"))
        .map(|n| ps.join(n))
        .collect();
    assert_eq!(finals.len(), 6);
    for f in &finals {
        filetime::set_file_mtime(f, marker()).unwrap();
    }

    let report = resume(&env, &job, &trash);
    assert_eq!(report.items[0].status, ItemStatus::Extracted, "{report:?}");
    assert_eq!(
        names_in(&ps),
        final_names(&entries),
        "no (2) copies, no temps"
    );
    assert_extracted(&env, "Tetris Plus", 11, &entries);
    for f in &finals {
        assert_eq!(mtime(f), marker(), "{} rewritten", f.display());
    }
    let records = job_transfers(&env.data, &job).unwrap();
    for e in &entries {
        let done = completions(&records, &env.root.join(&e.target));
        assert_eq!(done.len(), 1, "{}: {done:?}", e.name);
    }
}

#[test]
fn crash_between_rename_and_completion_adopted_by_crc() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, entries) = disc_archive(&env, "Next Tetris, The", 2, "PS");
    let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Keep, None);
    let plan = plan_of(&env, vec![item], TransferMode::Copy);
    // Three intents, one completion; the second final is renamed only.
    let job = apply_killed(&env, &plan, &trash, 3 + 1).job.unwrap();
    let records = job_transfers(&env.data, &job).unwrap();
    let second = &entries[1];
    let target = env.root.join(&second.target);
    assert!(target.is_file());
    assert!(completions(&records, &target).is_empty());
    let intent_temp = records
        .iter()
        .find(|r| r.target == target)
        .and_then(|r| r.extracted.as_ref())
        .map(|x| x.temp.clone())
        .unwrap();
    filetime::set_file_mtime(&target, marker()).unwrap();

    let report = resume(&env, &job, &trash);
    assert_eq!(report.items[0].status, ItemStatus::Extracted, "{report:?}");
    assert_eq!(mtime(&target), marker(), "adopted, not rewritten");
    assert_extracted(&env, "Next Tetris, The", 2, &entries);
    assert!(temps_in(&env.root.join("PS")).is_empty());
    // Its completion line closes the killed run's intent.
    let records = job_transfers(&env.data, &job).unwrap();
    let done = completions(&records, &target);
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].extracted.as_ref().unwrap().temp, intent_temp);
    // And undo knows the file as the job's.
    let undo = undo_job_with(&env.data, &job, &trash).unwrap();
    assert!(undo.removed.contains(&target), "{undo:?}");
    assert!(names_in(&env.root.join("PS")).is_empty());
}

#[test]
fn a_renamed_final_with_other_bytes_is_not_adopted() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, entries) = disc_archive(&env, "Next Tetris, The", 2, "PS");
    let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Keep, None);
    let plan = plan_of(&env, vec![item], TransferMode::Copy);
    let job = apply_killed(&env, &plan, &trash, 3 + 1).job.unwrap();
    // Same size, other bytes: not provably the entry, so not the job's.
    let target = env.root.join(&entries[1].target);
    let mut bytes = fs::read(&target).unwrap();
    bytes[10] ^= 0xff;
    fs::write(&target, &bytes).unwrap();

    let report = resume(&env, &job, &trash);
    assert_eq!(
        report.items[0].status,
        ItemStatus::Skipped {
            reason: SkipReason::CollisionSet
        },
        "{report:?}"
    );
    assert_eq!(fs::read(&target).unwrap(), bytes, "never touched");
    assert!(temps_in(&env.root.join("PS")).is_empty());
}

#[test]
fn crash_before_playlist_resume_writes_it() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, entries) = disc_archive(&env, "Alpha", 2, "PS/Alpha");
    let m3u = PathBuf::from("PS/Alpha/Alpha.m3u");
    let item = extract_item(
        &env,
        &archive,
        entries.clone(),
        ArchiveAfter::Keep,
        Some(m3u.clone()),
    );
    let plan = plan_of(&env, vec![item], TransferMode::Copy);
    let job = apply_killed(&env, &plan, &trash, 3 + 3).job.unwrap();
    assert!(!env.root.join(&m3u).exists());

    let report = resume(&env, &job, &trash);
    assert_eq!(report.items[0].status, ItemStatus::Extracted, "{report:?}");
    assert_eq!(
        fs::read_to_string(env.root.join(&m3u)).unwrap(),
        "Alpha.cue\n"
    );
    assert!(report.items[0].targets.contains(&env.root.join(&m3u)));
    let undo = undo_job_with(&env.data, &job, &trash).unwrap();
    assert!(undo.removed.contains(&env.root.join(&m3u)), "{undo:?}");
}

#[test]
fn crash_before_trash_call_resume_trashes() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, entries) = disc_archive(&env, "Next Tetris, The", 2, "PS");
    let before = fs::read(&archive).unwrap();
    let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash, None);
    let plan = plan_of(&env, vec![item], TransferMode::Move);
    // Three intents, three completions, the `archive-trash` intent.
    let job = apply_killed(&env, &plan, &trash, 3 + 3 + 1).job.unwrap();
    assert!(archive.is_file());
    assert_eq!(trash.trashed(), 0);
    assert!(matches!(
        last_step(&env, &job, 0),
        Some(ArchiveStep::TrashIntent { .. })
    ));

    let trash = FakeTrash::new(&env.trash);
    let report = resume(&env, &job, &trash);
    assert_eq!(report.items[0].status, ItemStatus::Extracted, "{report:?}");
    assert!(!archive.exists());
    assert_eq!(trash.trashed(), 1);
    assert!(matches!(
        last_step(&env, &job, 0),
        Some(ArchiveStep::Trashed { trash: Some(_) })
    ));
    assert_extracted(&env, "Next Tetris, The", 2, &entries);

    let undo = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(fs::read(&archive).unwrap(), before);
    assert!(undo.archive_lost.is_empty(), "{undo:?}");
}

#[test]
fn crash_during_trash_call_found_in_trash() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, entries) = disc_archive(&env, "Next Tetris, The", 2, "PS");
    let before = fs::read(&archive).unwrap();
    let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash, None);
    let plan = plan_of(&env, vec![item], TransferMode::Move);
    let job = apply_killed(&env, &plan, &trash, 3 + 3 + 1).job.unwrap();
    // The OS finished the Trash call; the process died before its line.
    let at = trash.trash(&archive).unwrap().unwrap();

    let trash = FakeTrash::new(&env.trash);
    let report = resume(&env, &job, &trash);
    assert_eq!(report.items[0].status, ItemStatus::Extracted, "{report:?}");
    assert_eq!(trash.trashed(), 0, "never trashed twice");
    assert_eq!(
        last_step(&env, &job, 0),
        Some(ArchiveStep::Trashed { trash: Some(at) })
    );
    assert_extracted(&env, "Next Tetris, The", 2, &entries);

    let undo = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(fs::read(&archive).unwrap(), before);
    assert_eq!(undo.restored, [archive]);
}

#[test]
fn archive_changed_since_plan_rolls_back_committed() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, entries) = disc_archive(&env, "Next Tetris, The", 2, "PS");
    let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash, None);
    let plan = plan_of(&env, vec![item], TransferMode::Move);
    // One final committed, one renamed without its line, one temp.
    let job = apply_killed(&env, &plan, &trash, 3 + 1).job.unwrap();
    let ps = env.root.join("PS");
    assert_eq!(names_in(&ps).len(), 3);
    // The download is replaced meanwhile.
    fs::write(&archive, b"another download under the same name").unwrap();

    let report = resume(&env, &job, &trash);
    assert_eq!(
        report.items[0].status,
        ItemStatus::Skipped {
            reason: SkipReason::ChangedSincePlan
        },
        "{report:?}"
    );
    assert!(names_in(&ps).is_empty(), "{:?}", names_in(&ps));
    assert_eq!(
        fs::read(&archive).unwrap(),
        b"another download under the same name"
    );
    assert_eq!(trash.trashed(), 0);
    // Every intent is closed.
    let records = job_transfers(&env.data, &job).unwrap();
    assert!(records.iter().filter(|r| !r.finished).all(|r| r
        .extracted
        .as_ref()
        .unwrap()
        .rolled_back));
}

#[test]
fn item_done_only_when_archive_line_final() {
    // Killed before the first item's Trash call: that item is not done,
    // its archive still goes to the Trash on resume.
    for (lines, trashed_by_crash) in [(3 + 3 + 1, 0), (3 + 3 + 2, 1)] {
        let env = env();
        let trash = FakeTrash::new(&env.trash);
        let (a, a_entries) = disc_archive(&env, "Alpha", 2, "PS");
        let (b, b_entries) = disc_archive(&env, "Beta", 2, "PS");
        let items = vec![
            extract_item(&env, &a, a_entries.clone(), ArchiveAfter::Trash, None),
            extract_item(&env, &b, b_entries.clone(), ArchiveAfter::Trash, None),
        ];
        let plan = plan_of(&env, items, TransferMode::Move);
        let crashed = apply_killed(&env, &plan, &trash, lines);
        let job = crashed.job.unwrap();
        assert_eq!(trash.trashed(), trashed_by_crash, "{lines}");
        assert_eq!(a.exists(), trashed_by_crash == 0);
        let lines_of_a = |env: &Env| {
            job_transfers(&env.data, &job)
                .unwrap()
                .iter()
                .filter(|r| r.item == Some(0))
                .count()
                + job_archive_steps(&env.data, &job)
                    .unwrap()
                    .iter()
                    .filter(|s| s.item == Some(0))
                    .count()
        };
        let a_before = lines_of_a(&env);

        let again = FakeTrash::new(&env.trash);
        let report = resume(&env, &job, &again);
        assert_eq!(report.extracted, 2, "{report:?}");
        assert!(report
            .items
            .iter()
            .all(|o| o.status == ItemStatus::Extracted));
        assert!(!a.exists() && !b.exists());
        assert_eq!(trash.trashed() + again.trashed(), 2);
        if trashed_by_crash == 1 {
            assert_eq!(lines_of_a(&env), a_before, "a done item is left alone");
        } else {
            assert!(matches!(
                last_step(&env, &job, 0),
                Some(ArchiveStep::Trashed { .. })
            ));
        }
        assert_extracted(&env, "Alpha", 2, &a_entries);
        assert_extracted(&env, "Beta", 2, &b_entries);
        assert!(temps_in(&env.root.join("PS")).is_empty());
    }
}
