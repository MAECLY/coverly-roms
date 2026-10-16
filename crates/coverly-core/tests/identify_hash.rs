use std::fs;
use std::io::Write;
use std::path::PathBuf;

use coverly_core::identify::hash::{hash_rom, HashOutcome, HeaderRule, DEFAULT_HASH_MAX_BYTES};
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
            inner_name: "Game (USA).gba".into()
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
