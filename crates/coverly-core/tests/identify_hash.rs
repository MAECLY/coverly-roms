use std::fs;
use std::io::Write;
use std::path::PathBuf;

use coverly_core::identify::hash::{
    hash_rom, hash_rom_mode, hash_stream, n64_to_z64, HashMode, HashOutcome, HeaderRule, N64Order,
    DEFAULT_HASH_MAX_BYTES, HASH_OUTCOME_VERSION,
};
use tempfile::TempDir;

/// The standard check input and its digests.
const CHECK: &[u8] = b"123456789";
const CRC: &str = "CBF43926";
const MD5: &str = "25F9E794323B453885F5181F1B624D0B";
const SHA1: &str = "F7C3BC1D808E04732ADF679965CCC34CA7AE3441";

fn file(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.path().join(name);
    fs::write(&p, bytes).unwrap();
    p
}

fn content(outcome: HashOutcome) -> coverly_core::identify::hash::RomHashes {
    match outcome {
        HashOutcome::Content(h) => h,
        other => panic!("expected content hashes, got {other:?}"),
    }
}

#[test]
fn hashes_a_plain_file_in_one_pass() {
    let dir = TempDir::new().unwrap();
    let p = file(&dir, "Game.gba", CHECK);
    let h = content(hash_rom(&p, "gba", HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap());
    assert_eq!(h.full.crc32, CRC);
    assert_eq!(h.full.md5, MD5);
    assert_eq!(h.full.sha1, SHA1);
    assert_eq!(h.full.size, 9);
    assert_eq!(h.headerless, None);
}

#[test]
fn skips_the_ines_header_and_keeps_the_full_digest() {
    let dir = TempDir::new().unwrap();
    let mut bytes = b"NES\x1a".to_vec();
    bytes.extend([0u8; 12]);
    bytes.extend(CHECK);
    let p = file(&dir, "Game.nes", &bytes);
    let h = content(hash_rom(&p, "nes", HeaderRule::Nes, DEFAULT_HASH_MAX_BYTES).unwrap());
    let body = h.headerless.expect("headerless digest");
    assert_eq!((body.crc32.as_str(), body.size), (CRC, 9));
    assert_eq!(h.full.size, 25);
    assert_ne!(h.full.crc32, CRC);
}

#[test]
fn a_rule_without_its_magic_skips_nothing() {
    let dir = TempDir::new().unwrap();
    let p = file(&dir, "Game.nes", CHECK);
    let h = content(hash_rom(&p, "nes", HeaderRule::Nes, DEFAULT_HASH_MAX_BYTES).unwrap());
    assert_eq!(h.headerless, None);
}

#[test]
fn header_rules_detect_their_headers() {
    let payload = vec![7u8; 1024];
    let crc = format!("{:08X}", crc32fast::hash(&payload));
    let mut lynx = b"LYNX".to_vec();
    lynx.extend([0u8; 60]);
    let mut a7800 = vec![1u8];
    a7800.extend(b"ATARI7800");
    a7800.extend([0u8; 118]);
    let mut fds = b"FDS\x1a".to_vec();
    fds.extend([0u8; 12]);
    let copier = vec![0u8; 512];
    for (rule, header) in [
        (HeaderRule::Lynx, lynx),
        (HeaderRule::A7800, a7800),
        (HeaderRule::Fds, fds),
        (HeaderRule::SnesCopier, copier),
    ] {
        let dir = TempDir::new().unwrap();
        let mut bytes = header.clone();
        bytes.extend(&payload);
        let p = file(&dir, "rom.bin", &bytes);
        let h = content(hash_rom(&p, "bin", rule, DEFAULT_HASH_MAX_BYTES).unwrap());
        let body = h
            .headerless
            .unwrap_or_else(|| panic!("{rule:?}: no headerless digest"));
        assert_eq!(body.crc32, crc, "{rule:?}");
        assert_eq!(body.size, 1024, "{rule:?}");
    }
}

#[test]
fn single_file_zip_uses_the_central_directory_crc() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("Game.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&p).unwrap());
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("Game (USA).gba", opts).unwrap();
    zip.write_all(CHECK).unwrap();
    zip.finish().unwrap();
    assert_eq!(
        hash_rom(&p, "ZIP", HeaderRule::Plain, 1).unwrap(),
        HashOutcome::ZipEntry {
            crc32: CRC.into(),
            size: 9,
            inner_name: "Game (USA).gba".into(),
            payload_sha1: None,
        },
        "zip CRC is read even above the size cap: nothing is decompressed"
    );
}

#[test]
fn multi_file_zips_and_disc_formats_are_identified_by_name() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("set.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&p).unwrap());
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for name in ["a.bin", "b.bin"] {
        zip.start_file(name, opts).unwrap();
        zip.write_all(CHECK).unwrap();
    }
    zip.finish().unwrap();
    assert_eq!(
        hash_rom(&p, "zip", HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap(),
        HashOutcome::NameOnly
    );
    for ext in ["cue", "CUE", "gdi", "m3u", "chd", "7z", "pbp"] {
        let q = file(&dir, &format!("game.{ext}"), CHECK);
        assert_eq!(
            hash_rom(&q, ext, HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap(),
            HashOutcome::NameOnly,
            "{ext}"
        );
    }
}

#[test]
fn files_over_the_cap_are_not_read() {
    let dir = TempDir::new().unwrap();
    let p = file(&dir, "big.gba", CHECK);
    assert_eq!(
        hash_rom(&p, "gba", HeaderRule::Plain, 4).unwrap(),
        HashOutcome::TooLarge { size: 9 }
    );
}

#[test]
fn a_corrupt_zip_is_hashed_as_plain_content() {
    let dir = TempDir::new().unwrap();
    let p = file(&dir, "bad.zip", CHECK);
    let h = content(hash_rom(&p, "zip", HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap());
    assert_eq!(h.full.crc32, CRC);
}

fn sha1_hex(bytes: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    Sha1::digest(bytes)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}

fn zip_one(
    dir: &TempDir,
    name: &str,
    inner: &str,
    bytes: &[u8],
    method: zip::CompressionMethod,
) -> PathBuf {
    let p = dir.path().join(name);
    let mut zip = zip::ZipWriter::new(fs::File::create(&p).unwrap());
    let opts = zip::write::SimpleFileOptions::default().compression_method(method);
    zip.start_file(inner, opts).unwrap();
    zip.write_all(bytes).unwrap();
    zip.finish().unwrap();
    p
}

#[test]
fn sms_copier_header_is_skipped() {
    assert_eq!(HeaderRule::parse("sms-copier"), Some(HeaderRule::SmsCopier));
    let payload: Vec<u8> = (0..32768u32).map(|i| (i * 7) as u8).collect();
    let mut bytes = vec![0u8; 512];
    bytes.extend(&payload);
    let dir = TempDir::new().unwrap();
    let headered = file(&dir, "Game.sms", &bytes);
    let plain = file(&dir, "Game2.sms", &payload);
    let h = content(
        hash_rom(
            &headered,
            "sms",
            HeaderRule::SmsCopier,
            DEFAULT_HASH_MAX_BYTES,
        )
        .unwrap(),
    );
    let body = h.headerless.expect("headerless digest");
    assert_eq!(body.size, 32768);
    assert_eq!(body.sha1, sha1_hex(&payload));
    assert_eq!(h.payload_sha1.as_deref(), Some(sha1_hex(&payload).as_str()));
    let p =
        content(hash_rom(&plain, "sms", HeaderRule::SmsCopier, DEFAULT_HASH_MAX_BYTES).unwrap());
    assert_eq!(p.headerless, None, "a 16 KiB multiple has no copier header");
    assert_eq!(p.payload_sha1, h.payload_sha1);
    assert_eq!(
        HeaderRule::SmsCopier.header_len(&payload, 16384 * 3 + 512),
        512
    );
    assert_eq!(HeaderRule::SmsCopier.header_len(&payload, 1024 + 512), 0);
}

/// A z64 (big-endian) image: the boot magic then filler.
fn z64_image() -> Vec<u8> {
    let mut rom = vec![0x80, 0x37, 0x12, 0x40];
    rom.extend((0..(CHUNK_PLUS)).map(|i| (i % 251) as u8));
    rom
}
/// Longer than one 64 KiB read chunk, a multiple of 4.
const CHUNK_PLUS: usize = 64 * 1024 + 4092;

fn v64(z: &[u8]) -> Vec<u8> {
    z.chunks(2).flat_map(|c| [c[1], c[0]]).collect()
}
fn n64le(z: &[u8]) -> Vec<u8> {
    z.chunks(4).flat_map(|c| [c[3], c[2], c[1], c[0]]).collect()
}

#[test]
fn n64_three_orders_share_payload_sha1() {
    let z = z64_image();
    let (v, n) = (v64(&z), n64le(&z));
    assert_eq!(n64_to_z64(&z), Some(N64Order::Z64));
    assert_eq!(n64_to_z64(&v), Some(N64Order::V64));
    assert_eq!(n64_to_z64(&n), Some(N64Order::N64));
    assert_eq!(n64_to_z64(b"NES\x1a"), None);
    let dir = TempDir::new().unwrap();
    let want = sha1_hex(&z);
    for (name, bytes) in [("a.z64", &z), ("a.v64", &v), ("a.n64", &n)] {
        let p = file(&dir, name, bytes);
        let ext = name.rsplit('.').next().unwrap();
        let h = content(hash_rom(&p, ext, HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap());
        assert_eq!(
            h.full.sha1,
            sha1_hex(bytes),
            "{name}: full digest is the file as stored"
        );
        assert_eq!(h.payload_sha1.as_deref(), Some(want.as_str()), "{name}");
    }
}

#[test]
fn single_entry_zip_payload_sha1_matches_raw_file() {
    let dir = TempDir::new().unwrap();
    let payload: Vec<u8> = (0..40_000u32).map(|i| (i % 13) as u8).collect();
    let raw = file(&dir, "Game.gba", &payload);
    let raw_sha =
        content(hash_rom(&raw, "gba", HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap())
            .payload_sha1
            .expect("raw payload sha1");
    for (name, method) in [
        ("stored.zip", zip::CompressionMethod::Stored),
        ("deflate.zip", zip::CompressionMethod::Deflated),
    ] {
        let p = zip_one(&dir, name, "Game.gba", &payload, method);
        match hash_rom_mode(
            &p,
            "zip",
            HeaderRule::Plain,
            DEFAULT_HASH_MAX_BYTES,
            HashMode::WithPayload,
        )
        .unwrap()
        {
            HashOutcome::ZipEntry {
                size, payload_sha1, ..
            } => {
                assert_eq!(size, 40_000);
                assert_eq!(payload_sha1.as_deref(), Some(raw_sha.as_str()), "{name}");
            }
            other => panic!("{name}: {other:?}"),
        }
        match hash_rom(&p, "zip", HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap() {
            HashOutcome::ZipEntry { payload_sha1, .. } => {
                assert_eq!(payload_sha1, None, "{name}: quick mode decodes nothing")
            }
            other => panic!("{name}: {other:?}"),
        }
    }
    // The header rule applies inside the zip too.
    let mut smc = vec![0u8; 512];
    smc.extend(&payload[..32768]);
    let p = zip_one(
        &dir,
        "smc.zip",
        "Game.smc",
        &smc,
        zip::CompressionMethod::Deflated,
    );
    match hash_rom_mode(
        &p,
        "zip",
        HeaderRule::SnesCopier,
        DEFAULT_HASH_MAX_BYTES,
        HashMode::WithPayload,
    )
    .unwrap()
    {
        HashOutcome::ZipEntry { payload_sha1, .. } => {
            assert_eq!(payload_sha1, Some(sha1_hex(&payload[..32768])))
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn zip_entry_over_budget_has_no_payload_sha1() {
    let dir = TempDir::new().unwrap();
    let p = zip_one(
        &dir,
        "big.zip",
        "Game.gba",
        &[3u8; 4096],
        zip::CompressionMethod::Deflated,
    );
    assert_eq!(
        hash_rom_mode(&p, "zip", HeaderRule::Plain, 4095, HashMode::WithPayload).unwrap(),
        HashOutcome::ZipEntry {
            crc32: format!("{:08X}", crc32fast::hash(&[3u8; 4096])),
            size: 4096,
            inner_name: "Game.gba".into(),
            payload_sha1: None,
        }
    );
}

#[test]
fn v1_quick_id_rows_are_treated_as_missing() {
    use coverly_core::cache::Cache;
    assert_eq!(HASH_OUTCOME_VERSION, 2);
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("c.sqlite3");
    let rom = std::path::Path::new("/lib/ROMS/GBA/a.gba");
    let v2 = HashOutcome::Content(coverly_core::identify::hash::RomHashes {
        full: coverly_core::identify::hash::Digests {
            crc32: CRC.into(),
            md5: MD5.into(),
            sha1: SHA1.into(),
            size: 9,
        },
        headerless: None,
        payload_sha1: Some(SHA1.into()),
    });
    {
        let cache = Cache::open(&db).unwrap();
        cache.put_quick_id(rom, 9, 1_000, &v2).unwrap();
        assert_eq!(cache.quick_id(rom, 9, 1_000).unwrap(), Some(v2.clone()));
    }
    // A row written by the v1 code: the bare outcome JSON, no payload hash.
    let v1 = format!(
        r#"{{"kind":"content","full":{{"crc32":"{CRC}","md5":"{MD5}","sha1":"{SHA1}","size":9}},"headerless":null}}"#
    );
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE quick_ids SET outcome = ?1 WHERE path = ?2",
            rusqlite::params![v1, rom.to_string_lossy()],
        )
        .unwrap();
    }
    let cache = Cache::open(&db).unwrap();
    assert_eq!(
        cache.quick_id(rom, 9, 1_000).unwrap(),
        None,
        "v1 rows are re-hashed"
    );
}

/// Counts the bytes a reader hands out.
struct Counting<R> {
    inner: R,
    read: std::rc::Rc<std::cell::Cell<u64>>,
}
impl<R: std::io::Read> std::io::Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read.set(self.read.get() + n as u64);
        Ok(n)
    }
}

#[test]
fn headerless_sha1_is_computed_in_the_same_pass() {
    let mut bytes = b"NES\x1a".to_vec();
    bytes.extend([0u8; 12]);
    let body: Vec<u8> = (0..200_000u32).map(|i| (i % 97) as u8).collect();
    bytes.extend(&body);
    let read = std::rc::Rc::new(std::cell::Cell::new(0));
    let reader = Counting {
        inner: std::io::Cursor::new(bytes.clone()),
        read: read.clone(),
    };
    let h = hash_stream(reader, bytes.len() as u64, HeaderRule::Nes).unwrap();
    assert_eq!(
        read.get(),
        bytes.len() as u64,
        "every byte read exactly once"
    );
    assert_eq!(h.full.sha1, sha1_hex(&bytes));
    assert_eq!(
        h.headerless.as_ref().map(|d| d.sha1.as_str()),
        Some(sha1_hex(&body).as_str())
    );
    assert_eq!(h.payload_sha1, Some(sha1_hex(&body)));
}
