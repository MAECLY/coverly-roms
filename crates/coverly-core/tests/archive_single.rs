//! Single-file compressors of the archive reader (spec §5.1, §9.1). Fixtures
//! are synthetic.

use std::io::Read;

use ruzstd::decoding::{FrameDecoder, StreamingDecoder};
use ruzstd::encoding::{compress_to_vec, CompressionLevel};

/// Decode one zstd frame and return the (stored, calculated) content checksums.
fn zstd_checksums(frame: &[u8]) -> (Option<u32>, Option<u32>) {
    let mut decoder = StreamingDecoder::new(frame).expect("frame header");
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).expect("decodes");
    let frame: FrameDecoder = decoder.into_frame_decoder();
    (
        frame.get_checksum_from_data(),
        frame.get_calculated_checksum(),
    )
}

/// Spec §9.1 checks the zstd content checksum when a frame has one. `ruzstd`
/// only computes it with its `hash` feature: without it this test does not
/// compile, and the dev encoder writes frames without a checksum.
#[test]
fn ruzstd_verifies_content_checksum() {
    let data = b"Coverly synthetic payload ".repeat(64);
    let mut frame = compress_to_vec(&data[..], CompressionLevel::Fastest);
    // Frame header descriptor: bit 2 is Content_Checksum_flag.
    assert_ne!(frame[4] & 0b100, 0, "encoder wrote no content checksum");

    let (stored, calculated) = zstd_checksums(&frame);
    assert!(stored.is_some());
    assert_eq!(stored, calculated);

    let last = frame.len() - 1;
    frame[last] ^= 0xFF;
    let (stored, calculated) = zstd_checksums(&frame);
    assert_ne!(stored, calculated, "a flipped checksum byte must be seen");
}

// ---------------------------------------------------------------------------
// `SingleReader` (plan Task 11): one virtual entry per compressed file.
// ---------------------------------------------------------------------------

use std::io::{Cursor, Write};

use coverly_core::archive::single::{SingleReader, SizeSource};
use coverly_core::archive::{
    ArchiveError, ArchiveReader, Budget, Compressor, Format, Limits, Method,
};

/// Deterministic, mildly compressible synthetic bytes.
fn sample(len: usize) -> Vec<u8> {
    let mut state = 0x2545_F491u32;
    (0..len)
        .map(|i| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            if i % 7 < 4 {
                b'A' + (i % 13) as u8
            } else {
                state as u8
            }
        })
        .collect()
}

fn gz(data: &[u8], fname: Option<&[u8]>) -> Vec<u8> {
    let mut b = flate2::GzBuilder::new();
    if let Some(name) = fname {
        b = b.filename(name);
    }
    let mut w = b.write(Vec::new(), flate2::Compression::fast());
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
    compress_to_vec(data, CompressionLevel::Fastest)
}

fn lz4(data: &[u8], content_size: bool) -> Vec<u8> {
    let mut info = lz4_flex::frame::FrameInfo::new();
    if content_size {
        info = info.content_size(Some(data.len() as u64));
    }
    info.content_checksum = true;
    let mut w = lz4_flex::frame::FrameEncoder::with_frame_info(info, Vec::new());
    w.write_all(data).unwrap();
    w.finish().unwrap()
}

fn lzma_alone(data: &[u8], declare_size: bool) -> Vec<u8> {
    let mut options = lzma_rust2::LzmaOptions::with_preset(1);
    options.dict_size = 1 << 16;
    let size = declare_size.then_some(data.len() as u64);
    let mut w = lzma_rust2::LzmaWriter::new_use_header(Vec::new(), &options, size).unwrap();
    w.write_all(data).unwrap();
    w.finish().unwrap()
}

fn open(
    bytes: Vec<u8>,
    c: Compressor,
    name: &str,
    limits: &Limits,
) -> Result<SingleReader<Cursor<Vec<u8>>>, ArchiveError> {
    SingleReader::new(Cursor::new(bytes), c, name, limits)
}

/// Every decoded byte of the one entry, or the error that stopped it.
fn read_entry(r: &mut dyn ArchiveReader) -> Result<Vec<u8>, ArchiveError> {
    let mut budget = Budget::apply();
    let mut stream = r.open_entry(0, &mut budget)?;
    let mut out = Vec::new();
    stream.read_to_end(&mut out)?;
    Ok(out)
}

/// Opens and decodes: the error of whichever step failed.
fn open_and_read(bytes: Vec<u8>, c: Compressor, limits: &Limits) -> Result<Vec<u8>, ArchiveError> {
    let mut r = open(bytes, c, "x.bin.cmp", limits)?;
    read_entry(&mut r)
}

fn round_trip(bytes: Vec<u8>, c: Compressor, data: &[u8]) -> SingleReader<Cursor<Vec<u8>>> {
    let mut r = open(bytes, c, "Game (USA).gba.cmp", &Limits::default()).expect("lists");
    assert_eq!(r.format(), Format::Single(c));
    assert_eq!(r.entries().len(), 1);
    let e = &r.entries()[0];
    assert_eq!(e.index, 0);
    assert_eq!(e.name.as_str(), "Game (USA).gba");
    assert_eq!(e.size, data.len() as u64);
    assert_eq!(e.crc32, Some(crc32fast::hash(data)));
    assert!(r.listing_complete());
    assert_eq!(read_entry(&mut r).expect("decodes"), data);
    r
}

#[test]
fn gz_fname_used_and_validated() {
    let data = sample(20_000);
    let mut r = open(
        gz(&data, Some(b"Tetris (World).gb")),
        Compressor::Gzip,
        "renamed.gz",
        &Limits::default(),
    )
    .unwrap();
    let e = &r.entries()[0];
    assert_eq!(e.name.as_str(), "Tetris (World).gb");
    assert_eq!(e.unsafe_name, None);
    assert_eq!(e.method, Method::Deflate);
    assert_eq!(e.crc32, Some(crc32fast::hash(&data)));
    assert_eq!(read_entry(&mut r).unwrap(), data);

    // Latin-1 FNAME bytes (RFC 1952) decode instead of being dropped.
    let r = open(
        gz(&data, Some(b"Pok\xe9mon.gb")),
        Compressor::Gzip,
        "x.gz",
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(r.entries()[0].name.as_str(), "Pokémon.gb");
}

#[test]
fn gz_unsafe_fname_falls_back_to_stem() {
    let data = sample(1_000);
    for evil in [
        &b"../../etc/passwd"[..],
        b"/abs/game.gba",
        b"C:\\x.gba",
        b"CON",
        b"a:b",
    ] {
        let r = open(
            gz(&data, Some(evil)),
            Compressor::Gzip,
            "Zelda (USA).gba.gz",
            &Limits::default(),
        )
        .unwrap();
        let e = &r.entries()[0];
        assert_eq!(
            e.name.as_str(),
            "Zelda (USA).gba",
            "{}",
            String::from_utf8_lossy(evil)
        );
        assert_eq!(e.unsafe_name, None);
    }
    // No FNAME at all: the stem as well.
    let r = open(
        gz(&data, None),
        Compressor::Gzip,
        "game.gba.gz",
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(r.entries()[0].name.as_str(), "game.gba");
    // A stem that is itself unsafe stays flagged, never extracted.
    let r = open(
        gz(&data, None),
        Compressor::Gzip,
        "CON.gz",
        &Limits::default(),
    )
    .unwrap();
    assert!(r.entries()[0].unsafe_name.is_some());
}

#[test]
fn gz_concatenated_members() {
    let a = sample(30_000);
    let b = b"second member ".repeat(500);
    let mut bytes = gz(&a, None);
    bytes.extend_from_slice(&gz(&b, None));
    let mut whole = a.clone();
    whole.extend_from_slice(&b);
    let r = round_trip(bytes, Compressor::Gzip, &whole);
    assert_eq!(r.size_source(), SizeSource::Measured);
}

/// Concatenated lz4 frames are one stream, as the `lz4` tool reads them:
/// `lz4_flex` ends each frame with a clean `Ok(0)`, so a reader that
/// stopped there would lose every later frame (and disagree with the
/// content sizes the frame walk adds up).
#[test]
fn lz4_concatenated_frames() {
    let a = sample(30_000);
    let b = b"second frame ".repeat(500);
    for content_size in [false, true] {
        let mut bytes = lz4(&a, content_size);
        bytes.extend_from_slice(&lz4(b"", content_size));
        bytes.extend_from_slice(&lz4(&b, content_size));
        let mut whole = a.clone();
        whole.extend_from_slice(&b);
        round_trip(bytes, Compressor::Lz4, &whole);
    }
}

#[test]
fn gz_bad_crc_damaged() {
    let mut bytes = gz(&sample(5_000), None);
    let at = bytes.len() - 8;
    bytes[at] ^= 0xFF;
    let err = open_and_read(bytes, Compressor::Gzip, &Limits::default()).unwrap_err();
    assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
}

#[test]
fn bz2_round_trip() {
    let data = sample(150_000);
    let r = round_trip(bz2(&data), Compressor::Bzip2, &data);
    assert_eq!(r.entries()[0].method, Method::Bzip2);
}

#[test]
fn xz_round_trip() {
    let data = sample(150_000);
    let r = round_trip(xz(&data), Compressor::Xz, &data);
    assert_eq!(r.entries()[0].method, Method::Xz { dict: 1 << 16 });
}

#[test]
fn zst_round_trip() {
    let data = sample(150_000);
    let r = round_trip(zst(&data), Compressor::Zstd, &data);
    assert!(matches!(r.entries()[0].method, Method::Zstd { .. }));
}

#[test]
fn lz4_round_trip() {
    let data = sample(150_000);
    for content_size in [false, true] {
        let r = round_trip(lz4(&data, content_size), Compressor::Lz4, &data);
        assert_eq!(r.entries()[0].method, Method::Lz4);
    }
}

#[test]
fn lzma_alone_round_trip() {
    let data = sample(150_000);
    for declared in [false, true] {
        let r = round_trip(lzma_alone(&data, declared), Compressor::LzmaAlone, &data);
        assert_eq!(r.entries()[0].method, Method::Lzma { dict: 1 << 16 });
    }
}

#[test]
fn lzma_alone_huge_dict_refused_at_list() {
    let mut bytes = lzma_alone(&sample(1_000), true);
    bytes[1..5].copy_from_slice(&0x6000_0000u32.to_le_bytes()); // 1.5 GiB
    let err = open(bytes, Compressor::LzmaAlone, "x.lzma", &Limits::default())
        .err()
        .unwrap();
    assert!(
        matches!(err, ArchiveError::NeedsTooMuchMemory { .. }),
        "{err:?}"
    );
}

/// A zstd frame header that is never decoded: magic, descriptor and the
/// fields it announces, then one raw block of `payload`.
fn zstd_crafted(desc: u8, fields: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut out = 0xFD2F_B528u32.to_le_bytes().to_vec();
    out.push(desc);
    out.extend_from_slice(fields);
    // Last block, raw, size = payload length.
    let header = 1u32 | ((payload.len() as u32) << 3);
    out.extend_from_slice(&header.to_le_bytes()[..3]);
    out.extend_from_slice(payload);
    out
}

#[test]
fn zstd_second_frame_huge_window_refused() {
    let mut bytes = zst(&sample(10_000));
    // Window descriptor exponent 21: 1 << 31 = 2 GiB.
    bytes.extend_from_slice(&zstd_crafted(0x00, &[21 << 3], b"x"));
    let err = open_and_read(bytes, Compressor::Zstd, &Limits::default()).unwrap_err();
    assert!(
        matches!(err, ArchiveError::NeedsTooMuchMemory { .. }),
        "{err:?}"
    );
}

#[test]
fn zstd_single_segment_huge_content_size_refused() {
    // Single segment, 8-byte content size of 1 TiB: the window is the size.
    let bytes = zstd_crafted(0xE0, &(1u64 << 40).to_le_bytes(), b"x");
    let err = open(bytes, Compressor::Zstd, "x.zst", &Limits::default())
        .err()
        .unwrap();
    assert!(
        matches!(err, ArchiveError::NeedsTooMuchMemory { .. }),
        "{err:?}"
    );
}

fn skippable(len: u32, body: &[u8]) -> Vec<u8> {
    let mut out = 0x184D_2A50u32.to_le_bytes().to_vec();
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(body);
    out
}

#[test]
fn zstd_skippable_frame_bounded() {
    let data = sample(5_000);
    // A small skippable frame before the data is fine.
    let mut ok = skippable(1024, &[0u8; 1024]);
    ok.extend_from_slice(&zst(&data));
    round_trip(ok, Compressor::Zstd, &data);

    // One that declares 4 GiB in a tiny file ends at once, without
    // allocating what it declares.
    let mut huge = skippable(u32::MAX, &[0u8; 16]);
    huge.extend_from_slice(&zst(&data));
    let started = std::time::Instant::now();
    let err = open_and_read(huge, Compressor::Zstd, &Limits::default()).unwrap_err();
    assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(1));

    // Skippable bytes come out of a bounded allowance.
    let limits = Limits {
        max_header: 512,
        ..Limits::default()
    };
    let mut over = skippable(1024, &[0u8; 1024]);
    over.extend_from_slice(&zst(&data));
    let err = open_and_read(over, Compressor::Zstd, &limits).unwrap_err();
    assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
}

#[test]
fn single_size_unknown_measured_within_budget() {
    // bzip2 never declares a size: the bounded decode measures it.
    let data = sample(200_000);
    let r = round_trip(bz2(&data), Compressor::Bzip2, &data);
    assert_eq!(r.size_source(), SizeSource::Measured);

    // Past the bounded decode the size is unknown: the listing is not
    // complete (never extracted), the head is still readable.
    let limits = Limits {
        plan_head_budget: 16 * 1024,
        ..Limits::default()
    };
    let mut r = open(bz2(&data), Compressor::Bzip2, "big.iso.bz2", &limits).unwrap();
    assert_eq!(r.size_source(), SizeSource::Unknown);
    assert!(!r.listing_complete());
    assert_eq!(r.entries()[0].crc32, None);
    assert_eq!(read_entry(&mut r).unwrap(), data);
}

#[test]
fn single_size_declared_past_the_budget() {
    let data = sample(200_000);
    let limits = Limits {
        plan_head_budget: 16 * 1024,
        ..Limits::default()
    };
    let cases: Vec<(Vec<u8>, Compressor)> = vec![
        (xz(&data), Compressor::Xz),
        (zstd_raw_frames(&data), Compressor::Zstd),
        (lz4(&data, true), Compressor::Lz4),
        (lzma_alone(&data, true), Compressor::LzmaAlone),
    ];
    for (bytes, c) in cases {
        let mut r = open(bytes, c, "big.iso.cmp", &limits).unwrap();
        assert_eq!(r.size_source(), SizeSource::Declared, "{c:?}");
        assert!(r.listing_complete(), "{c:?}");
        assert_eq!(r.entries()[0].size, data.len() as u64, "{c:?}");
        assert_eq!(read_entry(&mut r).unwrap(), data, "{c:?}");
    }
    // Frames without a content size stay unknown (`ruzstd` writes none).
    let r = open(lz4(&data, false), Compressor::Lz4, "big.iso.lz4", &limits).unwrap();
    assert_eq!(r.size_source(), SizeSource::Unknown);
    let r = open(zst(&data), Compressor::Zstd, "big.iso.zst", &limits).unwrap();
    assert_eq!(r.size_source(), SizeSource::Unknown);
}

/// The gzip trailer is not a size: `ISIZE` is the size mod 4 GiB (a PS2
/// DVD image of 4.5 GiB would declare 0.5 GiB, passing a FAT32 check it
/// must fail) and, with several members, only the last member's size and
/// CRC32. Past the bounded decode a gzip size is unknown, and the stream
/// still reads back whole (`MultiGzDecoder` checks every member).
#[test]
fn gz_trailer_never_declares_the_size() {
    let limits = Limits {
        plan_head_budget: 16 * 1024,
        ..Limits::default()
    };
    let data = sample(200_000);
    let mut r = open(gz(&data, None), Compressor::Gzip, "big.iso.gz", &limits).unwrap();
    assert_eq!(r.size_source(), SizeSource::Unknown);
    assert!(!r.listing_complete());
    assert_eq!(r.entries()[0].crc32, None);
    assert_eq!(read_entry(&mut r).unwrap(), data);

    // Two members, the last one larger than the bounded decode: its
    // trailer must not become the entry's size and CRC32.
    let a = sample(30_000);
    let b = b"second member payload ".repeat(5_000);
    let mut bytes = gz(&a, None);
    bytes.extend_from_slice(&gz(&b, None));
    let mut whole = a.clone();
    whole.extend_from_slice(&b);
    let mut r = open(bytes, Compressor::Gzip, "two.iso.gz", &limits).unwrap();
    assert_eq!(r.size_source(), SizeSource::Unknown);
    assert_ne!(r.entries()[0].size, b.len() as u64);
    assert_eq!(read_entry(&mut r).unwrap(), whole);
}

/// Two zstd frames of raw blocks, each declaring its content size (4-byte
/// field, 128 KiB window), so the frame walk can sum them.
fn zstd_raw_frames(data: &[u8]) -> Vec<u8> {
    let (a, b) = data.split_at(data.len() / 3);
    let mut out = Vec::new();
    for part in [a, b] {
        out.extend_from_slice(&0xFD2F_B528u32.to_le_bytes());
        out.push(0x80); // FCS 4 bytes, not single segment, no checksum
        out.push((17 - 10) << 3);
        out.extend_from_slice(&(part.len() as u32).to_le_bytes());
        let chunks: Vec<&[u8]> = part.chunks(128 * 1024).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let last = u32::from(i + 1 == chunks.len());
            let header = last | ((chunk.len() as u32) << 3);
            out.extend_from_slice(&header.to_le_bytes()[..3]);
            out.extend_from_slice(chunk);
        }
    }
    out
}

#[test]
fn lz4_truncated_at_block_boundary_damaged() {
    // lz4_flex ends cleanly when a frame stops between blocks; the frame
    // walk must still see that the end mark is missing.
    let data = sample(300_000);
    let bytes = lz4(&data, false);
    // Drop the content checksum and the end mark (4 + 4 bytes).
    let cut = bytes[..bytes.len() - 8].to_vec();
    let err = open_and_read(cut, Compressor::Lz4, &Limits::default()).unwrap_err();
    assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
}

#[test]
fn single_garbled_inputs_never_panic() {
    let fixtures: Vec<(Vec<u8>, Compressor)> = vec![
        (gz(&sample(4_000), Some(b"g.gb")), Compressor::Gzip),
        (bz2(&sample(4_000)), Compressor::Bzip2),
        (xz(&sample(4_000)), Compressor::Xz),
        (zst(&sample(4_000)), Compressor::Zstd),
        (lz4(&sample(4_000), true), Compressor::Lz4),
        (lzma_alone(&sample(4_000), false), Compressor::LzmaAlone),
    ];
    for (bytes, c) in fixtures {
        let mut seed = 0x9E37_79B9u32;
        for _ in 0..300 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let mut m = bytes.clone();
            let at = seed as usize % m.len();
            if seed & 1 == 0 {
                m[at] ^= 1 << ((seed >> 8) % 8);
            } else {
                m.truncate(at);
            }
            let _ = open_and_read(m, c, &Limits::default());
        }
    }
}
