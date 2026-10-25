//! tar and compressed tar of the archive reader (plan Task 12; spec §4.3,
//! §13 S6, S6b and the extras). Fixtures are synthetic.

use std::io::{Cursor, Read, Write};
use std::time::{Duration, Instant};

use coverly_core::archive::tar::TarReader;
use coverly_core::archive::testing::{
    gzip_with_zeros, pax_records, tar, tar_crafted, tar_gz, tar_records, TarSpec,
};
use coverly_core::archive::{
    ArchiveError, ArchiveReader, Budget, BudgetKind, Compressor, EntryKind, Format, Limits, Method,
    UnsafeWhy,
};

const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;

/// Deterministic, poorly compressible synthetic bytes.
fn noise(len: usize, seed: u32) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}

fn gz(data: &[u8]) -> Vec<u8> {
    let mut w = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
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
    let mut w = lz4_flex::frame::FrameEncoder::new(Vec::new());
    w.write_all(data).unwrap();
    w.finish().unwrap()
}

fn compress(c: Compressor, data: &[u8]) -> Vec<u8> {
    match c {
        Compressor::Gzip => gz(data),
        Compressor::Bzip2 => bz2(data),
        Compressor::Xz => xz(data),
        Compressor::Zstd => zst(data),
        Compressor::Lz4 => lz4(data),
        Compressor::LzmaAlone => unreachable!("not used for tar here"),
    }
}

fn open(
    bytes: Vec<u8>,
    outer: Option<Compressor>,
    limits: &Limits,
) -> Result<TarReader<Cursor<Vec<u8>>>, ArchiveError> {
    TarReader::new(Cursor::new(bytes), outer, limits)
}

fn read_entry(r: &mut dyn ArchiveReader, index: usize) -> Result<Vec<u8>, ArchiveError> {
    let mut budget = Budget::apply();
    let mut stream = r.open_entry(index, &mut budget)?;
    let mut out = Vec::new();
    stream.read_to_end(&mut out)?;
    Ok(out)
}

/// Streams `wanted` with `budget`; every sunk entry read to its end.
fn stream(
    r: &mut dyn ArchiveReader,
    wanted: &[usize],
    budget: &mut Budget,
) -> Result<Vec<(usize, Vec<u8>)>, ArchiveError> {
    let mut got = Vec::new();
    r.stream_entries(wanted, budget, &mut |entry, data| {
        let mut out = Vec::new();
        data.read_to_end(&mut out)?;
        got.push((entry.index, out));
        Ok(())
    })?;
    Ok(got)
}

fn is_damaged(e: &ArchiveError) -> bool {
    matches!(e, ArchiveError::Damaged { .. })
}

fn detail(e: &ArchiveError) -> &str {
    match e {
        ArchiveError::Damaged { detail } => detail,
        _ => "",
    }
}

fn spent_bytes(e: &ArchiveError) -> bool {
    matches!(
        e,
        ArchiveError::BudgetSpent {
            what: BudgetKind::DecodedBytes
        }
    )
}

// ---------------------------------------------------------------------------
// Listing and reading.
// ---------------------------------------------------------------------------

#[test]
fn plain_tar_lists_and_reads() {
    let rom = noise(3000, 7);
    let sheet = b"FILE \"Game (Track 01).bin\" BINARY\n".to_vec();
    let bytes = tar(&[
        ("Game/", b""),
        ("Game/Game (Track 01).bin", &rom),
        ("Game/Game.cue", &sheet),
    ]);
    let mut r = open(bytes, None, &Limits::default()).expect("lists");
    assert_eq!(r.format(), Format::Tar { outer: None });
    assert!(r.listing_complete());
    let names: Vec<(&str, EntryKind, u64)> = r
        .entries()
        .iter()
        .map(|e| (e.name.as_str(), e.kind, e.size))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Game", EntryKind::Dir, 0),
            ("Game/Game (Track 01).bin", EntryKind::File, 3000),
            ("Game/Game.cue", EntryKind::File, sheet.len() as u64),
        ]
    );
    for e in r.entries() {
        assert_eq!(e.crc32, None, "tar stores no CRC");
        assert_eq!(e.method, Method::Stored);
        assert!(e.mtime.is_some());
    }
    assert_eq!(read_entry(&mut r, 1).unwrap(), rom);
    assert_eq!(read_entry(&mut r, 2).unwrap(), sheet);
    let got = stream(&mut r, &[2, 1], &mut Budget::apply()).unwrap();
    assert_eq!(got, vec![(1, rom), (2, sheet)]);
}

#[test]
fn compressed_tar_round_trips() {
    let a = noise(70_000, 3);
    let b = b"Coverly synthetic cartridge".repeat(100);
    let plain = tar(&[("a.gba", &a), ("dir/b.gb", &b)]);
    for c in [
        Compressor::Gzip,
        Compressor::Bzip2,
        Compressor::Xz,
        Compressor::Zstd,
        Compressor::Lz4,
    ] {
        let mut r = open(compress(c, &plain), Some(c), &Limits::default())
            .unwrap_or_else(|e| panic!("{c:?}: {e}"));
        assert_eq!(r.format(), Format::Tar { outer: Some(c) });
        assert!(r.listing_complete(), "{c:?}");
        assert_eq!(r.entries().len(), 2, "{c:?}");
        assert_eq!(r.entries()[0].solid_block, Some(0), "{c:?}: one stream");
        assert_eq!(read_entry(&mut r, 1).unwrap(), b, "{c:?}");
        let got = stream(&mut r, &[0, 1], &mut Budget::apply()).unwrap();
        assert_eq!(got, vec![(0, a.clone()), (1, b.clone())], "{c:?}");
    }
}

#[test]
fn compressed_open_entry_charges_the_skipped_prefix() {
    let big = noise(3 * MIB as usize, 9);
    let plain = tar(&[("big.bin", &big), ("small.gba", b"tiny")]);
    let mut r = open(gz(&plain), Some(Compressor::Gzip), &Limits::default()).unwrap();
    let mut budget = Budget::apply();
    let mut out = Vec::new();
    r.open_entry(1, &mut budget)
        .unwrap()
        .read_to_end(&mut out)
        .unwrap();
    assert_eq!(out, b"tiny");
    assert!(budget.spent() >= 3 * MIB, "spent {}", budget.spent());
}

#[test]
fn ustar_prefix_and_gnu_header_names() {
    let mut prefixed = TarSpec::file("rom.gba", b"x");
    prefixed.patch = b"Long Folder"
        .iter()
        .enumerate()
        .map(|(i, &b)| (345 + i, b))
        .collect();
    let bytes = tar_crafted(&[prefixed, TarSpec::file("gnu.gb", b"y").gnu()]);
    let r = open(bytes, None, &Limits::default()).unwrap();
    assert_eq!(r.entries()[0].name.as_str(), "Long Folder/rom.gba");
    assert_eq!(r.entries()[1].name.as_str(), "gnu.gb");
}

#[test]
fn bad_header_checksum_damaged() {
    let bytes = tar_crafted(&[
        TarSpec::file("a.gba", b"a"),
        TarSpec::file("b.gba", b"b").bad_checksum(),
    ]);
    let err = open(bytes, None, &Limits::default())
        .err()
        .expect("refused");
    assert!(is_damaged(&err), "{err:?}");
}

#[test]
fn truncated_tar_damaged() {
    let mut bytes = tar(&[("a.gba", &noise(2000, 1))]);
    bytes.truncate(512 + 700);
    let err = open(bytes, None, &Limits::default())
        .err()
        .expect("refused");
    assert!(is_damaged(&err), "{err:?}");
}

#[test]
fn compressed_tar_without_end_marker_damaged() {
    let plain = tar_records(&[TarSpec::file("a.gba", b"abc")]);
    let err = open(gz(&plain), Some(Compressor::Gzip), &Limits::default())
        .err()
        .expect("refused");
    assert!(is_damaged(&err), "{err:?}");
}

#[test]
fn empty_tar_damaged() {
    let err = open(vec![0u8; 1024], None, &Limits::default())
        .err()
        .expect("refused");
    assert!(is_damaged(&err), "{err:?}");
}

#[test]
fn lone_zero_block_then_data_damaged() {
    let mut bytes = tar_records(&[TarSpec::file("a.gba", b"a")]);
    bytes.resize(bytes.len() + 512, 0);
    bytes.extend_from_slice(&tar_records(&[TarSpec::file("hidden.gba", b"h")]));
    bytes.resize(bytes.len() + 1024, 0);
    let err = open(bytes, None, &Limits::default())
        .err()
        .expect("refused");
    assert!(is_damaged(&err), "{err:?}");
}

#[test]
fn archive_changed_after_listing_damaged() {
    let bytes = tar(&[("a.gba", b"aaaa"), ("b.gba", b"bbbb")]);
    let mut r = open(bytes, None, &Limits::default()).unwrap();
    // Same length, another name for the second entry.
    let other = tar(&[("a.gba", b"aaaa"), ("c.gba", b"bbbb")]);
    *r.source_mut() = Cursor::new(other);
    let err = read_entry(&mut r, 1).expect_err("refused");
    assert!(is_damaged(&err), "{err:?}");
    let err = stream(&mut r, &[1], &mut Budget::apply()).expect_err("refused");
    assert!(is_damaged(&err), "{err:?}");
}

#[test]
fn compressed_stream_with_more_entries_than_a_complete_listing_damaged() {
    let a = tar(&[("a.gba", b"aaaa")]);
    let mut r = open(gz(&a), Some(Compressor::Gzip), &Limits::default()).unwrap();
    assert!(r.listing_complete());
    // The same first entry, then one the listing never saw.
    let grown = tar(&[("a.gba", b"aaaa"), ("b.gba", b"bbbb")]);
    *r.source_mut() = Cursor::new(gz(&grown));
    let err = stream(&mut r, &[0], &mut Budget::apply()).expect_err("refused");
    assert!(is_damaged(&err), "{err:?}");
}

#[test]
fn stream_of_partial_listing_bounds_the_trailer() {
    // The plan listing stops early; the apply pass walks to the end marker
    // and must still cap the drain after it (S6).
    let big = noise(2 * MIB as usize, 17);
    let mut plain = tar(&[("a.gba", b"first"), ("big.bin", &big)]);
    let tar_len = plain.len() as u64;
    plain.extend(std::iter::repeat_n(0u8, 8 * MIB as usize));
    let limits = Limits {
        plan_listing_budget: MIB,
        ..Limits::default()
    };
    let mut r = open(gz(&plain), Some(Compressor::Gzip), &limits).expect("partial listing");
    assert!(!r.listing_complete());
    let mut budget = Budget::apply();
    let err = stream(&mut r, &[0], &mut budget).expect_err("refused");
    assert!(is_damaged(&err), "{err:?}");
    assert_eq!(detail(&err), "trailing data");
    assert!(budget.spent() <= tar_len + limits.tar_trailer_allowance + 1);
}

// ---------------------------------------------------------------------------
// S6: the drain after the end marker is capped.
// ---------------------------------------------------------------------------

/// The listing of a compressed tar whose stream goes on after the end
/// marker: refused fast with `BudgetSpent` at plan time (and decoding no
/// more than the tar plus the allowance), `Damaged { "trailing data" }`
/// with the apply budget.
fn assert_trailer_bounded(bytes: Vec<u8>, c: Compressor, tar_len: u64) {
    let limits = Limits::default();
    let started = Instant::now();
    let mut budget = Budget::plan_listing(&limits);
    let err = TarReader::new_with_budget(Cursor::new(bytes.clone()), Some(c), &limits, &mut budget)
        .err()
        .expect("trailer refused");
    let elapsed = started.elapsed();
    assert!(spent_bytes(&err), "{c:?}: {err:?}");
    assert!(
        budget.spent() <= tar_len + limits.tar_trailer_allowance + 1,
        "{c:?}: decoded {} bytes",
        budget.spent()
    );
    assert!(elapsed < Duration::from_secs(1), "{c:?}: took {elapsed:?}");

    let started = Instant::now();
    let err =
        TarReader::new_with_budget(Cursor::new(bytes), Some(c), &limits, &mut Budget::apply())
            .err()
            .expect("trailer refused");
    assert!(is_damaged(&err), "{c:?}: {err:?}");
    assert_eq!(detail(&err), "trailing data", "{c:?}");
    assert!(started.elapsed() < Duration::from_secs(1), "{c:?}");
}

/// `count` copies of `unit` after `head`: a compressed stream of the tar
/// followed by streams/frames/members of 1 MiB of zeros each.
fn repeated(head: Vec<u8>, unit: &[u8], count: usize) -> Vec<u8> {
    let mut out = head;
    out.reserve(unit.len() * count);
    for _ in 0..count {
        out.extend_from_slice(unit);
    }
    out
}

fn small_tar() -> Vec<u8> {
    tar(&[("Tetris (World).gb", &noise(32 * 1024, 5))])
}

#[test]
fn tar_gz_huge_zero_trailer_bounded() {
    let plain = small_tar();
    // One gzip member: the tar, then 4 GiB of zeros (about 4 MB packed).
    let bytes = tar_gz(&[("Tetris (World).gb", &noise(32 * 1024, 5))], 4 * GIB);
    assert!(bytes.len() < 64 * MIB as usize);
    assert_trailer_bounded(bytes, Compressor::Gzip, plain.len() as u64);
}

#[test]
fn tar_gz_builder_is_valid_gzip() {
    let bytes = gzip_with_zeros(b"prefix", 5 * MIB + 17);
    let mut out = Vec::new();
    flate2::read::MultiGzDecoder::new(&bytes[..])
        .read_to_end(&mut out)
        .expect("valid member");
    assert_eq!(out.len() as u64, 6 + 5 * MIB + 17);
    assert_eq!(&out[..6], b"prefix");
    assert!(out[6..].iter().all(|&b| b == 0));
}

#[test]
fn tar_xz_huge_zero_trailer_bounded() {
    let plain = small_tar();
    let unit = xz(&vec![0u8; MIB as usize]);
    let bytes = repeated(xz(&plain), &unit, 4096);
    assert_trailer_bounded(bytes, Compressor::Xz, plain.len() as u64);
}

#[test]
fn tar_zst_huge_zero_trailer_bounded() {
    let plain = small_tar();
    let unit = zst(&vec![0u8; MIB as usize]);
    let bytes = repeated(zst(&plain), &unit, 4096);
    assert_trailer_bounded(bytes, Compressor::Zstd, plain.len() as u64);
}

#[test]
fn tar_bz2_huge_zero_trailer_bounded() {
    let plain = small_tar();
    let unit = bz2(&vec![0u8; MIB as usize]);
    let bytes = repeated(bz2(&plain), &unit, 4096);
    assert_trailer_bounded(bytes, Compressor::Bzip2, plain.len() as u64);
}

/// lz4 frames of zeros after the tar's frame count against the same
/// allowance as gzip members and xz streams.
#[test]
fn tar_lz4_huge_zero_trailer_bounded() {
    let plain = small_tar();
    let unit = lz4(&vec![0u8; MIB as usize]);
    let bytes = repeated(lz4(&plain), &unit, 4096);
    assert_trailer_bounded(bytes, Compressor::Lz4, plain.len() as u64);
}

/// A legacy `.lzma` stream that goes on with zeros after the tar's end
/// marker, inside the one stream (the format has no members).
#[test]
fn tar_lzma_huge_zero_trailer_bounded() {
    let plain = small_tar();
    let mut data = plain.clone();
    data.resize(plain.len() + 16 * MIB as usize, 0);
    let mut options = lzma_rust2::LzmaOptions::with_preset(1);
    options.dict_size = 1 << 16;
    let mut w = lzma_rust2::LzmaWriter::new_no_header(Vec::new(), &options, false).unwrap();
    w.write_all(&data).unwrap();
    let raw = w.finish().unwrap();
    let props = coverly_core::archive::testing::lzma_props(
        options.lc as u8,
        options.lp as u8,
        options.pb as u8,
        options.dict_size,
    );
    let mut bytes = props.to_vec();
    bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&raw);
    assert_trailer_bounded(bytes, Compressor::LzmaAlone, plain.len() as u64);
}

#[test]
fn tar_gz_second_member_after_marker_bounded() {
    let plain = small_tar();
    let mut bytes = gz(&plain);
    bytes.extend_from_slice(&gzip_with_zeros(b"", 64 * MIB));
    assert_trailer_bounded(bytes, Compressor::Gzip, plain.len() as u64);
}

#[test]
fn tar_gz_normal_padding_ok() {
    // GNU tar pads the archive to a 10 KiB record with zeros.
    let mut plain = tar(&[("Tetris (World).gb", &noise(3000, 2))]);
    plain.resize(plain.len().next_multiple_of(10 * 1024), 0);
    let mut r = open(gz(&plain), Some(Compressor::Gzip), &Limits::default()).expect("lists");
    assert!(r.listing_complete());
    let got = stream(&mut r, &[0], &mut Budget::apply()).expect("streams");
    assert_eq!(got[0].1.len(), 3000);
}

// ---------------------------------------------------------------------------
// S6b: metadata records are capped and parsed by us.
// ---------------------------------------------------------------------------

#[test]
fn tar_pax_header_huge_size_refused() {
    for size in [2 * GIB, MIB + 1] {
        let bytes = tar_crafted(&[
            TarSpec::pax(&[("path", b"a.gba")]).declared_size(size),
            TarSpec::file("a.gba", b"a"),
        ]);
        let started = Instant::now();
        let err = open(bytes, None, &Limits::default())
            .err()
            .expect("refused");
        assert!(is_damaged(&err), "{size}: {err:?}");
        assert!(started.elapsed() < Duration::from_secs(1));
    }
    // A global header too.
    let bytes = tar_crafted(&[
        TarSpec::pax_global(&[("comment", b"x")]).declared_size(2 * GIB),
        TarSpec::file("a.gba", b"a"),
    ]);
    let err = open(bytes, None, &Limits::default())
        .err()
        .expect("refused");
    assert!(is_damaged(&err), "{err:?}");
}

#[test]
fn tar_pax_record_at_cap_accepted() {
    let comment = vec![b'c'; (MIB - 64) as usize];
    let bytes = tar_crafted(&[
        TarSpec::pax(&[("comment", &comment)]),
        TarSpec::file("a.gba", b"a"),
    ]);
    let r = open(bytes, None, &Limits::default()).expect("lists");
    assert_eq!(r.entries()[0].name.as_str(), "a.gba");
}

#[test]
fn tar_gnu_longname_huge_refused() {
    for spec in [
        TarSpec::gnu_longname(b"x.gba").declared_size(GIB),
        TarSpec::gnu_longlink(b"x.gba").declared_size(GIB),
    ] {
        let bytes = tar_crafted(&[spec, TarSpec::file("a.gba", b"a").gnu()]);
        let err = open(bytes, None, &Limits::default())
            .err()
            .expect("refused");
        assert!(is_damaged(&err), "{err:?}");
    }
}

#[test]
fn tar_gnu_longname_used() {
    let long = format!(
        "{}/Game (Track 01).bin",
        ["Very Long Folder Name"; 8].join(" - ")
    );
    let bytes = tar_crafted(&[
        TarSpec::gnu_longname(long.as_bytes()),
        TarSpec::file("truncated", b"data").gnu(),
    ]);
    let r = open(bytes, None, &Limits::default()).unwrap();
    assert_eq!(r.entries().len(), 1);
    assert_eq!(r.entries()[0].name.as_str(), long);
}

#[test]
fn tar_pax_length_overflow_damaged() {
    let cases: [&[u8]; 6] = [
        b"99999999999999999999999 path=x\n",
        b"18446744073709551616 path=x\n",
        b"200 path=x\n",
        b"0 path=x\n",
        b"11 path=x\nX",
        b"9 pathx\n\n\n",
    ];
    for data in cases {
        let bytes = tar_crafted(&[
            TarSpec::raw(b'x', b"PaxHeaders/a", data),
            TarSpec::file("a.gba", b"a"),
        ]);
        let err = open(bytes, None, &Limits::default())
            .err()
            .unwrap_or_else(|| panic!("{:?} accepted", String::from_utf8_lossy(data)));
        assert!(is_damaged(&err), "{err:?}");
    }
    // pax `size` with more digits than u64 holds.
    let bytes = tar_crafted(&[
        TarSpec::pax(&[("size", b"99999999999999999999999")]),
        TarSpec::file("a.gba", b"a"),
    ]);
    assert!(is_damaged(
        &open(bytes, None, &Limits::default()).err().unwrap()
    ));
}

#[test]
fn tar_pax_path_overrides_validated_by_relpath() {
    let bytes = tar_crafted(&[
        TarSpec::pax(&[("path", "Games/Pokémon Rojo (Spain).gb".as_bytes())]),
        TarSpec::file("short.gb", b"good"),
        TarSpec::pax(&[("path", b"../../etc/evil.gba")]),
        TarSpec::file("safe.gba", b"evil"),
        TarSpec::file("next.gba", b"next"),
    ]);
    let mut r = open(bytes, None, &Limits::default()).unwrap();
    let e = r.entries();
    assert_eq!(e.len(), 3);
    assert_eq!(e[0].name.as_str(), "Games/Pokémon Rojo (Spain).gb");
    assert_eq!(e[0].unsafe_name, None);
    assert_eq!(e[1].unsafe_name, Some(UnsafeWhy::DotDot));
    assert_eq!(e[1].raw_name, b"../../etc/evil.gba");
    assert_eq!(
        e[2].name.as_str(),
        "next.gba",
        "the override is for one entry"
    );
    assert_eq!(read_entry(&mut r, 0).unwrap(), b"good");
    let err = read_entry(&mut r, 1).expect_err("refused");
    assert!(
        matches!(
            err,
            ArchiveError::UnsafeEntry {
                why: UnsafeWhy::DotDot,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn tar_pax_size_overrides_header_size() {
    let data = noise(1500, 4);
    let bytes = tar_crafted(&[
        TarSpec::pax(&[("size", b"1500")]),
        TarSpec::file("a.bin", &data).declared_size(0),
        TarSpec::file("b.bin", b"bb"),
    ]);
    let mut r = open(bytes, None, &Limits::default()).unwrap();
    assert_eq!(r.entries()[0].size, 1500);
    assert_eq!(read_entry(&mut r, 0).unwrap(), data);
    assert_eq!(read_entry(&mut r, 1).unwrap(), b"bb");
}

#[test]
fn tar_pax_global_override_damaged() {
    let bytes = tar_crafted(&[
        TarSpec::pax_global(&[("path", b"other.gba")]),
        TarSpec::file("a.gba", b"a"),
    ]);
    assert!(is_damaged(
        &open(bytes, None, &Limits::default()).err().unwrap()
    ));
    // A global comment (git archive writes one) is fine.
    let bytes = tar_crafted(&[
        TarSpec::pax_global(&[("comment", b"0123abcd")]),
        TarSpec::file("a.gba", b"a"),
    ]);
    assert_eq!(
        open(bytes, None, &Limits::default())
            .unwrap()
            .entries()
            .len(),
        1
    );
}

#[test]
fn tar_metadata_records_per_entry_capped() {
    let mut specs: Vec<TarSpec> = (0..9)
        .map(|i| TarSpec::pax_global(&[("comment", format!("{i}").as_bytes())]))
        .collect();
    specs.push(TarSpec::file("a.gba", b"a"));
    assert!(is_damaged(
        &open(tar_crafted(&specs), None, &Limits::default())
            .err()
            .unwrap()
    ));
    specs.drain(..1);
    assert!(open(tar_crafted(&specs), None, &Limits::default()).is_ok());
}

#[test]
fn tar_two_pax_headers_for_one_entry_damaged() {
    let bytes = tar_crafted(&[
        TarSpec::pax(&[("path", b"a.gba")]),
        TarSpec::pax(&[("path", b"b.gba")]),
        TarSpec::file("c.gba", b"c"),
    ]);
    assert!(is_damaged(
        &open(bytes, None, &Limits::default()).err().unwrap()
    ));
}

#[test]
fn tar_sparse_entry_other() {
    let bytes = tar_crafted(&[
        TarSpec::raw(b'S', b"sparse.img", &[0u8; 512]).gnu(),
        TarSpec::pax(&[("GNU.sparse.major", b"1"), ("GNU.sparse.minor", b"0")]),
        TarSpec::file("GNUSparseFile.0/disc.iso", &[1u8; 600]),
        TarSpec::raw(b'M', b"volume.bin", b"cont"),
        TarSpec::file("after.gba", b"after"),
    ]);
    let mut r = open(bytes, None, &Limits::default()).unwrap();
    let kinds: Vec<EntryKind> = r.entries().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            EntryKind::Other,
            EntryKind::Other,
            EntryKind::Other,
            EntryKind::File
        ]
    );
    assert!(read_entry(&mut r, 0).is_err(), "never read");
    assert_eq!(read_entry(&mut r, 3).unwrap(), b"after");
}

#[test]
fn tar_old_gnu_sparse_extension_blocks_skipped() {
    // isextended (offset 482) set: one extension block follows the header.
    let mut ext = [0u8; 512];
    ext[504] = 0; // no further extension
    let mut bytes = tar_header_bytes(
        TarSpec::raw(b'S', b"sparse.img", b"")
            .gnu()
            .patch(482, 1)
            .declared_size(512),
    );
    bytes.extend_from_slice(&ext);
    bytes.extend_from_slice(&[7u8; 512]);
    bytes.extend_from_slice(&tar_crafted(&[TarSpec::file("after.gba", b"after")]));
    let mut r = open(bytes, None, &Limits::default()).unwrap();
    assert_eq!(r.entries()[0].kind, EntryKind::Other);
    assert_eq!(read_entry(&mut r, 1).unwrap(), b"after");
}

fn tar_header_bytes(spec: TarSpec) -> Vec<u8> {
    coverly_core::archive::testing::tar_header(&spec).to_vec()
}

// ---------------------------------------------------------------------------
// Links, caps and budgets.
// ---------------------------------------------------------------------------

#[test]
fn tar_symlink_then_file_through_it() {
    let bytes = tar_crafted(&[
        TarSpec::symlink("d", "/etc"),
        TarSpec::file("d/passwd", b"root:x:0:0"),
        TarSpec::file("D/Shadow", b"case twin of the link"),
        TarSpec::file("dx/ok.gba", b"ok"),
    ]);
    let mut r = open(bytes, None, &Limits::default()).unwrap();
    let e = r.entries();
    assert_eq!(e[0].kind, EntryKind::Symlink);
    assert_eq!(e[1].unsafe_name, Some(UnsafeWhy::Link));
    assert_eq!(e[2].unsafe_name, Some(UnsafeWhy::Link));
    assert_eq!(e[3].unsafe_name, None);
    for index in [0, 1, 2] {
        let err = read_entry(&mut r, index).expect_err("refused");
        assert!(
            matches!(
                err,
                ArchiveError::UnsafeEntry {
                    why: UnsafeWhy::Link,
                    ..
                }
            ),
            "{index}: {err:?}"
        );
    }
    let err = stream(&mut r, &[1], &mut Budget::apply()).err().unwrap();
    assert!(matches!(err, ArchiveError::UnsafeEntry { .. }), "{err:?}");
    assert_eq!(read_entry(&mut r, 3).unwrap(), b"ok");
}

#[test]
fn tar_hardlink_refused() {
    let bytes = tar_crafted(&[
        TarSpec::file("game.gba", b"game"),
        TarSpec::hardlink("copy.gba", "game.gba"),
    ]);
    let mut r = open(bytes, None, &Limits::default()).unwrap();
    assert_eq!(r.entries()[1].kind, EntryKind::Hardlink);
    let err = read_entry(&mut r, 1).err().unwrap();
    assert!(
        matches!(
            err,
            ArchiveError::UnsafeEntry {
                why: UnsafeWhy::Link,
                ..
            }
        ),
        "{err:?}"
    );
    let err = stream(&mut r, &[0, 1], &mut Budget::apply()).err().unwrap();
    assert!(matches!(err, ArchiveError::UnsafeEntry { .. }), "{err:?}");
}

#[test]
fn tar_device_and_fifo_entries_other() {
    let bytes = tar_crafted(&[
        TarSpec::raw(b'3', b"tty", b""),
        TarSpec::raw(b'4', b"disk", b""),
        TarSpec::raw(b'6', b"fifo", b""),
        TarSpec::file("a.gba", b"a"),
    ]);
    let r = open(bytes, None, &Limits::default()).unwrap();
    let kinds: Vec<EntryKind> = r.entries().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            EntryKind::Other,
            EntryKind::Other,
            EntryKind::Other,
            EntryKind::File
        ]
    );
}

#[test]
fn tar_many_tiny_entries_capped() {
    let limits = Limits {
        max_entries: 100,
        ..Limits::default()
    };
    let names: Vec<String> = (0..101).map(|i| format!("{i}.gb")).collect();
    let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
    let err = open(tar(&entries), None, &limits).err().unwrap();
    assert!(
        matches!(err, ArchiveError::TooManyEntries { count: 101 }),
        "{err:?}"
    );
    assert!(open(tar(&entries[..100]), None, &limits).is_ok());

    // Metadata records count against the same cap.
    let mut specs = Vec::new();
    for name in &names[..51] {
        specs.push(TarSpec::pax_global(&[("comment", b"c")]));
        specs.push(TarSpec::pax(&[("comment", b"c")]));
        specs.push(TarSpec::file(name, b"x"));
    }
    let err = open(tar_crafted(&specs), None, &limits).err().unwrap();
    assert!(
        matches!(err, ArchiveError::TooManyEntries { .. }),
        "{err:?}"
    );
}

#[test]
fn tar_entry_and_total_size_caps() {
    let limits = Limits {
        max_entry_size: 1000,
        max_total_size: 1500,
        ..Limits::default()
    };
    let one = tar_crafted(&[TarSpec::file("a.bin", b"a").declared_size(1001)]);
    let err = open(one, None, &limits).err().unwrap();
    assert!(matches!(err, ArchiveError::TooLarge { .. }), "{err:?}");
    let two = tar(&[("a.bin", &[0u8; 800]), ("b.bin", &[0u8; 800])]);
    let err = open(two, None, &limits).err().unwrap();
    assert!(matches!(err, ArchiveError::TooLarge { .. }), "{err:?}");
}

#[test]
fn tar_listing_incomplete_when_budget_spent() {
    let big = noise(2 * MIB as usize, 11);
    let plain = tar(&[("a.gba", b"first"), ("big.bin", &big), ("c.gba", b"last")]);
    let limits = Limits {
        plan_listing_budget: MIB,
        ..Limits::default()
    };
    let mut r = open(gz(&plain), Some(Compressor::Gzip), &limits).expect("partial listing");
    assert!(!r.listing_complete());
    let names: Vec<&str> = r.entries().iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["a.gba", "big.bin"]);
    assert_eq!(read_entry(&mut r, 0).unwrap(), b"first");
    // The apply pass still goes to the end and checks the stream.
    let got = stream(&mut r, &[1], &mut Budget::apply()).unwrap();
    assert_eq!(got, vec![(1, big)]);

    // Not even one entry within the budget: not read at all.
    let limits = Limits {
        plan_listing_budget: 100,
        ..Limits::default()
    };
    let err = open(gz(&plain), Some(Compressor::Gzip), &limits)
        .err()
        .unwrap();
    assert!(spent_bytes(&err), "{err:?}");

    // A plain tar is listed by seeking: no decode budget is involved.
    let r = open(plain, None, &limits).unwrap();
    assert!(r.listing_complete());
    assert_eq!(r.entries().len(), 3);
}

#[test]
fn tar_gz_bad_crc_at_eof_damaged_on_apply_budget() {
    let big = noise(2 * MIB as usize, 13);
    let plain = tar(&[("a.gba", b"first"), ("big.bin", &big)]);
    let mut bytes = gz(&plain);
    let at = bytes.len() - 8; // CRC32 of the member trailer
    bytes[at] ^= 0xFF;

    // The plan listing stops before the end: nothing seems wrong yet.
    let limits = Limits {
        plan_listing_budget: MIB,
        ..Limits::default()
    };
    let mut r = open(bytes.clone(), Some(Compressor::Gzip), &limits).expect("partial listing");
    assert!(!r.listing_complete());
    // Every entry's size matches, but the stream check at EOF fails.
    let err = stream(&mut r, &[0, 1], &mut Budget::apply()).expect_err("refused");
    assert!(is_damaged(&err), "{err:?}");

    // A listing that reaches the end sees it at once.
    let err = open(bytes, Some(Compressor::Gzip), &Limits::default())
        .err()
        .unwrap();
    assert!(is_damaged(&err), "{err:?}");
}

#[test]
fn pax_records_builder_lengths_count_themselves() {
    let data = pax_records(&[("path", b"a"), ("comment", &[b'x'; 95])]);
    let text = String::from_utf8(data).unwrap();
    let mut rest = text.as_str();
    while !rest.is_empty() {
        let (len, _) = rest.split_once(' ').unwrap();
        let len: usize = len.parse().unwrap();
        assert!(rest[..len].ends_with('\n'));
        rest = &rest[len..];
    }
}

#[test]
fn garbled_tar_never_panics() {
    let base = tar(&[("a.gba", &noise(700, 1)), ("b/c.gb", b"cc")]);
    let mut state = 0x9E37_79B9u32;
    for round in 0..400 {
        let mut bytes = base.clone();
        for _ in 0..1 + round % 4 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let at = state as usize % bytes.len();
            bytes[at] ^= 1 << (state >> 29);
        }
        if round % 7 == 0 {
            bytes.truncate(state as usize % bytes.len());
        }
        for outer in [None, Some(Compressor::Gzip)] {
            let input = match outer {
                Some(c) => compress(c, &bytes),
                None => bytes.clone(),
            };
            if let Ok(mut r) = open(input, outer, &Limits::default()) {
                let all: Vec<usize> = (0..r.entries().len()).collect();
                let _ = stream(&mut r, &all, &mut Budget::apply());
            }
        }
    }
}
