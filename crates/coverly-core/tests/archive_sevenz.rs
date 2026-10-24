//! 7z bounded header walker (spec §4.3, §13 S4, S5, §13.1). Every archive
//! here is crafted in the test; no copyrighted bytes.

use std::io::Cursor;

use coverly_core::archive::sevenz::{walk, Walked};
use coverly_core::archive::testing::{
    sevenz_copy, sevenz_copy_parts, sevenz_encoded_header_bytes, sevenz_encoded_streams,
    sevenz_file, sevenz_header_bytes, sevenz_with_encoded, sevenz_with_encoded_header, SzCoder,
    SzFile, SzFolder, SzHeader, SzStreams, SzSubstreams,
};
use coverly_core::archive::{ArchiveError, EntryKind, Limits, Method, UnsafeWhy, DECODER_MEM_CAP};

const MIB: u64 = 1024 * 1024;

fn sample(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i * 7 % 251) as u8 ^ (i / 300) as u8)
        .collect()
}

fn walk_with(bytes: &[u8], limits: &Limits) -> Result<Walked, ArchiveError> {
    let mut src = Cursor::new(bytes.to_vec());
    walk(&mut src, bytes.len() as u64, limits)
}

fn walk_bytes(bytes: &[u8]) -> Result<Walked, ArchiveError> {
    walk_with(bytes, &Limits::default())
}

#[track_caller]
fn assert_damaged(r: Result<Walked, ArchiveError>) {
    match r {
        Err(ArchiveError::Damaged { .. }) => {}
        other => panic!("expected Damaged, got {:?}", other.map(|w| w.files.len())),
    }
}

/// The two-file Copy archive most tests start from, as parts.
fn two_files() -> (Vec<u8>, SzHeader) {
    sevenz_copy_parts(&[
        ("Game (Track 01).bin", &sample(3000)),
        ("Game.cue", b"FILE"),
    ])
}

// ---------------------------------------------------------------------------
// Valid archives
// ---------------------------------------------------------------------------

#[test]
fn walk_lists_copy_archive() {
    let a = sample(5000);
    let bytes = sevenz_copy(&[("dir/a.bin", &a), ("b.txt", b"hello"), ("empty.bin", b"")]);
    let w = walk_bytes(&bytes).expect("valid 7z");
    assert_eq!(w.files.len(), 3);
    assert_eq!(w.folders.len(), 2);
    assert_eq!(w.header_end, bytes.len() as u64);
    let f0 = &w.files[0];
    assert_eq!(f0.entry.name.as_str(), "dir/a.bin");
    assert_eq!(f0.entry.kind, EntryKind::File);
    assert_eq!(f0.entry.size, 5000);
    assert_eq!(f0.entry.crc32, Some(crc32fast::hash(&a)));
    assert_eq!(f0.entry.method, Method::Stored);
    assert_eq!(f0.entry.solid_block, Some(0));
    assert_eq!(f0.folder, Some(0));
    assert_eq!(f0.offset, 0);
    assert_eq!(w.files[1].entry.size, 5);
    assert_eq!(w.files[1].folder, Some(1));
    let empty = &w.files[2];
    assert_eq!(empty.entry.kind, EntryKind::File);
    assert_eq!(empty.entry.size, 0);
    assert_eq!(empty.folder, None);
    // Folder geometry: pack ranges are absolute file offsets.
    assert_eq!(w.folders[0].pack_pos, 32);
    assert_eq!(w.folders[0].pack_size, 5000);
    assert_eq!(w.folders[1].pack_pos, 32 + 5000);
    assert_eq!(w.folders[1].unpack_size, 5);
}

#[test]
fn walk_lists_directories_and_mtime() {
    let (packed, mut h) = sevenz_copy_parts(&[("Game/Game.cue", b"FILE")]);
    h.files.push(SzFile::dir("Game"));
    // 2001-09-09T01:46:40Z = 1_000_000_000 s after the Unix epoch.
    let filetime = (11_644_473_600u64 + 1_000_000_000) * 10_000_000;
    h.files[0] = SzFile::file("Game/Game.cue").mtime(filetime);
    let w = walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))).expect("valid");
    assert_eq!(w.files[1].entry.kind, EntryKind::Dir);
    let mtime = w.files[0].entry.mtime.expect("mtime");
    let secs = mtime
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after epoch")
        .as_secs();
    assert_eq!(secs, 1_000_000_000);
}

#[test]
fn walk_lists_solid_folder_substreams() {
    let a = sample(4000);
    let b = sample(1000);
    let mut packed = a.clone();
    packed.extend_from_slice(&b);
    let h = SzHeader {
        streams: Some(SzStreams {
            pack_pos: 0,
            pack_sizes: vec![5000],
            folders: vec![SzFolder::single(SzCoder::copy(), 5000)],
            substreams: Some(SzSubstreams {
                num_unpack: Some(vec![2]),
                sizes: Some(vec![4000]),
                crcs: Some(vec![crc32fast::hash(&a), crc32fast::hash(&b)]),
            }),
        }),
        files: vec![SzFile::file("t1.bin"), SzFile::file("t2.bin")],
        ..SzHeader::default()
    };
    let w = walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))).expect("valid");
    assert_eq!(w.files[1].entry.size, 1000);
    assert_eq!(w.files[1].offset, 4000);
    assert_eq!(w.files[1].entry.solid_block, Some(0));
    assert_eq!(w.files[1].entry.crc32, Some(crc32fast::hash(&b)));
}

#[test]
fn walk_lists_encoded_header() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    let bytes = sevenz_with_encoded_header(&packed, &raw);
    let w = walk_bytes(&bytes).expect("encoded header");
    assert_eq!(w.files.len(), 2);
    assert_eq!(w.files[1].entry.name.as_str(), "Game.cue");
}

/// An archive written by `sevenz-rust2` (LZMA2 solid, LZMA-compressed
/// header) walks: the walker is not only compatible with our own writer.
#[test]
fn walk_lists_crate_written_solid_archive() {
    use sevenz_rust2::encoder_options::Lzma2Options;
    use sevenz_rust2::{ArchiveEntry, ArchiveWriter, SourceReader};
    let tracks: Vec<(String, Vec<u8>)> = (1..=3)
        .map(|n| (format!("Game (Track {n:02}).bin"), sample(20_000 + n)))
        .collect();
    let mut w = ArchiveWriter::new(Cursor::new(Vec::new())).expect("writer");
    let mut opts = Lzma2Options::from_level(6);
    opts.set_dictionary_size(1 << 20);
    w.set_content_methods(vec![opts.into()]);
    let entries = tracks
        .iter()
        .map(|(n, _)| ArchiveEntry::new_file(n))
        .chain(std::iter::once(ArchiveEntry::new_file("Game.cue")))
        .collect();
    let readers = tracks
        .iter()
        .map(|(_, d)| SourceReader::new(Cursor::new(d.clone())))
        .chain(std::iter::once(SourceReader::new(Cursor::new(
            b"FILE \"Game (Track 01).bin\" BINARY".to_vec(),
        ))))
        .collect();
    w.push_archive_entries(entries, readers).expect("push");
    let bytes = w.finish().expect("finish").into_inner();
    let walked = walk_bytes(&bytes).expect("crate-written 7z");
    assert_eq!(walked.folders.len(), 1);
    assert_eq!(walked.files.len(), 4);
    assert_eq!(walked.files[3].entry.name.as_str(), "Game.cue");
    assert_eq!(walked.files[3].offset, 3 * 20_000 + 6);
    assert_eq!(
        walked.files[0].entry.method,
        Method::Lzma2 { dict: 1 << 20 }
    );
    for (file, (_, data)) in walked.files.iter().zip(&tracks) {
        assert_eq!(file.entry.crc32, Some(crc32fast::hash(data)));
    }
}

#[test]
fn walk_empty_archive_has_no_files() {
    let w = walk_bytes(&sevenz_file(&[], &[])).expect("empty 7z");
    assert!(w.files.is_empty());
}

// ---------------------------------------------------------------------------
// Start header and next header
// ---------------------------------------------------------------------------

#[test]
fn start_header_crc_mismatch_damaged() {
    let mut bytes = sevenz_copy(&[("a.bin", b"abc")]);
    bytes[12] ^= 1;
    assert_damaged(walk_bytes(&bytes));
}

#[test]
fn next_header_crc_mismatch_damaged() {
    let mut bytes = sevenz_copy(&[("a.bin", b"abc")]);
    let last = bytes.len() - 2;
    bytes[last] ^= 0x40;
    assert_damaged(walk_bytes(&bytes));
}

/// Rewrites the start header's offset/size fields and fixes its CRC.
fn with_next_header(mut bytes: Vec<u8>, offset: u64, size: u64) -> Vec<u8> {
    bytes[12..20].copy_from_slice(&offset.to_le_bytes());
    bytes[20..28].copy_from_slice(&size.to_le_bytes());
    let crc = crc32fast::hash(&bytes[12..32]);
    bytes[8..12].copy_from_slice(&crc.to_le_bytes());
    bytes
}

#[test]
fn next_header_offset_overflow_damaged() {
    let bytes = sevenz_copy(&[("a.bin", b"abc")]);
    assert_damaged(walk_bytes(&with_next_header(
        bytes.clone(),
        u64::MAX - 8,
        40,
    )));
    let len = bytes.len() as u64;
    assert_damaged(walk_bytes(&with_next_header(bytes, len, 1)));
}

#[test]
fn next_header_over_max_header_too_large() {
    let bytes = sevenz_copy(&[("a.bin", b"abc")]);
    let header_len = bytes.len() as u64 - 32 - 3;
    let limits = Limits {
        max_header: header_len - 1,
        ..Limits::default()
    };
    assert!(matches!(
        walk_with(&bytes, &limits),
        Err(ArchiveError::TooLarge { .. })
    ));
}

#[test]
fn unknown_first_header_byte_damaged() {
    assert_damaged(walk_bytes(&sevenz_file(&[], &[0x02, 0x00])));
}

// ---------------------------------------------------------------------------
// S4: encoded header
// ---------------------------------------------------------------------------

#[test]
fn nested_encoded_header_is_damaged() {
    // The decoded header is itself a kEncodedHeader.
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    let (_, inner_streams) = sevenz_encoded_streams(packed.len() as u64, &raw);
    let inner = sevenz_encoded_header_bytes(&inner_streams);
    assert_eq!(inner[0], 0x17);
    assert_damaged(walk_bytes(&sevenz_with_encoded_header(&packed, &inner)));
}

#[test]
fn encoded_header_unpack_size_over_cap() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    let limits = Limits {
        max_header: raw.len() as u64 - 1,
        ..Limits::default()
    };
    let bytes = sevenz_with_encoded_header(&packed, &raw);
    assert!(
        matches!(
            walk_with(&bytes, &limits),
            Err(ArchiveError::TooLarge { .. })
        ),
        "decoded header over max_header"
    );
    // A declared size of 4 GiB is refused before anything is decoded.
    let bytes = sevenz_with_encoded(&packed, &raw, |s| s.folders[0].unpack_sizes = vec![1 << 32]);
    assert!(matches!(
        walk_bytes(&bytes),
        Err(ArchiveError::TooLarge { .. })
    ));
}

#[test]
fn encoded_header_dict_over_cap() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    // LZMA2 dictionary property 40 = 4 GiB.
    let bytes = sevenz_with_encoded(&packed, &raw, |s| {
        s.folders[0].coders[0] = SzCoder::lzma2(40);
    });
    assert!(matches!(
        walk_bytes(&bytes),
        Err(ArchiveError::NeedsTooMuchMemory { cap, .. }) if cap == DECODER_MEM_CAP
    ));
    // LZMA with a 1.5 GiB dictionary, likewise (S3 formula).
    let bytes = sevenz_with_encoded(&packed, &raw, |s| {
        s.folders[0].coders[0] = SzCoder::lzma(3 << 29);
    });
    assert!(matches!(
        walk_bytes(&bytes),
        Err(ArchiveError::NeedsTooMuchMemory { .. })
    ));
}

#[test]
fn encoded_header_two_folders_damaged() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    let bytes = sevenz_with_encoded(&packed, &raw, |s| {
        let f = s.folders[0].clone();
        s.folders.push(f);
        s.pack_sizes.push(0);
        if let Some(sub) = s.substreams.as_mut() {
            sub.crcs = Some(vec![1, 2]);
        }
    });
    assert_damaged(walk_bytes(&bytes));
}

#[test]
fn encoded_header_ppmd_coder_damaged() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    let bytes = sevenz_with_encoded(&packed, &raw, |s| {
        s.folders[0].coders[0] = SzCoder::ppmd(6, 16 << 20);
    });
    assert_damaged(walk_bytes(&bytes));
    let bytes = sevenz_with_encoded(&packed, &raw, |s| {
        s.folders[0].coders[0] = SzCoder::copy();
    });
    assert_damaged(walk_bytes(&bytes));
}

#[test]
fn encoded_header_without_crc_damaged() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    let bytes = sevenz_with_encoded(&packed, &raw, |s| s.substreams = None);
    assert_damaged(walk_bytes(&bytes));
}

#[test]
fn encoded_header_crc_in_folder_info_accepted() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    let bytes = sevenz_with_encoded(&packed, &raw, |s| {
        s.substreams = None;
        s.folders[0].crc = Some(crc32fast::hash(&raw));
    });
    assert_eq!(walk_bytes(&bytes).expect("folder CRC").files.len(), 2);
}

#[test]
fn encoded_header_crc_mismatch_damaged() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    let bytes = sevenz_with_encoded(&packed, &raw, |s| {
        s.substreams.as_mut().expect("sub").crcs = Some(vec![0xDEAD_BEEF]);
    });
    assert_damaged(walk_bytes(&bytes));
}

#[test]
fn encoded_header_pack_outside_file_damaged() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    let bytes = sevenz_with_encoded(&packed, &raw, |s| s.pack_pos = 1 << 40);
    assert_damaged(walk_bytes(&bytes));
    let bytes = sevenz_with_encoded(&packed, &raw, |s| s.pack_sizes[0] += 10_000);
    assert_damaged(walk_bytes(&bytes));
}

#[test]
fn encoded_header_with_bcj_filter_decodes() {
    // BCJ x86 over a header with no E8/E9 bytes leaves it unchanged.
    let (packed, h) = sevenz_copy_parts(&[("a.bin", b"abc")]);
    let raw = sevenz_header_bytes(&h);
    assert!(!raw.iter().any(|&b| b == 0xE8 || b == 0xE9));
    let bytes = sevenz_with_encoded(&packed, &raw, |s| {
        let size = s.folders[0].unpack_sizes[0];
        s.folders[0] = SzFolder::chain(vec![SzCoder::lzma2(16), SzCoder::bcj_x86()], size);
    });
    assert_eq!(walk_bytes(&bytes).expect("bcj header").files.len(), 1);
}

#[test]
fn decoded_header_with_too_many_files() {
    let files: Vec<(String, Vec<u8>)> = (0..4).map(|i| (format!("f{i}.bin"), vec![i])).collect();
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice()))
        .collect();
    let (packed, h) = sevenz_copy_parts(&refs);
    let bytes = sevenz_with_encoded_header(&packed, &sevenz_header_bytes(&h));
    let limits = Limits {
        max_entries: 3,
        ..Limits::default()
    };
    match walk_with(&bytes, &limits) {
        Err(ArchiveError::TooManyEntries { count }) => assert!(count > 3),
        other => panic!(
            "expected TooManyEntries, got {:?}",
            other.map(|w| w.files.len())
        ),
    }
    assert_eq!(walk_bytes(&bytes).expect("within default").files.len(), 4);
}

#[test]
fn sevenz_encrypted_header() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    let bytes = sevenz_with_encoded(&packed, &raw, |s| {
        let size = s.folders[0].unpack_sizes[0];
        s.folders[0] = SzFolder::chain(vec![SzCoder::aes(), SzCoder::lzma2(16)], size);
    });
    assert!(matches!(walk_bytes(&bytes), Err(ArchiveError::Encrypted)));
}

// ---------------------------------------------------------------------------
// S5: full kHeader validation, fail closed
// ---------------------------------------------------------------------------

#[test]
fn sevenz_declared_files_mismatch_names() {
    let (packed, mut h) = two_files();
    h.num_files = Some(1_000_000);
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
    // Small lies too: three declared, two names.
    h.num_files = Some(3);
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
    h.num_files = Some(1);
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
}

#[test]
fn sevenz_unknown_property_fails_closed() {
    let (packed, mut h) = two_files();
    h.extra_props = vec![(0x30, vec![1, 2, 3])];
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
    // kStartPos and kComment are not written by 7-Zip: refused as well.
    for id in [0x16, 0x18] {
        h.extra_props = vec![(id, vec![1, 0])];
        assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
    }
    // kDummy (alignment padding) is accepted, as often as 7-Zip writes it.
    h.extra_props = vec![(0x19, vec![0, 0, 0]), (0x19, vec![0])];
    walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))).expect("kDummy");
}

#[test]
fn sevenz_additional_streams_damaged() {
    // kHeader, kAdditionalStreamsInfo …
    assert_damaged(walk_bytes(&sevenz_file(&[], &[0x01, 0x03, 0x00, 0x00])));
    // kHeader, kArchiveProperties …
    assert_damaged(walk_bytes(&sevenz_file(&[], &[0x01, 0x02, 0x00, 0x00])));
}

#[test]
fn sevenz_trailing_bytes_after_kend_damaged() {
    let (packed, mut h) = two_files();
    h.trailing = vec![0x00];
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
}

#[test]
fn sevenz_truncated_header_damaged() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    for cut in 1..raw.len() {
        assert_damaged(walk_bytes(&sevenz_file(&packed, &raw[..cut])));
    }
}

#[test]
fn sevenz_pack_pos_outside_file() {
    let (packed, mut h) = two_files();
    h.streams.as_mut().expect("streams").pack_pos = 1 << 40;
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
    let (packed, mut h) = two_files();
    h.streams.as_mut().expect("streams").pack_pos = u64::MAX - 10;
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
    let (packed, mut h) = two_files();
    h.streams.as_mut().expect("streams").pack_sizes[1] += 1;
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
}

#[test]
fn sevenz_coder_count_over_cap() {
    let (packed, mut h) = two_files();
    let s = h.streams.as_mut().expect("streams");
    let size = s.folders[0].unpack_sizes[0];
    s.folders[0] = SzFolder::chain(vec![SzCoder::copy(); 9], size);
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
    // Eight is the limit and walks.
    let (packed, mut h) = two_files();
    let s = h.streams.as_mut().expect("streams");
    let size = s.folders[0].unpack_sizes[0];
    s.folders[0] = SzFolder::chain(vec![SzCoder::copy(); 8], size);
    walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))).expect("eight coders");
}

#[test]
fn sevenz_bind_pairs_inconsistent_damaged() {
    let base = |edit: &dyn Fn(&mut SzFolder)| {
        let (packed, mut h) = two_files();
        let s = h.streams.as_mut().expect("streams");
        let size = s.folders[0].unpack_sizes[0];
        s.folders[0] = SzFolder::chain(vec![SzCoder::copy(), SzCoder::copy()], size);
        edit(&mut s.folders[0]);
        sevenz_file(&packed, &sevenz_header_bytes(&h))
    };
    walk_bytes(&base(&|_| {})).expect("valid chain");
    // A coder bound to itself (a cycle).
    assert_damaged(walk_bytes(&base(&|f| f.bind_pairs = vec![(0, 0)])));
    // An index out of range.
    assert_damaged(walk_bytes(&base(&|f| f.bind_pairs = vec![(5, 0)])));
    assert_damaged(walk_bytes(&base(&|f| f.bind_pairs = vec![(1, 7)])));
}

#[test]
fn sevenz_size_preserving_coders_must_agree() {
    // Copy, BCJ and Delta write exactly what they read: a filter whose
    // input and output sizes differ is a lie (fail closed, S5).
    let (packed, mut h) = two_files();
    let s = h.streams.as_mut().expect("streams");
    let size = s.folders[0].unpack_sizes[0];
    s.folders[0] = SzFolder::chain(vec![SzCoder::copy(), SzCoder::copy()], size);
    s.folders[0].unpack_sizes[0] = size + 1000;
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
    // A Copy coder reading a packed stream of another size, likewise.
    let (packed, mut h) = two_files();
    let s = h.streams.as_mut().expect("streams");
    s.folders[0].unpack_sizes[0] -= 1;
    s.substreams = None;
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
    // In the encoded header: LZMA2 declares more than BCJ produces.
    let (packed, h) = sevenz_copy_parts(&[("a.bin", b"abc")]);
    let raw = sevenz_header_bytes(&h);
    let bytes = sevenz_with_encoded(&packed, &raw, |s| {
        let size = s.folders[0].unpack_sizes[0];
        s.folders[0] = SzFolder::chain(vec![SzCoder::lzma2(16), SzCoder::bcj_x86()], size);
        s.folders[0].unpack_sizes[0] = 1 << 40;
    });
    assert_damaged(walk_bytes(&bytes));
}

#[test]
fn sevenz_substreams_must_sum_to_unpack_size() {
    let (packed, mut h) = two_files();
    let s = h.streams.as_mut().expect("streams");
    s.substreams = Some(SzSubstreams {
        num_unpack: Some(vec![2, 1]),
        sizes: Some(vec![5000]), // folder 0 is only 3000 bytes
        crcs: None,
    });
    h.files.insert(1, SzFile::file("extra.bin"));
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
}

#[test]
fn sevenz_stream_count_must_match_files() {
    // Two streams, but one file with a stream and one empty.
    let (packed, mut h) = two_files();
    h.files[1] = SzFile::empty("Game.cue");
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
}

#[test]
fn sevenz_anti_items_damaged() {
    let (packed, mut h) = sevenz_copy_parts(&[("a.bin", b"abc")]);
    h.files.push(SzFile::empty("gone.bin"));
    // kAnti over the one empty-stream entry, bit set.
    h.extra_props = vec![(0x10, vec![0x80])];
    assert_damaged(walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))));
}

#[test]
fn sevenz_external_flag_damaged() {
    let (packed, h) = two_files();
    let mut raw = sevenz_header_bytes(&h);
    // The byte after kFolder + count is "external".
    let at = raw
        .windows(3)
        .position(|w| w == [0x0B, 0x02, 0x00])
        .expect("kFolder");
    raw[at + 2] = 1;
    assert_damaged(walk_bytes(&sevenz_file(&packed, &raw)));
}

#[test]
fn sevenz_ppmd_mem_4gib_refused() {
    let (packed, mut h) = two_files();
    let s = h.streams.as_mut().expect("streams");
    s.folders[0].coders[0] = SzCoder::ppmd(6, u32::MAX);
    assert!(matches!(
        walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))),
        Err(ArchiveError::NeedsTooMuchMemory { needed, .. }) if needed >= u64::from(u32::MAX)
    ));
    // 16 MiB walks and reports the model size.
    let (packed, mut h) = two_files();
    let s = h.streams.as_mut().expect("streams");
    s.folders[0].coders[0] = SzCoder::ppmd(6, 16 << 20);
    let w = walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))).expect("ppmd");
    assert_eq!(
        w.files[0].entry.method,
        Method::Ppmd {
            order: 6,
            mem: 16 << 20
        }
    );
    assert!(w.folders[0].mem_needed >= 16 * MIB);
}

#[test]
fn sevenz_main_lzma_dict_over_cap_refused() {
    let (packed, mut h) = two_files();
    let s = h.streams.as_mut().expect("streams");
    s.folders[0].coders[0] = SzCoder::lzma(3 << 29);
    assert!(matches!(
        walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))),
        Err(ArchiveError::NeedsTooMuchMemory { .. })
    ));
}

#[test]
fn sevenz_total_size_over_cap() {
    let (packed, mut h) = two_files();
    let limits = Limits {
        max_total_size: 3000,
        ..Limits::default()
    };
    let bytes = sevenz_file(&packed, &sevenz_header_bytes(&h));
    assert!(matches!(
        walk_with(&bytes, &limits),
        Err(ArchiveError::TooLarge { .. })
    ));
    // One declared entry over max_entry_size (a compressor, since a Copy
    // coder declaring more than its packed stream is already a lie).
    let s = h.streams.as_mut().expect("streams");
    s.folders[0] = SzFolder::single(SzCoder::lzma2(16), 32 << 30);
    let bytes = sevenz_file(&packed, &sevenz_header_bytes(&h));
    assert!(matches!(
        walk_bytes(&bytes),
        Err(ArchiveError::TooLarge { .. })
    ));
}

#[test]
fn sevenz_reparse_point_not_extracted() {
    let (packed, mut h) = two_files();
    h.files[0] = SzFile::file("Game (Track 01).bin").attrib(0x400);
    // Unix extension: S_IFLNK in the high word, 0x8000 set.
    h.files[1] = SzFile::file("Game.cue").attrib((0o120_777 << 16) | 0x8000);
    let w = walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))).expect("valid");
    assert_eq!(w.files[0].entry.kind, EntryKind::Symlink);
    assert_eq!(w.files[1].entry.kind, EntryKind::Symlink);
    // A regular unix file stays a file.
    let (packed, mut h) = two_files();
    h.files[1] = SzFile::file("Game.cue").attrib((0o100_644 << 16) | 0x8000);
    let w = walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))).expect("valid");
    assert_eq!(w.files[1].entry.kind, EntryKind::File);
}

#[test]
fn sevenz_unpaired_surrogate_unsafe() {
    let (packed, mut h) = two_files();
    let mut units: Vec<u16> = "Game".encode_utf16().collect();
    units.push(0xD800);
    h.files[1] = SzFile::file("x").raw_name(&units);
    let w = walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))).expect("listing");
    assert_eq!(w.files[1].entry.unsafe_name, Some(UnsafeWhy::Encoding));
    assert_eq!(w.files[0].entry.unsafe_name, None);
}

#[test]
fn sevenz_dotdot_name_unsafe() {
    let (packed, mut h) = two_files();
    h.files[1] = SzFile::file("../evil.cue");
    let w = walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))).expect("listing");
    assert_eq!(w.files[1].entry.unsafe_name, Some(UnsafeWhy::DotDot));
}

#[test]
fn sevenz_main_aes_folder_encrypted() {
    let (packed, mut h) = sevenz_copy_parts(&[("a.bin", b"abc")]);
    let s = h.streams.as_mut().expect("streams");
    s.folders[0] = SzFolder::chain(vec![SzCoder::aes(), SzCoder::lzma2(16)], 3);
    assert!(matches!(
        walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))),
        Err(ArchiveError::Encrypted)
    ));
}

#[test]
fn sevenz_unknown_main_coder_listed() {
    // An unknown coder in a main folder is listed (decoding decides later).
    let (packed, mut h) = two_files();
    let s = h.streams.as_mut().expect("streams");
    s.folders[0].coders[0] = SzCoder::new(&[0x04, 0xF7, 0x11, 0x01], &[1, 5, 0]);
    let w = walk_bytes(&sevenz_file(&packed, &sevenz_header_bytes(&h))).expect("listing");
    assert!(matches!(w.files[0].entry.method, Method::Other(_)));
}

// ---------------------------------------------------------------------------
// S1: the walker never unwinds on garbled input
// ---------------------------------------------------------------------------

#[test]
fn garbled_sevenz_headers_never_panic() {
    let (packed, h) = two_files();
    let raw = sevenz_header_bytes(&h);
    let plain = sevenz_file(&packed, &raw);
    let encoded = sevenz_with_encoded_header(&packed, &raw);
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for base in [&plain, &encoded] {
        let header_at = base.len() - base.len().min(raw.len() + 40);
        for _ in 0..1500 {
            let mut bytes = base.clone();
            for _ in 0..1 + next() % 4 {
                let at = header_at + (next() as usize) % (bytes.len() - header_at);
                bytes[at] = next() as u8;
            }
            // Fix both CRCs so mutations reach the parser.
            let off = u64::from_le_bytes(bytes[12..20].try_into().expect("8")) as usize;
            let size = u64::from_le_bytes(bytes[20..28].try_into().expect("8")) as usize;
            if let Some(hdr) = bytes.get(32 + off..32 + off + size) {
                let crc = crc32fast::hash(hdr);
                bytes[28..32].copy_from_slice(&crc.to_le_bytes());
            }
            let crc = crc32fast::hash(&bytes[12..32]);
            bytes[8..12].copy_from_slice(&crc.to_le_bytes());
            let _ = walk_bytes(&bytes);
        }
    }
}

/// The three real 7z files named `.zip` (spec §1) walk with the shapes
/// `7zz l -slt` reports. Read-only; needs the owner's volume (plan AP-1).
#[test]
#[ignore = "needs the real download pile (COVERLY_SAMPLE_DIR)"]
fn sevenz_walks_real_sample_headers() {
    let dir = std::env::var("COVERLY_SAMPLE_DIR")
        .unwrap_or_else(|_| "/Volumes/TinyExt/ROMs-Downloaded".into());
    let dir = std::path::Path::new(&dir);
    if !dir.is_dir() {
        eprintln!("skipped: {} is not mounted", dir.display());
        return;
    }
    // (file, folders, files incl. the directory, sum of sizes)
    let cases = [
        ("Next Tetris, The.zip", 12, 13, 405_749_493u64),
        (
            "Next Tetris, The_ On-Line Edition.zip",
            1,
            17,
            1_191_067_563,
        ),
        ("Tetris Plus.zip", 1, 53, 591_881_790),
    ];
    for (name, folders, files, total) in cases {
        let path = dir.join(name);
        let mut f = std::fs::File::open(&path).expect("open read-only");
        let len = f.metadata().expect("metadata").len();
        let w = walk(&mut f, len, &Limits::default()).expect(name);
        assert_eq!(w.folders.len(), folders, "{name}");
        assert_eq!(w.files.len(), files, "{name}");
        let sum: u64 = w.files.iter().map(|f| f.entry.size).sum();
        assert_eq!(sum, total, "{name}");
        assert_eq!(w.header_end, len, "{name}");
    }
}

// ---------------------------------------------------------------------------
// Decoding over `sevenz-rust2` after the crate-view cross-check (plan Task 10)
// ---------------------------------------------------------------------------

mod decoding {
    use std::io::{self, Cursor, Read};
    use std::path::Path;

    use coverly_core::archive::sevenz::{testing_hooks, SevenZReader};
    use coverly_core::archive::testing::{
        sevenz, sevenz_copy, sevenz_file, sevenz_header_bytes, SevenZCoder, SevenZShape,
    };
    use coverly_core::archive::{
        ArchiveError, ArchiveReader, Budget, BudgetKind, Entry, EntryKind, Format, Limits, Method,
    };

    use super::{sample, two_files, MIB};

    type Files = Vec<(String, Vec<u8>)>;

    fn as_refs(files: &Files) -> Vec<(&str, &[u8])> {
        files
            .iter()
            .map(|(n, d)| (n.as_str(), d.as_slice()))
            .collect()
    }

    fn reader(bytes: &[u8]) -> SevenZReader<Cursor<Vec<u8>>> {
        SevenZReader::new(Cursor::new(bytes.to_vec()), &Limits::default()).expect("valid 7z")
    }

    fn index_of(r: &dyn ArchiveReader, name: &str) -> usize {
        r.entries()
            .iter()
            .position(|e| e.name.as_str() == name)
            .unwrap_or_else(|| panic!("no entry {name}"))
    }

    fn read_entry(r: &mut dyn ArchiveReader, index: usize) -> Result<Vec<u8>, ArchiveError> {
        let mut budget = Budget::apply();
        let mut stream = r.open_entry(index, &mut budget)?;
        let mut out = Vec::new();
        stream.read_to_end(&mut out)?;
        Ok(out)
    }

    fn stream_all(
        r: &mut dyn ArchiveReader,
        wanted: &[usize],
    ) -> Result<Vec<(usize, Vec<u8>)>, ArchiveError> {
        let mut got = Vec::new();
        let mut budget = Budget::apply();
        r.stream_entries(wanted, &mut budget, &mut |e: &Entry, s: &mut dyn Read| {
            let mut out = Vec::new();
            s.read_to_end(&mut out)?;
            got.push((e.index, out));
            Ok(())
        })?;
        Ok(got)
    }

    /// PS1 shape of spec §1: a directory, 11 tracks and a cue sheet.
    fn ps1_files() -> Files {
        let dir = "Next Tetris, The (USA)";
        let mut files: Files = vec![(format!("{dir}/"), Vec::new())];
        for n in 1..=11u8 {
            files.push((
                format!("{dir}/Next Tetris, The (USA) (Track {n:02}).bin"),
                sample(2352 * 4 + usize::from(n) * 17),
            ));
        }
        files.push((
            format!("{dir}/Next Tetris, The (USA).cue"),
            b"FILE \"Next Tetris, The (USA) (Track 01).bin\" BINARY\r\n".to_vec(),
        ));
        files
    }

    /// Dreamcast shape of spec §1: cue + gdi + 14 tracks, sheets stored last.
    fn dreamcast_files() -> Files {
        let mut files: Files = vec![
            (
                "Game.cue".into(),
                b"FILE \"Game (Track 01).bin\" BINARY".to_vec(),
            ),
            (
                "Game.gdi".into(),
                b"14\n1 0 4 2352 \"Game (Track 01).bin\" 0".to_vec(),
            ),
        ];
        for n in 1..=14u8 {
            files.push((
                format!("Game (Track {n:02}).bin"),
                sample(3000 + usize::from(n) * 101),
            ));
        }
        files
    }

    #[test]
    fn sevenz_valid_real_shaped_archives_list() {
        // PS1: LZMA, one folder per track, 12 folders.
        let ps1 = ps1_files();
        let bytes = sevenz(&as_refs(&ps1), SevenZShape::non_solid_lzma());
        let mut r = reader(&bytes);
        assert_eq!(r.format(), Format::SevenZip);
        assert_eq!(r.entries().len(), 13);
        assert_eq!(r.entries()[0].kind, EntryKind::Dir);
        let blocks: std::collections::BTreeSet<_> =
            r.entries().iter().filter_map(|e| e.solid_block).collect();
        assert_eq!(blocks.len(), 12, "one folder per file with data");
        for (name, data) in ps1.iter().skip(1) {
            let e = r
                .entries()
                .iter()
                .find(|e| e.name.as_str() == name)
                .expect(name);
            assert_eq!(e.size, data.len() as u64, "{name}");
            assert_eq!(e.crc32, Some(crc32fast::hash(data)), "{name}");
            assert_eq!(e.method, Method::Lzma { dict: 1 << 20 });
        }
        let cue = index_of(&r, "Next Tetris, The (USA)/Next Tetris, The (USA).cue");
        assert_eq!(read_entry(&mut r, cue).expect("cue"), ps1[12].1);
        let wanted: Vec<usize> = (1..13).collect();
        let got = stream_all(&mut r, &wanted).expect("stream all");
        assert_eq!(got.len(), 12);
        for (index, data) in got {
            let name = r.entries()[index].name.as_str().to_owned();
            let want = &ps1.iter().find(|(n, _)| *n == name).expect("known").1;
            assert_eq!(&data, want, "{name}");
        }

        // Dreamcast: LZMA2, one solid folder, sheets last.
        let dc = dreamcast_files();
        let bytes = sevenz(&as_refs(&dc), SevenZShape::solid_lzma2_sheets_last());
        let mut r = reader(&bytes);
        assert_eq!(r.entries().len(), 16);
        assert!(r.entries().iter().all(|e| e.solid_block == Some(0)));
        let names: Vec<&str> = r.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            &names[14..],
            &["Game.cue", "Game.gdi"],
            "sheets stored last"
        );
        assert_eq!(r.entries()[0].method, Method::Lzma2 { dict: 1 << 20 });
        let t3 = index_of(&r, "Game (Track 03).bin");
        assert_eq!(read_entry(&mut r, t3).expect("track 03"), dc[4].1);
        let gdi = index_of(&r, "Game.gdi");
        assert_eq!(read_entry(&mut r, gdi).expect("gdi"), dc[1].1);
        let got = stream_all(&mut r, &[gdi, t3]).expect("stream two");
        assert_eq!(got.iter().map(|g| g.0).collect::<Vec<_>>(), vec![t3, gdi]);
        assert_eq!(got[0].1, dc[4].1);
        assert_eq!(got[1].1, dc[1].1);
    }

    #[test]
    fn sevenz_open_entry_head_read_needs_no_full_decode() {
        // A head read of a big entry stops early: only what is read (plus
        // the decoder's own read-ahead) is charged.
        let big = sample(8 * MIB as usize);
        let bytes = sevenz(
            &[("Game (Track 01).bin", &big)],
            SevenZShape::non_solid_lzma(),
        );
        let mut r = reader(&bytes);
        let mut budget = Budget::plan_head(&Limits {
            plan_head_budget: MIB,
            ..Limits::default()
        });
        let mut head = vec![0u8; 64 * 1024];
        r.open_entry(0, &mut budget)
            .expect("open")
            .read_exact(&mut head)
            .expect("head");
        assert_eq!(head, big[..64 * 1024]);
        assert!(budget.spent() < MIB, "spent {}", budget.spent());
    }

    #[test]
    fn sevenz_solid_open_entry_charges_prefix() {
        let mut files: Files = (1..=4u8)
            .map(|n| (format!("Game (Track {n:02}).bin"), sample(2 * MIB as usize)))
            .collect();
        files.push((
            "Game.cue".into(),
            b"FILE \"Game (Track 01).bin\" BINARY".to_vec(),
        ));
        let bytes = sevenz(&as_refs(&files), SevenZShape::solid_lzma2_sheets_last());
        let mut r = reader(&bytes);
        let cue = index_of(&r, "Game.cue");
        assert_eq!(r.entries()[cue].solid_block, Some(0));
        let limits = Limits {
            plan_head_budget: 4 * MIB,
            ..Limits::default()
        };
        let mut budget = Budget::plan_head(&limits);
        // The prefix is decoded while the entry is opened.
        let Err(err) = r.open_entry(cue, &mut budget).map(|_| ()) else {
            panic!("8 MiB prefix read with a 4 MiB budget");
        };
        assert!(
            matches!(
                err,
                ArchiveError::BudgetSpent {
                    what: BudgetKind::DecodedBytes
                }
            ),
            "{err:?}"
        );
        assert!(budget.spent() >= 4 * MIB);
        // With the whole prefix allowed, the same entry reads.
        assert_eq!(read_entry(&mut r, cue).expect("cue"), files[4].1);
    }

    #[test]
    fn sevenz_stream_entries_one_pass_per_folder() {
        use sevenz_rust2::encoder_options::Lzma2Options;
        use sevenz_rust2::{ArchiveEntry, ArchiveWriter, SourceReader};
        // Two solid folders of three files each.
        let groups: Vec<Files> = (0..2)
            .map(|g| {
                (0..3)
                    .map(|i| (format!("g{g}/f{i}.bin"), sample(4000 + g * 10 + i)))
                    .collect()
            })
            .collect();
        let mut w = ArchiveWriter::new(Cursor::new(Vec::new())).expect("writer");
        let mut opts = Lzma2Options::from_level(5);
        opts.set_dictionary_size(1 << 20);
        w.set_content_methods(vec![opts.into()]);
        for g in &groups {
            let entries = g.iter().map(|(n, _)| ArchiveEntry::new_file(n)).collect();
            let readers = g
                .iter()
                .map(|(_, d)| SourceReader::new(Cursor::new(d.clone())))
                .collect();
            w.push_archive_entries(entries, readers).expect("push");
        }
        let bytes = w.finish().expect("finish").into_inner();
        let mut r = reader(&bytes);
        assert_eq!(r.entries().len(), 6);
        let before = testing_hooks::folder_passes();
        // Every file, asked for in reverse: one pass per folder, in storage order.
        let got = stream_all(&mut r, &[5, 4, 3, 2, 1, 0]).expect("stream");
        assert_eq!(testing_hooks::folder_passes() - before, 2);
        assert_eq!(
            got.iter().map(|g| g.0).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4, 5]
        );
        for (index, data) in &got {
            assert_eq!(data, &groups[index / 3][index % 3].1);
        }
        // One file of the second folder: one pass, the unwanted files
        // before it decoded to nowhere.
        let before = testing_hooks::folder_passes();
        let got = stream_all(&mut r, &[4]).expect("stream one");
        assert_eq!(testing_hooks::folder_passes() - before, 1);
        assert_eq!(got, vec![(4, groups[1][1].1.clone())]);
    }

    #[test]
    fn sevenz_stream_entries_sink_may_stop_early() {
        // A sink that reads only part of an entry: the rest is drained and
        // checked, the next entry still lines up.
        let files: Files = (0..3)
            .map(|i| (format!("f{i}.bin"), sample(10_000 + i)))
            .collect();
        let bytes = sevenz(&as_refs(&files), SevenZShape::solid_lzma2_sheets_last());
        let mut r = reader(&bytes);
        let mut got = Vec::new();
        let mut budget = Budget::apply();
        r.stream_entries(&[0, 1, 2], &mut budget, &mut |e, s| {
            let mut head = [0u8; 16];
            s.read_exact(&mut head)?;
            got.push((e.index, head.to_vec()));
            Ok(())
        })
        .expect("stream");
        for (index, head) in got {
            assert_eq!(head, files[index].1[..16]);
        }
    }

    #[test]
    fn sevenz_stream_entries_sink_error_is_returned() {
        let files: Files = (0..2)
            .map(|i| (format!("f{i}.bin"), sample(1000)))
            .collect();
        let bytes = sevenz(&as_refs(&files), SevenZShape::solid_lzma2_sheets_last());
        let mut r = reader(&bytes);
        let mut budget = Budget::apply();
        let err = r
            .stream_entries(&[0, 1], &mut budget, &mut |_, _| {
                Err(ArchiveError::Cancelled)
            })
            .expect_err("sink failed");
        assert!(matches!(err, ArchiveError::Cancelled), "{err:?}");
    }

    #[test]
    fn sevenz_bcj_and_ppmd_decode() {
        let files: Files = vec![
            ("a.bin".into(), sample(50_000)),
            (
                "b.bin".into(),
                b"call \xE8\x00\x00\x00\x00 jump".repeat(500),
            ),
            ("c.txt".into(), b"FILE \"a.bin\" BINARY".to_vec()),
        ];
        for coder in [
            SevenZCoder::BcjLzma2 { dict: 1 << 20 },
            SevenZCoder::Ppmd {
                order: 6,
                mem: 16 << 20,
            },
            SevenZCoder::Copy,
            SevenZCoder::Lzma { dict: 1 << 16 },
        ] {
            for solid in [false, true] {
                let shape = SevenZShape {
                    solid,
                    coder,
                    sheets_last: false,
                };
                let bytes = sevenz(&as_refs(&files), shape);
                let mut r = reader(&bytes);
                for (i, (_, data)) in files.iter().enumerate() {
                    assert_eq!(&read_entry(&mut r, i).expect("open"), data, "{shape:?} {i}");
                }
                let got = stream_all(&mut r, &[0, 1, 2]).expect("stream");
                for (i, data) in got {
                    assert_eq!(data, files[i].1, "{shape:?} stream {i}");
                }
            }
        }
    }

    #[test]
    fn sevenz_crc_mismatch_damaged() {
        let a = sample(5000);
        let mut bytes = sevenz_copy(&[("a.bin", &a), ("b.bin", b"hello")]);
        // Copy coder: the data sits right after the start header.
        bytes[32 + 100] ^= 0x55;
        let mut r = reader(&bytes);
        let err = read_entry(&mut r, 0).expect_err("crc");
        assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
        let err = stream_all(&mut r, &[0, 1]).expect_err("crc");
        assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
        // The other file is fine.
        assert_eq!(read_entry(&mut r, 1).expect("b"), b"hello");

        // A compressed folder with a flipped payload byte.
        let files: Files = vec![("t.bin".into(), sample(40_000))];
        let mut bytes = sevenz(&as_refs(&files), SevenZShape::solid_lzma2_sheets_last());
        bytes[32 + 200] ^= 0x01;
        let mut r = reader(&bytes);
        let err = read_entry(&mut r, 0).expect_err("open_entry");
        assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
        let err = stream_all(&mut r, &[0]).expect_err("stream");
        assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
    }

    #[test]
    fn sevenz_declared_files_mismatch_names_crate_never_called() {
        let (packed, mut h) = two_files();
        h.num_files = Some(1_000_000);
        let bytes = sevenz_file(&packed, &sevenz_header_bytes(&h));
        let before = testing_hooks::crate_opened();
        let err = SevenZReader::new(Cursor::new(bytes), &Limits::default())
            .err()
            .expect("refused");
        assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
        assert_eq!(testing_hooks::crate_opened(), before, "crate never called");
    }

    #[test]
    fn sevenz_listing_never_opens_the_crate() {
        let bytes = sevenz_copy(&[("a.bin", b"abc")]);
        let before = testing_hooks::crate_opened();
        let mut r = reader(&bytes);
        assert_eq!(r.entries().len(), 1);
        assert_eq!(testing_hooks::crate_opened(), before, "listing is the walk");
        assert_eq!(read_entry(&mut r, 0).expect("a"), b"abc");
        assert_eq!(read_entry(&mut r, 0).expect("again"), b"abc");
        assert_eq!(testing_hooks::crate_opened(), before + 1, "opened once");
    }

    fn write_file(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).expect("write");
    }

    #[test]
    fn sevenz_crate_view_differs_damaged() {
        // Same length, same mtime, another name: only the crate's own view
        // of the file can tell (spec S5, R17).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("game.zip");
        let first = sevenz_copy(&[("Game (Track 01).bin", &sample(4000))]);
        let second = sevenz_copy(&[("Game (Track 02).bin", &sample(4000))]);
        assert_eq!(first.len(), second.len());
        write_file(&path, &first);
        let mut r = SevenZReader::open(&path, &Limits::default()).expect("valid 7z");
        let mtime = filetime::FileTime::from_last_modification_time(
            &std::fs::metadata(&path).expect("metadata"),
        );
        let swap = path.clone();
        testing_hooks::before_crate_open(move || {
            write_file(&swap, &second);
            filetime::set_file_mtime(&swap, mtime).expect("keep mtime");
        });
        let before = testing_hooks::crate_opened();
        let err = read_entry(&mut r, 0).expect_err("crate view differs");
        assert!(
            matches!(&err, ArchiveError::Damaged { detail } if detail.contains("crate view differs")),
            "{err:?}"
        );
        assert_eq!(testing_hooks::crate_opened(), before + 1);
    }

    #[test]
    fn sevenz_file_changed_after_listing_damaged() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("game.7z");
        write_file(&path, &sevenz_copy(&[("a.bin", &sample(4000))]));
        let mut r = SevenZReader::open(&path, &Limits::default()).expect("valid 7z");
        // A different size after the walk: refused before the crate runs.
        write_file(&path, &sevenz_copy(&[("a.bin", &sample(5000))]));
        let before = testing_hooks::crate_opened();
        let err = read_entry(&mut r, 0).expect_err("changed");
        assert!(
            matches!(&err, ArchiveError::Damaged { detail } if detail.contains("changed since")),
            "{err:?}"
        );
        assert_eq!(testing_hooks::crate_opened(), before);
    }

    #[test]
    fn sevenz_dirs_and_empty_files() {
        let bytes = sevenz(
            &[("d/", b""), ("d/a.bin", b"data"), ("d/empty.bin", b"")],
            SevenZShape::solid_lzma2_sheets_last(),
        );
        let mut r = reader(&bytes);
        let dir = index_of(&r, "d");
        let empty = index_of(&r, "d/empty.bin");
        let a = index_of(&r, "d/a.bin");
        assert_eq!(r.entries()[dir].kind, EntryKind::Dir);
        assert_eq!(read_entry(&mut r, empty).expect("empty"), b"");
        let mut budget = Budget::apply();
        assert!(
            r.open_entry(dir, &mut budget).is_err(),
            "a directory is no stream"
        );
        let got = stream_all(&mut r, &[empty, a]).expect("stream");
        let mut got = got;
        got.sort();
        let mut want = vec![(a, b"data".to_vec()), (empty, Vec::new())];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn sevenz_unknown_coder_unsupported_needs_7zip() {
        use coverly_core::archive::testing::{
            SzCoder, SzFile, SzFolder, SzHeader, SzStreams, SzSubstreams,
        };
        use coverly_core::archive::Tool;
        // A zstd coder (sevenz-rust2 built without it): listed, not decoded.
        let data = b"zstd-ish payload".to_vec();
        let header = SzHeader {
            streams: Some(SzStreams {
                pack_sizes: vec![data.len() as u64],
                folders: vec![SzFolder::single(
                    SzCoder::new(&[0x04, 0xF7, 0x11, 0x01], &[]),
                    data.len() as u64,
                )],
                substreams: Some(SzSubstreams {
                    crcs: Some(vec![crc32fast::hash(&data)]),
                    ..SzSubstreams::default()
                }),
                ..SzStreams::default()
            }),
            files: vec![SzFile::file("game.gba")],
            ..SzHeader::default()
        };
        let bytes = sevenz_file(&data, &sevenz_header_bytes(&header));
        let mut r = reader(&bytes);
        assert_eq!(r.entries()[0].name.as_str(), "game.gba");
        assert!(r.unsupported_coder().is_some());
        for err in [
            read_entry(&mut r, 0).expect_err("open"),
            stream_all(&mut r, &[0]).expect_err("stream"),
        ] {
            assert!(
                matches!(
                    &err,
                    ArchiveError::Unsupported {
                        need: Some(Tool::SevenZip),
                        format,
                    } if format.starts_with("7z (")
                ),
                "{err:?}"
            );
        }
    }

    #[test]
    fn sevenz_garbled_payload_never_panics() {
        // S1: flipped bytes all over a compressed archive end in an error
        // or in data, never in a panic out of the reader.
        let files: Files = vec![
            ("t.bin".into(), sample(30_000)),
            ("u.cue".into(), b"x".to_vec()),
        ];
        let bytes = sevenz(&as_refs(&files), SevenZShape::solid_lzma2_sheets_last());
        for step in [7usize, 13, 101] {
            let mut bad = bytes.clone();
            let mut i = 33;
            while i < bad.len() {
                bad[i] ^= 0xA5;
                i += step * 97;
            }
            if let Ok(mut r) = SevenZReader::new(Cursor::new(bad), &Limits::default()) {
                let _ = read_entry(&mut r, 0);
                let _ = stream_all(&mut r, &[0, 1]);
            }
        }
    }

    #[test]
    fn sevenz_wrong_index_is_an_error() {
        let bytes = sevenz_copy(&[("a.bin", b"abc")]);
        let mut r = reader(&bytes);
        let mut budget = Budget::apply();
        assert!(r.open_entry(9, &mut budget).is_err());
        let err = r
            .stream_entries(&[9], &mut budget, &mut |_, s| {
                io::copy(s, &mut io::sink())?;
                Ok(())
            })
            .expect_err("no entry 9");
        assert!(matches!(err, ArchiveError::Io(_)), "{err:?}");
    }

    /// The three real 7z files named `.zip` (spec §1): the head of the
    /// first track decodes within the plan head budget. Read-only.
    #[test]
    #[ignore = "needs the real download pile (COVERLY_SAMPLE_DIR)"]
    fn sevenz_reads_real_sample_heads() {
        let dir = std::env::var("COVERLY_SAMPLE_DIR")
            .unwrap_or_else(|_| "/Volumes/TinyExt/ROMs-Downloaded".into());
        let dir = Path::new(&dir);
        if !dir.is_dir() {
            eprintln!("skipped: {} is not mounted", dir.display());
            return;
        }
        let limits = Limits::default();
        for (name, magic) in [
            ("Next Tetris, The.zip", &b"PLAYSTATION"[..]),
            ("Next Tetris, The_ On-Line Edition.zip", &b""[..]),
            ("Tetris Plus.zip", &b"SEGA SEGASATURN"[..]),
        ] {
            let mut r = SevenZReader::open(&dir.join(name), &limits).expect(name);
            let first = r
                .entries()
                .iter()
                .position(|e| e.kind == EntryKind::File && e.solid_block.is_some())
                .expect("a track");
            let mut budget = Budget::plan_head(&limits);
            let mut head = vec![0u8; 64 * 1024];
            r.open_entry(first, &mut budget)
                .expect(name)
                .read_exact(&mut head)
                .expect(name);
            assert!(head.iter().any(|&b| b != 0), "{name}");
            assert!(
                head.windows(magic.len().max(1))
                    .any(|w| magic.is_empty() || w == magic),
                "{name}: no {magic:?} in the first track's head"
            );
        }
    }
}
