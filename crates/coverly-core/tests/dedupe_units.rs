//! Duplicates: what is compared (units) and the seams that read archives
//! and discs (`dedupe::source`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use coverly_core::dedupe::source::{
    DiscIdSource, DiscProbe, EntryListing, ListedEntry, NoDiscProbe, ZipListing,
};
use coverly_core::dedupe::testing::{disc_id, fingerprint, FakeDiscProbe, FakeListing};
use coverly_core::identify::hash::HeaderRule;
use sha1::{Digest, Sha1};
use tempfile::TempDir;

fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}

/// A zip at `dir/name` holding `entries` (name, bytes), stored uncompressed.
fn zip_with(dir: &TempDir, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
    let p = dir.path().join(name);
    let mut zip = zip::ZipWriter::new(fs::File::create(&p).unwrap());
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.add_directory("sub/", opts).unwrap();
    for (inner, bytes) in entries {
        zip.start_file(*inner, opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    p
}

/// Flips every occurrence of `needle` in the file, leaving the central
/// directory intact: a lister that decodes would notice.
fn corrupt(path: &Path, needle: &[u8]) {
    let mut bytes = fs::read(path).unwrap();
    let mut hits = 0;
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            for b in &mut bytes[i..i + needle.len()] {
                *b ^= 0xFF;
            }
            hits += 1;
            i += needle.len();
        } else {
            i += 1;
        }
    }
    assert!(hits > 0, "payload not found in the zip");
    fs::write(path, bytes).unwrap();
}

#[test]
fn zip_listing_reads_central_directory_without_decoding() {
    let dir = TempDir::new().unwrap();
    let a: Vec<u8> = (0..4096u32).map(|i| (i * 13 + 7) as u8).collect();
    let b = b"second entry payload, quite distinct".to_vec();
    let p = zip_with(&dir, "Set.ZIP", &[("a.bin", &a), ("b.bin", &b)]);
    corrupt(&p, &a[..64]);

    let listed = ZipListing.list(&p).unwrap().expect("a zip is listed");
    assert_eq!(
        listed,
        vec![
            ListedEntry {
                name: "a.bin".into(),
                size: a.len() as u64,
                crc32: Some(crc32fast::hash(&a)),
            },
            ListedEntry {
                name: "b.bin".into(),
                size: b.len() as u64,
                crc32: Some(crc32fast::hash(&b)),
            },
        ]
    );

    // Not a zip: not this lister's business.
    let rom = dir.path().join("Game.sfc");
    fs::write(&rom, &a).unwrap();
    assert_eq!(ZipListing.list(&rom).unwrap(), None);
    // A broken zip is an error, not "not an archive".
    let broken = dir.path().join("Broken.zip");
    fs::write(&broken, b"PK\x03\x04 not really").unwrap();
    assert!(ZipListing.list(&broken).is_err());
}

#[test]
fn zip_payload_sha1_respects_budget() {
    let dir = TempDir::new().unwrap();
    let payload: Vec<u8> = (0..32768u32).map(|i| (i * 7) as u8).collect();
    let mut headered = vec![0u8; 512];
    headered.extend(&payload);
    let p = zip_with(
        &dir,
        "Game.zip",
        &[("readme.txt", b"hello"), ("Game.sms", &headered)],
    );

    // Index 1 of the listing (the directory entry is not listed).
    let size = headered.len() as u64;
    assert_eq!(
        ZipListing
            .payload_sha1(&p, 1, HeaderRule::SmsCopier, size)
            .unwrap(),
        Some(sha1_hex(&payload)),
        "copier header skipped"
    );
    assert_eq!(
        ZipListing
            .payload_sha1(&p, 1, HeaderRule::Plain, size)
            .unwrap(),
        Some(sha1_hex(&headered))
    );
    assert_eq!(
        ZipListing
            .payload_sha1(&p, 1, HeaderRule::SmsCopier, size - 1)
            .unwrap(),
        None,
        "over budget: nothing decoded"
    );
    assert!(ZipListing
        .payload_sha1(&p, 2, HeaderRule::Plain, size)
        .is_err());

    // An entry whose bytes do not match its directory CRC is an error.
    corrupt(&p, &payload[..64]);
    assert!(ZipListing
        .payload_sha1(&p, 1, HeaderRule::SmsCopier, size)
        .is_err());
}

#[test]
fn no_disc_probe_disables_level_three() {
    let dir = TempDir::new().unwrap();
    let iso = dir.path().join("Game.iso");
    fs::write(&iso, vec![0u8; 4096]).unwrap();
    assert!(!NoDiscProbe.reads_discs());
    assert_eq!(NoDiscProbe.probe(&iso, "psx").unwrap(), None);
}

#[test]
fn fake_disc_probe_returns_scripted_ids() {
    let one = disc_id(&["SLUS-01161"], Some(1), Some(fingerprint(1)));
    let two = disc_id(&["SLUS-01318"], Some(2), None);
    let probe = FakeDiscProbe::new()
        .with("/roms/psx/Driver 2.img", one.clone())
        .with("/roms/psx/Driver 2.bin", two.clone())
        .failing("/roms/psx/Broken.chd");

    assert!(probe.reads_discs());
    assert_eq!(
        probe
            .probe(Path::new("/roms/psx/Driver 2.img"), "psx")
            .unwrap(),
        Some(one.clone())
    );
    assert_eq!(
        probe
            .probe(Path::new("/roms/psx/Driver 2.bin"), "psx")
            .unwrap(),
        Some(two)
    );
    assert_eq!(
        probe
            .probe(Path::new("/roms/psx/Other.iso"), "psx")
            .unwrap(),
        None
    );
    assert!(probe
        .probe(Path::new("/roms/psx/Broken.chd"), "psx")
        .is_err());
    assert_eq!(one.source, DiscIdSource::SystemCnf);
    assert_ne!(fingerprint(1), fingerprint(2));
    assert_eq!(probe.calls().len(), 4);
}

#[test]
fn fake_listing_scripts_entries_and_payloads() {
    let listing = FakeListing::new()
        .with_entries(
            "/roms/arcade/ffight.zip",
            vec![ListedEntry {
                name: "ff.01".into(),
                size: 1024,
                crc32: Some(0xDEAD_BEEF),
            }],
        )
        .with_payload("/roms/arcade/ffight.zip", 0, "ABC");
    let p = Path::new("/roms/arcade/ffight.zip");
    assert_eq!(listing.list(p).unwrap().unwrap().len(), 1);
    assert_eq!(
        listing.payload_sha1(p, 0, HeaderRule::Plain, 1024).unwrap(),
        Some("ABC".into())
    );
    assert_eq!(
        listing.payload_sha1(p, 0, HeaderRule::Plain, 1023).unwrap(),
        None
    );
    assert_eq!(listing.list(Path::new("/roms/x.sfc")).unwrap(), None);
}
