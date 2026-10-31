//! Opening organise candidates (spec §7.2; archives spec §3, §6.1): raw
//! files, then every container through the archive reader, detected by
//! magic first. Fixtures are synthetic.

mod common;

use common::roms::{zip_encrypted, zip_file};
use coverly_core::archive::external::{Search, ToolsOnDemand};
use coverly_core::archive::testing::{fake_tool, FakeScript};
use coverly_core::archive::testing::{
    sevenz, sevenz_copy_parts, sevenz_header_bytes, sevenz_with_encoded, tar_gz, SevenZShape,
    SzCoder, SzFolder,
};
use coverly_core::archive::{Compressor, Format, Limits, Tool};
use coverly_core::identify::sniff::fixtures as fx;
use coverly_core::identify::sniff::NotARom;
use coverly_core::organize::evidence::{
    identify_candidate, CatalogueHit, Facts, Identification, LadderEnv, Thresholds,
};
use coverly_core::organize::inspect::{
    inspect_file, inspect_file_with, inspect_set_primary, ArchiveIssue, Container, Inspection,
    MAX_ZIP_ENTRIES,
};
use coverly_core::systems::SystemDb;

fn inspect(path: &std::path::Path) -> Inspection {
    let len = std::fs::metadata(path).unwrap().len();
    inspect_file(path, len, &SystemDb::builtin()).unwrap()
}

fn inspect_limited(path: &std::path::Path, limits: &Limits) -> Inspection {
    let len = std::fs::metadata(path).unwrap().len();
    inspect_file_with(path, len, &SystemDb::builtin(), limits, None).unwrap()
}

/// Limits that never run an external tool (deterministic on any machine).
fn no_tools() -> Limits {
    Limits {
        external_tools: false,
        ..Limits::default()
    }
}

fn gz(data: &[u8], fname: Option<&[u8]>) -> Vec<u8> {
    use std::io::Write;
    let mut b = flate2::GzBuilder::new();
    if let Some(name) = fname {
        b = b.filename(name);
    }
    let mut w = b.write(Vec::new(), flate2::Compression::fast());
    w.write_all(data).unwrap();
    w.finish().unwrap()
}

/// No DAT at all: identification from the inspection alone.
struct NoCatalogue;

impl coverly_core::organize::evidence::Catalogue for NoCatalogue {
    fn hits(&self, _: &[String], _: &str, _: u64) -> Vec<CatalogueHit> {
        Vec::new()
    }
    fn any_hits(&self, _: &str, _: u64) -> Vec<CatalogueHit> {
        Vec::new()
    }
    fn stems(&self, _: &str) -> Vec<(String, String)> {
        Vec::new()
    }
    fn stem_systems(&self) -> Vec<String> {
        Vec::new()
    }
    fn set_roms(&self, _: &str, _: &str) -> Option<(String, Vec<(String, u64)>)> {
        None
    }
    fn unconsulted(&self) -> Vec<String> {
        Vec::new()
    }
}

fn identify_offline(p: &std::path::Path, inspection: Inspection) -> Identification {
    let db = SystemDb::builtin();
    let cache = coverly_core::cache::Cache::in_memory().unwrap();
    let env = LadderEnv {
        db: &db,
        catalogue: &NoCatalogue,
        cache: &cache,
        thresholds: Thresholds::default(),
        broad_root: false,
        need_hash: false,
        arcade_order: &[],
        online: None,
    };
    let len = std::fs::metadata(p).unwrap().len();
    let name = p.file_name().unwrap().to_str().unwrap().to_string();
    let mut facts = Facts {
        inspection,
        raw_crcs: vec![],
        hashed: false,
    };
    identify_candidate(p, &name, len, &mut facts, false, &env)
}

fn crc_hex(bytes: &[u8]) -> String {
    format!("{:08X}", crc32fast::hash(bytes))
}

#[test]
fn a_raw_file_is_one_sniffed_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.gba");
    std::fs::write(&p, fx::gba()).unwrap();
    let i = inspect(&p);
    assert_eq!(i.container, Container::Raw);
    assert_eq!(i.issue, None);
    assert_eq!(i.payloads.len(), 1);
    assert_eq!(i.payloads[0].name, "Game.gba");
    assert_eq!(i.payloads[0].ext, "gba");
    assert_eq!(i.payloads[0].crc32, None, "raw files are not hashed here");
    assert_eq!(i.payloads[0].sniff.hits[0].system, "gba");
}

#[test]
fn a_raw_web_page_is_not_a_rom() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.gba");
    std::fs::write(&p, fx::html_page()).unwrap();
    assert_eq!(
        inspect(&p).issue,
        Some(ArchiveIssue::NotARom {
            what: NotARom::WebPage
        })
    );
}

#[test]
fn single_entry_zips_stored_and_deflated() {
    let tmp = tempfile::tempdir().unwrap();
    for deflate in [false, true] {
        let p = tmp.path().join(format!("t{deflate}.zip"));
        let rom = fx::gb(0xC0);
        zip_file(&p, &[("Tetris DX.gbc", &rom, deflate)]);
        let i = inspect(&p);
        assert_eq!(i.container, Container::Zip);
        assert_eq!(i.issue, None);
        let pay = &i.payloads[0];
        assert_eq!(pay.ext, "gbc");
        assert_eq!(pay.size, rom.len() as u64);
        assert_eq!(pay.crc32.as_deref(), Some(crc_hex(&rom).as_str()));
        assert_eq!(pay.sniff.hits[0].system, "gbc");
    }
}

#[test]
fn readme_entries_are_not_payloads() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("g.zip");
    zip_file(
        &p,
        &[
            ("readme.txt", b"hello", true),
            ("dir/Game.gba", &fx::gba(), true),
        ],
    );
    let i = inspect(&p);
    assert_eq!(i.payloads.len(), 1);
    assert_eq!(i.payloads[0].name, "dir/Game.gba");
    assert_eq!(i.entries.len(), 2);
    assert_eq!(i.entries[0].name, "readme.txt");
    assert_eq!(
        i.entries[0].crc32.as_deref(),
        Some(crc_hex(b"hello").as_str())
    );
}

#[test]
fn a_lone_generic_entry_is_a_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Sonic.zip");
    zip_file(&p, &[("Sonic.bin", &fx::md(), true)]);
    let i = inspect(&p);
    assert_eq!(i.payloads.len(), 1);
    assert_eq!(i.payloads[0].ext, "bin");
    assert_eq!(i.payloads[0].sniff.hits[0].system, "megadrive");
}

#[test]
fn arcade_shaped_zips_keep_entries_without_payloads() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("pacman.zip");
    zip_file(
        &p,
        &[
            ("pacman.6e", b"aaaa", false),
            ("pacman.6f", b"bbbb", false),
            ("82s123.7f", b"cccc", false),
        ],
    );
    let i = inspect(&p);
    assert_eq!(i.issue, None);
    assert!(i.payloads.is_empty());
    assert_eq!(i.entries.len(), 3);
}

#[test]
fn an_html_page_named_zip_is_not_a_rom() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("3-D Tetris (USA).zip");
    std::fs::write(&p, fx::html_page()).unwrap();
    let i = inspect(&p);
    assert_eq!(i.container, Container::Zip);
    assert_eq!(
        i.issue,
        Some(ArchiveIssue::NotARom {
            what: NotARom::WebPage
        })
    );
}

#[test]
fn a_truncated_zip_is_damaged() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("t.zip");
    zip_file(&p, &[("a.gba", &fx::gba(), true)]);
    let bytes = std::fs::read(&p).unwrap();
    std::fs::write(&p, &bytes[..bytes.len() / 2]).unwrap();
    assert_eq!(inspect(&p).issue, Some(ArchiveIssue::Damaged));
}

#[test]
fn an_empty_file_named_zip_is_damaged() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("empty.zip");
    std::fs::write(&p, b"").unwrap();
    assert_eq!(inspect(&p).issue, Some(ArchiveIssue::Damaged));
}

#[test]
fn a_zip_bomb_entry_is_read_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("bomb.zip");
    let zeros = vec![0u8; 64 * 1024 * 1024]; // 64 MiB of zeros deflate to ~64 KiB
    zip_file(&p, &[("big.sfc", &zeros, true)]);
    let started = std::time::Instant::now();
    let i = inspect(&p);
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(i.payloads[0].size, 64 * 1024 * 1024);
    assert!(i.payloads[0].sniff.hits.is_empty());
}

#[test]
fn too_many_roms_is_a_collection() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("collection.zip");
    let rom = fx::nes();
    let names: Vec<String> = (0..20).map(|i| format!("g{i}.nes")).collect();
    let entries: Vec<(&str, &[u8], bool)> = names
        .iter()
        .map(|n| (n.as_str(), rom.as_slice(), false))
        .collect();
    zip_file(&p, &entries);
    let i = inspect(&p);
    assert_eq!(i.issue, Some(ArchiveIssue::TooManyRoms { count: 20 }));
    assert!(i.payloads.is_empty());
}

#[test]
fn zip_20000_entries_archive_too_big_not_damaged() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("huge.zip");
    let count = 2 * MAX_ZIP_ENTRIES;
    let names: Vec<String> = (0..count).map(|i| format!("f{i}.txt")).collect();
    let entries: Vec<(&str, &[u8], bool)> = names
        .iter()
        .map(|n| (n.as_str(), &b""[..], false))
        .collect();
    zip_file(&p, &entries);
    let i = inspect(&p);
    assert_eq!(
        i.issue,
        Some(ArchiveIssue::TooBig {
            entries: Some(count as u64),
            bytes: None
        })
    );
    assert!(i.entries.is_empty());
    assert!(i.payloads.is_empty());
}

#[test]
fn a_zip_whose_every_entry_is_encrypted_is_encrypted() {
    // Archives spec §4.2: "header or every payload encrypted" → Encrypted.
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("locked.zip");
    zip_encrypted(&p, "Game.gba", &fx::gba());
    let i = inspect(&p);
    assert_eq!(i.container, Container::Zip);
    assert_eq!(i.issue, Some(ArchiveIssue::Encrypted));
    assert!(i.payloads.is_empty());
}

#[test]
fn an_encrypted_entry_next_to_a_plain_one_is_noted_and_not_sniffed() {
    use coverly_core::archive::testing::{zip_crafted, ZipMethod, ZipSpec};
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("half.zip");
    let rom = fx::gba();
    std::fs::write(
        &p,
        zip_crafted(&[
            ZipSpec::new("Game.gba", &rom, ZipMethod::Stored).aes(0),
            ZipSpec::new("readme.txt", b"hi", ZipMethod::Stored),
        ]),
    )
    .unwrap();
    let i = inspect(&p);
    assert_eq!(i.issue, None);
    assert_eq!(i.notes, vec!["encrypted".to_string()]);
    assert_eq!(i.payloads.len(), 1);
    assert_eq!(i.payloads[0].crc32.as_deref(), Some(crc_hex(&rom).as_str()));
    assert!(i.payloads[0].sniff.hits.is_empty());
}

#[test]
fn a_zip_named_bin_is_opened_as_a_zip() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("game.bin");
    zip_file(&p, &[("Game.gba", &fx::gba(), true)]);
    let i = inspect(&p);
    assert_eq!(i.container, Container::Zip);
    assert_eq!(i.payloads[0].sniff.hits[0].system, "gba");
}

#[test]
fn seven_zip_chd_and_rar_by_magic() {
    let tmp = tempfile::tempdir().unwrap();
    let rar_missing = ArchiveIssue::Unsupported {
        format: Some("RAR".into()),
        need: Some(Tool::Unrar),
    };
    let cases: [(&str, &[u8], Container, Option<ArchiveIssue>); 6] = [
        // A bare signature is no 7z listing: damaged now that 7z is read.
        (
            "a.7z",
            &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 4],
            Container::SevenZip,
            Some(ArchiveIssue::Damaged),
        ),
        (
            "b.7z",
            b"\x00\x01binary, not a 7z",
            Container::SevenZip,
            Some(ArchiveIssue::Damaged),
        ),
        // Text under an archive name is not a ROM (spec §3.2 rule 3).
        (
            "t.7z",
            b"not a 7z at all",
            Container::SevenZip,
            Some(ArchiveIssue::NotARom {
                what: NotARom::Text,
            }),
        ),
        ("c.chd", b"MComprHD\0\0\0\x7c", Container::Chd, None),
        (
            "d.chd",
            b"garbage!",
            Container::Chd,
            Some(ArchiveIssue::Damaged),
        ),
        (
            "e.rar",
            b"Rar!\x1a\x07\x01\x00",
            Container::Rar,
            Some(rar_missing),
        ),
    ];
    for (name, bytes, container, issue) in cases {
        let p = tmp.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        let i = inspect_limited(&p, &no_tools());
        assert_eq!((i.container, i.issue), (container, issue), "{name}");
        assert!(i.payloads.is_empty());
    }
}

#[test]
fn the_primary_track_of_a_set_is_sniffed_from_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game (Track 1).bin");
    std::fs::write(&p, fx::psx_iso(true, false)).unwrap();
    let s = inspect_set_primary(&p).unwrap();
    assert_eq!(s.hits[0].system, "psx");
}

#[test]
fn inspections_serialise_with_kebab_case_tags() {
    let json = serde_json::to_value(ArchiveIssue::TooManyRoms { count: 9 }).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "kind": "too-many-roms", "count": 9 })
    );
    let json = serde_json::to_value(Container::SevenZip).unwrap();
    assert_eq!(json, serde_json::json!("seven-zip"));
    let json = serde_json::to_value(ArchiveIssue::Unsupported {
        format: Some("RAR".into()),
        need: Some(Tool::Unrar),
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "kind": "unsupported", "format": "RAR", "need": "unrar" })
    );
}

#[test]
fn inspections_from_before_archives_still_parse() {
    // Facts memoised before the archive reader: no new fields, the old
    // unit `unsupported` issue.
    let old = serde_json::json!({
        "container": "rar",
        "payloads": [],
        "entries": [{ "name": "a.gba", "crc32": "0000ABCD", "size": 4 }],
        "issue": { "kind": "unsupported" },
        "notes": [],
    });
    let i: Inspection = serde_json::from_value(old).unwrap();
    assert_eq!(
        i.issue,
        Some(ArchiveIssue::Unsupported {
            format: None,
            need: None
        })
    );
    assert_eq!(i.format, None);
    assert_eq!(i.mismatch, None);
    assert!(i.listing_complete);
    assert!(!i.budget_spent);
    assert_eq!(i.entries[0].crc32.as_deref(), Some("0000ABCD"));
}

#[test]
fn macos_metadata_entries_are_not_payloads() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.zip");
    zip_file(
        &p,
        &[
            ("Game.gba", &fx::gba(), true),
            ("__MACOSX/._Game.gba", b"\x00\x05\x16\x07appledouble", true),
            ("__MACOSX/.DS_Store", b"junk", true),
        ],
    );
    let i = inspect(&p);
    assert_eq!(i.payloads.len(), 1);
    assert_eq!(i.payloads[0].name, "Game.gba");
    assert_eq!(i.entries.len(), 3, "entries still list every file");
}

#[test]
fn a_generic_entry_next_to_macos_metadata_is_still_lone() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Sonic.zip");
    zip_file(
        &p,
        &[
            ("Sonic.bin", &fx::md(), true),
            ("__MACOSX/._Sonic.bin", b"\x00\x05\x16\x07appledouble", true),
        ],
    );
    let i = inspect(&p);
    assert_eq!(i.payloads.len(), 1);
    assert_eq!(i.payloads[0].sniff.hits[0].system, "megadrive");
}

#[test]
fn a_generic_entry_with_a_readme_is_a_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Sonic.zip");
    zip_file(
        &p,
        &[
            ("Sonic.bin", &fx::md(), true),
            ("readme.txt", b"enjoy", true),
            ("file_id.diz", b"scene", true),
        ],
    );
    let i = inspect(&p);
    assert_eq!(i.payloads.len(), 1);
    assert_eq!(i.payloads[0].name, "Sonic.bin");
}

#[test]
fn arcade_data_with_a_readme_stays_without_payloads() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("outrun.zip");
    zip_file(
        &p,
        &[
            ("epr-10381.bin", b"aaaa", false),
            ("epr-10382.ic12", b"bbbb", false),
            ("readme.txt", b"cccc", false),
        ],
    );
    assert!(inspect(&p).payloads.is_empty());
}

#[test]
fn an_oversized_central_directory_is_too_big_before_parsing() {
    // A hand-made end record that claims a 32 MiB directory.
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("claims.zip");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PK\x03\x04");
    bytes.extend_from_slice(&[0u8; 26]);
    bytes.extend_from_slice(b"PK\x05\x06");
    bytes.extend_from_slice(&[0, 0, 0, 0]); // disk numbers
    bytes.extend_from_slice(&5u16.to_le_bytes()); // entries on disk
    bytes.extend_from_slice(&5u16.to_le_bytes()); // entries total
    bytes.extend_from_slice(&(32u32 * 1024 * 1024).to_le_bytes()); // directory size
    bytes.extend_from_slice(&0u32.to_le_bytes()); // directory offset
    bytes.extend_from_slice(&0u16.to_le_bytes()); // comment length
    std::fs::write(&p, &bytes).unwrap();
    let i = inspect(&p);
    assert_eq!(
        i.issue,
        Some(ArchiveIssue::TooBig {
            entries: None,
            bytes: Some(32 * 1024 * 1024)
        })
    );
}

#[test]
fn central_directory_limits_are_read_from_the_end_record() {
    use coverly_core::organize::inspect::central_directory_claim;
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("g.zip");
    zip_file(&p, &[("a.gba", &fx::gba(), true), ("b.txt", b"x", false)]);
    let mut f = std::fs::File::open(&p).unwrap();
    let len = f.metadata().unwrap().len();
    let claim = central_directory_claim(&mut f, len).unwrap();
    assert_eq!(claim.entries, 2);
    assert!(claim.size > 0 && claim.size < 1024);
}

// --- Archives spec §3.2, §6.1: magic first, every format (Task 24) ---

#[test]
fn sevenz_named_zip_is_not_damaged_and_notes_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Next Tetris, The.zip");
    let rom = fx::gba();
    std::fs::write(
        &p,
        sevenz(&[("Game.gba", &rom)], SevenZShape::non_solid_lzma()),
    )
    .unwrap();
    let i = inspect(&p);
    assert_eq!(i.container, Container::SevenZip);
    assert_eq!(i.issue, None, "a 7z named .zip is never damaged");
    assert_eq!(i.format, Some(Format::SevenZip));
    assert_eq!(i.mismatch, Some(("zip".to_string(), Format::SevenZip)));
    assert_eq!(i.payloads.len(), 1);
    assert_eq!(i.payloads[0].crc32.as_deref(), Some(crc_hex(&rom).as_str()));
    assert_eq!(i.payloads[0].sniff.hits[0].system, "gba");
}

#[test]
fn a_matching_extension_is_no_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    for (name, bytes, format) in [
        (
            "a.7z",
            sevenz(&[("Game.gba", &fx::gba())], SevenZShape::non_solid_lzma()),
            Format::SevenZip,
        ),
        (
            "b.tgz",
            tar_gz(&[("Game.gba", &fx::gba())], 0),
            Format::Tar {
                outer: Some(Compressor::Gzip),
            },
        ),
        (
            "c.gba.gz",
            gz(&fx::gba(), None),
            Format::Single(Compressor::Gzip),
        ),
    ] {
        let p = tmp.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        let i = inspect(&p);
        assert_eq!(i.format, Some(format), "{name}");
        assert_eq!(i.mismatch, None, "{name}");
    }
    // A zip named .bin is opened by magic; `.bin` names no archive.
    let p = tmp.path().join("game.bin");
    zip_file(&p, &[("Game.gba", &fx::gba(), true)]);
    assert_eq!(inspect(&p).mismatch, None);
}

#[test]
fn html_zip_is_not_a_rom() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("3-D Tetris (USA).zip");
    std::fs::write(&p, fx::html_page()).unwrap();
    let i = inspect(&p);
    assert_eq!(
        i.issue,
        Some(ArchiveIssue::NotARom {
            what: NotARom::WebPage
        })
    );
    assert_eq!(i.format, None);
    assert_eq!(i.mismatch, None);
}

#[test]
fn cartridge_in_7z_identified_by_header() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Tetris DX.7z");
    let rom = fx::gb(0xC0);
    std::fs::write(
        &p,
        sevenz(
            &[("Tetris DX.gbc", &rom), ("readme.txt", b"hello")],
            SevenZShape::solid_lzma2_sheets_last(),
        ),
    )
    .unwrap();
    let i = inspect(&p);
    assert_eq!(i.container, Container::SevenZip);
    assert_eq!(i.issue, None);
    assert_eq!(i.entries.len(), 2);
    assert_eq!(i.payloads.len(), 1);
    let pay = &i.payloads[0];
    assert_eq!(pay.name, "Tetris DX.gbc");
    assert_eq!(pay.size, rom.len() as u64);
    assert_eq!(pay.crc32.as_deref(), Some(crc_hex(&rom).as_str()));
    assert_eq!(pay.sniff.hits[0].system, "gbc");
}

#[test]
fn cartridge_in_tar_gz_crc_from_bounded_decode() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.tar.gz");
    let rom = fx::gba();
    std::fs::write(&p, tar_gz(&[("Game.gba", &rom), ("readme.txt", b"hi")], 0)).unwrap();
    let i = inspect(&p);
    assert_eq!(i.container, Container::Tar);
    assert_eq!(
        i.format,
        Some(Format::Tar {
            outer: Some(Compressor::Gzip)
        })
    );
    assert_eq!(i.issue, None);
    assert!(i.listing_complete);
    assert_eq!(i.payloads.len(), 1);
    // tar stores no CRC: it comes from decoding the (small) entry.
    assert_eq!(i.payloads[0].crc32.as_deref(), Some(crc_hex(&rom).as_str()));
    assert_eq!(i.payloads[0].sniff.hits[0].system, "gba");
    let readme = i.entries.iter().find(|e| e.name == "readme.txt").unwrap();
    assert_eq!(readme.crc32, None, "only ROM-like entries are decoded");
}

#[test]
fn game_gba_gz_single_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("game.gba.gz");
    let rom = fx::gba();
    std::fs::write(&p, gz(&rom, None)).unwrap();
    let i = inspect(&p);
    assert_eq!(i.container, Container::Gzip);
    assert_eq!(i.format, Some(Format::Single(Compressor::Gzip)));
    assert_eq!(i.issue, None);
    assert_eq!(i.payloads.len(), 1);
    let pay = &i.payloads[0];
    assert_eq!(pay.name, "game.gba");
    assert_eq!(pay.ext, "gba");
    assert_eq!(pay.size, rom.len() as u64);
    assert_eq!(pay.crc32.as_deref(), Some(crc_hex(&rom).as_str()));
    assert_eq!(pay.sniff.hits[0].system, "gba");
}

#[test]
fn rar_without_tool_unsupported_need_unrar() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.rar");
    let mut bytes = b"Rar!\x1a\x07\x00".to_vec();
    bytes.extend_from_slice(&[0u8; 64]);
    std::fs::write(&p, bytes).unwrap();
    let i = inspect_limited(&p, &no_tools());
    assert_eq!(i.container, Container::Rar);
    assert_eq!(
        i.issue,
        Some(ArchiveIssue::Unsupported {
            format: Some("RAR".into()),
            need: Some(Tool::Unrar)
        })
    );
}

#[test]
fn encrypted_7z_header_left_encrypted() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("locked.7z");
    let (packed, h) = sevenz_copy_parts(&[("Game.gba", &fx::gba())]);
    let raw = sevenz_header_bytes(&h);
    let bytes = sevenz_with_encoded(&packed, &raw, |s| {
        let size = s.folders[0].unpack_sizes[0];
        s.folders[0] = SzFolder::chain(vec![SzCoder::aes(), SzCoder::lzma2(16)], size);
    });
    std::fs::write(&p, bytes).unwrap();
    let i = inspect(&p);
    assert_eq!(i.container, Container::SevenZip);
    assert_eq!(i.issue, Some(ArchiveIssue::Encrypted));
}

#[test]
fn nested_archive_only_payload_left_nested() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.zip");
    let inner = sevenz(&[("Game.gba", &fx::gba())], SevenZShape::non_solid_lzma());
    zip_file(
        &p,
        &[("Game.7z", &inner, false), ("readme.txt", b"hi", false)],
    );
    let i = inspect(&p);
    assert_eq!(i.issue, Some(ArchiveIssue::Nested));
    assert!(i.payloads.is_empty());
}

#[test]
fn patch_only_archive_is_not_a_rom() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Pokemon Jupiter (Ruby Hack).zip");
    zip_file(
        &p,
        &[
            (
                "Pok\u{e9}mon Jupiter Version 6.ips",
                b"PATCH\0\0\0EOF",
                false,
            ),
            ("readme.txt", b"apply to Ruby", false),
        ],
    );
    let i = inspect(&p);
    assert_eq!(
        i.issue,
        Some(ArchiveIssue::NotARom {
            what: NotARom::Patch
        })
    );
    assert!(i.payloads.is_empty());
}

#[test]
fn a_rom_shipped_with_its_patch_is_still_the_rom() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Hack.zip");
    zip_file(
        &p,
        &[
            ("Game.gba", &fx::gba(), false),
            ("Hack.ips", b"PATCHEOF", false),
        ],
    );
    let i = inspect(&p);
    assert_eq!(i.issue, None);
    assert_eq!(i.payloads.len(), 1);
}

#[test]
fn a_multi_volume_part_is_never_opened() {
    let tmp = tempfile::tempdir().unwrap();
    let whole = sevenz(&[("Game.gba", &fx::gba())], SevenZShape::non_solid_lzma());
    let (a, b) = whole.split_at(whole.len() / 2);
    let first = tmp.path().join("Game.7z.001");
    let second = tmp.path().join("Game.7z.002");
    std::fs::write(&first, a).unwrap();
    std::fs::write(&second, b).unwrap();
    for p in [&first, &second] {
        let i = inspect(p);
        assert_eq!(i.container, Container::MultiVolume, "{p:?}");
        assert_eq!(
            i.issue,
            Some(ArchiveIssue::MultiVolume { parts: 2 }),
            "{p:?}"
        );
    }
}

/// A tar.gz whose listing outruns the plan listing budget after its
/// first entry: the listing is incomplete and the item is not fully read.
fn spent_tar_gz(dir: &std::path::Path) -> (std::path::PathBuf, Limits) {
    let p = dir.join("Game.tar.gz");
    let pad: Vec<u8> = (0..2 * 1024 * 1024u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    std::fs::write(
        &p,
        tar_gz(
            &[("Game.gba", &fx::gba()), ("pad.dat", &pad), ("z.txt", b"z")],
            0,
        ),
    )
    .unwrap();
    let limits = Limits {
        plan_listing_budget: 256 * 1024,
        ..no_tools()
    };
    (p, limits)
}

#[test]
fn budget_spent_goes_to_review_not_left() {
    use coverly_core::organize::evidence::Verdict;
    use coverly_core::organize::plan::ItemNote;

    let tmp = tempfile::tempdir().unwrap();
    let (p, limits) = spent_tar_gz(tmp.path());
    let i = inspect_limited(&p, &limits);
    assert_eq!(i.issue, None, "a spent budget is never damage");
    assert!(i.budget_spent);
    assert!(!i.listing_complete);
    assert_eq!(i.payloads[0].sniff.hits[0].system, "gba");

    let id = identify_offline(&p, i);
    assert_eq!(id.left, None);
    assert_eq!(id.combined.verdict, Verdict::Review);
    assert_eq!(id.combined.system.as_deref(), Some("gba"));
    assert!(id.notes.contains(&ItemNote::ArchiveNotFullyRead));
}

#[test]
fn a_mismatched_container_becomes_an_item_note() {
    use coverly_core::organize::plan::ItemNote;

    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.zip");
    std::fs::write(
        &p,
        sevenz(&[("Game.gba", &fx::gba())], SevenZShape::non_solid_lzma()),
    )
    .unwrap();
    let id = identify_offline(&p, inspect(&p));
    assert_eq!(id.left, None);
    assert_eq!(id.combined.system.as_deref(), Some("gba"));
    assert!(id.notes.contains(&ItemNote::ContainerMismatch {
        ext: "zip".into(),
        actual: "7z".into()
    }));
}

#[test]
fn a_crc_decode_cut_by_the_budget_keeps_the_header_sniff() {
    // A tar stores no CRC, so a ROM-like entry within `CRC_DECODE_CAP` is
    // decoded whole for one. When the plan head budget runs out after the
    // header was sniffed, the header evidence stays; only the CRC is lost.
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.tar.gz");
    let mut rom = fx::gba();
    rom.resize(12 * 1024 * 1024, 0);
    std::fs::write(&p, tar_gz(&[("Game.gba", &rom)], 0)).unwrap();
    let limits = Limits {
        plan_head_budget: 9 * 1024 * 1024,
        ..no_tools()
    };
    let i = inspect_limited(&p, &limits);
    assert_eq!(i.issue, None, "a spent budget is never damage");
    assert!(i.budget_spent);
    assert!(i.listing_complete, "the listing itself was whole");
    assert_eq!(i.payloads.len(), 1);
    assert_eq!(i.payloads[0].crc32, None);
    assert_eq!(i.payloads[0].sniff.hits[0].system, "gba");
}

/// A RAR v4 signature and nothing a tool could list.
fn rar_stub(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("Game.rar");
    let mut bytes = b"Rar!\x1a\x07\x00".to_vec();
    bytes.extend_from_slice(&[0u8; 64]);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn tools_in(bin: &std::path::Path, avoid: &[&std::path::Path]) -> ToolsOnDemand {
    let search = Search {
        path_var: Some(std::env::join_paths([bin.as_os_str()]).unwrap()),
        cwd: None,
        fixed: Vec::new(),
    };
    ToolsOnDemand::with_search(search, avoid)
}

#[test]
fn a_tool_inside_the_plan_roots_is_never_run() {
    // Spec §5.2: a `PATH` entry inside the source or target root is never
    // searched, so an `unrar` planted beside the ROMs is not run.
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("ROMs");
    let bin = source.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    fake_tool(
        &bin,
        "unrar",
        &FakeScript::banner("UNRAR 7.01 freeware      Copyright (c) 1993-2024 Alexander Roshal"),
    );
    let p = rar_stub(&source);
    let len = std::fs::metadata(&p).unwrap().len();
    let limits = Limits::default();
    assert!(limits.external_tools);

    let avoided = tools_in(&bin, &[source.as_path()]);
    let i = inspect_file_with(&p, len, &SystemDb::builtin(), &limits, Some(&avoided)).unwrap();
    assert_eq!(
        i.issue,
        Some(ArchiveIssue::Unsupported {
            format: Some("RAR".into()),
            need: Some(Tool::Unrar)
        })
    );

    // The same folder outside the roots is searched: the tool is used.
    let allowed = tools_in(&bin, &[]);
    let i = inspect_file_with(&p, len, &SystemDb::builtin(), &limits, Some(&allowed)).unwrap();
    assert_ne!(
        i.issue,
        Some(ArchiveIssue::Unsupported {
            format: Some("RAR".into()),
            need: Some(Tool::Unrar)
        })
    );
}

#[test]
fn tools_are_looked_for_only_when_an_archive_needs_one() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.zip");
    zip_file(&p, &[("Game.gba", &fx::gba(), false)]);
    let len = std::fs::metadata(&p).unwrap().len();
    let tools = tools_in(tmp.path(), &[]);
    let i = inspect_file_with(
        &p,
        len,
        &SystemDb::builtin(),
        &Limits::default(),
        Some(&tools),
    )
    .unwrap();
    assert_eq!(i.issue, None);
    assert!(!tools.discovered(), "a zip needs no external tool");

    let rar = rar_stub(tmp.path());
    let len = std::fs::metadata(&rar).unwrap().len();
    let _ = inspect_file_with(
        &rar,
        len,
        &SystemDb::builtin(),
        &Limits::default(),
        Some(&tools),
    );
    assert!(tools.discovered());
}

// --- Archives spec §6.2: disc games inside archives (Task 25) ---

mod discs {
    use super::*;
    use coverly_core::archive::testing::SevenZCoder;
    use coverly_core::disc::sheets::MAX_SHEET_BYTES;
    use coverly_core::disc::testing as dt;
    use coverly_core::organize::evidence::Verdict;
    use coverly_core::organize::inspect::{DiscSetInfo, FoundSystem, MAX_DISC_SHEETS};
    use coverly_core::organize::plan::{ItemNote, LeftReason, SheetCheck};

    const MIB: usize = 1024 * 1024;

    /// A cue sheet over `files`, one track per file (`MODE2/2352` for the
    /// first, `AUDIO` for the rest).
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

    /// `bytes` padded with zeros to `len`.
    fn padded(mut bytes: Vec<u8>, len: usize) -> Vec<u8> {
        bytes.resize(len.max(bytes.len()), 0);
        bytes
    }

    /// Bytes LZMA cannot shrink much: a solid prefix that really has to
    /// be decoded to reach what follows.
    fn noise(len: usize) -> Vec<u8> {
        (0..len as u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect()
    }

    fn write(dir: &std::path::Path, name: &str, bytes: Vec<u8>) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn set_of(i: &Inspection) -> &DiscSetInfo {
        i.disc_set.as_ref().expect("a disc archive")
    }

    fn known(i: &Inspection) -> (String, Vec<String>) {
        let found = set_of(i).identity.as_ref().expect("an identity");
        match &found.system {
            FoundSystem::Known { id } => (id.clone(), found.id.serials.clone()),
            other => panic!("unsupported {other:?}"),
        }
    }

    /// Limits with a plan head budget of `head` bytes (the 64 MiB budget
    /// scaled down with the archive, so tests stay small).
    fn head_budget(head: usize) -> Limits {
        Limits {
            plan_head_budget: head as u64,
            ..no_tools()
        }
    }

    #[test]
    fn ps1_7z_named_zip_cue_two_tracks_identified() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = "Next Tetris, The (USA)";
        let t1 = format!("{dir}/Next Tetris, The (USA) (Track 01).bin");
        let t2 = format!("{dir}/Next Tetris, The (USA) (Track 02).bin");
        let sheet = format!("{dir}/Next Tetris, The (USA).cue");
        let track = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");
        let p = write(
            tmp.path(),
            "Next Tetris, The.zip",
            sevenz(
                &[
                    (
                        &sheet,
                        &cue(&[
                            "Next Tetris, The (USA) (Track 01).bin",
                            "Next Tetris, The (USA) (Track 02).bin",
                        ]),
                    ),
                    (&t1, &track),
                    (&t2, &[0u8; 2352 * 4]),
                ],
                SevenZShape::non_solid_lzma(),
            ),
        );
        let i = inspect(&p);
        assert_eq!(i.container, Container::SevenZip);
        assert_eq!(i.issue, None);
        assert_eq!(i.mismatch, Some(("zip".to_string(), Format::SevenZip)));
        let set = set_of(&i);
        assert_eq!(set.sheet_check, SheetCheck::Read);
        assert!(set.extractable);
        assert!(!set.multi_disc);
        assert_eq!(set.sheets, [sheet.as_str()]);
        assert_eq!(set.members, [sheet, t1.clone(), t2]);
        assert_eq!(set.primary.as_deref(), Some(t1.as_str()));
        assert_eq!(known(&i), ("psx".into(), vec!["SLUS-00862".into()]));
        assert!(set.identity.as_ref().unwrap().id.volume.is_some());

        let id = identify_offline(&p, i);
        assert_eq!(id.left, None);
        assert_eq!(id.combined.verdict, Verdict::Auto);
        assert_eq!(id.combined.system.as_deref(), Some("psx"));
        assert!(id.notes.contains(&ItemNote::ContainerMismatch {
            ext: "zip".into(),
            actual: "7z".into()
        }));
        assert_eq!(id.disc.as_ref().unwrap().serials, ["SLUS-00862"]);
    }

    #[test]
    fn saturn_solid_sheet_last_deferred_identified() {
        // The real archive stores its `.cue` after 592 MB of tracks in one
        // solid LZMA2 folder; here 8 MiB of tracks against a 6 MiB head
        // budget stand in for 600 MB against 64 MiB.
        let tmp = tempfile::tempdir().unwrap();
        let names = [
            "Tetris Plus (Japan) (Track 01).bin",
            "Tetris Plus (Japan) (Track 02).bin",
            "Tetris Plus (Japan) (Track 03).bin",
        ];
        let t1 = padded(dt::saturn_track("T-5704H   "), MIB);
        let t2 = noise(4 * MIB);
        let t3 = noise(3 * MIB);
        let p = write(
            tmp.path(),
            "Tetris Plus.zip",
            sevenz(
                &[
                    ("Tetris Plus (Japan).cue", &cue(&names)),
                    (names[0], &t1),
                    (names[1], &t2),
                    (names[2], &t3),
                ],
                SevenZShape {
                    solid: true,
                    coder: SevenZCoder::Lzma2 { dict: 1 << 20 },
                    sheets_last: true,
                },
            ),
        );
        let budget = 6 * MIB;
        let i = inspect_limited(&p, &head_budget(budget));
        assert_eq!(i.issue, None);
        let set = set_of(&i);
        assert_eq!(set.sheet_check, SheetCheck::Deferred);
        assert!(set.extractable, "the Redump pattern names every track");
        assert_eq!(set.members.len(), 4);
        assert_eq!(set.primary.as_deref(), Some(names[0]));
        assert_eq!(known(&i), ("saturn".into(), vec!["T-5704H".into()]));
        assert!(i.decoded <= budget as u64, "spent {}", i.decoded);
        assert!(
            i.decoded < (t1.len() + t2.len() + t3.len()) as u64,
            "never decoded to reach the sheet"
        );

        let id = identify_offline(&p, i);
        assert_eq!(id.left, None);
        assert_eq!(id.combined.system.as_deref(), Some("saturn"));
        assert_eq!(id.combined.verdict, Verdict::Auto);
        assert!(id.notes.contains(&ItemNote::SheetNotReadYet));
    }

    /// The GD-ROM of the real case: track 01 repeats an IP.BIN (a decoy
    /// serial here), track 03 is the high-density data track.
    fn gd_rom() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        (
            dt::dreamcast_track03("T-99999N  ", "1/1"),
            vec![0u8; 2352 * 4],
            dt::dreamcast_track03("T40214N   ", "1/1"),
        )
    }

    const GDI: &[u8] =
        b"3\r\n1 0 4 2352 track01.bin 0\r\n2 756 0 2352 track02.raw 0\r\n3 45000 4 2352 track03.bin 0\r\n";

    const GD_CUE: &[u8] = b"REM SINGLE-DENSITY AREA\r\n\
        FILE \"track01.bin\" BINARY\r\n  TRACK 01 MODE1/2352\r\n    INDEX 01 00:00:00\r\n\
        FILE \"track02.raw\" BINARY\r\n  TRACK 02 AUDIO\r\n    INDEX 01 00:00:00\r\n\
        REM HIGH-DENSITY AREA\r\n\
        FILE \"track03.bin\" BINARY\r\n  TRACK 03 MODE1/2352\r\n    INDEX 01 00:00:00\r\n";

    #[test]
    fn dreamcast_cue_and_gdi_twins_one_disc() {
        let tmp = tempfile::tempdir().unwrap();
        let (t1, t2, t3) = gd_rom();
        let p = write(
            tmp.path(),
            "Next Tetris, The_ On-Line Edition.zip",
            sevenz(
                &[
                    ("Tetris Online.cue", GD_CUE),
                    ("Tetris Online.gdi", GDI),
                    ("track01.bin", &t1),
                    ("track02.raw", &t2),
                    ("track03.bin", &t3),
                ],
                SevenZShape::non_solid_lzma(),
            ),
        );
        let i = inspect(&p);
        assert_eq!(i.issue, None, "two sheets of one disc share no track");
        let set = set_of(&i);
        assert_eq!(set.sheet_check, SheetCheck::Read);
        assert!(!set.multi_disc);
        assert_eq!(set.twins, ["Tetris Online.cue", "Tetris Online.gdi"]);
        assert_eq!(set.members.len(), 5);
        assert_eq!(set.primary.as_deref(), Some("track03.bin"));
        assert_eq!(known(&i), ("dreamcast".into(), vec!["T-40214N".into()]));

        let id = identify_offline(&p, i);
        assert_eq!(id.combined.system.as_deref(), Some("dreamcast"));
        assert_eq!(id.combined.verdict, Verdict::Auto);
        assert!(id.notes.contains(&ItemNote::TwinSheets {
            kept: vec!["Tetris Online.cue".into(), "Tetris Online.gdi".into()]
        }));
    }

    /// Four Redump-named tracks of a GD-ROM (track 04 is 3 MiB of data a
    /// solid decoder must go through to reach the sheets stored last).
    fn gd_tracks(stem: &str) -> (Vec<String>, Vec<Vec<u8>>) {
        let (t1, t2, t3) = gd_rom();
        let names = (1..=4).map(|n| format!("{stem} (Track {n}).bin")).collect();
        (names, vec![t1, t2, t3, noise(3 * MIB)])
    }

    #[test]
    fn dreamcast_solid_sheets_last_deferred_reads_track_03() {
        // The real Dreamcast archive: one solid folder, `.cue` and `.gdi`
        // stored after every track. Both sheets are inferred from the
        // Redump names, they are twins, and the identity is track 03's.
        let tmp = tempfile::tempdir().unwrap();
        let stem = "Next Tetris, The - On-Line Edition (USA)";
        let (names, data) = gd_tracks(stem);
        let (cue_name, gdi_name) = (format!("{stem}.cue"), format!("{stem}.gdi"));
        let mut files: Vec<(&str, &[u8])> =
            vec![(&cue_name, b"REM unread\r\n"), (&gdi_name, b"4\r\n")];
        files.extend(
            names
                .iter()
                .map(String::as_str)
                .zip(data.iter().map(Vec::as_slice)),
        );
        let p = write(
            tmp.path(),
            "Next Tetris, The_ On-Line Edition.zip",
            sevenz(&files, SevenZShape::solid_lzma2_sheets_last()),
        );
        let i = inspect_limited(&p, &head_budget(2 * MIB));
        assert_eq!(i.issue, None);
        let set = set_of(&i);
        assert_eq!(set.sheet_check, SheetCheck::Deferred);
        assert!(set.extractable);
        assert_eq!(set.twins, [cue_name, gdi_name]);
        assert_eq!(set.members.len(), 6);
        assert_eq!(set.primary.as_deref(), Some(names[2].as_str()));
        assert_eq!(known(&i), ("dreamcast".into(), vec!["T-40214N".into()]));
        assert!(i.decoded <= 2 * MIB as u64);
    }

    #[test]
    fn a_deferred_cue_of_a_gd_rom_reads_track_03_not_track_01() {
        // No `.gdi`: the inferred cue's track 01 names a Dreamcast, so its
        // track 03 is read for the real serial.
        let tmp = tempfile::tempdir().unwrap();
        let (names, data) = gd_tracks("Game");
        let mut files: Vec<(&str, &[u8])> = vec![("Game.cue", b"REM unread\r\n")];
        files.extend(
            names
                .iter()
                .map(String::as_str)
                .zip(data.iter().map(Vec::as_slice)),
        );
        let p = write(
            tmp.path(),
            "Game.7z",
            sevenz(&files, SevenZShape::solid_lzma2_sheets_last()),
        );
        let i = inspect_limited(&p, &head_budget(2 * MIB));
        let set = set_of(&i);
        assert_eq!(set.sheet_check, SheetCheck::Deferred);
        assert_eq!(set.primary.as_deref(), Some(names[2].as_str()));
        assert_eq!(known(&i), ("dreamcast".into(), vec!["T-40214N".into()]));
    }

    #[test]
    fn non_solid_sheet_read_at_plan_time() {
        // Every file its own block: the sheet stored last costs only its
        // own bytes, so it is read even after more tracks than the budget.
        let tmp = tempfile::tempdir().unwrap();
        let names = ["Game (USA) (Track 01).bin", "Game (USA) (Track 02).bin"];
        let p = write(
            tmp.path(),
            "Game.7z",
            sevenz(
                &[
                    ("Game (USA).cue", &cue(&names)),
                    (
                        names[0],
                        &padded(dt::psx_track(true, "cdrom:\\SLUS_008.62;1"), MIB),
                    ),
                    (names[1], &noise(4 * MIB)),
                ],
                SevenZShape {
                    solid: false,
                    coder: SevenZCoder::Lzma { dict: 1 << 20 },
                    sheets_last: true,
                },
            ),
        );
        let i = inspect_limited(&p, &head_budget(3 * MIB));
        assert_eq!(i.issue, None);
        let set = set_of(&i);
        assert_eq!(set.sheet_check, SheetCheck::Read);
        assert!(set.extractable);
        assert_eq!(known(&i).0, "psx");
    }

    #[test]
    fn deferred_without_redump_pattern_is_review() {
        let tmp = tempfile::tempdir().unwrap();
        let p = write(
            tmp.path(),
            "Game.7z",
            sevenz(
                &[
                    ("Game.cue", &cue(&["a.bin", "b.bin"])),
                    (
                        "a.bin",
                        &padded(dt::psx_track(true, "cdrom:\\SLUS_008.62;1"), MIB),
                    ),
                    ("b.bin", &noise(4 * MIB)),
                ],
                SevenZShape::solid_lzma2_sheets_last(),
            ),
        );
        let i = inspect_limited(&p, &head_budget(3 * MIB));
        assert_eq!(i.issue, None);
        let set = set_of(&i);
        assert_eq!(set.sheet_check, SheetCheck::Deferred);
        assert!(!set.extractable, "no pattern: the members are a guess");

        let id = identify_offline(&p, i);
        assert_eq!(id.left, None);
        assert_eq!(id.combined.verdict, Verdict::Review);
        assert!(!id.notes.contains(&ItemNote::SheetNotReadYet));
        assert!(id.notes.contains(&ItemNote::ArchiveNotFullyRead));
    }

    #[test]
    fn sheet_dotdot_reference_outside_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let track = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");
        for reference in ["../Game.bin", "/abs/Game.bin", "C:\\Games\\Game.bin"] {
            let p = write(
                tmp.path(),
                "Game.7z",
                sevenz(
                    &[
                        ("Game/Game.cue", &cue(&[reference])),
                        ("Game/Game.bin", &track),
                    ],
                    SevenZShape::non_solid_lzma(),
                ),
            );
            let i = inspect(&p);
            assert_eq!(i.issue, Some(ArchiveIssue::SetOutsideFolder), "{reference}");
            assert_eq!(
                identify_offline(&p, i).left,
                Some(LeftReason::SetOutsideFolder),
                "{reference}"
            );
        }
    }

    #[test]
    fn missing_track_set_incomplete() {
        let tmp = tempfile::tempdir().unwrap();
        let p = write(
            tmp.path(),
            "Game.zip",
            sevenz(
                &[
                    (
                        "Game.cue",
                        &cue(&["Game (Track 1).bin", "Game (Track 2).bin"]),
                    ),
                    (
                        "game (track 1).BIN",
                        &dt::psx_track(true, "cdrom:\\SLUS_008.62;1"),
                    ),
                ],
                SevenZShape::non_solid_lzma(),
            ),
        );
        let i = inspect(&p);
        let missing = vec!["Game (Track 2).bin".to_string()];
        assert_eq!(
            i.issue,
            Some(ArchiveIssue::SetIncomplete {
                missing: missing.clone()
            }),
            "track 1 resolves case-insensitively, track 2 is missing"
        );
        assert_eq!(
            identify_offline(&p, i).left,
            Some(LeftReason::SetIncomplete { missing })
        );
    }

    #[test]
    fn thousands_of_sheets_are_a_collection_judged_quickly() {
        // 4,000 unreadable sheets beside 4,000 tracks in one directory:
        // each guessed set would hold every track, and pairing them for
        // twins or shared tracks grows with the square of the sheets.
        // A disc archive with that many sheets is a collection, judged
        // from its listing alone.
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("Pile.zip");
        let count = 4_000;
        let names: Vec<(String, String)> = (0..count)
            .map(|n| (format!("Game {n}.cue"), format!("Game {n}.bin")))
            .collect();
        let mut entries: Vec<(&str, &[u8], bool)> = Vec::new();
        for (sheet, track) in &names {
            entries.push((sheet.as_str(), &b"\0not a sheet"[..], false));
            entries.push((track.as_str(), &b""[..], false));
        }
        zip_file(&p, &entries);
        let started = std::time::Instant::now();
        let i = inspect(&p);
        assert_eq!(i.issue, Some(ArchiveIssue::SeveralGames { count }));

        // Playlists count too: each is read and matched against every disc.
        let p = tmp.path().join("Playlists.zip");
        let lists: Vec<String> = (0..count).map(|n| format!("Game {n}.m3u")).collect();
        let mut entries: Vec<(&str, &[u8], bool)> = vec![("Game 0.cue", &b"\0"[..], false)];
        for list in &lists {
            entries.push((list.as_str(), &b"Game 0.cue\n"[..], false));
        }
        zip_file(&p, &entries);
        let i = inspect(&p);
        assert_eq!(
            i.issue,
            Some(ArchiveIssue::SeveralGames { count: count + 1 })
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
        let p = tmp.path().join("Pile.zip");
        let i = inspect(&p);
        assert_eq!(i.disc_set, None);
        assert_eq!(i.decoded, 0, "no sheet is read");
        assert_eq!(
            identify_offline(&p, i).left,
            Some(LeftReason::MultiGameArchive { count })
        );
    }

    #[test]
    fn a_full_listing_with_discs_under_the_cap_is_judged_quickly() {
        // Past the cap, a thousand sheets in a listing of the most entries
        // an archive may hold are a collection at once: the time this
        // takes is the cost of the listing alone.
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("Thousand.zip");
        let mut names: Vec<String> = (0..1_000).map(|n| format!("Game {n}.cue")).collect();
        let mut n = 0;
        while names.len() < MAX_ZIP_ENTRIES {
            names.push(format!("Game {n}.bin"));
            n += 1;
        }
        let entries: Vec<(&str, &[u8], bool)> = names
            .iter()
            .map(|name| (name.as_str(), &b"\0"[..], false))
            .collect();
        zip_file(&p, &entries);
        let started = std::time::Instant::now();
        let i = inspect(&p);
        let listing = started.elapsed();
        assert_eq!(i.issue, Some(ArchiveIssue::SeveralGames { count: 1_000 }));
        assert!(
            listing < std::time::Duration::from_secs(2),
            "took {listing:?}"
        );

        // Under the cap, as many discs as one archive may hold in the same
        // full listing: sheets too large to read (their set inferred, then
        // their directory guessed) and sheetless images beside thousands
        // of image companions. Each disc looks its files up once instead
        // of folding every listed name again, so judging them costs about
        // what the listing does.
        let p = tmp.path().join("Full.zip");
        let big = vec![b' '; MAX_SHEET_BYTES + 1];
        let mut names: Vec<(String, &[u8])> = Vec::new();
        for n in 0..MAX_DISC_SHEETS / 2 {
            names.push((format!("Big/Large {n}.cue"), &big[..]));
        }
        for n in 0..MAX_DISC_SHEETS - names.len() {
            names.push((format!("Image {n}.iso"), &b""[..]));
        }
        let mut n = 0;
        while names.len() < MAX_ZIP_ENTRIES {
            names.push((format!("Image {n}.sub"), &b""[..]));
            n += 1;
        }
        let entries: Vec<(&str, &[u8], bool)> = names
            .iter()
            .map(|(name, bytes)| (name.as_str(), *bytes, false))
            .collect();
        zip_file(&p, &entries);
        let started = std::time::Instant::now();
        let i = inspect(&p);
        let took = started.elapsed();
        assert_eq!(
            i.issue,
            Some(ArchiveIssue::SeveralGames {
                count: MAX_DISC_SHEETS
            })
        );
        assert!(
            took < listing * 3 + std::time::Duration::from_millis(50),
            "took {took:?}, the listing alone {listing:?}"
        );
    }

    #[test]
    fn judging_discs_past_the_wall_time_is_not_fully_read() {
        // Two sheetless images read nothing: only the wall time stops the
        // judging, and a plan past it is not fully read, never a verdict.
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("Two.zip");
        zip_file(
            &p,
            &[
                ("Alpha.chd", &b""[..], false),
                ("Beta.chd", &b""[..], false),
            ],
        );
        let i = inspect_limited(&p, &no_tools());
        assert_eq!(i.issue, Some(ArchiveIssue::SeveralGames { count: 2 }));
        assert!(!i.budget_spent);
        let late = Limits {
            plan_wall_time: std::time::Duration::ZERO,
            ..no_tools()
        };
        let i = inspect_limited(&p, &late);
        assert!(i.budget_spent);
        assert!(i.listing_complete, "the listing itself was whole");
        assert_eq!((i.issue.clone(), i.disc_set.clone()), (None, None));
        assert!(identify_offline(&p, i)
            .notes
            .contains(&ItemNote::ArchiveNotFullyRead));
    }

    #[test]
    fn a_track_of_two_discs_is_shared() {
        let tmp = tempfile::tempdir().unwrap();
        let p = write(
            tmp.path(),
            "Game.7z",
            sevenz(
                &[
                    ("Game (Disc 1).cue", &cue(&["Game.bin"])),
                    (
                        "Game (Disc 2).cue",
                        &cue(&["Game.bin", "Game (Disc 2).bin"]),
                    ),
                    ("Game.bin", &dt::psx_track(true, "cdrom:\\SLUS_008.62;1")),
                    ("Game (Disc 2).bin", &[0u8; 2352]),
                ],
                SevenZShape::non_solid_lzma(),
            ),
        );
        let i = inspect(&p);
        assert_eq!(i.issue, Some(ArchiveIssue::SharedTrack));
        assert_eq!(identify_offline(&p, i).left, Some(LeftReason::SharedTrack));
    }

    #[test]
    fn two_disc_one_game_one_item() {
        let tmp = tempfile::tempdir().unwrap();
        let d1 = "Final Game (USA) (Disc 1).bin";
        let d2 = "Final Game (USA) (Disc 2).bin";
        let p = write(
            tmp.path(),
            "Final Game.7z",
            sevenz(
                &[
                    ("Final Game (USA) (Disc 1).cue", &cue(&[d1])),
                    ("Final Game (USA) (Disc 2).cue", &cue(&[d2])),
                    (d1, &dt::psx_track(true, "cdrom:\\SLUS_008.62;1")),
                    (d2, &dt::psx_track(true, "cdrom:\\SLUS_008.63;1")),
                ],
                SevenZShape::non_solid_lzma(),
            ),
        );
        let i = inspect(&p);
        assert_eq!(i.issue, None);
        let set = set_of(&i);
        assert!(set.multi_disc);
        assert_eq!(set.members.len(), 4);
        assert_eq!(set.primary.as_deref(), Some(d1));
        assert_eq!(known(&i), ("psx".into(), vec!["SLUS-00862".into()]));
        let id = identify_offline(&p, i);
        assert_eq!(id.left, None);
        assert_eq!(id.combined.system.as_deref(), Some("psx"));
    }

    #[test]
    fn two_games_multi_game_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let p = write(
            tmp.path(),
            "Two.7z",
            sevenz(
                &[
                    ("Alpha (USA).cue", &cue(&["Alpha (USA).bin"])),
                    ("Beta (USA).cue", &cue(&["Beta (USA).bin"])),
                    (
                        "Alpha (USA).bin",
                        &dt::psx_track(true, "cdrom:\\SLUS_008.62;1"),
                    ),
                    ("Beta (USA).bin", &dt::saturn_track("T-5704H   ")),
                ],
                SevenZShape::non_solid_lzma(),
            ),
        );
        let i = inspect(&p);
        assert_eq!(i.issue, Some(ArchiveIssue::SeveralGames { count: 2 }));
        assert_eq!(
            identify_offline(&p, i).left,
            Some(LeftReason::MultiGameArchive { count: 2 })
        );
    }

    #[test]
    fn primary_track_beyond_budget_name_only_review() {
        // The gdi is read (first in the solid block), but track 03 lies
        // after more bytes than the budget: judged by name and extension.
        let tmp = tempfile::tempdir().unwrap();
        let (_, t2, t3) = gd_rom();
        let p = write(
            tmp.path(),
            "Tetris Online.7z",
            sevenz(
                &[
                    ("Tetris Online.gdi", GDI),
                    ("track01.bin", &noise(4 * MIB)),
                    ("track02.raw", &t2),
                    ("track03.bin", &t3),
                ],
                SevenZShape {
                    solid: true,
                    coder: SevenZCoder::Lzma2 { dict: 1 << 20 },
                    sheets_last: false,
                },
            ),
        );
        let i = inspect_limited(&p, &head_budget(2 * MIB));
        assert_eq!(i.issue, None);
        assert!(i.budget_spent);
        let set = set_of(&i);
        assert_eq!(set.sheet_check, SheetCheck::Read);
        assert_eq!(set.primary.as_deref(), Some("track03.bin"));
        assert_eq!(set.identity, None);

        let id = identify_offline(&p, i);
        assert_eq!(id.left, None);
        assert_eq!(id.combined.verdict, Verdict::Review);
        assert_eq!(id.combined.system.as_deref(), Some("dreamcast"));
        assert!(id.notes.contains(&ItemNote::ArchiveNotFullyRead));
    }

    #[test]
    fn a_ps2_disc_in_an_archive_is_unsupported() {
        let tmp = tempfile::tempdir().unwrap();
        let p = write(
            tmp.path(),
            "Game.7z",
            sevenz(
                &[("Game.iso", &dt::ps2_iso())],
                SevenZShape::non_solid_lzma(),
            ),
        );
        let i = inspect(&p);
        assert!(matches!(
            set_of(&i).identity.as_ref().unwrap().system,
            FoundSystem::Unsupported { .. }
        ));
        assert_eq!(
            identify_offline(&p, i).left,
            Some(LeftReason::UnsupportedSystem {
                name: "PlayStation 2".into()
            })
        );
    }

    #[test]
    fn cartridge_disks_with_an_m3u_stay_cartridges() {
        // An m3u alone (floppy playlists) does not make a disc archive.
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("Game.zip");
        zip_file(
            &p,
            &[
                ("Game.m3u", b"Game (Disk 1).adf\n", false),
                ("Game (Disk 1).adf", &[0u8; 901_120], false),
            ],
        );
        let i = inspect(&p);
        assert_eq!(i.disc_set, None);
        assert!(i.payloads.iter().any(|p| p.ext == "adf"));
    }
}

#[test]
fn package_exts_are_raw_items() {
    // D25: zip-based game packages are one raw file, never extracted.
    let tmp = tempfile::tempdir().unwrap();
    let arduboy = tmp.path().join("Game.arduboy");
    zip_file(
        &arduboy,
        &[
            ("info.json", b"{\"title\":\"Game\"}", true),
            ("game.hex", b":00000001FF\n", true),
        ],
    );
    let rpk = tmp.path().join("Game.rpk");
    zip_file(
        &rpk,
        &[
            ("layout.xml", b"<romset></romset>", true),
            ("game.bin", &[0x55u8; 64], false),
        ],
    );
    for (p, ext) in [(&arduboy, "arduboy"), (&rpk, "rpk")] {
        let i = inspect(p);
        assert_eq!(i.container, Container::Raw, "{ext}");
        assert_eq!(i.issue, None, "{ext}");
        assert!(i.entries.is_empty(), "{ext}: nothing listed");
        assert_eq!(i.payloads.len(), 1, "{ext}");
        assert_eq!(i.payloads[0].ext, ext);
        assert!(i.disc_set.is_none());
    }
    assert!(coverly_core::organize::inspect::PACKAGE_EXTS.contains(&"uef"));
}
