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
