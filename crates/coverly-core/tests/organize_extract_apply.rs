//! Extraction on apply (archives spec §9.1): phase A decodes every entry
//! of an `Extract` item to verified job temps in its target folders.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use coverly_core::archive::testing::{
    sevenz, zip_crafted, zip_with, SevenZShape, ZipMethod, ZipSpec,
};
use coverly_core::archive::{
    open_archive, ArchiveError, ArchiveReader, Budget, Entry, EntryKind, Format, Limits, Method,
    RelPath,
};
use coverly_core::organize::apply::set_free_space_override;
use coverly_core::organize::extract::{
    extract_to_temps, set_write_fault, ExtractError, ExtractRequest, ExtractTick, Fault, PhaseA,
    CANCEL_EVERY,
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
    let progress = |t: ExtractTick| {
        let bytes = t.bytes;
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

/// D2: at every tick, the staged entries are the first `entries_done` of
/// the plan entries by archive index, whatever order the plan lists them.
#[test]
fn staged_entries_are_a_prefix_by_archive_index() {
    let env = env();
    let files: Vec<(String, Vec<u8>)> = (1..=5)
        .map(|i| (format!("Game (Track {i}).bin"), vec![i as u8; 300_000 * i]))
        .collect();
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice()))
        .collect();
    let (archive, mut reader) = write_archive(
        &env,
        "Game.7z",
        &sevenz(&refs, SevenZShape::solid_lzma2_sheets_last()),
    );
    let mut entries = plan_entries(reader.as_ref(), "PS/Game");
    entries.reverse(); // the plan lists them out of archive order
    let mut writer = env.writer();
    let cancel = Arc::new(AtomicBool::new(false));
    let ticks: Mutex<Vec<ExtractTick>> = Mutex::default();
    let progress = |t: ExtractTick| ticks.lock().unwrap().push(t);
    let done = extract_to_temps(
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
    .unwrap();
    let ticks = ticks.into_inner().unwrap();
    let staged: Vec<usize> = done.staged.iter().map(|s| s.entry.index).collect();
    let mut by_index: Vec<usize> = entries.iter().map(|e| e.index).collect();
    by_index.sort_unstable();
    assert_eq!(staged, by_index, "staged in ascending archive index");
    assert_eq!(ticks.first().unwrap().entries_done, 0);
    assert_eq!(ticks.last().unwrap().entries_done, entries.len());
    assert!(ticks
        .windows(2)
        .all(|w| w[0].entries_done <= w[1].entries_done));
    assert!(ticks.windows(2).all(|w| w[0].bytes <= w[1].bytes));
    // Every count from 1 to 5 is reported right after its entry is staged.
    for k in 1..=entries.len() {
        assert!(ticks.iter().any(|t| t.entries_done == k), "no tick for {k}");
    }
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
    let progress = |_: ExtractTick| cancel.store(true, Ordering::SeqCst);
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

// ---------------------------------------------------------------------------
// Apply phases B–D, item status and progress (plan Task 32)
// ---------------------------------------------------------------------------

mod apply_phases {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use coverly_core::archive::external::FoundTool;
    use coverly_core::archive::testing::{
        answer, fake_tool, read_args, tar_gz, unrar_l, unrar_listing_script, unrar_lt, ToolEntry,
        UNRAR_READ_ARGS,
    };
    use coverly_core::archive::{open_archive_with, Tool};
    use coverly_core::organize::extract::{set_phase_hook, HookPoint};
    use coverly_core::organize::{
        apply_organize, ApplyEnv, ArchiveAfter, Counts, ExtractSource, ItemKind, ItemStatus,
        NameMode, OrganizeAction, OrganizeItem, OrganizePhase, OrganizePlan, OrganizeProgress,
        OrganizeReport, SheetCheck, SkipReason, SourceFp, Transfer, TransferMode,
    };
    use coverly_core::write::trash::{FakeTrash, TrashBackend};
    use coverly_core::write::{job_archive_steps, ArchiveStep};

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

    /// One automatic `Extract` item for `archive` (inside `env.src`).
    fn extract_item(
        env: &Env,
        archive: &Path,
        entries: Vec<ExtractEntry>,
        after: ArchiveAfter,
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
                m3u: None,
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
            duplicates: Vec::new(),
            set_aside_trash: false,
        }
    }

    fn set_transfer(
        item: &mut OrganizeItem,
        f: impl FnOnce(&mut SheetCheck, &mut ExtractSource, &mut Option<PathBuf>),
    ) {
        let Transfer::Extract {
            sheet_check,
            source,
            m3u,
            ..
        } = &mut item.transfer
        else {
            unreachable!()
        };
        f(sheet_check, source, m3u);
    }

    fn trash_dir(env: &Env) -> PathBuf {
        env.data.parent().unwrap().join("Trash")
    }

    fn apply_full(
        env: &Env,
        plan: &OrganizePlan,
        trash: &dyn TrashBackend,
        tools: Option<&[FoundTool]>,
        progress: &(dyn Fn(OrganizeProgress) + Sync),
    ) -> OrganizeReport {
        let env = ApplyEnv {
            data_dir: &env.data,
            progress,
            cancel: &AtomicBool::new(false),
            trash,
            tools,
        };
        apply_organize(plan, &env).unwrap()
    }

    fn apply(env: &Env, plan: &OrganizePlan) -> OrganizeReport {
        let trash = FakeTrash::new(trash_dir(env));
        apply_full(env, plan, &trash, Some(&[]), &|_| {})
    }

    fn status(report: &OrganizeReport) -> &ItemStatus {
        &report.items[0].status
    }

    fn job_kinds(env: &Env, report: &OrganizeReport) -> Vec<String> {
        let job = report.job.as_deref().expect("a job");
        fs::read_to_string(env.data.join("jobs").join(format!("{job}.jsonl")))
            .unwrap()
            .lines()
            .filter_map(|l| {
                let v: serde_json::Value = serde_json::from_str(l).unwrap();
                v["op"]["kind"].as_str().map(str::to_string)
            })
            .collect()
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

    /// Nothing of the item reached the library and the archive is as it
    /// was.
    fn assert_untouched(env: &Env, archive: &Path, before: &[u8], entries: &[ExtractEntry]) {
        for e in entries {
            assert!(!env.root.join(&e.target).exists(), "{:?}", e.target);
        }
        assert_no_temps(&env.root);
        assert_eq!(fs::read(archive).unwrap(), before);
    }

    fn ps1(env: &Env, folder: &str) -> (PathBuf, Vec<ExtractEntry>) {
        let (archive, reader) = ps1_archive(env);
        let entries = plan_entries(reader.as_ref(), folder);
        (archive, entries)
    }

    // --- the real planner, end to end ---

    struct NoCloud;

    impl coverly_core::organize::MetaProbe for NoCloud {
        fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
            false
        }
    }

    #[test]
    fn extract_verify_rename_end_to_end() {
        use coverly_core::archive::testing::{sevenz, SevenZShape};
        use coverly_core::disc::testing as dt;
        use coverly_core::organize::{plan_organize, OrganizeEnv, OrganizeOptions};

        let env = env();
        let t1 = "Next Tetris, The (USA) (Track 01).bin";
        let t2 = "Next Tetris, The (USA) (Track 02).bin";
        let cue = format!(
            "FILE \"{t1}\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n\
             FILE \"{t2}\" BINARY\r\n  TRACK 02 AUDIO\r\n    INDEX 01 00:00:00\r\n"
        );
        let track1 = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");
        let track2 = vec![7u8; 2352 * 4];
        let dir = "Next Tetris, The (USA)";
        let files: Vec<(String, Vec<u8>)> = vec![
            (format!("{dir}/{t1}"), track1.clone()),
            (format!("{dir}/{t2}"), track2.clone()),
            (
                format!("{dir}/Next Tetris, The (USA).cue"),
                cue.clone().into_bytes(),
            ),
        ];
        let refs: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(n, d)| (n.as_str(), d.as_slice()))
            .collect();
        let archive = env.src.join("Next Tetris, The.zip");
        fs::write(
            &archive,
            sevenz(&refs, SevenZShape::solid_lzma2_sheets_last()),
        )
        .unwrap();
        let hour_ago = SystemTime::now() - std::time::Duration::from_secs(3600);
        filetime::set_file_mtime(&archive, filetime::FileTime::from_system_time(hour_ago)).unwrap();

        let db = coverly_core::systems::SystemDb::builtin();
        let profiles = coverly_core::profiles::builtin_profiles();
        let cache = coverly_core::cache::Cache::in_memory().unwrap();
        let penv = OrganizeEnv {
            db: &db,
            profiles: &profiles,
            cache: &cache,
            dats: None,
            probe: &NoCloud,
            online: None,
            now: SystemTime::now(),
        };
        let opts = OrganizeOptions {
            layout: "anbernic-stock".into(),
            target: Some(env.root.clone()),
            mode: Some(TransferMode::Move),
            offline: true,
            ..OrganizeOptions::default()
        };
        let plan = plan_organize(&env.src, &opts, &penv, &|_| {}, &AtomicBool::new(false)).unwrap();
        let item = plan
            .items
            .iter()
            .find(|i| i.sources[0] == Path::new("Next Tetris, The.zip"))
            .unwrap();
        assert!(matches!(
            item.transfer,
            Transfer::Extract {
                archive_after: ArchiveAfter::Trash,
                ..
            }
        ));

        let trash = FakeTrash::new(trash_dir(&env));
        let report = apply_full(&env, &plan, &trash, Some(&[]), &|_| {});
        let out = report.items.iter().find(|o| o.item == item.id).unwrap();
        assert_eq!(out.status, ItemStatus::Extracted, "{report:?}");
        assert_eq!(report.extracted, 1);
        assert_eq!(out.targets.len(), 3);
        let by_name = |name: &str| {
            out.targets
                .iter()
                .find(|t| t.file_name().unwrap() == name)
                .unwrap_or_else(|| panic!("no {name} in {:?}", out.targets))
        };
        assert_eq!(fs::read(by_name(t1)).unwrap(), track1);
        assert_eq!(fs::read(by_name(t2)).unwrap(), track2);
        assert_eq!(
            fs::read(by_name("Next Tetris, The (USA).cue")).unwrap(),
            cue.as_bytes()
        );
        // Flat on the Anbernic card: the archive's top folder is dropped.
        assert!(out
            .targets
            .iter()
            .all(|t| t.parent().unwrap().ends_with("PS")));
        assert_no_temps(&env.root);
        assert!(!archive.exists());
        assert_eq!(trash.trashed(), 1);
        let kinds = job_kinds(&env, &report);
        let extracted = kinds.iter().filter(|k| *k == "extracted").count();
        assert_eq!(extracted, 6, "intent + completion per entry: {kinds:?}");
        assert_eq!(
            &kinds[kinds.len() - 2..],
            ["archive-trash", "archive-trashed"]
        );
    }

    #[test]
    fn no_final_file_before_phase_c() {
        let env = env();
        let (archive, entries) = ps1(&env, "PS");
        let finals: Vec<PathBuf> = entries.iter().map(|e| env.root.join(&e.target)).collect();
        let seen: Rc<RefCell<Vec<(HookPoint, usize, usize)>>> = Rc::default();
        let (log, dir) = (Rc::clone(&seen), env.root.join("PS"));
        let watched = finals.clone();
        set_phase_hook(Some(Box::new(move |at| {
            let present = watched.iter().filter(|f| f.exists()).count();
            let temps = names_in(&dir)
                .iter()
                .filter(|n| n.starts_with(".coverly-"))
                .count();
            log.borrow_mut().push((at, present, temps));
        })));
        let plan = plan_of(
            &env,
            vec![extract_item(&env, &archive, entries, ArchiveAfter::Keep)],
            TransferMode::Move,
        );
        let report = apply(&env, &plan);
        set_phase_hook(None);
        assert_eq!(*status(&report), ItemStatus::Extracted);
        assert_eq!(
            *seen.borrow(),
            [
                (HookPoint::AfterPhaseA, 0, 3),
                (HookPoint::AfterPhaseB, 0, 3)
            ]
        );
        assert!(finals.iter().all(|f| f.is_file()));
        assert_no_temps(&env.root);
        // Keep: the archive stays and no archive line is written.
        assert!(archive.is_file());
        assert!(!job_kinds(&env, &report)
            .iter()
            .any(|k| k.starts_with("archive")));
    }

    /// A cue naming `named`; the archive holds `present` tracks.
    fn deferred_archive(
        env: &Env,
        named: &[&str],
        present: &[&str],
    ) -> (PathBuf, Vec<ExtractEntry>) {
        let mut cue = String::new();
        for (i, n) in named.iter().enumerate() {
            cue.push_str(&format!(
                "FILE \"{n}\" BINARY\r\n  TRACK {:02} MODE1/2352\r\n    INDEX 01 00:00:00\r\n",
                i + 1
            ));
        }
        let data: Vec<Vec<u8>> = present.iter().map(|n| n.as_bytes().repeat(500)).collect();
        let mut refs: Vec<(&str, &[u8])> = present
            .iter()
            .zip(&data)
            .map(|(n, d)| (*n, d.as_slice()))
            .collect();
        refs.push(("Game (USA).cue", cue.as_bytes()));
        let (path, reader) = write_archive(
            env,
            "Game.7z",
            &sevenz(&refs, SevenZShape::solid_lzma2_sheets_last()),
        );
        let entries = plan_entries(reader.as_ref(), "SATURN");
        (path, entries)
    }

    #[test]
    fn deferred_sheet_confirmed_at_phase_b() {
        let env = env();
        let tracks = ["Game (USA) (Track 1).bin", "Game (USA) (Track 2).bin"];
        let (archive, entries) = deferred_archive(&env, &tracks, &tracks);
        let mut item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Keep);
        set_transfer(&mut item, |sc, _, _| *sc = SheetCheck::Deferred);
        let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move));
        assert_eq!(*status(&report), ItemStatus::Extracted, "{report:?}");
        assert!(entries.iter().all(|e| env.root.join(&e.target).is_file()));
    }

    #[test]
    fn deferred_sheet_missing_track_rolls_back() {
        let env = env();
        let named = ["Game (USA) (Track 1).bin", "Game (USA) (Track 2).bin"];
        let (archive, entries) = deferred_archive(&env, &named, &named[..1]);
        let before = fs::read(&archive).unwrap();
        let mut item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash);
        set_transfer(&mut item, |sc, _, _| *sc = SheetCheck::Deferred);
        let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move));
        match status(&report) {
            ItemStatus::Failed { detail, .. } => assert!(detail.contains("Track 2"), "{detail}"),
            other => panic!("{other:?}"),
        }
        assert_untouched(&env, &archive, &before, &entries);
        assert!(job_kinds(&env, &report).contains(&"rolled-back".to_string()));
        assert!(!job_kinds(&env, &report)
            .iter()
            .any(|k| k.starts_with("archive")));
    }

    #[test]
    fn a_sheet_naming_a_track_in_another_case_extracts() {
        // The archive's track is `track 1.bin`; the cue names `Track 1.bin`.
        // The plan resolves it ignoring case and Unicode form (spec §6), so
        // phase B must too, or the dry run promises what apply refuses.
        let env = env();
        let (archive, entries) = deferred_archive(&env, &["Track 1.bin"], &["track 1.bin"]);
        let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Keep);
        let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move));
        assert_eq!(*status(&report), ItemStatus::Extracted, "{report:?}");
        assert!(entries.iter().all(|e| env.root.join(&e.target).is_file()));
    }

    #[test]
    fn a_deferred_sheet_naming_a_track_in_another_case_is_confirmed() {
        let env = env();
        let (archive, entries) = deferred_archive(&env, &["Track 1.bin"], &["track 1.bin"]);
        let mut item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Keep);
        set_transfer(&mut item, |sc, _, _| *sc = SheetCheck::Deferred);
        let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move));
        assert_eq!(*status(&report), ItemStatus::Extracted, "{report:?}");
    }

    #[test]
    fn tar_gz_bad_crc_at_eof_rolls_back() {
        use coverly_core::organize::extract::{extract_and_commit, CommitRequest};

        let env = env();
        let rom = b"GBA ROM bytes ".repeat(4000);
        let good = tar_gz(&[("Tetris (USA).gba", &rom)], 0);
        // The gzip trailer: CRC-32 then ISIZE. Tar has no per-entry
        // checksum: only the end of the stream can tell.
        let mut bad = good.clone();
        let at = bad.len() - 8;
        bad[at] ^= 0xFF;

        // Opened while good, damaged before the decode pass: phase A
        // writes the whole entry, then the stream check fails.
        let (archive, mut reader) = write_archive(&env, "Tetris.tar.gz", &good);
        let entries = plan_entries(reader.as_ref(), "GBA");
        fs::OpenOptions::new()
            .write(true)
            .open(&archive)
            .and_then(|mut f| io::Write::write_all(&mut f, &bad))
            .unwrap();
        let mut writer = env.writer();
        let cancel = Arc::new(AtomicBool::new(false));
        let err = extract_and_commit(
            &mut writer,
            reader.as_mut(),
            &CommitRequest {
                extract: ExtractRequest {
                    item: 0,
                    archive: &archive,
                    entries: &entries,
                    cancel: &cancel,
                    progress: &|_| {},
                },
                archive_fp: None,
                sheet_check: SheetCheck::Read,
                m3u: None,
                adopted: &[],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ExtractError::Archive(_)), "{err:?}");
        assert_rolled_back(&env, &writer, &env.root.join("GBA"), &entries);
        drop(reader);

        // Through apply, the damage is found when the archive is opened.
        let before = fs::read(&archive).unwrap();
        let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash);
        let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move));
        assert!(
            matches!(status(&report), ItemStatus::Failed { .. }),
            "{report:?}"
        );
        assert_untouched(&env, &archive, &before, &entries);
    }

    /// A RAR (by magic) listed and streamed by a fake unrar that prints
    /// the right bytes and exits `exit`.
    fn rar_case(env: &Env, exit: i32) -> (PathBuf, FoundTool, Vec<ExtractEntry>) {
        let files: [(&str, &[u8]); 2] = [("Game (Track 1).bin", b"track one"), ("Game.cue", b"FILE \"Game (Track 1).bin\" BINARY\r\n  TRACK 01 MODE1/2352\r\n    INDEX 01 00:00:00\r\n")];
        let archive = env.src.join("Game.rar");
        fs::write(&archive, b"Rar!\x1a\x07\x01\x00 stand-in").unwrap();
        let listing: Vec<ToolEntry> = files
            .iter()
            .map(|(n, d)| ToolEntry::file(n, d.len() as u64, crc32fast::hash(d)))
            .collect();
        let stream: Vec<u8> = files.iter().flat_map(|(_, d)| d.iter().copied()).collect();
        let script = unrar_listing_script(&unrar_lt(&listing), &unrar_l(&listing)).with_first(
            answer(read_args(UNRAR_READ_ARGS, &archive, None), &stream, exit),
        );
        let bin = env.data.parent().unwrap().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let tool = FoundTool {
            tool: Tool::Unrar,
            path: fake_tool(&bin, "unrar", &script),
            version: "7.01".into(),
            rar5: true,
        };
        let reader = open_archive_with(
            &archive,
            &Limits::default(),
            Some(std::slice::from_ref(&tool)),
        )
        .unwrap()
        .unwrap();
        let entries = plan_entries(reader.as_ref(), "PS");
        (archive, tool, entries)
    }

    fn tool_item(env: &Env, archive: &Path, entries: Vec<ExtractEntry>) -> OrganizeItem {
        let mut item = extract_item(env, archive, entries, ArchiveAfter::Trash);
        set_transfer(&mut item, |_, src, _| {
            *src = ExtractSource::Tool {
                tool: Tool::Unrar,
                version: "7.01".into(),
            }
        });
        item
    }

    #[test]
    fn tool_exit_3_at_end_rolls_back() {
        let env = env();
        let (archive, tool, entries) = rar_case(&env, 3);
        let before = fs::read(&archive).unwrap();
        let plan = plan_of(
            &env,
            vec![tool_item(&env, &archive, entries.clone())],
            TransferMode::Move,
        );
        let trash = FakeTrash::new(trash_dir(&env));
        let report = apply_full(
            &env,
            &plan,
            &trash,
            Some(std::slice::from_ref(&tool)),
            &|_| {},
        );
        match status(&report) {
            ItemStatus::Failed { detail, .. } => assert!(detail.contains("Unrar"), "{detail}"),
            other => panic!("{other:?}"),
        }
        assert_untouched(&env, &archive, &before, &entries);
        assert_eq!(trash.trashed(), 0);
    }

    #[test]
    fn tool_nonzero_exit_rolls_back_item_and_exit_0_extracts() {
        let env = env();
        let (archive, tool, entries) = rar_case(&env, 0);
        let plan = plan_of(
            &env,
            vec![tool_item(&env, &archive, entries.clone())],
            TransferMode::Move,
        );
        let trash = FakeTrash::new(trash_dir(&env));
        let report = apply_full(
            &env,
            &plan,
            &trash,
            Some(std::slice::from_ref(&tool)),
            &|_| {},
        );
        assert_eq!(*status(&report), ItemStatus::Extracted, "{report:?}");
        assert_eq!(
            fs::read(env.root.join("PS/Game (Track 1).bin")).unwrap(),
            b"track one"
        );
        assert_eq!(trash.trashed(), 1);
    }

    #[test]
    fn tool_gone_skips_item() {
        let env = env();
        let (archive, tool, entries) = rar_case(&env, 0);
        let before = fs::read(&archive).unwrap();
        let plan = plan_of(
            &env,
            vec![tool_item(&env, &archive, entries.clone())],
            TransferMode::Move,
        );
        let trash = FakeTrash::new(trash_dir(&env));
        let gone = apply_full(&env, &plan, &trash, Some(&[]), &|_| {});
        assert_eq!(
            *status(&gone),
            ItemStatus::Skipped {
                reason: SkipReason::ToolGone
            }
        );
        let other = FoundTool {
            version: "6.24".into(),
            ..tool
        };
        let upgraded = apply_full(&env, &plan, &trash, Some(&[other]), &|_| {});
        assert_eq!(
            *status(&upgraded),
            ItemStatus::Skipped {
                reason: SkipReason::ToolGone
            }
        );
        assert_untouched(&env, &archive, &before, &entries);
    }

    #[test]
    fn collision_appears_between_b_and_c_rolls_back() {
        let env = env();
        let (archive, entries) = ps1(&env, "PS");
        let before = fs::read(&archive).unwrap();
        // Sheets are stored last: the cue is renamed last.
        let cue = env.root.join("PS/Next Tetris, The.cue");
        let planted = cue.clone();
        set_phase_hook(Some(Box::new(move |at| {
            if at == HookPoint::AfterPhaseB {
                fs::write(&planted, b"someone else's").unwrap();
            }
        })));
        let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash);
        let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move));
        set_phase_hook(None);
        assert_eq!(
            *status(&report),
            ItemStatus::Skipped {
                reason: SkipReason::CollisionSet
            }
        );
        assert_eq!(fs::read(&cue).unwrap(), b"someone else's");
        for e in entries.iter().filter(|e| !e.name.ends_with(".cue")) {
            assert!(!env.root.join(&e.target).exists(), "{:?}", e.target);
        }
        assert_no_temps(&env.root);
        assert_eq!(fs::read(&archive).unwrap(), before);
        let kinds = job_kinds(&env, &report);
        assert_eq!(
            kinds.iter().filter(|k| *k == "rolled-back").count(),
            3,
            "{kinds:?}"
        );
    }

    #[test]
    fn a_final_name_taken_before_phase_b_rolls_back_nothing_renamed() {
        let env = env();
        let (archive, entries) = ps1(&env, "PS");
        let before = fs::read(&archive).unwrap();
        // A twin under another case, planted after the plan.
        let twin = env.root.join("PS/next tetris, the (track 2).bin");
        fs::create_dir_all(twin.parent().unwrap()).unwrap();
        fs::write(&twin, b"mine").unwrap();
        let item = extract_item(&env, &archive, entries, ArchiveAfter::Trash);
        let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move));
        assert_eq!(
            *status(&report),
            ItemStatus::Skipped {
                reason: SkipReason::CollisionSet
            }
        );
        assert_eq!(
            names_in(&env.root.join("PS")),
            ["next tetris, the (track 2).bin"]
        );
        assert_eq!(fs::read(&archive).unwrap(), before);
        // Nothing changed in the library: no job is left to undo.
        assert_eq!(report.job, None);
    }

    #[test]
    fn archive_replaced_after_plan_is_changed_since_plan() {
        let env = env();
        let (archive, entries) = ps1(&env, "PS");
        let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash);
        let plan = plan_of(&env, vec![item], TransferMode::Move);
        let other = zip_with(&[("Other.bin", b"another game", ZipMethod::Stored)]);
        fs::write(&archive, &other).unwrap();
        let report = apply(&env, &plan);
        assert_eq!(
            *status(&report),
            ItemStatus::Skipped {
                reason: SkipReason::ChangedSincePlan
            }
        );
        assert_untouched(&env, &archive, &other, &entries);
        assert!(!env.root.join("PS").exists());
    }

    #[test]
    fn archive_replaced_during_extraction_rolls_back() {
        let env = env();
        let (archive, entries) = ps1(&env, "PS");
        let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash);
        let plan = plan_of(&env, vec![item], TransferMode::Move);
        let target = archive.clone();
        set_phase_hook(Some(Box::new(move |at| {
            if at == HookPoint::AfterPhaseA {
                let mut f = fs::OpenOptions::new().append(true).open(&target).unwrap();
                io::Write::write_all(&mut f, b"!").unwrap();
            }
        })));
        let trash = FakeTrash::new(trash_dir(&env));
        let report = apply_full(&env, &plan, &trash, Some(&[]), &|_| {});
        set_phase_hook(None);
        assert_eq!(
            *status(&report),
            ItemStatus::Skipped {
                reason: SkipReason::ChangedSincePlan
            }
        );
        for e in &entries {
            assert!(!env.root.join(&e.target).exists());
        }
        assert_no_temps(&env.root);
        assert!(archive.is_file());
        assert_eq!(trash.trashed(), 0);
    }

    #[test]
    fn move_mode_trashes_archive_after_commit() {
        let env = env();
        let (archive, entries) = ps1(&env, "PS");
        let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash);
        let trash = FakeTrash::new(trash_dir(&env));
        let report = apply_full(
            &env,
            &plan_of(&env, vec![item], TransferMode::Move),
            &trash,
            Some(&[]),
            &|_| {},
        );
        assert_eq!(*status(&report), ItemStatus::Extracted);
        assert!(!archive.exists());
        let kinds = job_kinds(&env, &report);
        let last_extracted = kinds.iter().rposition(|k| k == "extracted").unwrap();
        let intent = kinds.iter().position(|k| k == "archive-trash").unwrap();
        assert!(intent > last_extracted, "{kinds:?}");
        let steps = job_archive_steps(&env.data, report.job.as_deref().unwrap()).unwrap();
        let ArchiveStep::Trashed { trash: Some(at) } = &steps.last().unwrap().step else {
            panic!("{steps:?}");
        };
        assert!(at.starts_with(trash_dir(&env)) && at.is_file());
        assert!(entries.iter().all(|e| env.root.join(&e.target).is_file()));
    }

    #[test]
    fn trash_unavailable_extracted_kept() {
        let env = env();
        let (archive, entries) = ps1(&env, "PS");
        let before = fs::read(&archive).unwrap();
        let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash);
        let trash = FakeTrash::unavailable(trash_dir(&env));
        let report = apply_full(
            &env,
            &plan_of(&env, vec![item], TransferMode::Move),
            &trash,
            Some(&[]),
            &|_| {},
        );
        assert_eq!(*status(&report), ItemStatus::ExtractedKept);
        assert_eq!(report.extracted, 1);
        assert_eq!(fs::read(&archive).unwrap(), before);
        assert!(entries.iter().all(|e| env.root.join(&e.target).is_file()));
        let kinds = job_kinds(&env, &report);
        assert_eq!(&kinds[kinds.len() - 2..], ["archive-trash", "archive-kept"]);
    }

    #[test]
    fn copy_mode_never_touches_archive() {
        let env = env();
        let (archive, entries) = ps1(&env, "PS");
        let before = (fs::read(&archive).unwrap(), fp_of(&archive));
        // Even a plan asking for the Trash: copy mode keeps the archive.
        let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash);
        let trash = FakeTrash::new(trash_dir(&env));
        let report = apply_full(
            &env,
            &plan_of(&env, vec![item], TransferMode::Copy),
            &trash,
            Some(&[]),
            &|_| {},
        );
        assert_eq!(*status(&report), ItemStatus::Extracted);
        assert_eq!((fs::read(&archive).unwrap(), fp_of(&archive)), before);
        assert_eq!(trash.trashed(), 0);
        assert!(!job_kinds(&env, &report)
            .iter()
            .any(|k| k.starts_with("archive")));
    }

    #[test]
    fn leftover_sav_archive_kept() {
        let env = env();
        let (archive, reader) = write_archive(
            &env,
            "Tetris.7z",
            &sevenz(
                &[
                    ("Tetris (USA).gba", b"cartridge rom bytes".as_slice()),
                    ("Tetris (USA).sav", b"the only copy of a save".as_slice()),
                ],
                SevenZShape::solid_lzma2_sheets_last(),
            ),
        );
        let entries: Vec<ExtractEntry> = plan_entries(reader.as_ref(), "GBA")
            .into_iter()
            .filter(|e| e.name.ends_with(".gba"))
            .collect();
        drop(reader);
        let before = fs::read(&archive).unwrap();
        let item = extract_item(&env, &archive, entries, ArchiveAfter::Keep);
        let trash = FakeTrash::new(trash_dir(&env));
        let report = apply_full(
            &env,
            &plan_of(&env, vec![item], TransferMode::Move),
            &trash,
            Some(&[]),
            &|_| {},
        );
        assert_eq!(*status(&report), ItemStatus::Extracted);
        assert_eq!(names_in(&env.root.join("GBA")), ["Tetris (USA).gba"]);
        assert_eq!(fs::read(&archive).unwrap(), before);
        assert_eq!(trash.trashed(), 0);
    }

    #[test]
    fn m3u_created_journaled() {
        let env = env();
        let cue = |bin: &str| {
            format!("FILE \"{bin}\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n")
        };
        let (b1, b2) = ("Final Game (Disc 1).bin", "Final Game (Disc 2).bin");
        let (c1, c2) = (cue(b1), cue(b2));
        let (archive, reader) = write_archive(
            &env,
            "Final Game.7z",
            &sevenz(
                &[
                    (b1, b"disc one".as_slice()),
                    (b2, b"disc two".as_slice()),
                    ("Final Game (Disc 1).cue", c1.as_bytes()),
                    ("Final Game (Disc 2).cue", c2.as_bytes()),
                ],
                SevenZShape::solid_lzma2_sheets_last(),
            ),
        );
        let entries = plan_entries(reader.as_ref(), "PS/Final Game");
        drop(reader);
        let mut item = extract_item(&env, &archive, entries, ArchiveAfter::Keep);
        let m3u = PathBuf::from("PS/Final Game/Final Game.m3u");
        set_transfer(&mut item, |_, _, m| *m = Some(m3u.clone()));
        let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move));
        assert_eq!(*status(&report), ItemStatus::Extracted, "{report:?}");
        let at = env.root.join(&m3u);
        assert_eq!(
            fs::read_to_string(&at).unwrap(),
            "Final Game (Disc 1).cue\nFinal Game (Disc 2).cue\n"
        );
        assert_eq!(report.items[0].targets.last(), Some(&at));
        let job = report.job.as_deref().unwrap();
        let text = fs::read_to_string(env.data.join("jobs").join(format!("{job}.jsonl"))).unwrap();
        let created: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
            .filter(|v| v["op"]["kind"] == "created")
            .collect();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0]["target"], at.to_str().unwrap());
        assert_eq!(created[0]["item"], 0);
        assert!(created[0]["sha1"].is_string());
        assert_no_temps(&env.root);
    }

    #[test]
    fn progress_is_byte_based_over_folder_unpack_size() {
        let env = env();
        // One solid block: the save is decoded before the ROM to reach it.
        let save = vec![0x11u8; 3 << 20];
        let rom: Vec<u8> = (0..(2u32 << 20)).map(|i| (i % 251) as u8).collect();
        let (archive, reader) = write_archive(
            &env,
            "Tetris.7z",
            &sevenz(
                &[("Tetris (USA).sav", &save), ("Tetris (USA).gba", &rom)],
                SevenZShape::solid_lzma2_sheets_last(),
            ),
        );
        let entries: Vec<ExtractEntry> = plan_entries(reader.as_ref(), "GBA")
            .into_iter()
            .filter(|e| e.name.ends_with(".gba"))
            .collect();
        drop(reader);
        // The playlist is not an entry: only the planned files count.
        let n = entries.len();
        let item = extract_item(&env, &archive, entries, ArchiveAfter::Keep);
        let mut plan = plan_of(&env, vec![item], TransferMode::Move);
        plan.items[0].id = 7;
        let seen: Mutex<Vec<(u64, u64, usize, usize)>> = Mutex::default();
        let items: Mutex<Vec<usize>> = Mutex::default();
        let trash = FakeTrash::new(trash_dir(&env));
        let report = apply_full(&env, &plan, &trash, Some(&[]), &|p| {
            if let OrganizePhase::Extract {
                item,
                bytes_done,
                bytes_total,
                entries_done,
                entries_total,
            } = p.phase
            {
                seen.lock()
                    .unwrap()
                    .push((bytes_done, bytes_total, entries_done, entries_total));
                items.lock().unwrap().push(item);
            }
        });
        // Every event names the item being extracted (the webview shows
        // its name).
        let items = items.into_inner().unwrap();
        assert!(items.iter().all(|&i| i == 7), "{items:?}");
        assert_eq!(*status(&report), ItemStatus::Extracted);
        let seen = seen.into_inner().unwrap();
        let total = (save.len() + rom.len()) as u64;
        assert!(seen.len() >= 2, "{seen:?}");
        assert!(seen.iter().all(|e| e.1 == total), "{seen:?}");
        assert!(seen.iter().all(|e| e.3 == n), "{seen:?}");
        assert_eq!(seen.first().unwrap().0, 0);
        assert_eq!(seen.first().unwrap().2, 0);
        assert_eq!(seen.last().unwrap().0, total);
        assert_eq!(seen.last().unwrap().2, n);
        assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "{seen:?}");
        assert!(seen.windows(2).all(|w| w[0].2 <= w[1].2), "{seen:?}");
        // The all-staged edge goes out at once, before the closing event.
        let edge = seen.iter().position(|e| e.2 == n).unwrap();
        assert!(
            seen[edge].0 < total || edge < seen.len() - 1,
            "no all-staged edge: {seen:?}"
        );
    }

    /// R5: hundreds of tiny entries still reach the UI at most every
    /// 100 ms (plus the first, the all-staged edge and the last).
    #[test]
    fn progress_coalesces_two_hundred_tiny_entries() {
        let env = env();
        let names: Vec<String> = (0..200)
            .map(|i| format!("Game (Track {i:03}).bin"))
            .collect();
        let datas: Vec<Vec<u8>> = (0..200u32)
            .map(|i| (0..1024u32).map(|j| ((i * 7 + j) % 251) as u8).collect())
            .collect();
        let specs: Vec<(&str, &[u8], ZipMethod)> = names
            .iter()
            .zip(&datas)
            .map(|(n, d)| (n.as_str(), d.as_slice(), ZipMethod::Stored))
            .collect();
        let (archive, reader) = write_archive(&env, "Game.zip", &zip_with(&specs));
        let entries = plan_entries(reader.as_ref(), "PS/Game");
        drop(reader);
        assert_eq!(entries.len(), 200);
        let item = extract_item(&env, &archive, entries, ArchiveAfter::Keep);
        let plan = plan_of(&env, vec![item], TransferMode::Move);
        let seen: Mutex<Vec<usize>> = Mutex::default();
        let trash = FakeTrash::new(trash_dir(&env));
        let start = std::time::Instant::now();
        let report = apply_full(&env, &plan, &trash, Some(&[]), &|p| {
            if let OrganizePhase::Extract { entries_done, .. } = p.phase {
                seen.lock().unwrap().push(entries_done);
            }
        });
        let elapsed_ms = start.elapsed().as_millis() as usize;
        assert_eq!(*status(&report), ItemStatus::Extracted, "{report:?}");
        let seen = seen.into_inner().unwrap();
        assert!(
            seen.len() <= elapsed_ms / 100 + 3,
            "{} events in {elapsed_ms} ms",
            seen.len()
        );
        assert_eq!(seen.last(), Some(&200));
    }

    #[test]
    fn too_little_room_for_what_is_left_pauses_before_writing() {
        let env = env();
        let (archive, entries) = ps1(&env, "PS");
        let before = fs::read(&archive).unwrap();
        let item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Trash);
        set_free_space_override(Some(|_| Some(1000)));
        let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move));
        set_free_space_override(None);
        match report.paused {
            Some(PauseKind::DiskFull { needed, free }) => {
                assert_eq!(free, 1000);
                assert!(needed > entries.iter().map(|e| e.size).sum::<u64>());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(report.resumable_items, [0]);
        assert_untouched(&env, &archive, &before, &entries);
    }

    /// A two-disc game extracted into `PS/Final Game/`.
    fn two_discs(env: &Env) -> (PathBuf, Vec<ExtractEntry>) {
        let cue = |bin: &str| {
            format!("FILE \"{bin}\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n")
        };
        let (b1, b2) = ("Final Game (Disc 1).bin", "Final Game (Disc 2).bin");
        let (c1, c2) = (cue(b1), cue(b2));
        let (archive, reader) = write_archive(
            env,
            "Final Game.7z",
            &sevenz(
                &[
                    (b1, b"disc one".as_slice()),
                    (b2, b"disc two".as_slice()),
                    ("Final Game (Disc 1).cue", c1.as_bytes()),
                    ("Final Game (Disc 2).cue", c2.as_bytes()),
                ],
                SevenZShape::solid_lzma2_sheets_last(),
            ),
        );
        let entries = plan_entries(reader.as_ref(), "PS/Final Game");
        (archive, entries)
    }

    #[test]
    fn an_unsafe_playlist_path_is_refused_before_any_temp() {
        let env = env();
        let (archive, entries) = two_discs(&env);
        let before = fs::read(&archive).unwrap();
        let mut item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Keep);
        set_transfer(&mut item, |_, _, m| {
            *m = Some(PathBuf::from("../Final Game.m3u"))
        });
        let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move));
        assert!(
            matches!(status(&report), ItemStatus::Failed { .. }),
            "{report:?}"
        );
        assert_untouched(&env, &archive, &before, &entries);
        assert!(!env.root.parent().unwrap().join("Final Game.m3u").exists());
    }

    static APPENDS_LEFT: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(-1);

    fn full_on_the_nth_append() -> Option<io::Error> {
        (APPENDS_LEFT.fetch_sub(1, Ordering::SeqCst) == 1)
            .then(|| io::Error::from(ErrorKind::StorageFull))
    }

    #[test]
    fn a_playlist_the_journal_cannot_record_is_not_left_behind() {
        let env = env();
        let (archive, entries) = two_discs(&env);
        let before = fs::read(&archive).unwrap();
        let mut item = extract_item(&env, &archive, entries.clone(), ArchiveAfter::Keep);
        let m3u = PathBuf::from("PS/Final Game/Final Game.m3u");
        set_transfer(&mut item, |_, _, m| *m = Some(m3u.clone()));
        // After phase B: four completion lines go in, the playlist's
        // `created` line finds the journal volume full.
        let renames = isize::try_from(entries.len()).unwrap();
        set_phase_hook(Some(Box::new(move |at| {
            if at == HookPoint::AfterPhaseB {
                APPENDS_LEFT.store(renames + 1, Ordering::SeqCst);
                set_journal_fault(Some(full_on_the_nth_append));
            }
        })));
        let report = apply(&env, &plan_of(&env, vec![item], TransferMode::Move));
        set_phase_hook(None);
        set_journal_fault(None);
        assert!(
            report
                .items
                .iter()
                .all(|o| o.status != ItemStatus::Extracted),
            "{report:?}"
        );
        assert!(!env.root.join(&m3u).exists(), "orphan playlist");
        assert_untouched(&env, &archive, &before, &entries);
    }
}
