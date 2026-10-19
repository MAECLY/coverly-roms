//! Duplicates: level 1 keys and bucketing (spec §5.2, §10).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use coverly_core::cache::Cache;
use coverly_core::dedupe::keys::{
    level_one, parse_chd_header, track_layout, ExactKey, KeyCtx, KeyedUnit,
};
use coverly_core::dedupe::source::ZipListing;
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

fn keys_of(units: &[Unit]) -> Vec<KeyedUnit> {
    let db = SystemDb::builtin();
    let ctx = KeyCtx::new(&db, &ZipListing);
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
    let mut ctx = KeyCtx::new(&db, &ZipListing);
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
    let listing = CountingListing::new(ZipListing);
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
    let mut ctx = KeyCtx::new(&db, &ZipListing);
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
    let mut ctx = KeyCtx::new(&db, &ZipListing);
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
    let mut ctx = KeyCtx::new(&db, &ZipListing);
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
    let mut ctx = KeyCtx::new(&db, &ZipListing);
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
    let mut ctx = KeyCtx::new(&db, &ZipListing);
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
