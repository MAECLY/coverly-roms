//! Property test over every reader of the archives and discs work (spec
//! `2026-10-23-organize-archives-and-discs.md` §13): truncated, bit-flipped
//! and field-rewritten versions of each synthetic fixture go through
//!
//! - the archive reader (`open_archive_with`, then `stream_entries` over
//!   every file and `open_entry` of each file on a fresh reader),
//! - the organise inspection of the same file (`inspect_file_with`, which
//!   adds container detection, sheet parsing and disc identification of
//!   entries read from the archive),
//! - the disc identifier over a raw track (`DiscReader::identify_with`),
//! - the sheet parser (`parse_sheet`, `infer_redump_set`),
//!
//! and each call must return `Ok` or a typed error: never unwind out of the
//! reader (the calls are not wrapped in `catch_unwind`) and never allocate
//! more than [`PEAK_LIMIT`] at once on the calling thread, under limits
//! lowered so a bound that is not enforced shows (16 MiB decoder memory,
//! 1 MiB headers, 4 MiB decode budget). The allocation bound is measured
//! by a counting global allocator; the readers decode on the calling
//! thread (spec S1), so their allocations are all seen.
//!
//! The external-tool listing parsers get the same treatment in their own
//! unit test (`archive::external::tests::garbled_listings_never_panic`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use coverly_core::archive::testing::{
    lzma_props, mutate, sevenz, sevenz_copy, tar, xz_stream, zip_crafted, zip_lzma_payload,
    zip_with, SevenZCoder, SevenZShape, XzBlock, ZipMethod, ZipSpec,
};
use coverly_core::archive::{file_entries, open_archive_with, Budget, Limits};
use coverly_core::disc::sheets::{infer_redump_set, parse_sheet, SheetKind};
use coverly_core::disc::testing as dt;
use coverly_core::disc::{DiscReader, WindowSource};
use coverly_core::organize::inspect::inspect_file_with;
use coverly_core::systems::SystemDb;

const MIB: u64 = 1 << 20;

/// Mutations per fixture and reader in the default run.
const ROUNDS: usize = 1_000;
/// Mutations per fixture in the ignored long run.
const ROUNDS_LONG: usize = 20_000;
/// Decoder memory one mutated archive may ask for.
/// (The 7z writer encodes its header with an 8 MiB dictionary.)
const MEM_CAP: u64 = 16 * MIB;
/// Most bytes live at once on the reading thread during one call: the
/// decoder cap, a header, the decode budget's buffers and slack.
const PEAK_LIMIT: u64 = 64 * MIB;
/// Wall time one mutated input may take in a release build.
const ONE_CALL_LIMIT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Counting allocator: live bytes and the peak per thread, only while a
// thread has tracking on.
// ---------------------------------------------------------------------------

struct Counting;

thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static LIVE: Cell<u64> = const { Cell::new(0) };
    static PEAK: Cell<u64> = const { Cell::new(0) };
    static LARGEST: Cell<u64> = const { Cell::new(0) };
}

fn on_alloc(size: usize) {
    let _ = TRACK.try_with(|t| {
        if t.get() {
            let size = size as u64;
            let live = LIVE.with(|l| {
                let v = l.get().saturating_add(size);
                l.set(v);
                v
            });
            PEAK.with(|p| p.set(p.get().max(live)));
            LARGEST.with(|m| m.set(m.get().max(size)));
        }
    });
}

fn on_dealloc(size: usize) {
    let _ = TRACK.try_with(|t| {
        if t.get() {
            LIVE.with(|l| l.set(l.get().saturating_sub(size as u64)));
        }
    });
}

// SAFETY: every call is forwarded to `System` unchanged; the counters are
// const-initialised thread locals that never allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        on_alloc(layout.size());
        // SAFETY: same layout as given.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        on_alloc(layout.size());
        // SAFETY: same layout as given.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        on_dealloc(layout.size());
        // SAFETY: `ptr` came from `System` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        on_dealloc(layout.size());
        on_alloc(new_size);
        // SAFETY: `ptr` came from `System` with this layout.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// The peak bytes live at once and the largest single request while `f`
/// ran on this thread, over what was live before it.
fn measured<T>(f: impl FnOnce() -> T) -> (T, u64, u64) {
    LIVE.with(|l| l.set(0));
    PEAK.with(|p| p.set(0));
    LARGEST.with(|m| m.set(0));
    TRACK.with(|t| t.set(true));
    let out = f();
    TRACK.with(|t| t.set(false));
    (out, PEAK.with(Cell::get), LARGEST.with(Cell::get))
}

/// One test at a time measures: the counters are per thread, but a
/// parallel test would compete for time and memory.
static SERIAL: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------------

/// Deterministic, mildly compressible synthetic bytes.
fn sample(len: usize, seed: u32) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|i| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            if i % 5 < 3 {
                b'A' + (i % 11) as u8
            } else {
                state as u8
            }
        })
        .collect()
}

fn gz(data: &[u8]) -> Vec<u8> {
    let mut w = flate2::GzBuilder::new()
        .filename("Game (USA).gba")
        .write(Vec::new(), flate2::Compression::fast());
    w.write_all(data).unwrap();
    w.finish().unwrap()
}

fn bz2(data: &[u8]) -> Vec<u8> {
    let mut w = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    w.write_all(data).unwrap();
    w.finish().unwrap()
}

fn xz(data: &[u8]) -> Vec<u8> {
    let mut options = lzma_rust2::XzOptions::with_preset(1);
    options.lzma_options.dict_size = 1 << 16;
    let mut w = lzma_rust2::XzWriter::new(Vec::new(), options).unwrap();
    w.write_all(data).unwrap();
    w.finish().unwrap()
}

fn zst(data: &[u8]) -> Vec<u8> {
    ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Fastest)
}

fn lz4(data: &[u8]) -> Vec<u8> {
    let mut info = lz4_flex::frame::FrameInfo::new();
    info = info.content_size(Some(data.len() as u64));
    info.content_checksum = true;
    let mut w = lz4_flex::frame::FrameEncoder::with_frame_info(info, Vec::new());
    w.write_all(data).unwrap();
    w.finish().unwrap()
}

fn lzma_raw(data: &[u8], dict: u32) -> ([u8; 5], Vec<u8>) {
    let mut options = lzma_rust2::LzmaOptions::with_preset(1);
    options.dict_size = dict;
    let mut w = lzma_rust2::LzmaWriter::new_no_header(Vec::new(), &options, false).unwrap();
    w.write_all(data).unwrap();
    let raw = w.finish().unwrap();
    let props = lzma_props(options.lc as u8, options.lp as u8, options.pb as u8, dict);
    (props, raw)
}

/// A legacy `.lzma` file: props, dictionary, declared size, raw stream.
fn lzma_alone(data: &[u8]) -> Vec<u8> {
    let (props, raw) = lzma_raw(data, 1 << 16);
    let mut out = props.to_vec();
    out.extend_from_slice(&(data.len() as u64).to_le_bytes());
    out.extend_from_slice(&raw);
    out
}

fn cue(files: &[&str]) -> Vec<u8> {
    let mut s = String::new();
    for (i, f) in files.iter().enumerate() {
        let mode = if i == 0 { "MODE2/2352" } else { "AUDIO" };
        s.push_str(&format!(
            "FILE \"{f}\" BINARY\r\n  TRACK {:02} {mode}\r\n    INDEX 01 00:00:00\r\n",
            i + 1
        ));
    }
    s.into_bytes()
}

fn gdi() -> Vec<u8> {
    b"3\r\n1 0 4 2352 track01.bin 0\r\n2 450 0 2352 track02.raw 0\r\n\
      3 45000 4 2352 track03.bin 0\r\n"
        .to_vec()
}

/// One archive fixture: a label, the name it is written under (the
/// extension is the fallback once a mutation breaks the magic) and bytes.
struct Fixture {
    label: &'static str,
    name: &'static str,
    bytes: Vec<u8>,
}

fn fixture(label: &'static str, name: &'static str, bytes: Vec<u8>) -> Fixture {
    Fixture { label, name, bytes }
}

/// Every container and compressor, including the ones the garbled suite
/// (`archive_garbled.rs`) leaves out: zip stored/deflate64/bzip2/xz, plain
/// `.xz`/`.zst`/`.bz2`/`.lzma`, tar inside lz4, RAR magic with no tool,
/// and disc sets stored uncompressed so mutations reach the disc
/// identifier and the sheet parser through the archive.
fn archive_fixtures() -> Vec<Fixture> {
    let a = sample(3_000, 0x1234_5678);
    let b = sample(2_000, 0x0BAD_F00D);
    let (props_a, lzma_a) = lzma_raw(&a, 1 << 16);
    let files: [(&str, &[u8]); 3] = [
        ("Game (USA)/", b""),
        ("Game (USA)/Game (USA) (Track 1).bin", &a),
        ("Game (USA)/Game (USA).cue", &b),
    ];
    let tarball = tar(&files);
    let xz_entry = xz_stream(&[
        XzBlock {
            data: a.clone(),
            dict_prop: 8,
        },
        XzBlock {
            data: b.clone(),
            dict_prop: 8,
        },
    ]);

    let dir = "Next Tetris, The (USA)";
    let t1 = format!("{dir}/Next Tetris, The (USA) (Track 01).bin");
    let t2 = format!("{dir}/Next Tetris, The (USA) (Track 02).bin");
    let sheet = format!("{dir}/Next Tetris, The (USA).cue");
    let sheet_bytes = cue(&[
        "Next Tetris, The (USA) (Track 01).bin",
        "Next Tetris, The (USA) (Track 02).bin",
    ]);
    let psx = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");
    let audio = vec![0u8; 2352 * 4];
    let ps1_set: [(&str, &[u8]); 3] = [(&sheet, &sheet_bytes), (&t1, &psx), (&t2, &audio)];
    let saturn = dt::saturn_track("T-5704H");
    let saturn_cue = cue(&["Tetris Plus (Track 1).bin"]);
    let saturn_set: [(&str, &[u8]); 2] = [
        ("Tetris Plus.cue", &saturn_cue),
        ("Tetris Plus (Track 1).bin", &saturn),
    ];
    let dc01 = dt::dreamcast_track01();
    let dc03 = dt::dreamcast_track03("T-40214N", "1/1");
    let dc_gdi = gdi();
    let dc_set: [(&str, &[u8]); 4] = [
        ("game.gdi", &dc_gdi),
        ("track01.bin", &dc01),
        ("track02.raw", &audio),
        ("track03.bin", &dc03),
    ];

    vec![
        fixture(
            "zip stored",
            "game.zip",
            zip_with(&[
                ("Game (USA).gba", &a, ZipMethod::Stored),
                ("Game (USA).sav", &b, ZipMethod::Stored),
            ]),
        ),
        fixture(
            "zip deflate64",
            "game.zip",
            zip_with(&[("Game (USA).gba", &a, ZipMethod::Deflate64)]),
        ),
        fixture(
            "zip bzip2",
            "game.zip",
            zip_with(&[("Game (USA).gba", &a, ZipMethod::Bzip2)]),
        ),
        fixture(
            "zip lzma",
            "game.zip",
            zip_crafted(&[ZipSpec::new(
                "a.gba",
                &a,
                ZipMethod::Raw {
                    method: 14,
                    payload: zip_lzma_payload(props_a, &lzma_a),
                },
            )]),
        ),
        fixture(
            "zip xz",
            "game.zip",
            zip_crafted(&[ZipSpec::new(
                "ab.gba",
                &[a.as_slice(), b.as_slice()].concat(),
                ZipMethod::Raw {
                    method: 95,
                    payload: xz_entry.clone(),
                },
            )]),
        ),
        fixture(
            "7z lzma2",
            "game.7z",
            sevenz(
                &files,
                SevenZShape {
                    solid: true,
                    coder: SevenZCoder::Lzma2 { dict: 1 << 16 },
                    sheets_last: true,
                },
            ),
        ),
        fixture("tar.lz4", "game.tar.lz4", lz4(&tarball)),
        fixture("xz", "Game (USA).gba.xz", xz(&a)),
        fixture("xz walker", "Game (USA).gba.xz", xz_entry),
        fixture("zst", "Game (USA).gba.zst", zst(&a)),
        fixture("bz2", "Game (USA).gba.bz2", bz2(&a)),
        fixture("gz", "Game (USA).gba.gz", gz(&a)),
        fixture("lzma alone", "Game (USA).gba.lzma", lzma_alone(&a)),
        fixture("rar5 no tool", "game.rar", {
            let mut v = b"Rar!\x1a\x07\x01\x00".to_vec();
            v.extend_from_slice(&b);
            v
        }),
        fixture("rar4 no tool", "game.rar", {
            let mut v = b"Rar!\x1a\x07\x00".to_vec();
            v.extend_from_slice(&b);
            v
        }),
        fixture(
            "ps1 set 7z copy",
            "Next Tetris, The.zip",
            sevenz_copy(&ps1_set),
        ),
        fixture(
            "ps1 set zip stored",
            "Next Tetris, The.zip",
            zip_with(&[
                (sheet.as_str(), &sheet_bytes, ZipMethod::Stored),
                (t1.as_str(), &psx, ZipMethod::Stored),
                (t2.as_str(), &audio, ZipMethod::Stored),
            ]),
        ),
        fixture("saturn set tar", "Tetris Plus.tar", tar(&saturn_set)),
        fixture(
            "dreamcast set 7z copy",
            "Next Tetris.7z",
            sevenz_copy(&dc_set),
        ),
    ]
}

/// Raw disc tracks for the identifier: one per system rule.
fn disc_fixtures() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("ps1 raw", dt::psx_track(true, "cdrom:\\SLUS_008.62;1")),
        ("ps1 iso", dt::psx_track(false, "cdrom:\\SCES_000.01;1")),
        ("ps2", dt::ps2_iso()),
        ("saturn", dt::saturn_track("T-5704H")),
        ("dreamcast", dt::dreamcast_track03("T-40214N", "1/1")),
        ("sega cd", dt::segacd_track()),
        ("pc engine cd", dt::pce_cd_track()),
        ("neo geo cd", dt::neocd_iso()),
        ("3do", dt::opera_3do(true)),
    ]
}

fn limits() -> Limits {
    Limits {
        decoder_mem_cap: MEM_CAP,
        max_header: MIB,
        plan_head_budget: 4 * MIB,
        plan_listing_budget: 4 * MIB,
        external_tools: false,
        ..Limits::default()
    }
}

// ---------------------------------------------------------------------------
// The readers under test.
// ---------------------------------------------------------------------------

/// Lists, streams every file, then reads each file through `open_entry`
/// on a fresh reader. Errors are fine; only an unwind is not. Returns
/// whether the archive opened and every file read to its end without an
/// error (the intact fixtures must).
fn read_archive(path: &Path, limits: &Limits) -> bool {
    let mut sink = [0u8; 8192];
    let Ok(Some(mut reader)) = open_archive_with(path, limits, Some(&[])) else {
        return false;
    };
    let wanted: Vec<usize> = file_entries(reader.as_ref()).map(|e| e.index).collect();
    let mut budget = Budget::plan_head(limits);
    let mut clean = reader
        .stream_entries(&wanted, &mut budget, &mut |_, data| {
            while data.read(&mut sink)? != 0 {}
            Ok(())
        })
        .is_ok();
    for index in wanted {
        let Ok(Some(mut reader)) = open_archive_with(path, limits, Some(&[])) else {
            return false;
        };
        let mut budget = Budget::plan_head(limits);
        let opened = reader.open_entry(index, &mut budget);
        let Ok(mut data) = opened else {
            clean = false;
            continue;
        };
        loop {
            match data.read(&mut sink) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => {
                    clean = false;
                    break;
                }
            }
        }
    }
    clean
}

/// The organise inspection of the file: detection, listing, sets, sheets
/// and disc identification of the entries.
fn inspect(path: &Path, limits: &Limits, db: &SystemDb) {
    let len = std::fs::metadata(path).map_or(0, |m| m.len());
    let _ = inspect_file_with(path, len, db, limits, None);
}

/// Disc identification of a raw track held in memory: whether it named a
/// system.
fn identify(bytes: &[u8]) -> bool {
    let mut src = WindowSource::new(bytes.to_vec());
    matches!(
        DiscReader::identify_with(&mut src, Some(bytes.len() as u64), None),
        Ok(Some(_))
    )
}

/// Sheet parsing of every kind, and set inference from the sheet's own
/// referenced names as a listing.
fn sheets(bytes: &[u8]) {
    for kind in [SheetKind::Cue, SheetKind::Gdi, SheetKind::M3u] {
        if let Ok(sheet) = parse_sheet(kind, bytes) {
            let mut listing: Vec<&str> = sheet.files.iter().map(String::as_str).collect();
            listing.push("Game.cue");
            let _ = infer_redump_set(&listing, "Game.cue");
        }
    }
    let text = String::from_utf8_lossy(bytes);
    let listing: Vec<&str> = text.lines().collect();
    if let Some(first) = listing.first() {
        let _ = infer_redump_set(&listing, first);
    }
}

/// Checks one measured call.
fn check(label: &str, round: usize, peak: u64, largest: u64, took: Duration) {
    assert!(
        peak <= PEAK_LIMIT,
        "{label} round {round}: {peak} bytes live at once (limit {PEAK_LIMIT})"
    );
    assert!(
        largest <= PEAK_LIMIT,
        "{label} round {round}: one allocation of {largest} bytes"
    );
    if !cfg!(debug_assertions) {
        assert!(
            took <= ONE_CALL_LIMIT,
            "{label} round {round} took {took:?}"
        );
    }
}

/// Runs `rounds` mutations of every fixture through every reader and
/// returns the highest peak seen.
fn run(rounds: usize) -> u64 {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let limits = limits();
    let db = SystemDb::builtin();
    let mut highest = 0u64;

    for (i, f) in archive_fixtures().iter().enumerate() {
        let path = dir.path().join(format!("{i:02}-{}", f.name));
        let started = Instant::now();
        // The intact fixture first, then its mutations.
        let inputs = std::iter::once(f.bytes.clone()).chain(mutate(
            0xF022_0000 + i as u64,
            &f.bytes,
            rounds,
        ));
        for (round, bytes) in inputs.enumerate() {
            std::fs::write(&path, &bytes).expect("write mutation");
            let one = Instant::now();
            let (clean, peak, largest) = measured(|| read_archive(&path, &limits));
            check(f.label, round, peak, largest, one.elapsed());
            let no_tool = f.label.starts_with("rar");
            if round == 0 && !no_tool {
                assert!(clean, "{}: the intact fixture reads", f.label);
            }
            highest = highest.max(peak);
            let one = Instant::now();
            let ((), peak, largest) = measured(|| inspect(&path, &limits, &db));
            check(f.label, round, peak, largest, one.elapsed());
            highest = highest.max(peak);
        }
        eprintln!(
            "{:<22} {rounds} mutations in {:.2?}",
            f.label,
            started.elapsed()
        );
    }

    for (i, (label, bytes)) in disc_fixtures().iter().enumerate() {
        let started = Instant::now();
        let inputs =
            std::iter::once(bytes.clone()).chain(mutate(0xD15C_0000 + i as u64, bytes, rounds));
        for (round, bytes) in inputs.enumerate() {
            let one = Instant::now();
            let (named, peak, largest) = measured(|| identify(&bytes));
            check(label, round, peak, largest, one.elapsed());
            if round == 0 {
                assert!(named, "{label}: the intact track is identified");
            }
            highest = highest.max(peak);
        }
        eprintln!(
            "{label:<22} {rounds} mutations in {:.2?}",
            started.elapsed()
        );
    }

    let sheet_texts: [(&str, Vec<u8>); 3] = [
        ("cue", cue(&["Game (Track 01).bin", "Game (Track 02).bin"])),
        ("gdi", gdi()),
        ("m3u", b"Game (Disc 1).cue\nGame (Disc 2).cue\n".to_vec()),
    ];
    for (i, (label, bytes)) in sheet_texts.iter().enumerate() {
        let inputs =
            std::iter::once(bytes.clone()).chain(mutate(0x5EE7_0000 + i as u64, bytes, rounds * 4));
        for (round, bytes) in inputs.enumerate() {
            let one = Instant::now();
            let ((), peak, largest) = measured(|| sheets(&bytes));
            check(label, round, peak, largest, one.elapsed());
            highest = highest.max(peak);
        }
    }
    eprintln!("highest peak {highest} bytes");
    highest
}

#[test]
fn every_reader_survives_garbled_inputs_within_memory() {
    let highest = run(ROUNDS);
    // The tracking works: reading archives allocates something.
    assert!(highest > 0);
}

#[test]
#[ignore = "long run: 20,000 mutations per fixture and reader"]
fn every_reader_survives_garbled_inputs_within_memory_long() {
    run(ROUNDS_LONG);
}

/// The allocator counts what a call keeps live, so the bound above means
/// something.
#[test]
fn counting_allocator_sees_a_large_buffer() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (len, peak, largest) = measured(|| vec![1u8; 3 * MIB as usize].len());
    assert_eq!(len, 3 * MIB as usize);
    assert!(peak >= 3 * MIB, "{peak}");
    assert!(largest >= 3 * MIB, "{largest}");
}
