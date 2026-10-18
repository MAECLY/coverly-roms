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

// --- Units (spec §5.1) ---

mod common;

use coverly_core::dedupe::units::{
    units_from_facts, units_from_scan, Exclusion, Unit, UnitOptions, UnitSet,
};
use coverly_core::dedupe::UnitKind;
use coverly_core::identify::dat::DatGame;
use coverly_core::organize::evidence::Facts;
use coverly_core::organize::inspect::{Container, Inspection, Payload};
use coverly_core::profiles::Profile;
use coverly_core::scan::{scan_library, LibraryScan, RomEntry, ScanOptions};
use coverly_core::systems::SystemDb;

/// Synthetic ROM bytes: binary (NUL bytes), distinct per `seed`.
fn rom_bytes(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| if i % 3 == 0 { 0 } else { (i as u8) ^ seed })
        .collect()
}

/// Writes `bytes` at `root/rel`, creating parent folders.
fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    p
}

fn scan_with(root: &Path, profile: &Profile, opts: &ScanOptions) -> LibraryScan {
    scan_library(root, profile, &SystemDb::builtin(), opts)
}

fn units_of(root: &Path) -> UnitSet {
    let profile = common::profile("anbernic-stock");
    let scan = scan_with(root, &profile, &ScanOptions::default());
    units_from_scan(
        &scan,
        &SystemDb::builtin(),
        &UnitOptions::for_profile(&profile),
    )
}

fn primaries(set: &UnitSet) -> Vec<PathBuf> {
    set.units.iter().map(|u| u.primary.clone()).collect()
}

fn excluded_as(set: &UnitSet, path: &Path) -> Option<Exclusion> {
    set.excluded
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, why)| *why)
}

fn unit_at<'a>(set: &'a UnitSet, path: &Path) -> &'a Unit {
    set.units
        .iter()
        .find(|u| u.primary == path)
        .unwrap_or_else(|| panic!("no unit at {}: {:?}", path.display(), primaries(set)))
}

#[test]
fn ports_bios_apps_art_and_saves_folders_are_excluded() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let rom = rom_bytes(1, 2048);
    put(root, "ROMS/PORTS/owlboy/owlboy.sh", b"#!/bin/sh\n");
    put(root, "ROMS/BIOS/gb_extra.bin", &rom);
    put(root, "ROMS/APPS/tool.sh", b"#!/bin/sh\n");
    let keep = put(root, "ROMS/GB/Tetris.gb", &rom);
    let art = put(root, "ROMS/GB/Imgs/Tetris.gb", &rom);
    let saves = put(root, "ROMS/GB/Saves/Tetris.gb", &rom);
    let states = put(root, "ROMS/GB/sub/states/Tetris.gb", &rom);

    // A scan that walks everything: the units still keep them out.
    let profile = common::profile("anbernic-stock");
    let open = Profile {
        art_dir_names: Vec::new(),
        ..profile.clone()
    };
    let opts = ScanOptions {
        ignored_dirs: Vec::new(),
        ..ScanOptions::default()
    };
    let scan = scan_with(root, &open, &opts);
    let set = units_from_scan(
        &scan,
        &SystemDb::builtin(),
        &UnitOptions::for_profile(&profile),
    );

    assert_eq!(primaries(&set), vec![keep]);
    for p in [&art, &saves, &states] {
        assert_eq!(
            excluded_as(&set, p),
            Some(Exclusion::Folder),
            "{}",
            p.display()
        );
    }
    for folder in ["PORTS", "BIOS", "APPS"] {
        assert_eq!(
            excluded_as(&set, &root.join("ROMS").join(folder)),
            Some(Exclusion::Folder),
            "{folder}"
        );
    }
}

#[test]
fn bios_twins_are_never_units() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let bios = rom_bytes(9, 4096);
    let a = put(root, "ROMS/PS/scph1001.bin", &bios);
    let b = put(root, "ROMS/PS/scph5501.bin", &bios);
    let c = put(root, "ROMS/PS/SCPH7003.BIN", &bios);
    let set = units_of(root);
    assert!(set.units.is_empty(), "{:?}", primaries(&set));
    for p in [&a, &b, &c] {
        assert_eq!(
            excluded_as(&set, p),
            Some(Exclusion::Bios),
            "{}",
            p.display()
        );
    }
}

#[test]
fn bios_by_dat_isbios_and_arcade_bios_sets() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let arcade = |name: &str, seed: u8| {
        let p = root.join("ROMS/FBNEO").join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        common::roms::zip_file(&p, &[("a.rom", &rom_bytes(seed, 512), false)]);
        p
    };
    let neogeo = arcade("neogeo.zip", 1);
    let pgm = arcade("pgm.zip", 2);
    let qsound = arcade("qsound.zip", 3);
    let skns = arcade("skns.zip", 4);
    let ffight = arcade("ffight.zip", 5);
    let sgb = put(
        root,
        "ROMS/SFC/[BIOS] Super Game Boy (World).sfc",
        &rom_bytes(6, 1024),
    );
    let dat_named = put(root, "ROMS/SFC/sufami.sfc", &rom_bytes(7, 1024));

    let profile = common::profile("anbernic-stock");
    let scan = scan_with(root, &profile, &ScanOptions::default());
    let mut opts = UnitOptions::for_profile(&profile);
    opts.add_dat_bios(
        "fbneo",
        &DatGame {
            name: "skns".into(),
            roms: Vec::new(),
            cloneof: None,
            region: None,
            is_bios: true,
        },
    );
    let sufami = coverly_core::identify::dat::parse_clrmamepro(
        "game ( name \"Sufami Turbo (Japan)\" isbios \"yes\" rom ( name \"sufami.sfc\" size 1024 crc 00000000 ) )",
    )
    .expect("tiny DAT parses")
    .remove(0);
    opts.add_dat_bios("snes", &sufami);
    // Not a BIOS in the DAT: ignored.
    opts.add_dat_bios(
        "fbneo",
        &DatGame {
            name: "ffight".into(),
            roms: Vec::new(),
            cloneof: None,
            region: None,
            is_bios: false,
        },
    );
    let set = units_from_scan(&scan, &SystemDb::builtin(), &opts);

    assert_eq!(primaries(&set), vec![ffight.clone()]);
    assert_eq!(unit_at(&set, &ffight).kind, UnitKind::Arcade);
    for p in [&neogeo, &pgm, &qsound, &skns, &sgb, &dat_named] {
        assert_eq!(
            excluded_as(&set, p),
            Some(Exclusion::Bios),
            "{}",
            p.display()
        );
    }
}

#[test]
fn zero_byte_and_html_named_zip_are_not_roms() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let empty = put(root, "ROMS/GB/Empty.gb", b"");
    let page = put(
        root,
        "ROMS/GB/Tetris.zip",
        b"<!DOCTYPE html>\n<html><body>404 Not Found</body></html>\n",
    );
    let raw = put(root, "ROMS/GB/Fake.zip", &rom_bytes(3, 2048));
    let web_gb = put(root, "ROMS/GB/Page.gb", b"<html><head></head></html>");
    let good = put(root, "ROMS/GB/Good.gb", &rom_bytes(4, 2048));
    let set = units_of(root);
    assert_eq!(primaries(&set), vec![good]);
    for p in [&empty, &page, &raw, &web_gb] {
        assert_eq!(
            excluded_as(&set, p),
            Some(Exclusion::NotARom),
            "{}",
            p.display()
        );
    }
}

#[test]
fn zips_are_units_by_their_rom_entries() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let gb = root.join("ROMS/GB");
    fs::create_dir_all(&gb).unwrap();
    let one = gb.join("One.zip");
    let rom = rom_bytes(5, 3000);
    common::roms::zip_file(
        &one,
        &[("readme.txt", b"hi", false), ("One.gb", &rom, true)],
    );
    let multi = gb.join("Multi.zip");
    common::roms::zip_file(
        &multi,
        &[
            ("A.gb", &rom_bytes(6, 100), false),
            ("B.gbc", &rom_bytes(7, 100), false),
        ],
    );
    let broken = put(root, "ROMS/GB/Broken.zip", b"PK\x03\x04 truncated");
    let set = units_of(root);

    let unit = unit_at(&set, &one);
    assert_eq!(unit.kind, UnitKind::Zip);
    assert_eq!(unit.payload_size, Some(rom.len() as u64));
    assert_eq!(unit.size, fs::metadata(&one).unwrap().len());
    assert_eq!(excluded_as(&set, &multi), Some(Exclusion::MultiGameArchive));
    assert_eq!(excluded_as(&set, &broken), Some(Exclusion::Unreadable));
}

#[cfg(unix)]
#[test]
fn symlink_is_not_a_unit_and_hard_links_are_one() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let target = put(root, "ROMS/GB/Tetris.gb", &rom_bytes(1, 2048));
    let link = root.join("ROMS/GB/Link.gb");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let hard = root.join("ROMS/GB/Hard.gb");
    fs::hard_link(&target, &hard).unwrap();
    let set = units_of(root);

    assert_eq!(
        primaries(&set),
        vec![hard.clone()],
        "first path of the twins stays"
    );
    assert!(unit_at(&set, &hard).file_id.is_some());
    assert_eq!(excluded_as(&set, &link), Some(Exclusion::Symlink));
    assert_eq!(excluded_as(&set, &target), Some(Exclusion::HardLinkTwin));
}

#[test]
fn cue_with_tracks_is_one_unit_and_tracks_are_not_units() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let t1 = put(root, "ROMS/PS/Game (Track 1).bin", &rom_bytes(1, 4704));
    let t2 = put(root, "ROMS/PS/Game (Track 2).bin", &rom_bytes(2, 2352));
    let cue = put(
        root,
        "ROMS/PS/Game.cue",
        b"FILE \"Game (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n\
          FILE \"Game (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n",
    );
    let set = units_of(root);
    assert_eq!(primaries(&set), vec![cue.clone()]);
    let unit = unit_at(&set, &cue);
    assert_eq!(unit.kind, UnitKind::DiscSet);
    assert_eq!(unit.files, vec![cue.clone(), t1, t2]);
    assert_eq!(unit.size, fs::metadata(&cue).unwrap().len() + 4704 + 2352);
    assert_eq!(unit.stem, "Game");
    assert_eq!(unit.ext, "cue");
    assert_eq!(unit.system, "psx");
    assert!(unit.in_library);
}

#[test]
fn two_cues_over_one_bin_merge_into_one_unit() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let bin = put(root, "ROMS/PS/Game.bin", &rom_bytes(1, 4704));
    let sheet = b"FILE \"Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n";
    let a = put(root, "ROMS/PS/Game.cue", sheet);
    let b = put(root, "ROMS/PS/Game (alt).cue", sheet);
    let set = units_of(root);
    assert_eq!(set.units.len(), 1, "{:?}", primaries(&set));
    let unit = &set.units[0];
    assert_eq!(unit.kind, UnitKind::DiscSet);
    let mut files = unit.files.clone();
    files.sort();
    let mut want = vec![a, b, bin];
    want.sort();
    assert_eq!(files, want);
    assert!(set.excluded.is_empty(), "{:?}", set.excluded);
}

#[test]
fn m3u_and_loose_cue_of_the_same_disc_merge() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let sheet = |bin: &str| format!("FILE \"{bin}\" BINARY\n  TRACK 01 MODE2/2352\n");
    put(root, "ROMS/PS/Game (Disc 1).bin", &rom_bytes(1, 2352));
    put(root, "ROMS/PS/Game (Disc 2).bin", &rom_bytes(2, 2352));
    put(
        root,
        "ROMS/PS/Game (Disc 1).cue",
        sheet("Game (Disc 1).bin").as_bytes(),
    );
    put(
        root,
        "ROMS/PS/Game (Disc 2).cue",
        sheet("Game (Disc 2).bin").as_bytes(),
    );
    let loose = put(
        root,
        "ROMS/PS/Game (Disc 1) [alt].cue",
        sheet("Game (Disc 1).bin").as_bytes(),
    );
    let m3u = put(
        root,
        "ROMS/PS/Game.m3u",
        b"Game (Disc 1).cue\nGame (Disc 2).cue\n",
    );
    let set = units_of(root);
    assert_eq!(primaries(&set), vec![m3u.clone()]);
    let unit = unit_at(&set, &m3u);
    assert_eq!(unit.kind, UnitKind::MultiDisc);
    assert_eq!(unit.files.len(), 6);
    assert!(unit.files.contains(&loose));
    assert_eq!(unit.files[0], m3u);
}

#[test]
fn ccd_img_sub_triple_is_one_unit() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let ccd = put(root, "ROMS/PS/Driver 2.ccd", b"[CloneCD]\nVersion=3\n");
    let img = put(root, "ROMS/PS/Driver 2.img", &rom_bytes(1, 4704));
    let sub = put(root, "ROMS/PS/Driver 2.sub", &rom_bytes(2, 96));
    let lone = put(root, "ROMS/PS/Other.img", &rom_bytes(3, 4704));
    let set = units_of(root);
    assert_eq!(primaries(&set), vec![ccd.clone(), lone.clone()]);
    let unit = unit_at(&set, &ccd);
    assert_eq!(unit.kind, UnitKind::DiscSet);
    assert_eq!(unit.files, vec![ccd, img, sub]);
    assert_eq!(unit_at(&set, &lone).kind, UnitKind::Image);
}

#[test]
fn soft_patches_attach_to_their_rom() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let game = put(root, "ROMS/SFC/Game.sfc", &rom_bytes(1, 2048));
    let ips = put(root, "ROMS/SFC/Game.ips", b"PATCH\0\0\0EOF");
    let other = put(root, "ROMS/SFC/Other.sfc", &rom_bytes(2, 2048));
    let bps = put(root, "ROMS/SFC/other.BPS", b"BPS1\0\0");
    let plain = put(root, "ROMS/SFC/Plain.sfc", &rom_bytes(3, 2048));
    let set = units_of(root);
    assert_eq!(unit_at(&set, &game).patches, vec![ips.clone()]);
    assert_eq!(unit_at(&set, &other).patches, vec![bps]);
    assert!(unit_at(&set, &plain).patches.is_empty());
    assert!(
        !unit_at(&set, &game).files.contains(&ips),
        "a patch is not the ROM"
    );
}

#[test]
fn quarantine_folder_is_never_walked() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let rom = rom_bytes(1, 2048);
    let keep = put(root, "ROMS/GB/x.gb", &rom);
    // A quarantine chosen inside a system folder.
    let inside = put(root, "ROMS/GB/_Duplicados/x.gb", &rom);
    let profile = common::profile("anbernic-stock");
    let scan = scan_with(root, &profile, &ScanOptions::default());
    let mut opts = UnitOptions::for_profile(&profile);
    opts.quarantine = Some(root.join("ROMS/GB/_duplicados"));
    let set = units_from_scan(&scan, &SystemDb::builtin(), &opts);
    assert_eq!(primaries(&set), vec![keep]);
    assert_eq!(excluded_as(&set, &inside), Some(Exclusion::Quarantine));

    // roms_root "": the quarantine sits among the system folders.
    let flat = TempDir::new().unwrap();
    let froot = flat.path();
    let gb = put(froot, "GB/x.gb", &rom);
    let parked = put(froot, "_Duplicados/GB/x.gb", &rom);
    let flat_profile = Profile {
        roms_root: String::new(),
        ..profile.clone()
    };
    let mut scan = scan_with(froot, &flat_profile, &ScanOptions::default());
    // Even a walk that took it as a system folder never yields its files.
    let mut fake = scan
        .systems
        .iter()
        .find(|s| s.folder_name == "GB")
        .unwrap()
        .clone();
    fake.dir = froot.join("_Duplicados/GB");
    fake.roms[0].path = parked.clone();
    scan.systems.push(fake);
    let mut opts = UnitOptions::for_profile(&flat_profile);
    opts.quarantine = Some(froot.join("_Duplicados"));
    let set = units_from_scan(&scan, &SystemDb::builtin(), &opts);
    assert_eq!(primaries(&set), vec![gb]);
    assert_eq!(excluded_as(&set, &parked), Some(Exclusion::Quarantine));
}

#[test]
fn apple_double_and_hidden_files_are_skipped() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let rom = rom_bytes(1, 2048);
    let keep = put(root, "ROMS/GB/Tetris.gb", &rom);
    let twin = put(root, "ROMS/GB/._Tetris.gb", &rom);
    let hidden = put(root, "ROMS/GB/.Tetris.gb", &rom);
    let profile = common::profile("anbernic-stock");
    let mut scan = scan_with(root, &profile, &ScanOptions::default());
    // Whatever the walk let through, units skip them.
    let gb = scan
        .systems
        .iter_mut()
        .find(|s| s.folder_name == "GB")
        .unwrap();
    let template: RomEntry = gb.roms[0].clone();
    for p in [&twin, &hidden] {
        gb.roms.push(RomEntry {
            path: p.clone(),
            stem: p.file_stem().unwrap().to_string_lossy().into_owned(),
            ..template.clone()
        });
    }
    let set = units_from_scan(
        &scan,
        &SystemDb::builtin(),
        &UnitOptions::for_profile(&profile),
    );
    assert_eq!(primaries(&set), vec![keep]);
    assert!(set.excluded.is_empty(), "{:?}", set.excluded);
}

#[test]
fn unit_ids_follow_scan_order_and_carry_metadata() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let a = put(root, "ROMS/GB/A.gb", &rom_bytes(1, 100));
    let b = put(root, "ROMS/GB/Sub/B.gb", &rom_bytes(2, 200));
    let set = units_of(root);
    assert_eq!(primaries(&set), vec![a.clone(), b.clone()]);
    assert_eq!(set.units[0].id, 0);
    assert_eq!(set.units[1].id, 1);
    let ub = unit_at(&set, &b);
    assert_eq!(ub.rel_dir, PathBuf::from("Sub"));
    assert_eq!(ub.kind, UnitKind::Single);
    assert_eq!(ub.payload_size, Some(200));
    assert_eq!(ub.size, 200);
    assert!(ub.mtime_ns > 0);
    assert_eq!(ub.to_ref().files, vec![b]);
}

#[test]
fn pile_units_come_from_organise_facts() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let raw = put(root, "pile/Tetris.gb", &rom_bytes(1, 2048));
    let zip = put(root, "pile/Tetris.zip", b"PK\x03\x04 stand-in");
    let multi = put(root, "pile/Multi.zip", b"PK\x03\x04 stand-in");
    let bios = put(root, "pile/gba_bios.bin", &rom_bytes(2, 1024));
    let unknown = put(root, "pile/what.bin", &rom_bytes(3, 1024));
    let payload = |name: &str, size: u64| Payload {
        name: name.into(),
        ext: name.rsplit_once('.').unwrap().1.into(),
        size,
        crc32: None,
        sniff: Default::default(),
    };
    let facts = |container: Container, payloads: Vec<Payload>| Facts {
        inspection: Inspection {
            container,
            payloads,
            entries: Vec::new(),
            issue: None,
            notes: Vec::new(),
            head_crc: None,
        },
        raw_crcs: Vec::new(),
        hashed: false,
    };
    let items = vec![
        (
            raw.clone(),
            facts(Container::Raw, vec![payload("Tetris.gb", 2048)]),
        ),
        (
            zip.clone(),
            facts(Container::Zip, vec![payload("Tetris.gb", 2048)]),
        ),
        (
            multi.clone(),
            facts(Container::Zip, vec![payload("A.gb", 1), payload("B.gb", 1)]),
        ),
        (
            bios.clone(),
            facts(Container::Raw, vec![payload("gba_bios.bin", 1024)]),
        ),
        (
            unknown.clone(),
            facts(Container::Raw, vec![payload("what.bin", 1024)]),
        ),
    ];
    let system_of = |i: usize| (i != 4).then(|| "gb".to_string());
    let set = units_from_facts(&items, &system_of);

    assert_eq!(primaries(&set), vec![raw.clone(), zip.clone()]);
    assert!(set.units.iter().all(|u| !u.in_library && u.system == "gb"));
    assert_eq!(unit_at(&set, &zip).kind, UnitKind::Zip);
    assert_eq!(unit_at(&set, &zip).payload_size, Some(2048));
    assert_eq!(unit_at(&set, &raw).kind, UnitKind::Single);
    assert_eq!(excluded_as(&set, &multi), Some(Exclusion::MultiGameArchive));
    assert_eq!(excluded_as(&set, &bios), Some(Exclusion::Bios));
    assert_eq!(
        excluded_as(&set, &unknown),
        None,
        "no system: not ours to report"
    );
}
