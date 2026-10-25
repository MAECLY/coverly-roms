//! Zip backend of the archive reader (spec §4, §5.1, S1, S2, S3, §13.1).
//! Fixtures are synthetic.

use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use coverly_core::archive::budget::with_gate_cap;
use coverly_core::archive::lzma::last_decoder_dict;
use coverly_core::archive::testing::{
    lzma_props, xz_stream, zip_crafted, zip_crafted_with, zip_lzma_payload, zip_ppmd_header,
    zip_raw_entry, zip_with, XzBlock, ZipEnd, ZipMethod, ZipSpec,
};
use coverly_core::archive::{
    file_entries, ArchiveError, ArchiveReader, Budget, EntryKind, Limits, MemoryGate, Method,
    UnsafeWhy, ZipReader, DECODER_MEM_CAP,
};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
/// 1.5 GiB, the S3 example.
const HUGE_DICT: u32 = 3 << 29;

fn sample(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i * 7 % 251) as u8 ^ (i / 300) as u8)
        .collect()
}

fn list(bytes: Vec<u8>) -> Result<ZipReader<Cursor<Vec<u8>>>, ArchiveError> {
    ZipReader::new(Cursor::new(bytes), &Limits::default())
}

fn list_with(bytes: Vec<u8>, limits: &Limits) -> Result<ZipReader<Cursor<Vec<u8>>>, ArchiveError> {
    ZipReader::new(Cursor::new(bytes), limits)
}

fn read_entry(r: &mut dyn ArchiveReader, index: usize) -> Result<Vec<u8>, ArchiveError> {
    let mut budget = Budget::apply();
    let mut stream = r.open_entry(index, &mut budget)?;
    let mut out = Vec::new();
    stream.read_to_end(&mut out)?;
    Ok(out)
}

fn lzma_encode(data: &[u8], dict: u32) -> ([u8; 5], Vec<u8>) {
    let mut options = lzma_rust2::LzmaOptions::with_preset(1);
    options.dict_size = dict;
    let mut w = lzma_rust2::LzmaWriter::new_no_header(Vec::new(), &options, false).unwrap();
    w.write_all(data).unwrap();
    let raw = w.finish().unwrap();
    let props = lzma_props(options.lc as u8, options.lp as u8, options.pb as u8, dict);
    (props, raw)
}

fn xz_encode(data: &[u8]) -> Vec<u8> {
    let mut options = lzma_rust2::XzOptions::with_preset(1);
    options.lzma_options.dict_size = 1 << 20;
    let mut w = lzma_rust2::XzWriter::new(Vec::new(), options).unwrap();
    w.write_all(data).unwrap();
    w.finish().unwrap()
}

fn ppmd_encode(data: &[u8], order: u8, mem_mib: u16) -> Vec<u8> {
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

fn zstd_encode(data: &[u8]) -> Vec<u8> {
    ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Fastest)
}

fn raw(method: u16, payload: Vec<u8>) -> ZipMethod {
    ZipMethod::Raw { method, payload }
}

/// The `zip` crate is built without its `lzma`, `xz`, `ppmd` and `zstd`
/// features, so it structurally cannot decode those methods: `archive` reads
/// them raw and decodes them with capped decoders (S2, S3). If this test
/// starts failing, a crate in the graph turned one of those features on
/// (`cargo tree -e features -i zip`, `scripts/check-zip-features.sh`).
#[test]
fn zip_crate_cannot_decode_lzma() {
    let payload = b"not really lzma, never decoded".to_vec();
    let bytes = zip_raw_entry(14, &payload);
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("valid central directory");
    assert_eq!(archive.len(), 1);

    match archive.by_index(0) {
        Ok(_) => panic!("zip crate decodes LZMA: a forbidden zip feature is enabled"),
        Err(err) => {
            let text = err.to_string().to_ascii_lowercase();
            assert!(
                text.contains("compression")
                    && (text.contains("unsupported") || text.contains("not supported")),
                "unexpected error: {err}"
            );
        }
    }

    let mut raw = archive
        .by_index_raw(0)
        .expect("raw read of a method-14 entry");
    let mut out = Vec::new();
    raw.read_to_end(&mut out).unwrap();
    assert_eq!(out, payload);
}

#[test]
fn zip_lists_stored_deflate_deflate64_bzip2() {
    let data = sample(50_000);
    let bytes = zip_with(&[
        ("Game (USA)/a.gba", &data, ZipMethod::Stored),
        ("Game (USA)/b.gba", &data, ZipMethod::Deflate),
        ("c.bin", &data, ZipMethod::Deflate64),
        ("d.bin", &data, ZipMethod::Bzip2),
    ]);
    let mut r = list(bytes).expect("listed");
    let entries = r.entries().to_vec();
    assert_eq!(entries.len(), 4);
    let methods: Vec<Method> = entries.iter().map(|e| e.method.clone()).collect();
    assert_eq!(
        methods,
        vec![
            Method::Stored,
            Method::Deflate,
            Method::Deflate64,
            Method::Bzip2
        ]
    );
    assert_eq!(entries[0].name.as_str(), "Game (USA)/a.gba");
    for e in &entries {
        assert_eq!(e.kind, EntryKind::File);
        assert_eq!(e.size, data.len() as u64);
        assert_eq!(e.crc32, Some(crc32fast::hash(&data)));
        assert!(!e.encrypted);
        assert_eq!(e.unsafe_name, None);
        assert!(e.mtime.is_some(), "DOS time read");
    }
    for i in 0..4 {
        assert_eq!(read_entry(&mut r, i).expect("decoded"), data, "entry {i}");
    }
}

#[test]
fn zip64_large_declared_sizes_listed() {
    let five_gib = 5 * GIB;
    let spec = ZipSpec::new("Disc.iso", &sample(1000), ZipMethod::Deflate)
        .zip64()
        .declared_size(five_gib);
    let r = list(zip_crafted(&[spec])).expect("zip64 extra read");
    assert_eq!(r.entries()[0].size, five_gib);
    assert!(r.entries()[0].packed.unwrap() < 1000);

    // The zip64 end record path (count 0xFFFF in the end record).
    let bytes = zip_crafted_with(
        &[ZipSpec::new("a.gba", b"abc", ZipMethod::Stored)],
        ZipEnd {
            zip64: true,
            ..ZipEnd::default()
        },
    );
    let mut r = list(bytes).expect("zip64 end record read");
    assert_eq!(read_entry(&mut r, 0).unwrap(), b"abc");
}

#[test]
fn zip_declared_size_over_cap_too_large() {
    let spec = ZipSpec::new("big.iso", &sample(10), ZipMethod::Deflate)
        .zip64()
        .declared_size(20 * GIB);
    assert!(matches!(
        list(zip_crafted(&[spec])),
        Err(ArchiveError::TooLarge { declared }) if declared == 20 * GIB
    ));
}

#[test]
fn zip_reads_lzma_xz_ppmd_zstd_entries() {
    let data = sample(300_000);
    let (props, lzma) = lzma_encode(&data, 1 << 20);
    let bytes = zip_crafted(&[
        ZipSpec::new("a.bin", &data, raw(14, zip_lzma_payload(props, &lzma))),
        ZipSpec::new("b.bin", &data, raw(95, xz_encode(&data))),
        ZipSpec::new("c.bin", &data, raw(98, ppmd_encode(&data, 6, 16))),
        ZipSpec::new("d.bin", &data, raw(93, zstd_encode(&data))),
    ]);
    let mut r = list(bytes).expect("listed");
    let methods: Vec<Method> = r.entries().iter().map(|e| e.method.clone()).collect();
    assert_eq!(methods[0], Method::Lzma { dict: 1 << 20 });
    assert_eq!(methods[1], Method::Xz { dict: 1 << 20 });
    assert_eq!(
        methods[2],
        Method::Ppmd {
            order: 6,
            mem: 16 << 20
        }
    );
    assert!(
        matches!(methods[3], Method::Zstd { window } if window > 0 && window <= 64 * MIB),
        "{:?}",
        methods[3]
    );
    with_gate_cap(512 * MIB, || {
        for i in 0..4 {
            assert_eq!(read_entry(&mut r, i).expect("decoded"), data, "entry {i}");
        }
        assert_eq!(MemoryGate::global().in_use(), 0, "permits released");
    });
}

#[test]
fn zip_lzma_huge_dict_refused_at_list_and_open() {
    let payload = zip_lzma_payload(lzma_props(3, 0, 2, HUGE_DICT), &[0u8; 64]);
    let bytes = zip_crafted(&[ZipSpec::new("game.bin", &[0u8; 1024], raw(14, payload))]);

    // List time: the property header is read and the dictionary checked.
    let err = list(bytes.clone()).err().expect("refused at list");
    assert!(
        matches!(err, ArchiveError::NeedsTooMuchMemory { needed, cap }
            if needed > u64::from(HUGE_DICT) && cap == DECODER_MEM_CAP),
        "{err:?}"
    );

    // Open time, past a lenient listing: the memory gate still refuses it
    // and no decoder is ever built.
    let lenient = Limits {
        decoder_mem_cap: 4 * GIB,
        ..Limits::default()
    };
    let mut r = list_with(bytes, &lenient).expect("lenient listing");
    let _ = last_decoder_dict();
    let err = with_gate_cap(512 * MIB, || read_entry(&mut r, 0)).expect_err("refused at open");
    assert!(
        matches!(err, ArchiveError::NeedsTooMuchMemory { .. }),
        "{err:?}"
    );
    assert_eq!(last_decoder_dict(), None, "no decoder built");
}

#[test]
fn zip_ppmd_mem_255_refused_with_lower_cap() {
    // Memory field 255 = 256 MiB, PPMd8's largest model.
    let mut payload = zip_ppmd_header(6, 256, 0).to_vec();
    payload.extend_from_slice(&[0u8; 32]);
    let bytes = zip_crafted(&[ZipSpec::new("game.bin", &[0u8; 100], raw(98, payload))]);

    let r = list(bytes.clone()).expect("256 MiB fits the default cap");
    assert_eq!(
        r.entries()[0].method,
        Method::Ppmd {
            order: 6,
            mem: 256 << 20
        }
    );

    let lower = Limits {
        decoder_mem_cap: 128 * MIB,
        ..Limits::default()
    };
    let err = list_with(bytes, &lower).err().expect("refused");
    assert!(
        matches!(err, ArchiveError::NeedsTooMuchMemory { needed, cap }
            if needed == 256 * MIB && cap == 128 * MIB),
        "{err:?}"
    );
}

#[test]
fn zip_xz_second_block_huge_dict_refused() {
    let first = sample(1000);
    let second = sample(10);
    let stream = xz_stream(&[
        XzBlock {
            data: first.clone(),
            dict_prop: 16,
        },
        XzBlock {
            data: second.clone(),
            dict_prop: 37, // 1.5 GiB
        },
    ]);
    let mut all = first.clone();
    all.extend_from_slice(&second);
    let bytes = zip_crafted(&[ZipSpec::new("game.bin", &all, raw(95, stream))]);
    let mut r = list(bytes).expect("the first block is small");
    assert_eq!(r.entries()[0].method, Method::Xz { dict: 1 << 20 });

    let mut budget = Budget::apply();
    let mut stream = r.open_entry(0, &mut budget).expect("opened");
    let mut out = Vec::new();
    let err = ArchiveError::from(
        stream
            .read_to_end(&mut out)
            .expect_err("second block refused"),
    );
    assert!(
        matches!(err, ArchiveError::NeedsTooMuchMemory { .. }),
        "{err:?}"
    );
    assert!(out.len() <= first.len());
}

#[test]
fn zip_zstd_window_over_cap_refused() {
    let mut frame = 0xFD2F_B528u32.to_le_bytes().to_vec();
    frame.push(0x00); // not single segment, no content size
    frame.push(20 << 3); // window 2^30
    frame.extend_from_slice(&[0u8; 32]);
    let bytes = zip_crafted(&[ZipSpec::new("game.bin", &[0u8; 100], raw(93, frame))]);
    let err = list(bytes).err().expect("refused at list");
    assert!(
        matches!(err, ArchiveError::NeedsTooMuchMemory { needed, .. } if needed > GIB),
        "{err:?}"
    );
}

#[test]
fn entry_changed_between_list_and_open_rechecked() {
    let data = sample(20_000);
    let (props, lzma) = lzma_encode(&data, 1 << 16);
    let name = "game.bin";
    let bytes = zip_crafted(&[ZipSpec::new(
        name,
        &data,
        raw(14, zip_lzma_payload(props, &lzma)),
    )]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.zip");
    std::fs::write(&path, &bytes).unwrap();

    let mut r = ZipReader::open(&path, &Limits::default()).expect("small dictionary listed");
    assert_eq!(r.entries()[0].method, Method::Lzma { dict: 1 << 16 });

    // Rewrite the dictionary in place: local header (30 + name), then the
    // 4-byte version/size prefix and the props byte.
    let dict_at = 30 + name.len() as u64 + 4 + 1;
    let mut f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    f.seek(SeekFrom::Start(dict_at)).unwrap();
    f.write_all(&HUGE_DICT.to_le_bytes()).unwrap();
    f.sync_all().unwrap();
    drop(f);

    let _ = last_decoder_dict();
    let err = read_entry(&mut r, 0).expect_err("rechecked at open");
    assert!(
        matches!(err, ArchiveError::NeedsTooMuchMemory { .. }),
        "{err:?}"
    );
    assert_eq!(last_decoder_dict(), None, "no decoder built");
}

#[test]
fn zip_lzma_decoder_gets_the_checked_dictionary() {
    let data = sample(5_000);
    let (props, lzma) = lzma_encode(&data, 1 << 18);
    let bytes =
        zip_crafted(&[
            ZipSpec::new("a.bin", &data, raw(14, zip_lzma_payload(props, &lzma)))
                .declared_size(data.len() as u64),
        ]);
    let mut r = list(bytes).unwrap();
    let _ = last_decoder_dict();
    assert_eq!(read_entry(&mut r, 0).unwrap(), data);
    assert_eq!(last_decoder_dict(), Some(1 << 18));
}

#[test]
fn zip_claims_one_million_entries() {
    let bytes = zip_crafted_with(
        &[ZipSpec::new("a.gba", b"x", ZipMethod::Stored)],
        ZipEnd {
            claimed_entries: Some(1_000_000),
            ..ZipEnd::default()
        },
    );
    assert!(matches!(
        list(bytes),
        Err(ArchiveError::TooManyEntries { count: 1_000_000 })
    ));
}

#[test]
fn zip_symlink_entry_not_extracted() {
    let bytes = zip_crafted(&[
        ZipSpec::new("link", b"/etc/passwd", ZipMethod::Stored).symlink(),
        ZipSpec::new("game.gba", b"rom", ZipMethod::Stored).unix_mode(0o100_644),
        ZipSpec::new("dir/", b"", ZipMethod::Stored),
    ]);
    let mut r = list(bytes).unwrap();
    let kinds: Vec<EntryKind> = r.entries().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![EntryKind::Symlink, EntryKind::File, EntryKind::Dir]
    );
    let files: Vec<usize> = file_entries(&r).map(|e| e.index).collect();
    assert_eq!(files, vec![1]);
    assert!(matches!(
        read_entry(&mut r, 0),
        Err(ArchiveError::UnsafeEntry {
            why: UnsafeWhy::Link,
            ..
        })
    ));
    assert_eq!(read_entry(&mut r, 1).unwrap(), b"rom");
}

#[test]
fn zip_aes_entry_note() {
    let rom = sample(500);
    let bytes = zip_crafted(&[
        ZipSpec::new("readme.txt", b"hello", ZipMethod::Stored),
        ZipSpec::new("game.gba", &rom, ZipMethod::Stored).aes(0),
    ]);
    let mut r = list(bytes).expect("one plain entry keeps the listing");
    let aes = &r.entries()[1];
    assert!(aes.encrypted);
    assert_eq!(aes.crc32, Some(crc32fast::hash(&rom)), "AE-1 keeps the CRC");
    assert_eq!(
        aes.method,
        Method::Stored,
        "the method inside the AES extra"
    );
    assert!(!r.entries()[0].encrypted);
    assert!(matches!(
        read_entry(&mut r, 1),
        Err(ArchiveError::Encrypted)
    ));
    assert_eq!(read_entry(&mut r, 0).unwrap(), b"hello");

    let only = zip_crafted(&[ZipSpec::new("game.gba", &rom, ZipMethod::Deflate).aes(8)]);
    assert!(matches!(list(only), Err(ArchiveError::Encrypted)));
}

#[test]
fn zip_entry_inflates_past_declared_size() {
    let data = sample(10_000);
    let spec = ZipSpec::new("game.gba", &data, ZipMethod::Deflate)
        .declared_size(100)
        .declared_crc(crc32fast::hash(&data[..100]));
    let mut r = list(zip_crafted(&[spec])).unwrap();
    let err = read_entry(&mut r, 0).expect_err("overrun");
    assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
}

#[test]
fn zip_crc_mismatch_damaged() {
    let spec = ZipSpec::new("game.gba", b"payload", ZipMethod::Stored).declared_crc(0xDEAD_BEEF);
    let mut r = list(zip_crafted(&[spec])).unwrap();
    let err = read_entry(&mut r, 0).expect_err("crc");
    assert!(
        matches!(&err, ArchiveError::Damaged { detail } if detail == "crc"),
        "{err:?}"
    );
}

#[test]
fn zip_cp437_name_decoded() {
    let bytes = zip_crafted(&[
        ZipSpec::new("", b"rom", ZipMethod::Stored).raw_name(b"Pok\x82mon.gb"),
        ZipSpec::new("", b"x", ZipMethod::Stored)
            .raw_name("Caf\u{e9}.gb".as_bytes())
            .utf8(),
    ]);
    let r = list(bytes).unwrap();
    assert_eq!(r.entries()[0].name.as_str(), "Pok\u{e9}mon.gb");
    assert_eq!(r.entries()[0].raw_name, b"Pok\x82mon.gb");
    assert_eq!(r.entries()[1].name.as_str(), "Caf\u{e9}.gb");
}

#[test]
fn zip_unsafe_names_kept_but_never_read() {
    let bytes = zip_crafted(&[
        ZipSpec::new("../evil.gba", b"x", ZipMethod::Stored),
        ZipSpec::new("CON.gba", b"y", ZipMethod::Stored),
        ZipSpec::new("", b"z", ZipMethod::Stored)
            .raw_name(b"bad\xFF.gba")
            .utf8(),
        ZipSpec::new("ok.gba", b"ok", ZipMethod::Stored),
    ]);
    let mut r = list(bytes).expect("unsafe names do not fail the listing");
    let why: Vec<Option<UnsafeWhy>> = r.entries().iter().map(|e| e.unsafe_name).collect();
    assert_eq!(
        why,
        vec![
            Some(UnsafeWhy::DotDot),
            Some(UnsafeWhy::Device),
            Some(UnsafeWhy::Encoding),
            None
        ]
    );
    assert!(matches!(
        read_entry(&mut r, 0),
        Err(ArchiveError::UnsafeEntry {
            why: UnsafeWhy::DotDot,
            ..
        })
    ));
    assert_eq!(read_entry(&mut r, 3).unwrap(), b"ok");
}

#[test]
fn zip_read_charges_budget() {
    let data = sample(10_000);
    let mut r = list(zip_with(&[("a.bin", &data, ZipMethod::Deflate)])).unwrap();
    let limits = Limits {
        plan_head_budget: 1000,
        ..Limits::default()
    };
    let mut budget = Budget::plan_head(&limits);
    let mut stream = r.open_entry(0, &mut budget).unwrap();
    let mut out = Vec::new();
    let err = ArchiveError::from(stream.read_to_end(&mut out).expect_err("budget"));
    assert!(matches!(err, ArchiveError::BudgetSpent { .. }), "{err:?}");
    drop(stream);
    assert!(budget.spent() <= 1001);
}

#[test]
fn zip_stream_entries_in_directory_order() {
    let bytes = zip_with(&[
        ("a.bin", b"aa", ZipMethod::Stored),
        ("b.bin", b"bb", ZipMethod::Deflate),
        ("c.bin", b"cc", ZipMethod::Bzip2),
    ]);
    let mut r = list(bytes).unwrap();
    let mut seen = Vec::new();
    let mut budget = Budget::apply();
    r.stream_entries(&[2, 0, 2], &mut budget, &mut |entry, stream| {
        let mut out = Vec::new();
        stream.read_to_end(&mut out)?;
        seen.push((entry.index, out));
        Ok(())
    })
    .unwrap();
    assert_eq!(seen, vec![(0, b"aa".to_vec()), (2, b"cc".to_vec())]);
}

#[test]
fn zip_duplicate_names_damaged() {
    let bytes = zip_crafted(&[
        ZipSpec::new("game.gba", b"one", ZipMethod::Stored),
        ZipSpec::new("game.gba", b"two", ZipMethod::Stored),
    ]);
    assert!(matches!(list(bytes), Err(ArchiveError::Damaged { .. })));
}

#[test]
fn html_named_zip_is_damaged() {
    let html = b"<!DOCTYPE html><html><body>Not a ROM</body></html>".repeat(20);
    assert!(matches!(
        list(html.to_vec()),
        Err(ArchiveError::Damaged { .. })
    ));
}

#[test]
fn zip_open_from_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Game.zip");
    std::fs::write(&path, zip_with(&[("g.gba", b"rom", ZipMethod::Deflate)])).unwrap();
    let mut r = ZipReader::open(&path, &Limits::default()).unwrap();
    assert_eq!(read_entry(&mut r, 0).unwrap(), b"rom");
}

/// S1 on the zip backend: mutated zips (bit flips, truncations, rewritten
/// length fields) list and decode to `Ok` or `Err`, never an unwind.
#[test]
fn garbled_zips_never_panic() {
    let data = sample(4_000);
    let (props, lzma) = lzma_encode(&data, 1 << 16);
    let fixtures = [
        zip_with(&[
            ("a.bin", &data, ZipMethod::Deflate),
            ("b.bin", &data, ZipMethod::Bzip2),
        ]),
        zip_crafted(&[
            ZipSpec::new("a.bin", &data, raw(14, zip_lzma_payload(props, &lzma))),
            ZipSpec::new("b.bin", &data, raw(98, ppmd_encode(&data, 4, 2))),
            ZipSpec::new("c.bin", &data, raw(93, zstd_encode(&data))),
            ZipSpec::new("d.bin", &data, raw(95, xz_encode(&data))),
        ]),
    ];
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for fixture in &fixtures {
        for round in 0..300 {
            let mut bytes = fixture.clone();
            match round % 3 {
                0 => {
                    for _ in 0..4 {
                        let at = (next() as usize) % bytes.len();
                        bytes[at] ^= 1 << (next() % 8);
                    }
                }
                1 => {
                    let cut = (next() as usize) % bytes.len();
                    bytes.truncate(cut);
                }
                _ => {
                    let at = (next() as usize) % bytes.len().saturating_sub(4).max(1);
                    let v = (next() as u32).to_le_bytes();
                    let end = (at + 4).min(bytes.len());
                    bytes[at..end].copy_from_slice(&v[..end - at]);
                }
            }
            if let Ok(mut r) = list(bytes) {
                for i in 0..r.entries().len() {
                    let _ = read_entry(&mut r, i);
                }
            }
        }
    }
}

/// A reader that counts the bytes it hands out.
struct Counting {
    inner: Cursor<Vec<u8>>,
    read: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl Read for Counting {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read
            .fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(n)
    }
}

impl Seek for Counting {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// An end record the precheck reads but the `zip` crate rejects (its
/// central directory offset lies past itself).
fn decoy_end_record(at: u64) -> Vec<u8> {
    let mut eocd = b"PK\x05\x06".to_vec();
    eocd.extend_from_slice(&[0, 0, 0, 0]); // disk numbers
    eocd.extend_from_slice(&1u16.to_le_bytes());
    eocd.extend_from_slice(&1u16.to_le_bytes());
    eocd.extend_from_slice(&0u32.to_le_bytes()); // directory size
    eocd.extend_from_slice(&(at as u32 + 5).to_le_bytes());
    eocd.extend_from_slice(&0u16.to_le_bytes());
    eocd
}

/// The crate falls back to an earlier end record when the last one fails
/// its own checks, searching the whole file. The precheck only vouched for
/// the last one, so the crate must never see another: the bytes before the
/// checked central directory are hidden from it while it parses, and the
/// listing reads only the tail.
#[test]
fn zip_crate_never_falls_back_to_an_unchecked_end_record() {
    let entries: Vec<(String, Vec<u8>)> = (0..5)
        .map(|i| (format!("g{i}.gba"), vec![i as u8; 10]))
        .collect();
    let refs: Vec<(&str, &[u8], ZipMethod)> = entries
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice(), ZipMethod::Stored))
        .collect();
    let mut bytes = zip_with(&refs);
    bytes.resize(bytes.len() + 8 * MIB as usize, 0);
    let decoy_at = bytes.len() as u64;
    bytes.extend_from_slice(&decoy_end_record(decoy_at));

    let read = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let src = Counting {
        inner: Cursor::new(bytes),
        read: std::sync::Arc::clone(&read),
    };
    let limits = Limits {
        max_entries: 3,
        ..Limits::default()
    };
    let err = ZipReader::new(src, &limits).err().expect("refused");
    assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
    let read = read.load(std::sync::atomic::Ordering::Relaxed);
    assert!(read < MIB, "the listing read {read} bytes of an 8 MiB file");
}

/// An end record inside the central directory (in a name) is one the crate
/// could fall back to: refused before the crate parses anything.
#[test]
fn zip_end_record_inside_central_directory_damaged() {
    for sig in [b"PK\x05\x06", b"PK\x06\x06"] {
        let mut name = b"game".to_vec();
        name.extend_from_slice(sig);
        name.extend_from_slice(b".gba");
        let bytes = zip_crafted(&[
            ZipSpec::new("", b"rom", ZipMethod::Stored).raw_name(&name),
            ZipSpec::new("ok.gba", b"ok", ZipMethod::Stored),
        ]);
        let err = list(bytes).err().expect("refused");
        assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
    }
}

fn zstd_with_skippable(skip: usize, data: &[u8]) -> Vec<u8> {
    let mut out = 0x184D_2A50u32.to_le_bytes().to_vec();
    out.extend_from_slice(&(skip as u32).to_le_bytes());
    out.resize(out.len() + skip, 0);
    out.extend_from_slice(&zstd_encode(data));
    out
}

/// The list-time probe of skippable zstd frames is one allowance for the
/// whole listing, not one per entry: 10,000 entries pointing at one 16 MiB
/// skippable frame must not make the listing read 160 GiB.
#[test]
fn zip_zstd_skippable_probe_shared_across_entries() {
    let data = sample(100);
    let limits = Limits {
        max_header: MIB,
        ..Limits::default()
    };
    let one = zip_crafted(&[ZipSpec::new(
        "a.bin",
        &data,
        raw(93, zstd_with_skippable(600 * 1024, &data)),
    )]);
    let mut r = list_with(one, &limits).expect("one probe fits");
    assert_eq!(read_entry(&mut r, 0).unwrap(), data);

    let two = zip_crafted(&[
        ZipSpec::new(
            "a.bin",
            &data,
            raw(93, zstd_with_skippable(600 * 1024, &data)),
        ),
        ZipSpec::new(
            "b.bin",
            &data,
            raw(93, zstd_with_skippable(600 * 1024, &data)),
        ),
    ]);
    let err = list_with(two, &limits).err().expect("allowance spent");
    assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
}

/// A self-extractor: a prefix before the zip shifts every offset. Hiding
/// the bytes before the directory while the crate parses still lets it
/// find the directory and, afterwards, read the entries in the prefix area.
#[test]
fn zip_with_a_prefix_still_lists_and_reads() {
    for zip64 in [false, true] {
        let mut bytes = vec![0x4Du8; 3000];
        bytes.extend_from_slice(&zip_crafted_with(
            &[
                ZipSpec::new("a.gba", b"first", ZipMethod::Stored),
                ZipSpec::new("b.gba", &sample(5000), ZipMethod::Deflate),
            ],
            ZipEnd {
                zip64,
                ..ZipEnd::default()
            },
        ));
        let mut r = list(bytes).expect("prefixed zip listed");
        assert_eq!(read_entry(&mut r, 0).unwrap(), b"first", "zip64 {zip64}");
        assert_eq!(read_entry(&mut r, 1).unwrap(), sample(5000));
    }
}

/// Two end records that both end at EOF (the first one's comment is a
/// whole second zip): tools that pick another one see another directory.
#[test]
fn zip_two_end_records_reaching_eof_damaged() {
    let mut bytes = zip_with(&[("a.gba", b"first", ZipMethod::Stored)]);
    let second = zip_with(&[("b.gba", b"second", ZipMethod::Stored)]);
    let n = bytes.len();
    bytes[n - 2..].copy_from_slice(&(second.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&second);
    let err = list(bytes).err().expect("refused");
    assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
}

/// The end record's comment must end exactly at EOF: bytes after it are
/// where another parser may look for another end record.
#[test]
fn zip_trailing_bytes_after_the_comment_damaged() {
    let mut bytes = zip_with(&[("a.gba", b"rom", ZipMethod::Stored)]);
    bytes.extend_from_slice(b"trailing junk");
    let err = list(bytes).err().expect("refused");
    assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");

    // A comment declared shorter than what follows the end record.
    let mut bytes = zip_with(&[("a.gba", b"rom", ZipMethod::Stored)]);
    let n = bytes.len();
    bytes[n - 2..].copy_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(b"hi and more");
    let err = list(bytes).err().expect("refused");
    assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
}
