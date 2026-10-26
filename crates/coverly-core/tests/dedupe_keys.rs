//! Duplicates: level 1 keys and bucketing (spec §5.2, §10).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use coverly_core::archive::testing::sevenz_copy;
use coverly_core::archive::Limits;
use coverly_core::cache::Cache;
use coverly_core::dedupe::archive::ArchiveEntryListing;
use coverly_core::dedupe::keys::{
    level_one, parse_chd_header, track_layout, ExactKey, KeyCtx, KeyedUnit,
};
use coverly_core::dedupe::testing::{chd_header, nes_header, CountingBytes, CountingListing};
use coverly_core::dedupe::units::Unit;
use coverly_core::dedupe::UnitKind;
use coverly_core::identify::hash::{hash_rom, HeaderRule, DEFAULT_HASH_MAX_BYTES};
use coverly_core::systems::SystemDb;
use sha1::{Digest, Sha1};
use tempfile::TempDir;

// --- Builders ---

/// Synthetic ROM bytes: binary, distinct per `seed`.
fn rom_bytes(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| if i % 3 == 0 { 0 } else { (i as u8) ^ seed })
        .collect()
}

fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    p
}

/// A zip at `root/rel` holding `entries`, deflated.
fn put_zip(root: &Path, rel: &str, entries: &[(&str, &[u8])]) -> PathBuf {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(&p).unwrap());
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (inner, bytes) in entries {
        zip.start_file(*inner, opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    p
}

fn ext_of(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// A unit of `system` over `files` (`files[0]` is the primary).
fn unit_of(system: &str, kind: UnitKind, files: &[PathBuf], payload_size: Option<u64>) -> Unit {
    let primary = files[0].clone();
    let size = files.iter().map(|f| fs::metadata(f).unwrap().len()).sum();
    let meta = fs::metadata(&primary).unwrap();
    let mtime_ns = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64;
    Unit {
        id: 0,
        system: system.to_string(),
        primary: primary.clone(),
        files: files.to_vec(),
        kind,
        size,
        payload_size,
        mtime_ns,
        file_id: None,
        patches: Vec::new(),
        rel_dir: PathBuf::new(),
        stem: primary.file_stem().unwrap().to_string_lossy().into_owned(),
        ext: ext_of(&primary),
        in_library: true,
    }
}

/// A loose ROM unit.
fn raw(system: &str, path: &Path) -> Unit {
    let len = fs::metadata(path).unwrap().len();
    unit_of(system, UnitKind::Single, &[path.to_path_buf()], Some(len))
}

/// A one-ROM zip unit whose ROM entry is `inner_len` bytes.
fn zipped(system: &str, path: &Path, inner_len: usize) -> Unit {
    unit_of(
        system,
        UnitKind::Zip,
        &[path.to_path_buf()],
        Some(inner_len as u64),
    )
}

fn numbered(mut units: Vec<Unit>) -> Vec<Unit> {
    for (i, u) in units.iter_mut().enumerate() {
        u.id = i;
    }
    units
}

fn run(units: &[Unit], ctx: &KeyCtx) -> Vec<KeyedUnit> {
    level_one(units, ctx, &|_, _, _| {}, &AtomicBool::new(false))
}

/// The archive reader's listing, with no external tool.
fn listing() -> ArchiveEntryListing {
    ArchiveEntryListing::new(Limits::default())
}

fn keys_of(units: &[Unit]) -> Vec<KeyedUnit> {
    let db = SystemDb::builtin();
    let archives = listing();
    let ctx = KeyCtx::new(&db, &archives);
    run(units, &ctx)
}

fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}

fn exact(k: &KeyedUnit) -> &ExactKey {
    k.exact
        .as_ref()
        .unwrap_or_else(|| panic!("unit {} has no level 1 key", k.unit))
}

// --- Headers and byte orders ---

#[test]
fn nes_with_and_without_ines_header_share_a_key() {
    let dir = TempDir::new().unwrap();
    let prg = rom_bytes(1, 32 * 1024);
    let mut headered = nes_header(0).to_vec();
    headered.extend(&prg);
    let a = put(dir.path(), "NES/Game.nes", &headered);
    let b = put(dir.path(), "NES/Game (1).nes", &prg);
    let units = numbered(vec![raw("nes", &a), raw("nes", &b)]);

    let keys = keys_of(&units);
    assert_eq!(
        exact(&keys[0]),
        &ExactKey::Payload {
            sha1: sha1_hex(&prg),
            size: prg.len() as u64
        }
    );
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
    assert!(keys[0].confirmed && keys[1].confirmed);
    assert_eq!(keys[0].header.as_deref(), Some(nes_header(0).as_slice()));
    assert_eq!(keys[1].header, None, "a headerless NES copy has no header");
}

#[test]
fn nes_headers_that_differ_are_report_only() {
    let dir = TempDir::new().unwrap();
    let prg = rom_bytes(2, 16 * 1024);
    let with = |mapper: u8| {
        let mut v = nes_header(mapper).to_vec();
        v.extend(&prg);
        v
    };
    let a = put(dir.path(), "NES/A.nes", &with(1));
    let b = put(dir.path(), "NES/B.nes", &with(4));
    let units = numbered(vec![raw("nes", &a), raw("nes", &b)]);

    let keys = keys_of(&units);
    assert_eq!(exact(&keys[0]), exact(&keys[1]), "same PRG/CHR, same key");
    let (ha, hb) = (keys[0].header.clone(), keys[1].header.clone());
    assert!(ha.is_some() && hb.is_some());
    assert_ne!(ha, hb, "the header bytes are kept for the grouping step");
}

#[test]
fn smc_copier_and_sfc_share_a_key() {
    let dir = TempDir::new().unwrap();
    let body = rom_bytes(3, 64 * 1024);
    let mut smc = vec![0xAA; 512];
    smc.extend(&body);
    let a = put(dir.path(), "SFC/Game.smc", &smc);
    let b = put(dir.path(), "SFC/Game.sfc", &body);
    let units = numbered(vec![raw("snes", &a), raw("snes", &b)]);

    let keys = keys_of(&units);
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
    assert_eq!(keys[0].header, None, "a copier header is not functional");
}

#[test]
fn sms_512_header_shares_a_key() {
    let dir = TempDir::new().unwrap();
    let body = rom_bytes(4, 32 * 1024);
    let mut headered = vec![0x55; 512];
    headered.extend(&body);
    let a = put(dir.path(), "MS/Game.sms", &headered);
    let b = put(dir.path(), "MS/Game (1).sms", &body);
    let units = numbered(vec![raw("mastersystem", &a), raw("mastersystem", &b)]);

    let keys = keys_of(&units);
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
}

#[test]
fn n64_orders_share_a_key() {
    let dir = TempDir::new().unwrap();
    let mut z64 = vec![0x80, 0x37, 0x12, 0x40];
    z64.extend(rom_bytes(5, 8 * 1024 - 4));
    let v64: Vec<u8> = z64.chunks(2).flat_map(|c| [c[1], c[0]]).collect();
    let n64: Vec<u8> = z64
        .chunks(4)
        .flat_map(|c| [c[3], c[2], c[1], c[0]])
        .collect();
    let a = put(dir.path(), "N64/Game.z64", &z64);
    let b = put(dir.path(), "N64/Game.v64", &v64);
    let c = put(dir.path(), "N64/Game.n64", &n64);
    let units = numbered(vec![raw("n64", &a), raw("n64", &b), raw("n64", &c)]);

    let keys = keys_of(&units);
    assert_eq!(
        exact(&keys[0]),
        &ExactKey::Payload {
            sha1: sha1_hex(&z64),
            size: z64.len() as u64
        }
    );
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
    assert_eq!(exact(&keys[0]), exact(&keys[2]));
}

// --- Containers ---

#[test]
fn raw_and_single_entry_zip_share_a_key_after_confirmation() {
    let dir = TempDir::new().unwrap();
    let rom = rom_bytes(6, 32 * 1024);
    let a = put(dir.path(), "GB/Tetris.gb", &rom);
    let b = put_zip(dir.path(), "GB/Tetris.zip", &[("Tetris.gb", &rom)]);
    let units = numbered(vec![raw("gb", &a), zipped("gb", &b, rom.len())]);

    let keys = keys_of(&units);
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
    assert!(keys[1].confirmed, "the zip entry was decoded and hashed");
}

#[test]
fn zip_over_budget_is_unconfirmed() {
    let dir = TempDir::new().unwrap();
    let rom = rom_bytes(7, 32 * 1024);
    let a = put_zip(dir.path(), "GB/A.zip", &[("A.gb", &rom)]);
    let b = put_zip(dir.path(), "GB/B.zip", &[("B.gb", &rom)]);
    let units = numbered(vec![
        zipped("gb", &a, rom.len()),
        zipped("gb", &b, rom.len()),
    ]);

    let db = SystemDb::builtin();
    let archives = listing();
    let mut ctx = KeyCtx::new(&db, &archives);
    ctx.max_hash_bytes = 1024;
    let keys = run(&units, &ctx);
    let want = ExactKey::ZipCrc {
        crc32: crc32fast::hash(&rom),
        size: rom.len() as u64,
    };
    assert_eq!(exact(&keys[0]), &want);
    assert_eq!(exact(&keys[1]), &want);
    assert!(!keys[0].confirmed && !keys[1].confirmed);
}

#[test]
fn arcade_zips_with_renamed_entries_share_a_key_and_one_changed_entry_does_not() {
    let dir = TempDir::new().unwrap();
    let (r1, r2) = (rom_bytes(8, 4096), rom_bytes(9, 2048));
    let mut r2b = r2.clone();
    r2b[100] ^= 0xFF;
    let a = put_zip(
        dir.path(),
        "FBNEO/ffight (1).zip",
        &[("a.bin", &r1), ("b.bin", &r2)],
    );
    let b = put_zip(
        dir.path(),
        "FBNEO/ffight copy.zip",
        &[("y.rom", &r2), ("x.rom", &r1)],
    );
    let c = put_zip(
        dir.path(),
        "FBNEO/ffight 2.zip",
        &[("a.bin", &r1), ("b.bin", &r2b)],
    );
    let total = Some((r1.len() + r2.len()) as u64);
    let units = numbered(vec![
        unit_of("fbneo", UnitKind::Arcade, &[a], total),
        unit_of("fbneo", UnitKind::Arcade, &[b], total),
        unit_of("fbneo", UnitKind::Arcade, &[c], total),
    ]);

    let keys = keys_of(&units);
    assert!(matches!(exact(&keys[0]), ExactKey::Arcade { .. }));
    assert_eq!(exact(&keys[0]), exact(&keys[1]), "entry names are ignored");
    assert_ne!(exact(&keys[0]), exact(&keys[2]), "one changed entry");
}

// --- Discs ---

#[test]
fn track_layout_keeps_tracks_and_indexes_only() {
    let a = "FILE \"Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n";
    let b = "REM GENRE Racing\r\nCATALOG 0000000000000\r\nfile \"Game (Track 1).bin\" binary\r\n\
             \ttrack 01 mode2/2352\r\n\t\ttitle \"x\"\r\n\t\tindex 01 00:00:00\r\n";
    let c = "FILE \"Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:02:00\n";
    let d = "FILE \"Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    PREGAP 00:02:00\n    INDEX 01 00:00:00\n";
    assert_eq!(track_layout(a), track_layout(b));
    assert_ne!(track_layout(a), track_layout(c));
    assert_ne!(track_layout(a), track_layout(d));
    assert!(!track_layout(a).contains("Game"), "file names are left out");
}

#[test]
fn cue_text_differences_do_not_matter_but_index_differences_do() {
    let dir = TempDir::new().unwrap();
    let track = rom_bytes(10, 2352 * 40);
    let cue = |bin: &str, index: &str, rem: &str| {
        format!("{rem}FILE \"{bin}\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 {index}\n")
    };
    let mut units = Vec::new();
    for (stem, index, rem) in [
        ("A", "00:00:00", ""),
        (
            "B (Track 1)",
            "00:00:00",
            "REM DISCID 1234\nREM COMMENT \"ripped\"\n",
        ),
        ("C", "00:02:00", ""),
    ] {
        let bin = put(dir.path(), &format!("PS/{stem}.bin"), &track);
        let sheet = put(
            dir.path(),
            &format!("PS/{stem}.cue"),
            cue(&format!("{stem}.bin"), index, rem).as_bytes(),
        );
        units.push(unit_of("psx", UnitKind::DiscSet, &[sheet, bin], None));
    }
    let units = numbered(units);

    let keys = keys_of(&units);
    assert_eq!(
        exact(&keys[0]),
        &ExactKey::DiscTracks {
            tracks: vec![sha1_hex(&track)],
            layout: track_layout(&cue("A.bin", "00:00:00", "")),
        }
    );
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
    // C differs only in its INDEX: same tracks, another layout. It may
    // stay unread (nothing else can match it) but never shares A's key.
    assert_ne!(keys[2].exact.as_ref(), Some(exact(&keys[0])));
}

#[test]
fn chd_sha1_and_rawsha1_from_header() {
    let (s1, r1) = ([0x11u8; 20], [0x22u8; 20]);
    let v5 = chd_header(5, s1, r1);
    assert_eq!(
        parse_chd_header(&v5),
        Some((5, Some("11".repeat(20)), Some("22".repeat(20))))
    );
    let v4 = chd_header(4, s1, r1);
    assert_eq!(
        parse_chd_header(&v4),
        Some((4, Some("11".repeat(20)), Some("22".repeat(20))))
    );
    let v3 = chd_header(3, s1, [0; 20]);
    assert_eq!(
        parse_chd_header(&v3),
        Some((3, Some("11".repeat(20)), None))
    );
    assert_eq!(parse_chd_header(b"not a chd at all"), None);
    assert_eq!(parse_chd_header(&v5[..60]), None, "truncated header");

    // Two CHDs with other compressors (other bytes) and the same hashes
    // share a key; same raw data with other metadata does not.
    let dir = TempDir::new().unwrap();
    let mut a = v5.clone();
    a.extend(rom_bytes(11, 3000));
    let mut b = v5.clone();
    b.extend(rom_bytes(12, 5000));
    let mut c = chd_header(5, [0x33; 20], r1);
    c.extend(rom_bytes(13, 4000));
    let pa = put(dir.path(), "PS/A.chd", &a);
    let pb = put(dir.path(), "PS/B.chd", &b);
    let pc = put(dir.path(), "PS/C.chd", &c);
    let units = numbered(vec![
        unit_of("psx", UnitKind::Chd, &[pa], None),
        unit_of("psx", UnitKind::Chd, &[pb], None),
        unit_of("psx", UnitKind::Chd, &[pc], None),
    ]);

    let keys = keys_of(&units);
    assert_eq!(
        exact(&keys[0]),
        &ExactKey::Chd {
            sha1: "11".repeat(20)
        }
    );
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
    assert_ne!(exact(&keys[0]), exact(&keys[2]));
    assert_eq!(keys[0].chd_raw, keys[2].chd_raw, "same raw data");
    assert_eq!(keys[2].chd_raw.as_deref(), Some("22".repeat(20).as_str()));
}

// --- Patches and names ---

#[test]
fn soft_patch_splits_identical_roms() {
    let dir = TempDir::new().unwrap();
    let rom = rom_bytes(14, 64 * 1024);
    let a = put(dir.path(), "SFC/Game.sfc", &rom);
    let ips = put(
        dir.path(),
        "SFC/Game.ips",
        b"PATCH\x00\x00\x10\x00\x01\xFFEOF",
    );
    let b = put(dir.path(), "SFC/Game (1).sfc", &rom);
    let mut ua = raw("snes", &a);
    ua.patches = vec![ips.clone()];
    let units = numbered(vec![ua.clone(), raw("snes", &b)]);

    let keys = keys_of(&units);
    assert_eq!(exact(&keys[0]), exact(&keys[1]), "same bytes");
    assert_eq!(keys[0].patch_set, vec![sha1_hex(&fs::read(&ips).unwrap())]);
    assert_ne!(
        keys[0].group_key("snes"),
        keys[1].group_key("snes"),
        "a patched ROM is another game"
    );

    let ips_b = put(dir.path(), "SFC/Game (1).ips", &fs::read(&ips).unwrap());
    let mut ub = raw("snes", &b);
    ub.patches = vec![ips_b];
    let units = numbered(vec![ua, ub]);
    let keys = keys_of(&units);
    assert_eq!(keys[0].group_key("snes"), keys[1].group_key("snes"));
}

#[test]
fn names_never_group() {
    let dir = TempDir::new().unwrap();
    let sfc = rom_bytes(15, 32 * 1024);
    let gb = rom_bytes(16, 32 * 1024);
    // Same name, two systems, other bytes.
    let a = put_zip(dir.path(), "SFC/Tetris 2.zip", &[("Tetris 2.sfc", &sfc)]);
    let b = put_zip(dir.path(), "GB/Tetris 2 (1).zip", &[("Tetris 2.gb", &gb)]);
    // Same bytes under other names in one system.
    let c = put_zip(dir.path(), "GB/Tetris 2.zip", &[("Tetris 2.gb", &gb)]);
    let d = put(dir.path(), "GB/Pokemon Red.gb", &gb);
    // The SFC bytes as a GB file: same bytes, other system.
    let e = put(dir.path(), "GB/Tetris 2.gb", &sfc);
    let units = numbered(vec![
        zipped("snes", &a, sfc.len()),
        zipped("gb", &b, gb.len()),
        zipped("gb", &c, gb.len()),
        raw("gb", &d),
        raw("gb", &e),
    ]);

    let keys = keys_of(&units);
    let gk = |i: usize| keys[i].group_key(&units[i].system);
    assert_ne!(gk(0), gk(1), "same name, other game");
    assert_eq!(gk(1), gk(2), "a \"(1)\" copy of the same bytes");
    assert_eq!(gk(1), gk(3), "a different name, same bytes");
    assert!(
        gk(4).is_none() || gk(4) != gk(0),
        "no key spans two systems"
    );
    assert!(gk(0).is_none() || gk(4) != gk(0));
}

#[test]
fn raw_vs_single_entry_7z_is_an_exact_copy() {
    let dir = TempDir::new().unwrap();
    let rom = rom_bytes(19, 32 * 1024);
    let loose = put(dir.path(), "GB/Tetris.gb", &rom);
    let sz = put(
        dir.path(),
        "GB/Tetris (Europe).7z",
        &sevenz_copy(&[("readme.txt", b"hi"), ("Tetris (Europe).gb", &rom)]),
    );
    let units = numbered(vec![raw("gb", &loose), zipped("gb", &sz, rom.len())]);

    let keys = keys_of(&units);
    assert_eq!(
        exact(&keys[1]),
        &ExactKey::Payload {
            sha1: sha1_hex(&rom),
            size: rom.len() as u64
        },
        "the 7z entry is decoded and hashed"
    );
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
    assert!(keys[0].confirmed && keys[1].confirmed);
    assert_eq!(keys[0].group_key("gb"), keys[1].group_key("gb"));
}

// --- Bucketing and reuse ---

#[test]
fn singleton_size_buckets_are_never_read() {
    let dir = TempDir::new().unwrap();
    let a = put(dir.path(), "GB/A.gb", &rom_bytes(17, 32 * 1024));
    let b = put(dir.path(), "GB/B.gb", &rom_bytes(18, 64 * 1024));
    let c = put_zip(
        dir.path(),
        "GB/C.zip",
        &[("C.gb", &rom_bytes(19, 128 * 1024))],
    );
    let d = put(dir.path(), "NES/D.nes", &rom_bytes(20, 40 * 1024));
    let units = numbered(vec![
        raw("gb", &a),
        raw("gb", &b),
        zipped("gb", &c, 128 * 1024),
        raw("nes", &d),
    ]);

    let db = SystemDb::builtin();
    let bytes = CountingBytes::new();
    let listing = CountingListing::new(listing());
    let mut ctx = KeyCtx::new(&db, &listing);
    ctx.bytes = &bytes;
    let keys = run(&units, &ctx);
    assert_eq!(bytes.opens(), 0, "no file opened");
    assert_eq!(listing.calls(), 0, "no archive listed");
    assert!(keys.iter().all(|k| k.exact.is_none()));
}

#[test]
fn bucket_key_is_payload_size_not_file_size() {
    let dir = TempDir::new().unwrap();
    let body = rom_bytes(21, 128 * 1024);
    let mut smc = vec![0u8; 512];
    smc.extend(&body);
    let a = put(dir.path(), "SFC/Game.smc", &smc);
    let b = put(dir.path(), "SFC/Game.sfc", &body);
    let units = numbered(vec![raw("snes", &a), raw("snes", &b)]);
    assert_ne!(units[0].size, units[1].size);

    let keys = keys_of(&units);
    assert_eq!(
        exact(&keys[0]),
        &ExactKey::Payload {
            sha1: sha1_hex(&body),
            size: body.len() as u64
        }
    );
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
}

#[test]
fn quick_ids_v2_rows_are_reused_without_reading() {
    let dir = TempDir::new().unwrap();
    let rom = rom_bytes(22, 32 * 1024);
    let a = put(dir.path(), "GB/A.gb", &rom);
    let b = put(dir.path(), "GB/B.gb", &rom);
    let units = numbered(vec![raw("gb", &a), raw("gb", &b)]);
    let cache = Cache::in_memory().unwrap();
    for u in &units {
        let outcome =
            hash_rom(&u.primary, "gb", HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap();
        cache
            .put_quick_id(&u.primary, u.size, u.mtime_ns, &outcome)
            .unwrap();
    }

    let db = SystemDb::builtin();
    let bytes = CountingBytes::new();
    let archives = listing();
    let mut ctx = KeyCtx::new(&db, &archives);
    ctx.bytes = &bytes;
    ctx.cache = Some(&cache);
    let keys = run(&units, &ctx);
    assert_eq!(bytes.opens(), 0);
    assert_eq!(
        exact(&keys[0]),
        &ExactKey::Payload {
            sha1: sha1_hex(&rom),
            size: rom.len() as u64
        }
    );
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
}

#[test]
fn colliding_bucket_ignores_max_hash_mb() {
    let dir = TempDir::new().unwrap();
    let rom = rom_bytes(23, 256 * 1024);
    let a = put(dir.path(), "GB/A.gb", &rom);
    let b = put(dir.path(), "GB/B.gb", &rom);
    let units = numbered(vec![raw("gb", &a), raw("gb", &b)]);

    let db = SystemDb::builtin();
    let archives = listing();
    let mut ctx = KeyCtx::new(&db, &archives);
    ctx.max_hash_bytes = 1024;
    let keys = run(&units, &ctx);
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
    assert!(keys[0].confirmed);
}

#[test]
fn prefix_crc_spares_full_reads_of_different_files() {
    let dir = TempDir::new().unwrap();
    let a = put(dir.path(), "GB/A.gb", &rom_bytes(24, 512 * 1024));
    let b = put(dir.path(), "GB/B.gb", &rom_bytes(25, 512 * 1024));
    let units = numbered(vec![raw("gb", &a), raw("gb", &b)]);

    let db = SystemDb::builtin();
    let bytes = CountingBytes::new();
    let archives = listing();
    let mut ctx = KeyCtx::new(&db, &archives);
    ctx.bytes = &bytes;
    let keys = run(&units, &ctx);
    assert!(keys.iter().all(|k| k.exact.is_none()));
    assert_eq!(bytes.opens(), 2, "one prefix read each, no full hash");
    // The prefix plus room for the largest header.
    assert!(bytes.bytes_read() <= 2 * (128 * 1024 + 512));
}

#[test]
fn computed_keys_are_cached_and_a_warm_run_reads_nothing() {
    let dir = TempDir::new().unwrap();
    let rom = rom_bytes(26, 256 * 1024);
    let a = put(dir.path(), "GB/A.gb", &rom);
    let b = put(dir.path(), "GB/B.gb", &rom);
    let c = put(dir.path(), "GB/C.gb", &rom_bytes(27, 256 * 1024));
    let units = numbered(vec![raw("gb", &a), raw("gb", &b), raw("gb", &c)]);
    let cache = Cache::in_memory().unwrap();
    let db = SystemDb::builtin();

    let cold = CountingBytes::new();
    let archives = listing();
    let mut ctx = KeyCtx::new(&db, &archives);
    ctx.cache = Some(&cache);
    ctx.volume_uuid = Some("VOL-1");
    ctx.bytes = &cold;
    let first = run(&units, &ctx);
    assert!(cold.opens() > 0);
    let row = cache
        .content_row("VOL-1", &units[0].primary, units[0].size, units[0].mtime_ns)
        .unwrap()
        .expect("a row for a hashed file");
    assert_eq!(row.index_keys, vec![exact(&first[0]).key_string("gb")]);

    let warm = CountingBytes::new();
    ctx.bytes = &warm;
    let second = run(&units, &ctx);
    assert_eq!(warm.opens(), 0, "a warm run reads nothing");
    assert_eq!(first, second);
}

#[test]
fn headered_nes_in_a_zip_and_a_headerless_raw_share_a_key() {
    let dir = TempDir::new().unwrap();
    let prg = rom_bytes(28, 32 * 1024);
    let mut headered = nes_header(2).to_vec();
    headered.extend(&prg);
    let a = put_zip(dir.path(), "NES/Game.zip", &[("Game.nes", &headered)]);
    let b = put(dir.path(), "NES/Game (1).nes", &prg);
    let units = numbered(vec![zipped("nes", &a, headered.len()), raw("nes", &b)]);

    let keys = keys_of(&units);
    assert_eq!(exact(&keys[0]), exact(&keys[1]));
    assert_eq!(keys[0].header.as_deref(), Some(nes_header(2).as_slice()));
    assert_eq!(keys[1].header, None);
}

#[test]
fn a_cached_disc_key_is_dropped_when_a_track_changes_behind_its_sheet() {
    // A same-size hard patch of a track (a translation) leaves the cue's
    // size and mtime as they were: the cache must not hand back the old key.
    let dir = TempDir::new().unwrap();
    let track = rom_bytes(30, 2352 * 40);
    let sheet = |stem: &str| {
        format!("FILE \"{stem}.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n")
    };
    let mut files = Vec::new();
    for stem in ["A", "B"] {
        let bin = put(dir.path(), &format!("PS/{stem}.bin"), &track);
        let cue = put(
            dir.path(),
            &format!("PS/{stem}.cue"),
            sheet(stem).as_bytes(),
        );
        files.push([cue, bin]);
    }
    let units_now = || {
        numbered(
            files
                .iter()
                .map(|f| unit_of("psx", UnitKind::DiscSet, f, None))
                .collect(),
        )
    };
    let cache = Cache::in_memory().unwrap();
    let db = SystemDb::builtin();
    let archives = listing();
    let mut ctx = KeyCtx::new(&db, &archives);
    ctx.cache = Some(&cache);
    ctx.volume_uuid = Some("VOL-1");

    let before = units_now();
    let first = run(&before, &ctx);
    assert_eq!(exact(&first[0]), exact(&first[1]));

    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(&files[1][1], rom_bytes(31, 2352 * 40)).unwrap();
    let after = units_now();
    assert_eq!(
        (after[1].size, after[1].mtime_ns),
        (before[1].size, before[1].mtime_ns),
        "the unit's own size and mtime do not see the change"
    );
    let second = run(&after, &ctx);
    assert_ne!(second[0].exact, second[1].exact);
}

// --- Level 2: same game, other version (spec §5.3) ---

use coverly_core::cache::DatRow;
use coverly_core::dedupe::keys::{
    level_three, level_two, FormatKey, FormatLink, VersionKey, VERSION_CONFIDENCE,
};
use coverly_core::dedupe::source::{CacheDats, DatLookup, NoDiscProbe};
use coverly_core::dedupe::testing::{disc_id, fingerprint, FakeDats, FakeDiscProbe};
use coverly_core::dedupe::ReportWhy;
use coverly_core::identify::dat::dat_sources;

/// A unit that exists only on paper: level 2 and 3 never read files.
fn paper_unit(id: usize, system: &str, path: &str, kind: UnitKind) -> Unit {
    let primary = PathBuf::from(path);
    Unit {
        id,
        system: system.to_string(),
        primary: primary.clone(),
        files: vec![primary.clone()],
        kind,
        size: 1024,
        payload_size: None,
        mtime_ns: 0,
        file_id: None,
        patches: Vec::new(),
        rel_dir: PathBuf::new(),
        stem: primary.file_stem().unwrap().to_string_lossy().into_owned(),
        ext: ext_of(&primary),
        in_library: true,
    }
}

/// Level 1 facts of unit `unit` with the catalogue CRCs `crcs`.
fn keyed_with_crc(unit: usize, crcs: &[(u32, u64)]) -> KeyedUnit {
    KeyedUnit {
        unit,
        exact: None,
        confirmed: false,
        patch_set: Vec::new(),
        header: None,
        dat_game: None,
        chd_raw: None,
        dat_crcs: crcs.to_vec(),
    }
}

fn version_keys(
    units: &[Unit],
    keyed: &[KeyedUnit],
    dats: &dyn DatLookup,
) -> Vec<Option<VersionKey>> {
    level_two(units, keyed, dats)
}

fn vkey(k: &Option<VersionKey>) -> &VersionKey {
    k.as_ref().expect("a version key")
}

#[test]
fn tetris_v10_and_v11_share_a_version_key() {
    let units = vec![
        paper_unit(0, "gb", "GB/Tetris (JUE) (V1.0) [!].zip", UnitKind::Zip),
        paper_unit(1, "gb", "GB/Tetris (JUE) (V1.1) [!].zip", UnitKind::Zip),
    ];
    let keyed = vec![
        keyed_with_crc(0, &[(0x63F9_407D, 32_768)]),
        keyed_with_crc(1, &[(0x46DF_91AD, 32_768)]),
    ];
    let dats = FakeDats::new()
        .with_crc("gb", 0x63F9_407D, "Tetris (Japan) (En)")
        .with_crc("gb", 0x46DF_91AD, "Tetris (World) (Rev 1)");

    let keys = version_keys(&units, &keyed, &dats);
    let (a, b) = (vkey(&keys[0]), vkey(&keys[1]));
    assert_eq!(a.title_key, "tetris");
    assert_eq!(a.key_string(), b.key_string());
    assert_eq!(a.system, "gb");
    assert_eq!(a.game, "Tetris (Japan) (En)");
    assert_eq!(b.game, "Tetris (World) (Rev 1)");
    assert_eq!(b.tags.revision_text.as_deref(), Some("Rev 1"));
    assert_eq!(a.disc, None);
    assert!((VERSION_CONFIDENCE - 0.90).abs() < f32::EPSILON);
}

#[test]
fn unknown_dump_never_gets_a_version_key() {
    // Same name, no catalogue hit by hash: names never group.
    let units = vec![
        paper_unit(0, "gb", "GB/Tetris (JUE) (V1.0) [!].zip", UnitKind::Zip),
        paper_unit(1, "gb", "GB/Tetris (World) (Rev 1).gb", UnitKind::Single),
    ];
    let keyed = vec![
        keyed_with_crc(0, &[(0xDEAD_BEEF, 32_768)]),
        keyed_with_crc(1, &[]),
    ];
    let dats = FakeDats::new().with_crc("gb", 0x46DF_91AD, "Tetris (World) (Rev 1)");
    assert_eq!(version_keys(&units, &keyed, &dats), vec![None, None]);
}

#[test]
fn a_hit_in_another_systems_catalogue_is_not_a_version_key() {
    let units = vec![paper_unit(0, "gbc", "GBC/Tetris.gb", UnitKind::Single)];
    let keyed = vec![keyed_with_crc(0, &[(0x46DF_91AD, 32_768)])];
    let dats = FakeDats::new().with_crc("gb", 0x46DF_91AD, "Tetris (World) (Rev 1)");
    assert_eq!(version_keys(&units, &keyed, &dats), vec![None]);
}

#[test]
fn a_bios_entry_or_an_ambiguous_crc_is_not_a_version_key() {
    let units = vec![
        paper_unit(0, "psx", "PS/a.bin", UnitKind::Single),
        paper_unit(1, "gb", "GB/b.gb", UnitKind::Single),
    ];
    let keyed = vec![
        keyed_with_crc(0, &[(0x1111_1111, 512)]),
        keyed_with_crc(1, &[(0x2222_2222, 512)]),
    ];
    let dats = FakeDats::new()
        .with_bios("psx", 0x1111_1111, "PlayStation BIOS (v3.0)")
        .with_crc("gb", 0x2222_2222, "Alpha (USA)")
        .with_crc("gb", 0x2222_2222, "Beta Game (Japan)");
    assert_eq!(version_keys(&units, &keyed, &dats), vec![None, None]);
}

#[test]
fn the_headerless_crc_finds_the_catalogue_entry_too() {
    let units = vec![paper_unit(0, "snes", "SNES/Game.smc", UnitKind::Single)];
    let keyed = vec![keyed_with_crc(
        0,
        &[(0xAAAA_AAAA, 512 + 1024), (0xBBBB_BBBB, 1024)],
    )];
    let dats = FakeDats::new().with_crc("snes", 0xBBBB_BBBB, "Game (USA)");
    assert_eq!(
        vkey(&version_keys(&units, &keyed, &dats)[0]).game,
        "Game (USA)"
    );
}

#[test]
fn disc_1_and_2_never_share() {
    // Level 2: the disc number stays in the title key.
    let units = vec![
        paper_unit(0, "psx", "PS/Driver 2 (Disc 1).bin", UnitKind::Single),
        paper_unit(1, "psx", "PS/Driver 2 (Disc 2).bin", UnitKind::Single),
    ];
    let keyed = vec![keyed_with_crc(0, &[(1, 10)]), keyed_with_crc(1, &[(2, 10)])];
    let dats = FakeDats::new()
        .with_crc("psx", 1, "Driver 2 (USA) (Disc 1)")
        .with_crc("psx", 2, "Driver 2 (USA) (Disc 2) (Rev 1)");
    let keys = version_keys(&units, &keyed, &dats);
    let (a, b) = (vkey(&keys[0]), vkey(&keys[1]));
    assert_eq!((a.disc, b.disc), (Some(1), Some(2)));
    assert_ne!(a.key_string(), b.key_string());

    // Level 3: Saturn reuses one product number on both discs.
    let units = vec![
        paper_unit(
            0,
            "saturn",
            "Saturn/Grandia (Disc 1).cue",
            UnitKind::DiscSet,
        ),
        paper_unit(
            1,
            "saturn",
            "Saturn/Grandia (Disc 2).cue",
            UnitKind::DiscSet,
        ),
    ];
    let probe = FakeDiscProbe::new()
        .with(
            &units[0].primary,
            disc_id(&["T-4507G"], Some(1), Some(fingerprint(1))),
        )
        .with(
            &units[1].primary,
            disc_id(&["T-4507G"], Some(2), Some(fingerprint(1))),
        );
    let keys = level_three(&units, &probe, &FakeDats::new());
    let (a, b) = (fkey(&keys[0]), fkey(&keys[1]));
    assert_ne!(a.key_string(), b.key_string());
    assert_eq!(a.link(b), None);
}

#[test]
fn translated_and_original_never_share() {
    let units = vec![
        paper_unit(0, "nes", "NES/Heracles.nes", UnitKind::Single),
        paper_unit(1, "nes", "NES/Heracles (T).nes", UnitKind::Single),
    ];
    let keyed = vec![keyed_with_crc(0, &[(1, 10)]), keyed_with_crc(1, &[(2, 10)])];
    let dats = FakeDats::new()
        .with_crc("nes", 1, "Heracles no Eikou (Japan)")
        .with_crc("nes", 2, "Heracles no Eikou (Japan) (Translated En)");
    let keys = version_keys(&units, &keyed, &dats);
    assert_ne!(vkey(&keys[0]).key_string(), vkey(&keys[1]).key_string());
}

#[test]
fn level_one_records_the_catalogue_crcs() {
    let dir = TempDir::new().unwrap();
    let rom = rom_bytes(40, 32 * 1024);
    let a = put(dir.path(), "GB/A.gb", &rom);
    let b = put_zip(dir.path(), "GB/B.zip", &[("B.gb", &rom)]);
    let units = numbered(vec![raw("gb", &a), zipped("gb", &b, rom.len())]);
    let keys = keys_of(&units);
    let crc = crc32fast::hash(&rom);
    assert_eq!(keys[0].dat_crcs, vec![(crc, rom.len() as u64)]);
    assert_eq!(keys[1].dat_crcs, vec![(crc, rom.len() as u64)]);
}

#[test]
fn cache_dats_answer_by_crc_and_serial() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let gb_dat = dat_sources(db.get("gb").unwrap()).remove(0);
    let ps_dat = dat_sources(db.get("psx").unwrap()).remove(0);
    let row = |game: &str, crc: &str, size: u64, serial: Option<&str>| DatRow {
        game: game.to_string(),
        rom_name: format!("{game}.bin"),
        size: Some(size),
        crc32: Some(crc.to_string()),
        md5: None,
        sha1: None,
        serial: serial.map(str::to_string),
    };
    cache
        .replace_dat(
            &gb_dat,
            &[row("Tetris (World) (Rev 1)", "46DF91AD", 32_768, None)],
        )
        .unwrap();
    cache
        .replace_dat(
            &ps_dat,
            &[row("Crash Bash (USA)", "12345678", 100, Some("SCUS-94570"))],
        )
        .unwrap();
    let dats = CacheDats::new(&cache, &db);

    let hits = dats.by_crc("gb", 0x46DF_91AD, 32_768);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].game, "Tetris (World) (Rev 1)");
    assert!(
        dats.by_crc("gb", 0x46DF_91AD, 99).is_empty(),
        "size must agree"
    );
    assert!(
        dats.by_crc("gbc", 0x46DF_91AD, 32_768).is_empty(),
        "other system"
    );
    assert_eq!(
        dats.by_serial("psx", "SCUS-94570")[0].game,
        "Crash Bash (USA)"
    );
}

// --- Level 3: same disc, other format (spec §5.4) ---

fn fkey(k: &Option<FormatKey>) -> &FormatKey {
    k.as_ref().expect("a format key")
}

#[test]
fn pbp_and_bin_same_serial_and_fingerprint_share_a_format_key() {
    let units = vec![
        paper_unit(0, "psx", "PS/Crash Bash.PBP", UnitKind::Image),
        paper_unit(1, "psx", "PS/Crash Bash.cue", UnitKind::DiscSet),
    ];
    let probe = FakeDiscProbe::new()
        .with(
            &units[0].primary,
            disc_id(&["SCUS-94570"], Some(1), Some(fingerprint(5))),
        )
        .with(
            &units[1].primary,
            disc_id(&["SCUS-94570"], None, Some(fingerprint(5))),
        );

    let keys = level_three(&units, &probe, &FakeDats::new());
    let (a, b) = (fkey(&keys[0]), fkey(&keys[1]));
    assert_eq!(a.key_string(), b.key_string());
    assert_eq!(a.serials, vec!["SCUS-94570".to_string()]);
    assert_eq!(a.link(b), Some(FormatLink::Acts { confidence: 0.90 }));

    // With the Redump DAT agreeing on one game the match is stronger.
    let dats = FakeDats::new().with_serial("psx", "SCUS-94570", "Crash Bash (USA)");
    let keys = level_three(&units, &probe, &dats);
    assert_eq!(
        fkey(&keys[0]).link(fkey(&keys[1])),
        Some(FormatLink::Acts { confidence: 0.97 })
    );
}

#[test]
fn different_serials_never_group() {
    // Same title, same name: discs 1 and 2, not copies.
    let units = vec![
        paper_unit(0, "psx", "PS/Driver 2.img", UnitKind::Single),
        paper_unit(1, "psx", "PS/Driver 2.bin", UnitKind::Single),
    ];
    let probe = FakeDiscProbe::new()
        .with(
            &units[0].primary,
            disc_id(&["SLUS-01161"], None, Some(fingerprint(1))),
        )
        .with(
            &units[1].primary,
            disc_id(&["SLUS-01318"], None, Some(fingerprint(1))),
        );
    let keys = level_three(&units, &probe, &FakeDats::new());
    let (a, b) = (fkey(&keys[0]), fkey(&keys[1]));
    assert_ne!(a.key_string(), b.key_string());
    assert_eq!(a.link(b), None);
}

#[test]
fn same_serial_other_build_is_report_only() {
    let units = vec![
        paper_unit(0, "psx", "PS/Tekken 3.PBP", UnitKind::Image),
        paper_unit(1, "psx", "PS/Iron fist 3.PBP", UnitKind::Image),
        paper_unit(2, "psx", "PS/Tekken 3.chd", UnitKind::Chd),
    ];
    let probe = FakeDiscProbe::new()
        .with(
            &units[0].primary,
            disc_id(&["SLPS-01300"], Some(1), Some(fingerprint(1))),
        )
        .with(
            &units[1].primary,
            disc_id(&["SLPS-01300"], Some(1), Some(fingerprint(2))),
        )
        .with(&units[2].primary, disc_id(&["SLPS-01300"], Some(1), None));
    let keys = level_three(&units, &probe, &FakeDats::new());
    let build = Some(FormatLink::ReportOnly {
        why: ReportWhy::BuildDiffers,
    });
    assert_eq!(fkey(&keys[0]).link(fkey(&keys[1])), build);
    assert_eq!(
        fkey(&keys[0]).link(fkey(&keys[2])),
        build,
        "unreadable fingerprint"
    );
}

#[test]
fn modification_tag_vetoes_level_three() {
    let mut patched = paper_unit(2, "psx", "PS/Game.bin", UnitKind::Single);
    patched.patches = vec![PathBuf::from("PS/Game.ppf")];
    let units = vec![
        paper_unit(0, "psx", "PS/Game (USA).cue", UnitKind::DiscSet),
        paper_unit(1, "psx", "PS/Game (Traducido).cue", UnitKind::DiscSet),
        patched,
        paper_unit(3, "psx", "PS/Game [T+Spa].PBP", UnitKind::Image),
    ];
    let mut probe = FakeDiscProbe::new();
    for u in &units {
        probe = probe.with(
            &u.primary,
            disc_id(&["SLUS-00001"], Some(1), Some(fingerprint(3))),
        );
    }
    let keys = level_three(&units, &probe, &FakeDats::new());
    assert!(!fkey(&keys[0]).modified);
    let modified = Some(FormatLink::ReportOnly {
        why: ReportWhy::Modified,
    });
    for k in &keys[1..] {
        assert!(fkey(k).modified);
        assert_eq!(fkey(&keys[0]).link(fkey(k)), modified);
    }
}

#[test]
fn multi_disc_lists_must_match_and_a_lone_disc_is_partial() {
    let units = vec![
        paper_unit(0, "psx", "PS/FF7.m3u", UnitKind::MultiDisc),
        paper_unit(1, "psx", "PS/FF7.PBP", UnitKind::Image),
        paper_unit(2, "psx", "PS/FF7 (Disc 1).chd", UnitKind::Chd),
        paper_unit(3, "psx", "PS/FF7 other.m3u", UnitKind::MultiDisc),
    ];
    let set = ["SCUS-94163", "SCUS-94164", "SCUS-94165"];
    let probe = FakeDiscProbe::new()
        .with(&units[0].primary, disc_id(&set, None, Some(fingerprint(7))))
        .with(&units[1].primary, disc_id(&set, None, Some(fingerprint(7))))
        .with(
            &units[2].primary,
            disc_id(&set[..1], Some(1), Some(fingerprint(7))),
        )
        .with(
            &units[3].primary,
            disc_id(&["SCUS-94163", "SCUS-94999"], None, Some(fingerprint(7))),
        );
    let keys = level_three(&units, &probe, &FakeDats::new());
    let k: Vec<&FormatKey> = keys.iter().map(fkey).collect();
    assert_eq!(k[0].key_string(), k[1].key_string());
    assert!(matches!(k[0].link(k[1]), Some(FormatLink::Acts { .. })));
    assert_eq!(
        k[0].link(k[2]),
        Some(FormatLink::ReportOnly {
            why: ReportWhy::PartialSet
        })
    );
    assert_eq!(k[2].link(k[0]), k[0].link(k[2]), "symmetric");
    assert_eq!(k[0].link(k[3]), None, "another list is another game");
}

#[test]
fn saturn_without_disc_no_takes_no_part_unless_dat_says_single_disc() {
    let units = vec![
        paper_unit(0, "saturn", "Saturn/Nights.cue", UnitKind::DiscSet),
        paper_unit(1, "saturn", "Saturn/Panzer.cue", UnitKind::DiscSet),
    ];
    let probe = FakeDiscProbe::new()
        .with(
            &units[0].primary,
            disc_id(&["MK-81020"], None, Some(fingerprint(1))),
        )
        .with(
            &units[1].primary,
            disc_id(&["T-15005G"], None, Some(fingerprint(2))),
        );

    assert_eq!(
        level_three(&units, &probe, &FakeDats::new()),
        vec![None, None]
    );

    let dats = FakeDats::new()
        .with_serial("saturn", "MK-81020", "NiGHTS into Dreams... (USA)")
        .with_serial("saturn", "T-15005G", "Panzer Dragoon Saga (Japan) (Disc 1)")
        .with_serial("saturn", "T-15005G", "Panzer Dragoon Saga (Japan) (Disc 2)");
    let keys = level_three(&units, &probe, &dats);
    assert_eq!(fkey(&keys[0]).disc_no, Some(1));
    assert_eq!(keys[1], None);

    // With its disc number read, a disc of that set takes part, and the
    // set's other discs under the same serial are no conflict.
    let units = vec![paper_unit(
        0,
        "saturn",
        "Saturn/Panzer 1.cue",
        UnitKind::DiscSet,
    )];
    let probe = FakeDiscProbe::new().with(
        &units[0].primary,
        disc_id(&["T-15005G"], Some(1), Some(fingerprint(2))),
    );
    let keys = level_three(&units, &probe, &dats);
    assert_eq!(fkey(&keys[0]).disc_no, Some(1));
    assert!(!fkey(&keys[0]).serial_conflict);
    assert!(fkey(&keys[0]).catalogued);
}

#[test]
fn serial_mapped_to_two_titles_is_serial_conflict() {
    let units = vec![
        paper_unit(0, "psx", "PS/Demo.bin", UnitKind::Single),
        paper_unit(1, "psx", "PS/Demo.PBP", UnitKind::Image),
    ];
    let probe = FakeDiscProbe::new()
        .with(
            &units[0].primary,
            disc_id(&["SCUS-94200"], Some(1), Some(fingerprint(4))),
        )
        .with(
            &units[1].primary,
            disc_id(&["SCUS-94200"], Some(1), Some(fingerprint(4))),
        );
    let dats = FakeDats::new()
        .with_serial("psx", "SCUS-94200", "Demo One (USA)")
        .with_serial("psx", "SCUS-94200", "Jampack Vol. 1 (USA)");
    let keys = level_three(&units, &probe, &dats);
    assert!(fkey(&keys[0]).serial_conflict);
    assert_eq!(
        fkey(&keys[0]).link(fkey(&keys[1])),
        Some(FormatLink::ReportOnly {
            why: ReportWhy::SerialConflict
        })
    );
}

#[test]
fn level_three_probes_only_disc_units_and_never_across_systems() {
    let units = vec![
        paper_unit(0, "gb", "GB/Tetris.gb", UnitKind::Single),
        paper_unit(1, "gb", "GB/Tetris.zip", UnitKind::Zip),
        paper_unit(2, "psx", "PS/Game.cue", UnitKind::DiscSet),
        paper_unit(3, "psp", "PSP/Game.iso", UnitKind::Image),
        paper_unit(4, "psx", "PS/Broken.chd", UnitKind::Chd),
    ];
    let probe = FakeDiscProbe::new()
        .with(
            &units[2].primary,
            disc_id(&["SLUS-00001"], Some(1), Some(fingerprint(1))),
        )
        .with(
            &units[3].primary,
            disc_id(&["SLUS-00001"], Some(1), Some(fingerprint(1))),
        )
        .failing(&units[4].primary);
    let keys = level_three(&units, &probe, &FakeDats::new());
    let probed: Vec<PathBuf> = probe.calls().into_iter().map(|(p, _)| p).collect();
    assert_eq!(
        probed,
        vec![
            units[2].primary.clone(),
            units[3].primary.clone(),
            units[4].primary.clone()
        ]
    );
    assert_eq!((&keys[0], &keys[1], &keys[4]), (&None, &None, &None));
    let (a, b) = (fkey(&keys[2]), fkey(&keys[3]));
    assert_ne!(a.key_string(), b.key_string());
    assert_eq!(a.link(b), None, "no key spans two systems");
}

#[test]
fn no_disc_probe_gives_no_format_keys() {
    let units = vec![paper_unit(0, "psx", "PS/Game.cue", UnitKind::DiscSet)];
    assert_eq!(
        level_three(&units, &NoDiscProbe, &FakeDats::new()),
        vec![None]
    );
}

#[test]
fn one_serial_on_two_known_discs_never_links() {
    // Some PS discs boot the first disc's serial; with 1G1R a report-only
    // link would let disc 2 go for disc 1.
    let units = vec![
        paper_unit(0, "psx", "PS/Game (Disc 1).chd", UnitKind::Chd),
        paper_unit(1, "psx", "PS/Game (Disc 2).chd", UnitKind::Chd),
    ];
    let probe = FakeDiscProbe::new()
        .with(
            &units[0].primary,
            disc_id(&["SLPS-01234"], Some(1), Some(fingerprint(1))),
        )
        .with(
            &units[1].primary,
            disc_id(&["SLPS-01234"], Some(2), Some(fingerprint(2))),
        );
    let keys = level_three(&units, &probe, &FakeDats::new());
    let (a, b) = (fkey(&keys[0]), fkey(&keys[1]));
    assert_ne!(a.key_string(), b.key_string());
    assert_eq!(a.link(b), None);
    assert_eq!(b.link(a), None);
}

#[test]
fn serials_are_normalised_and_a_bad_one_gives_no_key() {
    let units = vec![
        paper_unit(0, "psx", "PS/Game.cue", UnitKind::DiscSet),
        paper_unit(1, "psx", "PS/Game.PBP", UnitKind::Image),
        paper_unit(2, "psx", "PS/Set.m3u", UnitKind::MultiDisc),
    ];
    let probe = FakeDiscProbe::new()
        .with(
            &units[0].primary,
            disc_id(&["slus_005.43"], Some(1), Some(fingerprint(1))),
        )
        .with(
            &units[1].primary,
            disc_id(&["SLUS-00543"], Some(1), Some(fingerprint(1))),
        )
        .with(
            &units[2].primary,
            disc_id(
                &["SLUS-00543", "??", "SLUS-00545"],
                None,
                Some(fingerprint(1)),
            ),
        );
    let keys = level_three(&units, &probe, &FakeDats::new());
    assert_eq!(fkey(&keys[0]).serials, vec!["SLUS-00543".to_string()]);
    assert_eq!(fkey(&keys[0]).key_string(), fkey(&keys[1]).key_string());
    assert_eq!(keys[2], None, "a list with an unreadable serial is no list");
}
