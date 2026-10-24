//! Garbled-archive mutation suite (plan Task 17; spec §13 S1 across every
//! format). Every fixture is synthetic. Each mutation goes through the
//! public entry point, `open_archive`, and then a full `stream_entries`
//! with a 4 MiB budget. The calls are deliberately **not** wrapped in
//! `catch_unwind`: a panic that escapes the reader's own guard fails the
//! test.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use coverly_core::archive::guard::PANIC_DETAIL;
use coverly_core::archive::testing::{
    length_fields, lzma_props, mutate, sevenz, tar, zip_crafted, zip_lzma_payload, zip_ppmd_header,
    zip_with, FieldWidth, SevenZCoder, SevenZShape, ZipMethod, ZipSpec,
};
use coverly_core::archive::{file_entries, open_archive_with, ArchiveError, Budget, Limits};

const MIB: u64 = 1 << 20;

/// Mutations per fixture in the default run (plan Task 17).
const ROUNDS: usize = 2_000;
/// Mutations per fixture in the ignored long run.
const ROUNDS_LONG: usize = 100_000;
/// Decoded bytes one mutated archive may cost (plan Task 17).
const BUDGET: u64 = 4 * MIB;
/// Total runtime allowed for the default run in release-test mode.
const RELEASE_LIMIT: Duration = Duration::from_secs(30);

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

fn ppmd8(data: &[u8], order: u8, mem_mib: u16) -> Vec<u8> {
    let mut out = zip_ppmd_header(order, mem_mib, 0).to_vec();
    let mut w = ppmd_rust::Ppmd8Encoder::new(
        Vec::new(),
        u32::from(order),
        u32::from(mem_mib) << 20,
        ppmd_rust::RestoreMethod::Restart,
    )
    .unwrap();
    w.write_all(data).unwrap();
    out.extend_from_slice(&w.finish(true).unwrap());
    out
}

fn raw(method: u16, payload: Vec<u8>) -> ZipMethod {
    ZipMethod::Raw { method, payload }
}

/// One fixture: a label for the timing log, the file name it is written
/// under (the extension is the fallback when a mutation breaks the magic)
/// and its bytes.
struct Fixture {
    label: &'static str,
    name: &'static str,
    bytes: Vec<u8>,
}

fn fixture(label: &'static str, name: &'static str, bytes: Vec<u8>) -> Fixture {
    Fixture { label, name, bytes }
}

/// Every format of plan Task 17: zip deflate/lzma/ppmd/zstd, 7z
/// LZMA/LZMA2/BCJ/PPMd, tar.gz/bz2/xz/zst, gz and lz4. Small payloads with
/// two files where the format has entries, so mutations reach every
/// decoder quickly.
fn fixtures() -> Vec<Fixture> {
    let a = sample(3_000, 0x1234_5678);
    let b = sample(2_000, 0x0BAD_F00D);
    let (props_a, lzma_a) = lzma_raw(&a, 1 << 16);
    let (props_b, lzma_b) = lzma_raw(&b, 1 << 16);
    let files: [(&str, &[u8]); 3] = [
        ("Game (USA)/", b""),
        ("Game (USA)/Game (USA) (Track 1).bin", &a),
        ("Game (USA)/Game (USA).cue", &b),
    ];
    let tarball = tar(&files);
    let shape = |solid: bool, coder: SevenZCoder| SevenZShape {
        solid,
        coder,
        sheets_last: solid,
    };
    vec![
        fixture(
            "zip deflate",
            "game.zip",
            zip_with(&[
                ("Game (USA).gba", &a, ZipMethod::Deflate),
                ("Game (USA).sav", &b, ZipMethod::Deflate),
            ]),
        ),
        fixture(
            "zip lzma",
            "game.zip",
            zip_crafted(&[
                ZipSpec::new("a.gba", &a, raw(14, zip_lzma_payload(props_a, &lzma_a))),
                ZipSpec::new("b.gba", &b, raw(14, zip_lzma_payload(props_b, &lzma_b))),
            ]),
        ),
        fixture(
            "zip ppmd",
            "game.zip",
            zip_crafted(&[
                ZipSpec::new("a.gba", &a, raw(98, ppmd8(&a, 4, 1))),
                ZipSpec::new("b.gba", &b, raw(98, ppmd8(&b, 6, 1))),
            ]),
        ),
        fixture(
            "zip zstd",
            "game.zip",
            zip_crafted(&[
                ZipSpec::new("a.gba", &a, raw(93, zst(&a))),
                ZipSpec::new("b.gba", &b, raw(93, zst(&b))),
            ]),
        ),
        fixture(
            "7z lzma",
            "game.7z",
            sevenz(&files, shape(false, SevenZCoder::Lzma { dict: 1 << 16 })),
        ),
        fixture(
            "7z lzma2",
            "game.7z",
            sevenz(&files, shape(true, SevenZCoder::Lzma2 { dict: 1 << 16 })),
        ),
        fixture(
            "7z bcj",
            "game.7z",
            sevenz(&files, shape(true, SevenZCoder::BcjLzma2 { dict: 1 << 16 })),
        ),
        fixture(
            "7z ppmd",
            "game.7z",
            sevenz(
                &files,
                shape(
                    false,
                    SevenZCoder::Ppmd {
                        order: 6,
                        mem: 1 << 20,
                    },
                ),
            ),
        ),
        fixture("tar.gz", "game.tar.gz", gz(&tarball)),
        fixture("tar.bz2", "game.tar.bz2", bz2(&tarball)),
        fixture("tar.xz", "game.tar.xz", xz(&tarball)),
        fixture("tar.zst", "game.tar.zst", zst(&tarball)),
        fixture("gz", "Game (USA).gba.gz", gz(&a)),
        fixture("lz4", "Game (USA).gba.lz4", lz4(&a)),
    ]
}

/// The limits of one plan-time open, with the 4 MiB budget.
fn limits() -> Limits {
    Limits {
        plan_head_budget: BUDGET,
        plan_listing_budget: BUDGET,
        ..Limits::default()
    }
}

/// Opens `path` and streams every file entry to its end. The outcome is
/// ignored: `Ok` and `Err` are both fine, only an unwind is not. No
/// external tool runs (`Some(&[])`), so the suite is hermetic and fast.
fn open_and_stream(path: &Path, limits: &Limits) -> Result<(), ArchiveError> {
    let Some(mut reader) = open_archive_with(path, limits, Some(&[]))? else {
        return Ok(());
    };
    let wanted: Vec<usize> = file_entries(reader.as_ref()).map(|e| e.index).collect();
    let mut budget = Budget::plan_head(limits);
    let mut sink = [0u8; 8192];
    reader.stream_entries(&wanted, &mut budget, &mut |_, data| {
        while data.read(&mut sink)? != 0 {}
        Ok(())
    })
}

/// Runs `rounds` seeded mutations of every fixture and returns the total
/// time, logging the time per format.
fn run_suite(rounds: usize) -> Duration {
    let dir = tempfile::tempdir().expect("tempdir");
    let limits = limits();
    let mut total = Duration::ZERO;
    for (i, f) in fixtures().iter().enumerate() {
        // Every fixture is valid before it is garbled.
        let path = dir.path().join(format!("{i:02}-{}", f.name));
        std::fs::write(&path, &f.bytes).expect("write fixture");
        open_and_stream(&path, &limits)
            .unwrap_or_else(|e| panic!("{}: intact fixture fails: {e:?}", f.label));

        let started = Instant::now();
        let (mut ok, mut err, mut caught) = (0usize, 0usize, 0usize);
        for bytes in mutate(0xC0FE_0000 + i as u64, &f.bytes, rounds) {
            std::fs::write(&path, &bytes).expect("write mutation");
            match open_and_stream(&path, &limits) {
                Ok(()) => ok += 1,
                // A panic the reader's guard caught and mapped (S1).
                Err(ArchiveError::Damaged { detail }) if detail == PANIC_DETAIL => {
                    err += 1;
                    caught += 1;
                }
                Err(_) => err += 1,
            }
        }
        let took = started.elapsed();
        total += took;
        eprintln!(
            "{:<12} {rounds} mutations in {:>7.2?} ({ok} ok, {err} err, {caught} caught panics)",
            f.label, took
        );
        assert_eq!(ok + err, rounds, "{}: every mutation was run", f.label);
    }
    eprintln!("total {total:.2?}");
    total
}

#[test]
fn garbled_archives_never_panic() {
    let total = run_suite(ROUNDS);
    if !cfg!(debug_assertions) {
        assert!(
            total <= RELEASE_LIMIT,
            "mutation suite took {total:?}, over {RELEASE_LIMIT:?}"
        );
    }
}

#[test]
#[ignore = "long run: 100,000 mutations per format"]
fn garbled_archives_never_panic_100k() {
    run_suite(ROUNDS_LONG);
}

// ---------------------------------------------------------------------------
// The mutation engine itself.
// ---------------------------------------------------------------------------

#[test]
fn mutate_is_deterministic_and_counted() {
    let bytes = zip_with(&[("a.gba", b"cartridge bytes", ZipMethod::Deflate)]);
    let one: Vec<Vec<u8>> = mutate(7, &bytes, 50).collect();
    let two: Vec<Vec<u8>> = mutate(7, &bytes, 50).collect();
    assert_eq!(one.len(), 50);
    assert_eq!(one, two, "same seed, same mutations");
    let other: Vec<Vec<u8>> = mutate(8, &bytes, 50).collect();
    assert_ne!(one, other, "another seed, other mutations");
    assert!(
        one.iter().all(|m| *m != bytes),
        "every mutation changes bytes"
    );
    assert!(
        one.iter().any(|m| m.len() < bytes.len()),
        "some mutations truncate"
    );
    assert!(
        one.iter().any(|m| m.len() == bytes.len()),
        "some mutations keep the length"
    );
}

#[test]
fn mutate_of_empty_input_is_empty() {
    assert_eq!(mutate(1, &[], 10).count(), 0);
}

#[test]
fn length_fields_found_in_zip_7z_and_tar() {
    let zip = zip_with(&[("a.gba", b"abc", ZipMethod::Stored)]);
    let fields = length_fields(&zip);
    // Local header: compressed size at 18, uncompressed size at 22.
    assert!(fields.contains(&(18, FieldWidth::Le32)), "{fields:?}");
    assert!(fields.contains(&(22, FieldWidth::Le32)), "{fields:?}");
    // End record: central directory size and offset.
    let eocd = zip.len() - 22;
    assert!(
        fields.contains(&(eocd + 12, FieldWidth::Le32)),
        "{fields:?}"
    );
    assert!(
        fields.contains(&(eocd + 16, FieldWidth::Le32)),
        "{fields:?}"
    );

    let sz = sevenz(&[("a.bin", b"abc")], SevenZShape::non_solid_lzma());
    let fields = length_fields(&sz);
    assert!(fields.contains(&(12, FieldWidth::Le64)), "{fields:?}");
    assert!(fields.contains(&(20, FieldWidth::Le64)), "{fields:?}");

    let t = tar(&[("a.bin", b"abc"), ("b.bin", b"defg")]);
    let fields = length_fields(&t);
    assert!(fields.contains(&(124, FieldWidth::Octal12)), "{fields:?}");
    assert!(
        fields.contains(&(1024 + 124, FieldWidth::Octal12)),
        "{fields:?}"
    );
}

#[test]
fn mutate_rewrites_known_length_fields() {
    let bytes = zip_with(&[("a.gba", &sample(500, 3), ZipMethod::Stored)]);
    let fields = length_fields(&bytes);
    let touched = mutate(11, &bytes, 400)
        .filter(|m| m.len() == bytes.len())
        .filter(|m| {
            fields
                .iter()
                .any(|&(at, w)| m[at..at + w.size()] != bytes[at..at + w.size()])
        })
        .count();
    assert!(touched >= 50, "only {touched} mutations hit a length field");
}

/// 7z mutations that keep the length re-seal the start header CRC half of
/// the time, so they reach the header walker instead of all stopping at
/// the CRC check.
#[test]
fn mutate_reseals_sevenz_start_header() {
    let bytes = sevenz(&[("a.bin", &sample(500, 5))], SevenZShape::non_solid_lzma());
    let resealed = mutate(13, &bytes, 400)
        .filter(|m| m.len() >= 32 && m[..6] == bytes[..6])
        .filter(|m| crc32fast::hash(&m[12..32]).to_le_bytes() == m[8..12])
        .filter(|m| m[8..32] != bytes[8..32])
        .count();
    assert!(resealed >= 20, "only {resealed} resealed start headers");
}
