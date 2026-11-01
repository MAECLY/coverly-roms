//! Undo of extracted items (archives spec §9.2): the archive comes back
//! from the Trash first, then the extracted files go; an archive that
//! cannot come back keeps its extracted files (they are the only copy).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::UNIX_EPOCH;

use coverly_core::archive::testing::{sevenz, SevenZShape};
use coverly_core::archive::{file_entries, open_archive, Limits};
use coverly_core::organize::{
    apply_organize, ApplyEnv, ArchiveAfter, Counts, ExtractEntry, ExtractSource, ItemKind,
    ItemStatus, NameMode, OrganizeAction, OrganizeItem, OrganizePlan, OrganizeReport, SheetCheck,
    SourceFp, Transfer, TransferMode,
};
use coverly_core::write::trash::{FakeTrash, TrashBackend};
use coverly_core::write::{
    list_jobs, undo_job_with, ExtractedFile, JobKind, JobMeta, WriteError, Writer,
};
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

/// Synthetic disc tracks (no copyrighted bytes) and their sheet.
fn disc_files(stem: &str) -> Vec<(String, Vec<u8>)> {
    let t1 = format!("{stem} (Track 1).bin");
    let t2 = format!("{stem} (Track 2).bin");
    let cue = format!(
        "FILE \"{t1}\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n\
         FILE \"{t2}\" BINARY\r\n  TRACK 02 AUDIO\r\n    INDEX 01 00:00:00\r\n"
    );
    vec![
        (t1, (0..200_000u32).map(|i| (i % 251) as u8).collect()),
        (t2, (0..80_000u32).map(|i| (i % 13) as u8).collect()),
        (format!("{stem}.cue"), cue.into_bytes()),
    ]
}

/// A solid 7z of `stem`'s disc in the download folder, misnamed `.zip`
/// like the real case, and its plan entries under `folder`.
fn disc_archive(env: &Env, stem: &str, folder: &str) -> (PathBuf, Vec<ExtractEntry>) {
    let files = disc_files(stem);
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
        title_id: None,
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
        duplicates: Vec::new(),
        set_aside_trash: false,
        updates: 0,
    }
}

fn apply(env: &Env, plan: &OrganizePlan, trash: &dyn TrashBackend) -> OrganizeReport {
    let progress = |_| {};
    let aenv = ApplyEnv {
        data_dir: &env.data,
        progress: &progress,
        cancel: &AtomicBool::new(false),
        trash,
        tools: Some(&[]),
    };
    apply_organize(plan, &aenv).unwrap()
}

/// One disc item applied: the archive, its entries, the job id.
fn applied_disc(
    env: &Env,
    mode: TransferMode,
    after: ArchiveAfter,
    trash: &dyn TrashBackend,
) -> (PathBuf, Vec<u8>, Vec<ExtractEntry>, String) {
    let (archive, entries) = disc_archive(env, "Next Tetris, The", "PS");
    let before = fs::read(&archive).unwrap();
    let item = extract_item(env, &archive, entries.clone(), after, None);
    let report = apply(env, &plan_of(env, vec![item], mode), trash);
    assert!(
        matches!(
            report.items[0].status,
            ItemStatus::Extracted | ItemStatus::ExtractedKept
        ),
        "{report:?}"
    );
    (archive, before, entries, report.job.unwrap())
}

fn targets(env: &Env, entries: &[ExtractEntry]) -> Vec<PathBuf> {
    entries.iter().map(|e| env.root.join(&e.target)).collect()
}

fn sorted(mut v: Vec<PathBuf>) -> Vec<PathBuf> {
    v.sort();
    v
}

/// No job temp anywhere under `dir`, recursively.
fn assert_no_temps(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        assert!(!name.starts_with(".coverly-"), "temp left: {name}");
        if e.file_type().unwrap().is_dir() {
            assert_no_temps(&e.path());
        }
    }
}

fn is_undone(env: &Env, job: &str) -> bool {
    list_jobs(&env.data)
        .unwrap()
        .iter()
        .any(|j| j.job == job && j.undone)
}

#[test]
fn undo_restores_archive_from_fake_trash_and_removes_files() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, before, entries, job) =
        applied_disc(&env, TransferMode::Move, ArchiveAfter::Trash, &trash);
    assert!(!archive.exists(), "move mode trashed the archive");
    assert!(targets(&env, &entries).iter().all(|t| t.is_file()));

    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(
        fs::read(&archive).unwrap(),
        before,
        "archive back, same bytes"
    );
    assert!(targets(&env, &entries).iter().all(|t| !t.exists()));
    assert_eq!(report.restored, std::slice::from_ref(&archive));
    assert_eq!(
        sorted(report.removed.clone()),
        sorted(targets(&env, &entries))
    );
    assert!(report.archive_lost.is_empty());
    assert!(report.skipped_modified.is_empty());
    // The job made `PS/`; it holds nothing now.
    assert_eq!(report.dirs_removed, [env.root.join("PS")]);
    assert!(!env.root.join("PS").exists());
    assert!(is_undone(&env, &job));
}

#[test]
fn undo_trash_emptied_keeps_extracted_files_and_reports() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, _, entries, job) =
        applied_disc(&env, TransferMode::Move, ArchiveAfter::Trash, &trash);
    fs::remove_dir_all(&env.trash).unwrap();

    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(report.archive_lost, std::slice::from_ref(&archive));
    assert!(!archive.exists());
    // The extracted files are the only copy of the game now: they stay.
    assert!(targets(&env, &entries).iter().all(|t| t.is_file()));
    assert!(report.removed.is_empty() && report.restored.is_empty());
    assert!(report.dirs_removed.is_empty());
    assert!(env.root.join("PS").is_dir());
}

#[test]
fn undo_archive_occupied_by_another_file_is_lost_too() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, _, entries, job) =
        applied_disc(&env, TransferMode::Move, ArchiveAfter::Trash, &trash);
    fs::write(&archive, b"a different download").unwrap();

    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(report.archive_lost, std::slice::from_ref(&archive));
    assert_eq!(fs::read(&archive).unwrap(), b"a different download");
    assert!(targets(&env, &entries).iter().all(|t| t.is_file()));
}

#[test]
fn undo_finds_archive_in_trash_when_the_trash_line_is_missing() {
    // A crash during the Trash call: the intent is journaled, the archive
    // went to the Trash, the location line never made it.
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, before, entries, job) =
        applied_disc(&env, TransferMode::Move, ArchiveAfter::Keep, &trash);
    let journal = env.data.join("jobs").join(format!("{job}.jsonl"));
    let text = fs::read_to_string(&journal).unwrap();
    let fp = fp_of(&archive);
    let intent = serde_json::json!({
        "type": "entry",
        "target": archive,
        "op": {"kind": "archive-trash", "from": archive},
        "fp": {"size": fp.size, "mtime_ns": fp.mtime_ns},
        "item": 0,
    });
    let (body, done) = text.trim_end().rsplit_once('\n').unwrap();
    assert!(done.contains("\"done\""), "{done}");
    fs::write(&journal, format!("{body}\n{intent}\n{done}\n")).unwrap();
    trash.trash(&archive).unwrap();
    assert!(!archive.exists());

    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(fs::read(&archive).unwrap(), before);
    assert_eq!(report.restored, [archive]);
    assert!(targets(&env, &entries).iter().all(|t| !t.exists()));
}

#[test]
fn undo_copy_mode_only_deletes_extracted() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, before, entries, job) =
        applied_disc(&env, TransferMode::Copy, ArchiveAfter::Keep, &trash);

    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(fs::read(&archive).unwrap(), before);
    assert!(report.restored.is_empty());
    assert_eq!(sorted(report.removed), sorted(targets(&env, &entries)));
    assert!(targets(&env, &entries).iter().all(|t| !t.exists()));
    assert_eq!(trash.trashed(), 0);
}

#[test]
fn undo_modified_extracted_file_skipped() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (_, _, entries, job) = applied_disc(&env, TransferMode::Copy, ArchiveAfter::Keep, &trash);
    let all = targets(&env, &entries);
    let edited = all
        .iter()
        .find(|t| t.extension().unwrap() == "cue")
        .unwrap()
        .clone();
    // Same size, other bytes, other mtime: the user's file now.
    let mut bytes = fs::read(&edited).unwrap();
    bytes[0] ^= 0x20;
    fs::write(&edited, &bytes).unwrap();
    filetime::set_file_mtime(
        &edited,
        filetime::FileTime::from_unix_time(1_600_000_000, 0),
    )
    .unwrap();

    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(report.skipped_modified, std::slice::from_ref(&edited));
    assert_eq!(fs::read(&edited).unwrap(), bytes);
    let rest: Vec<PathBuf> = all.into_iter().filter(|t| *t != edited).collect();
    assert_eq!(sorted(report.removed), sorted(rest.clone()));
    assert!(rest.iter().all(|t| !t.exists()));
    // The folder still holds the user's file.
    assert!(env.root.join("PS").is_dir());
}

#[test]
fn undo_extracted_file_with_only_its_mtime_changed_is_removed() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (_, _, entries, job) = applied_disc(&env, TransferMode::Copy, ArchiveAfter::Keep, &trash);
    let all = targets(&env, &entries);
    // A copy tool that touched the file: same bytes, so still the job's.
    filetime::set_file_mtime(
        &all[0],
        filetime::FileTime::from_unix_time(1_600_000_000, 0),
    )
    .unwrap();
    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert!(report.skipped_modified.is_empty());
    assert!(all.iter().all(|t| !t.exists()));
}

#[test]
fn undo_intent_without_completion_deletes_only_after_crc() {
    let env = env();
    let archive = env.src.join("Game.zip");
    fs::write(&archive, b"7z").unwrap();
    fs::create_dir_all(env.root.join("PS")).unwrap();
    let mut w = Writer::begin_with(
        &env.root,
        &env.data,
        JobMeta {
            kind: JobKind::Organize,
            source_root: Some(env.src.clone()),
        },
    )
    .unwrap();
    let job = w.job_id().to_string();
    let good = b"track one bytes";
    let crc = |b: &[u8]| crc32fast::hash(b);
    let intent = |w: &mut Writer, name: &str, data: &[u8]| -> (PathBuf, PathBuf) {
        let temp = w.extract_temp();
        let target = env.root.join("PS").join(name);
        let file = ExtractedFile {
            from: archive.clone(),
            entry: name.to_string(),
            size: data.len() as u64,
            crc32: Some(crc(data)),
            temp: temp.clone(),
        };
        w.extract_intent(&target, &file, Some(0)).unwrap();
        (target, env.root.join("PS").join(temp))
    };
    // Renamed into place, crash before the completion line.
    let (renamed, _) = intent(&mut w, "Game (Track 1).bin", good);
    fs::write(&renamed, good).unwrap();
    // Same size, other bytes: not provably the job's.
    let (other, _) = intent(&mut w, "Game (Track 2).bin", good);
    let mut changed = good.to_vec();
    changed[3] ^= 1;
    fs::write(&other, &changed).unwrap();
    // Crash mid-decode: only the temp exists.
    let (never, temp) = intent(&mut w, "Game.cue", b"FILE");
    fs::write(&temp, b"FI").unwrap();
    drop(w);

    let trash = FakeTrash::new(&env.trash);
    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert!(!renamed.exists());
    assert_eq!(report.removed, [renamed]);
    assert_eq!(fs::read(&other).unwrap(), changed);
    assert_eq!(report.skipped_modified, [other]);
    assert!(!never.exists() && !temp.exists());
    assert_no_temps(&env.root);
    assert_eq!(fs::read(&archive).unwrap(), b"7z");
}

#[test]
fn a_rolled_back_intent_never_touches_its_target() {
    // A rolled-back extraction never wrote its target: whatever is there
    // now (same name, same bytes even) is not the job's.
    let env = env();
    let archive = env.src.join("Game.zip");
    fs::write(&archive, b"7z").unwrap();
    fs::create_dir_all(env.root.join("PS")).unwrap();
    let mut w = Writer::begin_with(
        &env.root,
        &env.data,
        JobMeta {
            kind: JobKind::Organize,
            source_root: Some(env.src.clone()),
        },
    )
    .unwrap();
    let job = w.job_id().to_string();
    let data = b"bytes";
    let target = env.root.join("PS/Game.bin");
    let file = ExtractedFile {
        from: archive.clone(),
        entry: "Game.bin".into(),
        size: data.len() as u64,
        crc32: Some(crc32fast::hash(data)),
        temp: w.extract_temp(),
    };
    w.extract_intent(&target, &file, Some(0)).unwrap();
    w.rolled_back(&target, &file.temp, Some(0)).unwrap();
    w.finish().unwrap();
    fs::write(&target, data).unwrap();

    let report = undo_job_with(&env.data, &job, &FakeTrash::new(&env.trash)).unwrap();
    assert!(target.is_file());
    assert!(report.removed.is_empty() && report.skipped_modified.is_empty());
}

/// The library, download folder and Trash contents, for comparing runs.
fn snapshot(env: &Env) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(read) = fs::read_dir(dir) else {
            return;
        };
        for e in read.flatten() {
            let p = e.path();
            let rel = p.strip_prefix(base).unwrap().to_path_buf();
            if e.file_type().unwrap().is_dir() {
                out.push((rel, Vec::new()));
                walk(base, &p, out);
            } else {
                out.push((rel, fs::read(&p).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    for d in [&env.root, &env.src, &env.trash] {
        walk(d.parent().unwrap(), d, &mut out);
    }
    out.sort();
    out
}

/// `paths` relative to the folder holding the env's folders.
fn rel(env: &Env, paths: &[PathBuf]) -> Vec<PathBuf> {
    let base = env.root.parent().unwrap();
    sorted(
        paths
            .iter()
            .map(|p| p.strip_prefix(base).unwrap().to_path_buf())
            .collect(),
    )
}

#[test]
fn partly_undone_by_hand_undo_finishes_the_rest() {
    // The state an interrupted undo leaves (archive back, one file gone)
    // undoes to the same end as a clean run.
    let clean = env();
    let trash = FakeTrash::new(&clean.trash);
    let (_, _, _, job) = applied_disc(&clean, TransferMode::Move, ArchiveAfter::Trash, &trash);
    let clean_report = undo_job_with(&clean.data, &job, &trash).unwrap();

    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, _, entries, job) =
        applied_disc(&env, TransferMode::Move, ArchiveAfter::Trash, &trash);
    let at = trash.find(&archive, "Next Tetris, The.zip", &{
        let steps = coverly_core::write::job_archive_steps(&env.data, &job).unwrap();
        let coverly_core::write::ArchiveStep::TrashIntent { fp: Some(fp) } = steps[0].step else {
            panic!("{steps:?}")
        };
        fp
    });
    fs::rename(at.unwrap(), &archive).unwrap();
    fs::remove_file(env.root.join(&entries[0].target)).unwrap();

    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(snapshot(&env), snapshot(&clean));
    assert_eq!(
        rel(&env, &report.restored),
        rel(&clean, &clean_report.restored)
    );
    assert!(report.archive_lost.is_empty());
    assert_eq!(report.removed.len(), clean_report.removed.len() - 1);
}

#[test]
fn undo_removes_empty_created_dirs_only() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (a, a_entries) = disc_archive(&env, "Alpha", "PS/Alpha");
    let (b, b_entries) = disc_archive(&env, "Beta", "PS/Beta");
    let m3u = PathBuf::from("PS/Alpha/Alpha.m3u");
    let items = vec![
        extract_item(&env, &a, a_entries, ArchiveAfter::Keep, Some(m3u.clone())),
        extract_item(&env, &b, b_entries, ArchiveAfter::Keep, None),
    ];
    let report = apply(&env, &plan_of(&env, items, TransferMode::Copy), &trash);
    assert!(env.root.join(&m3u).is_file(), "{report:?}");
    let job = report.job.unwrap();
    // The user saves a game next to Beta's tracks.
    let save = env.root.join("PS/Beta/Beta.srm");
    fs::write(&save, b"save").unwrap();
    fs::write(env.root.join("PS/Alpha/.DS_Store"), b"litter").unwrap();

    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(report.dirs_removed, [env.root.join("PS/Alpha")]);
    assert!(!env.root.join("PS/Alpha").exists());
    assert!(!env.root.join(&m3u).exists());
    assert_eq!(fs::read(&save).unwrap(), b"save");
    assert!(env.root.join("PS/Beta").is_dir());
    let names: Vec<String> = fs::read_dir(env.root.join("PS/Beta"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["Beta.srm"]);
    assert!(fs::read(&a).is_ok() && fs::read(&b).is_ok());
}

#[test]
fn a_second_undo_finds_nothing_to_undo() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (archive, before, _, job) =
        applied_disc(&env, TransferMode::Move, ArchiveAfter::Trash, &trash);
    undo_job_with(&env.data, &job, &trash).unwrap();
    assert!(matches!(
        undo_job_with(&env.data, &job, &trash),
        Err(WriteError::NothingToUndo)
    ));
    assert_eq!(fs::read(&archive).unwrap(), before);
}

#[cfg(unix)]
mod unix_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn interrupted_undo_run_twice_same_result() {
        let clean = env();
        let trash = FakeTrash::new(&clean.trash);
        let (_, _, _, job) = applied_disc(&clean, TransferMode::Move, ArchiveAfter::Trash, &trash);
        let clean_report = undo_job_with(&clean.data, &job, &trash).unwrap();

        let env = env();
        let trash = FakeTrash::new(&env.trash);
        let (archive, before, _, job) =
            applied_disc(&env, TransferMode::Move, ArchiveAfter::Trash, &trash);
        // The card's folder turns read-only: the archive comes back, then
        // deleting the first extracted file fails and the undo stops.
        let ps = env.root.join("PS");
        fs::set_permissions(&ps, fs::Permissions::from_mode(0o555)).unwrap();
        let first = undo_job_with(&env.data, &job, &trash);
        fs::set_permissions(&ps, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(first.is_err(), "{first:?}");
        assert_eq!(fs::read(&archive).unwrap(), before);
        assert!(!is_undone(&env, &job));

        let report = undo_job_with(&env.data, &job, &trash).unwrap();
        assert_eq!(snapshot(&env), snapshot(&clean));
        assert_eq!(
            rel(&env, &report.restored),
            rel(&clean, &clean_report.restored)
        );
        assert_eq!(
            rel(&env, &report.removed),
            rel(&clean, &clean_report.removed)
        );
        assert_eq!(
            rel(&env, &report.dirs_removed),
            rel(&clean, &clean_report.dirs_removed)
        );
        assert!(is_undone(&env, &job));
    }
}

#[test]
fn a_lost_archive_keeps_only_its_own_item_and_playlist() {
    // Two games extracted into the same system folder; only Alpha's
    // archive is gone from the Trash. Alpha's tracks, playlist and folder
    // stay (the only copy of that game); Beta is undone as usual.
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (a, a_entries) = disc_archive(&env, "Alpha", "PS/Alpha");
    let (b, b_entries) = disc_archive(&env, "Beta", "PS");
    let m3u = PathBuf::from("PS/Alpha/Alpha.m3u");
    let items = vec![
        extract_item(
            &env,
            &a,
            a_entries.clone(),
            ArchiveAfter::Trash,
            Some(m3u.clone()),
        ),
        extract_item(&env, &b, b_entries.clone(), ArchiveAfter::Trash, None),
    ];
    let report = apply(&env, &plan_of(&env, items, TransferMode::Move), &trash);
    assert!(env.root.join(&m3u).is_file(), "{report:?}");
    assert!(!a.exists() && !b.exists());
    let job = report.job.unwrap();
    // Empty Alpha's slot of the Trash only.
    let alpha_in_trash: Vec<PathBuf> = fs::read_dir(&env.trash)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|slot| slot.join("Alpha.zip").exists())
        .collect();
    assert_eq!(alpha_in_trash.len(), 1);
    fs::remove_dir_all(&alpha_in_trash[0]).unwrap();

    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(report.archive_lost, std::slice::from_ref(&a));
    assert_eq!(report.restored, std::slice::from_ref(&b));
    assert!(fs::read(&b).is_ok() && !a.exists());
    assert!(targets(&env, &a_entries).iter().all(|t| t.is_file()));
    assert!(
        env.root.join(&m3u).is_file(),
        "the playlist stays with Alpha"
    );
    assert!(targets(&env, &b_entries).iter().all(|t| !t.exists()));
    assert_eq!(
        sorted(report.removed.clone()),
        sorted(targets(&env, &b_entries))
    );
    assert!(report.dirs_removed.is_empty(), "{:?}", report.dirs_removed);
    assert!(env.root.join("PS/Alpha").is_dir());
    assert!(is_undone(&env, &job));
}

#[test]
fn undo_puts_the_archive_back_into_a_source_folder_removed_since() {
    let env = env();
    let trash = FakeTrash::new(&env.trash);
    let (made, entries) = disc_archive(&env, "Gamma", "PS");
    let sub = env.src.join("discs");
    fs::create_dir_all(&sub).unwrap();
    let archive = sub.join("Gamma.zip");
    fs::rename(&made, &archive).unwrap();
    let before = fs::read(&archive).unwrap();
    let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash, None);
    let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move), &trash);
    let job = report.job.unwrap();
    assert!(!archive.exists());
    // The user tidies the emptied download folder away.
    let _ = fs::remove_dir_all(&sub);

    let report = undo_job_with(&env.data, &job, &trash).unwrap();
    assert_eq!(fs::read(&archive).unwrap(), before);
    assert_eq!(report.restored, std::slice::from_ref(&archive));
    assert!(report.archive_lost.is_empty());
    assert!(targets(&env, &entries).iter().all(|t| !t.exists()));
}
