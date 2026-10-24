//! Extraction on apply (archives spec §9.1): phase A decodes every entry
//! of an `Extract` item to verified job temps in its target folders.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use coverly_core::archive::testing::{
    sevenz, zip_crafted, zip_with, SevenZShape, ZipMethod, ZipSpec,
};
use coverly_core::archive::{
    open_archive, ArchiveError, ArchiveReader, Budget, Entry, EntryKind, Format, Limits, Method,
    RelPath,
};
use coverly_core::organize::apply::set_free_space_override;
use coverly_core::organize::extract::{
    extract_to_temps, set_write_fault, ExtractError, ExtractRequest, Fault, PhaseA, CANCEL_EVERY,
};
use coverly_core::organize::ExtractEntry;
use coverly_core::progress::PauseKind;
use coverly_core::write::{job_transfers, set_journal_fault, JobKind, JobMeta, Writer};
use tempfile::TempDir;

/// A library root, a download folder and an app data dir.
struct Env {
    _tmp: TempDir,
    root: PathBuf,
    src: PathBuf,
    data: PathBuf,
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
    }
}

impl Env {
    fn writer(&self) -> Writer {
        Writer::begin_with(
            &self.root,
            &self.data,
            JobMeta {
                kind: JobKind::Organize,
                source_root: Some(self.src.clone()),
            },
        )
        .unwrap()
    }
}

/// Synthetic PS1-shaped tracks (no copyrighted bytes).
fn ps1_files() -> Vec<(&'static str, Vec<u8>)> {
    let track1: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let track2: Vec<u8> = (0..120_000u32).map(|i| (i % 13) as u8).collect();
    let cue = b"FILE \"Next Tetris, The (Track 1).bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\nFILE \"Next Tetris, The (Track 2).bin\" BINARY\r\n  TRACK 02 AUDIO\r\n    INDEX 01 00:00:00\r\n".to_vec();
    vec![
        ("Next Tetris, The (Track 1).bin", track1),
        ("Next Tetris, The (Track 2).bin", track2),
        ("Next Tetris, The.cue", cue),
    ]
}

/// The plan entries of every file entry of `reader`, extracted into
/// `PS/<stem>/` (the per-game folder layout).
fn plan_entries(reader: &dyn ArchiveReader, folder: &str) -> Vec<ExtractEntry> {
    coverly_core::archive::file_entries(reader)
        .map(|e| ExtractEntry {
            index: e.index,
            name: e.name.as_str().to_string(),
            size: e.size,
            crc32: e.crc32,
            target: Path::new(folder).join(e.name.as_str()),
        })
        .collect()
}

fn write_archive(env: &Env, name: &str, bytes: &[u8]) -> (PathBuf, Box<dyn ArchiveReader>) {
    let path = env.src.join(name);
    fs::write(&path, bytes).unwrap();
    let reader = open_archive(&path, &Limits::default()).unwrap().unwrap();
    (path, reader)
}

fn ps1_archive(env: &Env) -> (PathBuf, Box<dyn ArchiveReader>) {
    let files = ps1_files();
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    // Solid, sheets stored last: the real case's shape (7z named `.zip`).
    write_archive(
        env,
        "Next Tetris, The.zip",
        &sevenz(&refs, SevenZShape::solid_lzma2_sheets_last()),
    )
}

fn run(
    writer: &mut Writer,
    reader: &mut dyn ArchiveReader,
    archive: &Path,
    entries: &[ExtractEntry],
) -> Result<PhaseA, ExtractError> {
    let cancel = Arc::new(AtomicBool::new(false));
    extract_to_temps(
        writer,
        reader,
        &ExtractRequest {
            item: 0,
            archive,
            entries,
            cancel: &cancel,
            progress: &|_| {},
        },
    )
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

fn journal_kinds(writer: &Writer) -> Vec<String> {
    fs::read_to_string(writer.journal_path())
        .unwrap()
        .lines()
        .filter_map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            v["op"]["kind"].as_str().map(str::to_string)
        })
        .collect()
}

/// After a failed phase A: no temp is left, every intent is rolled back
/// and no final file exists.
fn assert_rolled_back(env: &Env, writer: &Writer, dir: &Path, entries: &[ExtractEntry]) {
    let temps: Vec<String> = names_in(dir)
        .into_iter()
        .filter(|n| n.starts_with(".coverly-"))
        .collect();
    assert!(temps.is_empty(), "temps left: {temps:?}");
    for e in entries {
        assert!(!env.root.join(&e.target).exists(), "{:?}", e.target);
    }
    let records = job_transfers(&env.data, writer.job_id()).unwrap();
    assert!(!records.is_empty());
    for r in &records {
        let x = r.extracted.as_ref().expect("an extracted record");
        assert!(x.rolled_back, "{r:?}");
        assert!(!r.finished, "{r:?}");
    }
}

#[test]
fn phase_a_writes_only_temps() {
    let env = env();
    let (archive, mut reader) = ps1_archive(&env);
    let before = fs::read(&archive).unwrap();
    let entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
    let mut writer = env.writer();
    let a = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap();

    // The hook point: phase A is over. Only job temps exist, no final.
    let dir = env.root.join("PS/Next Tetris, The");
    let names = names_in(&dir);
    assert_eq!(names.len(), 3, "{names:?}");
    let prefix = format!(".coverly-{}-", writer.job_id());
    assert!(
        names
            .iter()
            .all(|n| n.starts_with(&prefix) && n.ends_with(".tmp")),
        "{names:?}"
    );
    assert_eq!(names_in(&env.root.join("PS")), vec!["Next Tetris, The"]);
    let files = ps1_files();
    assert_eq!(a.staged.len(), 3);
    for s in &a.staged {
        let (_, data) = files.iter().find(|(n, _)| *n == s.entry.name).unwrap();
        assert_eq!(&fs::read(&s.temp).unwrap(), data, "{}", s.entry.name);
        assert_eq!(s.crc32, crc32fast::hash(data));
        assert_eq!(s.target, env.root.join(&s.entry.target));
        assert!(!s.target.exists());
        // Only the sheet is kept in memory.
        assert_eq!(s.bytes.is_some(), s.entry.name.ends_with(".cue"));
    }
    let cue = a
        .staged
        .iter()
        .find(|s| s.entry.name.ends_with(".cue"))
        .unwrap();
    assert_eq!(cue.bytes.as_deref(), Some(files[2].1.as_slice()));

    // Journal: two folders made, then three intents, nothing completed.
    assert_eq!(
        journal_kinds(&writer),
        [
            "dir-created",
            "dir-created",
            "extracted",
            "extracted",
            "extracted"
        ]
    );
    let records = job_transfers(&env.data, writer.job_id()).unwrap();
    assert_eq!(records.len(), 3);
    assert!(records
        .iter()
        .all(|r| !r.finished && r.extracted.as_ref().is_some_and(|x| !x.rolled_back)));
    // The archive is untouched.
    assert_eq!(fs::read(&archive).unwrap(), before);
}

#[test]
fn leftover_job_temps_are_cleaned_first() {
    let env = env();
    let (archive, mut reader) = ps1_archive(&env);
    let entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
    let mut writer = env.writer();
    let dir = env.root.join("PS/Next Tetris, The");
    fs::create_dir_all(&dir).unwrap();
    let stale = dir.join(format!(".coverly-{}-999.tmp", writer.job_id()));
    let foreign = dir.join(".coverly-other-job-1.tmp");
    fs::write(&stale, b"half").unwrap();
    fs::write(&foreign, b"not ours").unwrap();
    run(&mut writer, reader.as_mut(), &archive, &entries).unwrap();
    assert!(!stale.exists());
    assert_eq!(fs::read(&foreign).unwrap(), b"not ours");
}

#[test]
fn crc_mismatch_on_reread_rolls_back() {
    let env = env();
    let (archive, mut reader) = ps1_archive(&env);
    let entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
    let mut writer = env.writer();
    set_write_fault(Some(Fault::RereadMismatch));
    let err = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap_err();
    set_write_fault(None);
    assert!(
        matches!(err, ExtractError::RereadMismatch { .. }),
        "{err:?}"
    );
    assert_eq!(err.pause(), None);
    assert_rolled_back(
        &env,
        &writer,
        &env.root.join("PS/Next Tetris, The"),
        &entries,
    );
    assert!(archive.exists());
}

#[test]
fn bad_data_in_the_archive_rolls_back() {
    let env = env();
    let ok: &[u8] = b"good rom";
    let bad: &[u8] = b"bad crc!";
    let specs = [
        ZipSpec::new("Game (Track 1).bin", ok, ZipMethod::Stored),
        // The stored CRC is not the CRC of the data: damaged.
        ZipSpec::new("Game (Track 2).bin", bad, ZipMethod::Stored).declared_crc(0x1234_5678),
    ];
    let (archive, mut reader) = write_archive(&env, "Game.zip", &zip_crafted(&specs));
    let entries = plan_entries(reader.as_ref(), "PS/Game");
    let mut writer = env.writer();
    let err = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap_err();
    assert!(matches!(err, ExtractError::Archive(_)), "{err:?}");
    assert_eq!(err.pause(), None);
    assert_rolled_back(&env, &writer, &env.root.join("PS/Game"), &entries);
}

#[test]
fn a_plan_crc_the_archive_no_longer_lists_is_refused() {
    let env = env();
    let (archive, mut reader) = ps1_archive(&env);
    let mut entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
    // The plan recorded another CRC: the decoded bytes do not match it.
    entries[0].crc32 = entries[0].crc32.map(|c| c ^ 1);
    let mut writer = env.writer();
    let err = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap_err();
    assert!(
        matches!(err, ExtractError::ListingChanged { .. }),
        "{err:?}"
    );
    // Nothing was journaled past the folders.
    assert!(!journal_kinds(&writer).contains(&"extracted".to_string()));
}

#[test]
fn disk_full_mid_entry_rolls_back_and_pauses() {
    let env = env();
    let (archive, mut reader) = ps1_archive(&env);
    let entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
    let total: u64 = entries.iter().map(|e| e.size).sum();
    let mut writer = env.writer();
    set_free_space_override(Some(|_| Some(4096)));
    // Full in the middle of the second entry decoded.
    set_write_fault(Some(Fault::StorageFullAfter(350_000)));
    let err = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap_err();
    set_write_fault(None);
    set_free_space_override(None);
    match err {
        ExtractError::DiskFull { needed, free } => {
            assert_eq!(needed, total);
            assert_eq!(free, 4096);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        err_pause(&env, &archive, &entries),
        Some(PauseKind::DiskFull {
            needed: total,
            free: 4096
        })
    );
    let dir = env.root.join("PS/Next Tetris, The");
    assert_rolled_back(&env, &writer, &dir, &entries);
    let rolled = journal_kinds(&writer)
        .iter()
        .filter(|k| *k == "rolled-back")
        .count();
    assert_eq!(
        rolled, 2,
        "two intents were journaled before the disk filled"
    );
}

/// The pause a disk-full phase A reports (a second run, same fault).
fn err_pause(env: &Env, archive: &Path, entries: &[ExtractEntry]) -> Option<PauseKind> {
    let mut reader = open_archive(archive, &Limits::default()).unwrap().unwrap();
    let mut writer = env.writer();
    set_free_space_override(Some(|_| Some(4096)));
    set_write_fault(Some(Fault::StorageFullAfter(1)));
    let err = run(&mut writer, reader.as_mut(), archive, entries).unwrap_err();
    set_write_fault(None);
    set_free_space_override(None);
    err.pause()
}

#[test]
fn disk_full_journal_volume_stops_before_write() {
    let env = env();
    let (archive, mut reader) = ps1_archive(&env);
    let entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
    let dir = env.root.join("PS/Next Tetris, The");
    fs::create_dir_all(&dir).unwrap();
    let mut writer = env.writer();
    set_free_space_override(Some(|_| Some(10)));
    set_journal_fault(Some(|| Some(io::Error::from(ErrorKind::StorageFull))));
    let err = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap_err();
    set_journal_fault(None);
    set_free_space_override(None);
    assert!(
        matches!(err, ExtractError::DiskFull { free: 10, .. }),
        "{err:?}"
    );
    assert!(matches!(err.pause(), Some(PauseKind::DiskFull { .. })));
    // No intent made it to the journal, so nothing was written.
    assert!(names_in(&dir).is_empty(), "{:?}", names_in(&dir));
    assert!(job_transfers(&env.data, writer.job_id())
        .unwrap()
        .is_empty());
}

#[test]
fn cancel_inside_64mib_entry_returns_within_16mib() {
    let env = env();
    let big = vec![0x5Au8; 64 << 20];
    let (archive, mut reader) = write_archive(
        &env,
        "Big.zip",
        &zip_with(&[("Big (Track 1).bin", &big, ZipMethod::Stored)]),
    );
    drop(big);
    let entries = plan_entries(reader.as_ref(), "PS/Big");
    let mut writer = env.writer();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancelled_at = AtomicU64::new(0);
    let last = AtomicU64::new(0);
    let progress = |bytes: u64| {
        last.store(bytes, Ordering::SeqCst);
        if bytes >= 1 << 20 && !cancel.swap(true, Ordering::SeqCst) {
            cancelled_at.store(bytes, Ordering::SeqCst);
        }
    };
    let err = extract_to_temps(
        &mut writer,
        reader.as_mut(),
        &ExtractRequest {
            item: 0,
            archive: &archive,
            entries: &entries,
            cancel: &cancel,
            progress: &progress,
        },
    )
    .unwrap_err();
    assert!(matches!(err, ExtractError::Cancelled), "{err:?}");
    let at = cancelled_at.load(Ordering::SeqCst);
    let stopped = last.load(Ordering::SeqCst);
    assert!(at > 0);
    assert!(
        stopped - at <= CANCEL_EVERY,
        "cancelled at {at}, stopped at {stopped}"
    );
    assert!(stopped < 64 << 20);
    assert_rolled_back(&env, &writer, &env.root.join("PS/Big"), &entries);
}

/// A backend that streams entry 0, then decodes `skip` MiB it was not
/// asked for (a solid block's unwanted entries: charged to the budget, no
/// sink call), then streams entry 1.
struct SkippingReader {
    entries: Vec<Entry>,
    skip: u64,
    skipped: Arc<AtomicU64>,
    /// Hands entry 0 out twice (a buggy backend).
    repeat_first: bool,
}

fn fake_entry(index: usize, name: &str, size: u64) -> Entry {
    Entry {
        index,
        name: RelPath::parse(name).unwrap(),
        raw_name: name.as_bytes().to_vec(),
        kind: EntryKind::File,
        size,
        packed: None,
        crc32: None,
        encrypted: false,
        method: Method::Stored,
        solid_block: Some(0),
        mtime: None,
        unsafe_name: None,
    }
}

impl SkippingReader {
    fn new(skip: u64, repeat_first: bool) -> SkippingReader {
        SkippingReader {
            entries: vec![
                fake_entry(0, "Game (Track 1).bin", 4),
                fake_entry(1, "Game (Track 2).bin", 4),
            ],
            skip,
            skipped: Arc::new(AtomicU64::new(0)),
            repeat_first,
        }
    }
}

impl ArchiveReader for SkippingReader {
    fn format(&self) -> Format {
        Format::SevenZip
    }
    fn entries(&self) -> &[Entry] {
        &self.entries
    }
    fn open_entry<'a>(
        &'a mut self,
        _: usize,
        _: &'a mut Budget,
    ) -> Result<Box<dyn std::io::Read + 'a>, ArchiveError> {
        unreachable!("phase A streams")
    }
    fn stream_entries(
        &mut self,
        _wanted: &[usize],
        budget: &mut Budget,
        sink: &mut dyn FnMut(&Entry, &mut dyn std::io::Read) -> Result<(), ArchiveError>,
    ) -> Result<(), ArchiveError> {
        sink(&self.entries[0], &mut &b"aaaa"[..])?;
        if self.repeat_first {
            sink(&self.entries[0], &mut &b"aaaa"[..])?;
        }
        for _ in 0..self.skip {
            budget.charge(1 << 20)?;
            self.skipped.fetch_add(1, Ordering::SeqCst);
        }
        sink(&self.entries[1], &mut &b"bbbb"[..])
    }
}

#[test]
fn cancel_reaches_the_decoder_between_wanted_entries() {
    let env = env();
    let archive = env.src.join("Game.7z");
    fs::write(&archive, b"stand-in").unwrap();
    let mut reader = SkippingReader::new(1024, false);
    let skipped = Arc::clone(&reader.skipped);
    let entries = plan_entries(&reader, "PS/Game");
    let mut writer = env.writer();
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = |_: u64| cancel.store(true, Ordering::SeqCst);
    let err = extract_to_temps(
        &mut writer,
        &mut reader,
        &ExtractRequest {
            item: 0,
            archive: &archive,
            entries: &entries,
            cancel: &cancel,
            progress: &progress,
        },
    )
    .unwrap_err();
    assert!(matches!(err, ExtractError::Cancelled), "{err:?}");
    // The 1 GiB the item does not want was not decoded after the cancel.
    assert_eq!(skipped.load(Ordering::SeqCst), 0);
    assert_rolled_back(&env, &writer, &env.root.join("PS/Game"), &entries);
}

#[test]
fn an_entry_streamed_twice_fails_the_item() {
    let env = env();
    let archive = env.src.join("Game.7z");
    fs::write(&archive, b"stand-in").unwrap();
    let mut reader = SkippingReader::new(0, true);
    let entries = plan_entries(&reader, "PS/Game");
    let mut writer = env.writer();
    let err = run(&mut writer, &mut reader, &archive, &entries).unwrap_err();
    assert!(matches!(err, ExtractError::Archive(_)), "{err:?}");
    assert_rolled_back(&env, &writer, &env.root.join("PS/Game"), &entries);
}

#[test]
fn full_journal_volume_while_making_folders_reports_that_volume() {
    let env = env();
    let (archive, mut reader) = ps1_archive(&env);
    let entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
    let mut writer = env.writer();
    // The library has room; the app data volume (journal) has 10 bytes.
    set_free_space_override(Some(|p| {
        Some(
            if p.ends_with("data") || p.parent().is_some_and(|d| d.ends_with("data")) {
                10
            } else {
                1 << 40
            },
        )
    }));
    set_journal_fault(Some(|| Some(io::Error::from(ErrorKind::StorageFull))));
    let err = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap_err();
    set_journal_fault(None);
    set_free_space_override(None);
    match err {
        ExtractError::DiskFull { needed, free } => {
            assert_eq!(free, 10);
            assert_eq!(needed, coverly_core::organize::extract::JOURNAL_HEADROOM);
        }
        other => panic!("{other:?}"),
    }
    assert!(!env.root.join("PS").exists());
}

#[test]
fn set_modified_failure_ignored() {
    let env = env();
    let (archive, mut reader) = ps1_archive(&env);
    let entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
    let mut writer = env.writer();
    set_write_fault(Some(Fault::SetModified));
    let a = run(&mut writer, reader.as_mut(), &archive, &entries);
    set_write_fault(None);
    let a = a.unwrap();
    assert_eq!(a.staged.len(), 3);
    assert!(a.staged.iter().all(|s| s.temp.is_file()));
}

#[test]
fn temps_carry_the_entry_mtime() {
    let env = env();
    let (archive, mut reader) = write_archive(
        &env,
        "Tetris.zip",
        &zip_with(&[("Tetris.gb", b"rom bytes", ZipMethod::Deflate)]),
    );
    let want = reader.entries()[0].mtime.expect("zip entries carry a time");
    let entries = plan_entries(reader.as_ref(), "GB");
    let mut writer = env.writer();
    let a = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap();
    let got = fs::metadata(&a.staged[0].temp).unwrap().modified().unwrap();
    let diff = match got.duration_since(want) {
        Ok(d) => d,
        Err(e) => e.duration(),
    };
    // 2 s: FAT stores even seconds; 100 ns ticks on Windows.
    assert!(diff.as_secs() <= 2, "{got:?} vs {want:?}");
}

#[cfg(unix)]
mod unix_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn target_subfolder_symlink_refused() {
        let env = env();
        let (archive, mut reader) = ps1_archive(&env);
        let entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
        let outside = env.src.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(env.root.join("PS")).unwrap();
        symlink(&outside, env.root.join("PS/Next Tetris, The")).unwrap();
        let mut writer = env.writer();
        let err = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap_err();
        match &err {
            ExtractError::UnsafeTarget { path } => {
                assert_eq!(path, &env.root.join("PS/Next Tetris, The"))
            }
            other => panic!("{other:?}"),
        }
        assert!(names_in(&outside).is_empty());
        assert!(journal_kinds(&writer).is_empty());
    }

    #[test]
    fn a_symlinked_system_folder_is_refused_too() {
        let env = env();
        let (archive, mut reader) = ps1_archive(&env);
        let entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
        let outside = env.src.join("outside");
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, env.root.join("PS")).unwrap();
        let mut writer = env.writer();
        let err = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap_err();
        assert!(matches!(err, ExtractError::UnsafeTarget { .. }), "{err:?}");
        assert!(names_in(&outside).is_empty());
    }

    #[test]
    fn planted_symlink_at_temp_name_refused() {
        let env = env();
        let (archive, mut reader) = ps1_archive(&env);
        let entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
        let dir = env.root.join("PS/Next Tetris, The");
        fs::create_dir_all(&dir).unwrap();
        let victim = env.src.join("victim.txt");
        fs::write(&victim, b"precious").unwrap();
        let mut writer = env.writer();
        // Every temp name the job could use first points at the victim.
        let planted: Vec<PathBuf> = (0..16)
            .map(|n| dir.join(format!(".coverly-{}-{n}.tmp", writer.job_id())))
            .collect();
        for p in &planted {
            symlink(&victim, p).unwrap();
        }
        let err = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap_err();
        assert!(matches!(err, ExtractError::UnsafeTarget { .. }), "{err:?}");
        assert_eq!(fs::read(&victim).unwrap(), b"precious");
        // Foreign links are never deleted either.
        assert!(planted.iter().all(|p| p.symlink_metadata().is_ok()));
        let records = job_transfers(&env.data, writer.job_id()).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].extracted.as_ref().unwrap().rolled_back);
    }
}

#[cfg(windows)]
mod windows_tests {
    use super::*;

    #[test]
    fn target_junction_refused() {
        let env = env();
        let (archive, mut reader) = ps1_archive(&env);
        let entries = plan_entries(reader.as_ref(), "PS/Next Tetris, The");
        let outside = env.src.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(env.root.join("PS")).unwrap();
        let link = env.root.join("PS").join("Next Tetris, The");
        let status = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&link)
            .arg(&outside)
            .status()
            .unwrap();
        assert!(status.success());
        let mut writer = env.writer();
        let err = run(&mut writer, reader.as_mut(), &archive, &entries).unwrap_err();
        assert!(matches!(err, ExtractError::UnsafeTarget { .. }), "{err:?}");
        assert!(names_in(&outside).is_empty());
    }
}
