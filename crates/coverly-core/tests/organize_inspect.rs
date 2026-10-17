//! Opening organise candidates (spec §7.2): raw files, zip entries with
//! bounded reads, 7z/CHD/RAR magic. Fixtures are synthetic.

mod common;

use common::roms::{zip_encrypted, zip_file};
use coverly_core::identify::sniff::fixtures as fx;
use coverly_core::identify::sniff::NotARom;
use coverly_core::organize::inspect::{
    inspect_file, inspect_set_primary, ArchiveIssue, Container, Inspection, MAX_ZIP_ENTRIES,
};
use coverly_core::systems::SystemDb;

fn inspect(path: &std::path::Path) -> Inspection {
    let len = std::fs::metadata(path).unwrap().len();
    inspect_file(path, len, &SystemDb::builtin()).unwrap()
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
    assert_eq!(i.entries[0].crc32, crc_hex(b"hello"));
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
fn more_than_the_entry_cap_is_damaged() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("huge.zip");
    let names: Vec<String> = (0..=MAX_ZIP_ENTRIES).map(|i| format!("f{i}.txt")).collect();
    let entries: Vec<(&str, &[u8], bool)> = names
        .iter()
        .map(|n| (n.as_str(), &b""[..], false))
        .collect();
    zip_file(&p, &entries);
    let i = inspect(&p);
    assert_eq!(i.issue, Some(ArchiveIssue::Damaged));
    assert!(i.entries.is_empty());
}

#[test]
fn encrypted_entries_are_noted_and_not_sniffed() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("locked.zip");
    let rom = fx::gba();
    zip_encrypted(&p, "Game.gba", &rom);
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
fn seven_zip_chd_and_rar_are_recognised_only() {
    let tmp = tempfile::tempdir().unwrap();
    let cases: [(&str, &[u8], Container, Option<ArchiveIssue>); 5] = [
        (
            "a.7z",
            &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 4],
            Container::SevenZip,
            None,
        ),
        (
            "b.7z",
            b"not a 7z at all",
            Container::SevenZip,
            Some(ArchiveIssue::Damaged),
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
            Some(ArchiveIssue::Unsupported),
        ),
    ];
    for (name, bytes, container, issue) in cases {
        let p = tmp.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        let i = inspect(&p);
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
}
