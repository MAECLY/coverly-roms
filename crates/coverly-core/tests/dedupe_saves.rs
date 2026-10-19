//! Duplicates: real saves, blank saves and save conflicts (spec §8.2,
//! §8.3). Every file here is synthetic and lives in a temporary folder.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use coverly_core::dedupe::saves::{
    conflict, is_blank, off_card, saves_for, OffCard, SaveIndex, SAVE_LOCATIONS,
};
use coverly_core::dedupe::testing::{ps_card_blank, ps_card_used};
use coverly_core::dedupe::units::Unit;
use coverly_core::dedupe::{SaveInfo, SaveKey, UnitKind};

// --- Builders ---

fn write(path: &Path, bytes: &[u8]) -> PathBuf {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    path.to_path_buf()
}

fn unit(id: usize, system: &str, primary: &Path, kind: UnitKind) -> Unit {
    Unit {
        id,
        system: system.to_string(),
        primary: primary.to_path_buf(),
        files: vec![primary.to_path_buf()],
        kind,
        size: 1024,
        payload_size: Some(1024),
        mtime_ns: 0,
        file_id: None,
        patches: Vec::new(),
        rel_dir: PathBuf::new(),
        stem: primary.file_stem().unwrap().to_string_lossy().into_owned(),
        ext: primary
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default(),
        in_library: true,
    }
}

fn paths(saves: &[SaveInfo]) -> Vec<PathBuf> {
    saves.iter().map(|s| s.path.clone()).collect()
}

/// Bytes that are not blank: a little game progress.
fn progress(seed: u8) -> Vec<u8> {
    (0..8192u32)
        .map(|i| (i as u8).wrapping_mul(seed) ^ 0x5A)
        .collect()
}

// --- Blank detection ---

#[test]
fn all_zero_and_all_ff_are_blank() {
    for (bytes, ext) in [(vec![0u8; 8192], "srm"), (vec![0xFF; 32768], "sav")] {
        let len = bytes.len() as u64;
        assert!(is_blank(Cursor::new(bytes), len, ext).unwrap(), "{ext}");
    }
    let mut almost = vec![0u8; 8192];
    almost[4000] = 1;
    assert!(!is_blank(Cursor::new(almost), 8192, "srm").unwrap());
    let mut mixed = vec![0xFF; 8192];
    mixed[0] = 0;
    assert!(!is_blank(Cursor::new(mixed), 8192, "srm").unwrap());
    assert!(!is_blank(Cursor::new(progress(3)), 8192, "state1").unwrap());
    assert!(is_blank(Cursor::new(Vec::new()), 0, "srm").unwrap());
}

#[test]
fn blank_ps_card_is_no_save() {
    let card = ps_card_blank();
    assert_eq!(card.len(), 128 * 1024);
    assert_eq!(&card[..2], b"MC");
    // Formatted, so neither all 0x00 nor all 0xFF.
    let non_zero = card.iter().filter(|b| **b != 0).count();
    assert!(non_zero > 100 && non_zero < 400, "{non_zero}");
    assert!(is_blank(Cursor::new(card.clone()), card.len() as u64, "srm").unwrap());
    assert!(is_blank(Cursor::new(card.clone()), card.len() as u64, "mcd").unwrap());

    let used = ps_card_used();
    assert!(!is_blank(Cursor::new(used.clone()), used.len() as u64, "mcr").unwrap());

    // DexDrive cards carry a 3904-byte header before the card.
    let mut gme = vec![0u8; 3904];
    gme[..12].copy_from_slice(b"123-456-STD\0");
    gme.extend_from_slice(&card);
    assert!(is_blank(Cursor::new(gme.clone()), gme.len() as u64, "gme").unwrap());

    // The owner's three identical blank cards decide nothing.
    let dir = tempfile::tempdir().unwrap();
    let ps = dir.path().join("ROMS/PS");
    let mut saves = Vec::new();
    for (i, name) in ["Crash Bash", "Tekken 3", "Iron fist 3"].iter().enumerate() {
        let rom = write(&ps.join(format!("{name}.bin")), b"disc");
        write(&ps.join(format!("{name}.srm")), &card);
        let u = unit(i, "psx", &rom, UnitKind::DiscSet);
        let found = saves_for(&u, "anbernic-stock", dir.path(), &[]);
        assert!(found.is_empty(), "{name}: {found:?}");
        saves.push((i, found));
    }
    assert_eq!(conflict(&saves), None);
}

#[test]
fn a_card_sized_save_that_only_starts_with_mc_is_real() {
    // A 128 KiB save of another system (N64 flash, GBA) that happens to
    // begin with `MC`: its frame bytes are game data, not free-block
    // markers, so it is not a blank memory card.
    let mut bytes: Vec<u8> = (0..128 * 1024u32)
        .map(|i| (i as u8).wrapping_mul(13) ^ 0x21)
        .collect();
    bytes[..2].copy_from_slice(b"MC");
    for i in 1..=15 {
        bytes[i * 128] = 0x12;
    }
    let len = bytes.len() as u64;
    assert!(!is_blank(Cursor::new(bytes.clone()), len, "fla").unwrap());
    assert!(!is_blank(Cursor::new(bytes), len, "srm").unwrap());

    // A card whose saves were deleted holds nothing the game shows.
    let mut deleted = ps_card_blank();
    deleted[128] = 0xA1;
    assert!(is_blank(Cursor::new(deleted), len, "mcr").unwrap());
}

// --- Conflicts ---

#[test]
fn different_real_saves_make_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let gb = dir.path().join("ROMS/GB");
    let a = write(&gb.join("Tetris (World).gb"), b"rom");
    let b = write(&gb.join("Tetris (JUE) (V1.0) [!].gb"), b"rom");
    write(&gb.join("Tetris (World).sav"), &progress(3));
    write(&gb.join("Tetris (JUE) (V1.0) [!].sav"), &progress(7));
    let sa = saves_for(
        &unit(0, "gb", &a, UnitKind::Single),
        "anbernic-stock",
        dir.path(),
        &[],
    );
    let sb = saves_for(
        &unit(1, "gb", &b, UnitKind::Single),
        "anbernic-stock",
        dir.path(),
        &[],
    );
    assert_eq!(sa.len(), 1);
    assert_eq!(sb.len(), 1);
    assert_ne!(sa[0].sha1, sb[0].sha1);
    assert_eq!(sa[0].sha1.len(), 40);
    assert_eq!(sa[0].sha1, sa[0].sha1.to_lowercase());
    assert_eq!(sa[0].size, 8192);
    assert_eq!(sa[0].keyed_by, SaveKey::Stem);

    let found = conflict(&[(0, sa.clone()), (1, sb.clone())]).expect("a conflict");
    let mut want = vec![sa[0].path.clone(), sb[0].path.clone()];
    want.sort();
    assert_eq!(paths(&found), want);

    // A third copy with no save does not undo the conflict.
    assert!(conflict(&[(0, sa), (1, sb), (2, Vec::new())]).is_some());
}

#[test]
fn equal_real_saves_are_not_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let gb = dir.path().join("ROMS/GB");
    let a = write(&gb.join("Tetris 2.gb"), b"rom");
    let b = write(&gb.join("Tetris 2 (1).gb"), b"rom");
    write(&gb.join("Tetris 2.sav"), &progress(5));
    write(&gb.join("Tetris 2 (1).sav"), &progress(5));
    let sa = saves_for(
        &unit(0, "gb", &a, UnitKind::Single),
        "anbernic-stock",
        dir.path(),
        &[],
    );
    let sb = saves_for(
        &unit(1, "gb", &b, UnitKind::Single),
        "anbernic-stock",
        dir.path(),
        &[],
    );
    assert_eq!(sa[0].sha1, sb[0].sha1);
    assert_eq!(conflict(&[(0, sa.clone()), (1, sb)]), None);

    // One copy with a save is no conflict either: the save decides.
    assert_eq!(conflict(&[(0, sa), (1, Vec::new())]), None);
    assert_eq!(conflict(&[]), None);
}

// --- Where saves are found ---

#[test]
fn same_folder_saves_match_every_dot_case_insensitively() {
    let dir = tempfile::tempdir().unwrap();
    let snes = dir.path().join("ROMS/SFC");
    let rom = write(&snes.join("Super Mario World (USA).sfc"), b"rom");
    let srm = write(&snes.join("super mario world (usa).SRM"), &progress(2));
    let auto = write(
        &snes.join("Super Mario World (USA).state.auto"),
        &progress(4),
    );
    // Not saves: other stems, a stem that only starts the same, art, cheats,
    // AppleDouble twins.
    write(
        &snes.join("Super Mario World (USA) (Rev 1).srm"),
        &progress(6),
    );
    write(&snes.join("Super Mario World (USA).cht"), b"cheat");
    write(&snes.join("Super Mario World (USA).png"), b"png");
    write(&snes.join("._Super Mario World (USA).srm"), &progress(8));

    let found = saves_for(
        &unit(0, "snes", &rom, UnitKind::Single),
        "anbernic-stock",
        dir.path(),
        &[],
    );
    let mut want = vec![srm, auto];
    want.sort();
    assert_eq!(paths(&found), want);
}

#[test]
fn retroarch_sort_by_core_saves_are_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let rom = write(&root.join("ROMS/SFC/Chrono Trigger (USA).sfc"), b"rom");
    let srm = write(
        &root.join("saves/Snes9x/Chrono Trigger (USA).srm"),
        &progress(2),
    );
    let flat = write(&root.join("saves/Chrono Trigger (USA).rtc"), &progress(3));
    let st1 = write(
        &root.join("States/Snes9x/Chrono Trigger (USA).state1"),
        &progress(4),
    );
    write(
        &root.join("saves/Snes9x/Chrono Trigger (Japan).srm"),
        &progress(5),
    );
    // A blank save in a core folder is no save.
    write(
        &root.join("saves/bsnes/Chrono Trigger (USA).srm"),
        &[0u8; 2048],
    );

    let found = saves_for(
        &unit(0, "snes", &rom, UnitKind::Single),
        "retroarch",
        root,
        &[],
    );
    let mut want = vec![srm, flat, st1];
    want.sort();
    assert_eq!(paths(&found), want);
    assert!(found.iter().all(|s| s.keyed_by == SaveKey::Stem));

    // One index serves many units with the same answers.
    let index = SaveIndex::new("retroarch", root);
    let again = index.saves_for(&unit(0, "snes", &rom, UnitKind::Single), &[]);
    assert_eq!(again, found);
}

#[test]
fn duckstation_memcard_by_serial_is_shared() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = write(&root.join("ROMS/PS/Crash Bash.bin"), b"disc");
    let pbp = write(&root.join("ROMS/PS/Crash Bash (USA).PBP"), b"pbp");
    let card = write(&root.join("memcards/SCUS-94570_1.mcd"), &ps_card_used());
    // Another game's card, a serial that only starts the same, a blank card.
    write(&root.join("memcards/SLUS-01161_1.mcd"), &ps_card_used());
    write(&root.join("memcards/SCUS-945701_1.mcd"), &ps_card_used());
    write(&root.join("memcards/SCUS-94570_2.mcd"), &ps_card_blank());

    let serials = ["SCUS94570".to_string()];
    let a = saves_for(
        &unit(0, "psx", &bin, UnitKind::DiscSet),
        "anbernic-stock",
        root,
        &serials,
    );
    let b = saves_for(
        &unit(1, "psx", &pbp, UnitKind::Image),
        "anbernic-stock",
        root,
        &serials,
    );
    assert_eq!(paths(&a), vec![card.clone()]);
    assert_eq!(a[0].keyed_by, SaveKey::Serial);
    assert_eq!(a, b);
    assert_eq!(conflict(&[(0, a), (1, b)]), None);

    // Without a serial the card is not tied to the disc.
    let none = saves_for(
        &unit(0, "psx", &bin, UnitKind::DiscSet),
        "anbernic-stock",
        root,
        &[],
    );
    assert!(none.is_empty());
}

#[test]
fn ppsspp_savedata_and_mame_nvram_are_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let iso = write(&root.join("ROMS/PSP/Lumines.iso"), b"iso");
    let data = write(
        &root.join("PSP/SAVEDATA/ULUS10046DATA00/DATA.BIN"),
        &progress(2),
    );
    write(
        &root.join("PSP/SAVEDATA/ULUS10047DATA00/DATA.BIN"),
        &progress(3),
    );
    let found = saves_for(
        &unit(0, "psp", &iso, UnitKind::Image),
        "anbernic-stock",
        root,
        &["ULUS-10046".to_string()],
    );
    assert_eq!(paths(&found), vec![data]);
    assert_eq!(found[0].keyed_by, SaveKey::Serial);

    let set = write(&root.join("ROMS/ARCADE/ffight.zip"), b"zip");
    let nv = write(&root.join("nvram/ffight/eeprom"), &progress(4));
    write(&root.join("nvram/ffightu/eeprom"), &progress(5));
    let found = saves_for(
        &unit(1, "arcade", &set, UnitKind::Arcade),
        "anbernic-stock",
        root,
        &[],
    );
    assert_eq!(paths(&found), vec![nv]);
    assert_eq!(found[0].keyed_by, SaveKey::Set);
}

#[test]
fn shared_saves_never_make_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = write(&root.join("ROMS/PS/Crash Bash.bin"), b"disc");
    let pbp = write(&root.join("ROMS/PS/Crash Bash (USA).PBP"), b"pbp");
    write(&root.join("memcards/SCUS-94570_1.mcd"), &ps_card_used());
    write(&root.join("ROMS/PS/Crash Bash.srm"), &progress(3));
    let serials = ["SCUS-94570".to_string()];
    let a = saves_for(
        &unit(0, "psx", &bin, UnitKind::DiscSet),
        "anbernic-stock",
        root,
        &serials,
    );
    let b = saves_for(
        &unit(1, "psx", &pbp, UnitKind::Image),
        "anbernic-stock",
        root,
        &serials,
    );
    assert_eq!(a.len(), 2);
    assert_eq!(b.len(), 1);
    // The bin has its own save; the card is everybody's. No conflict.
    assert_eq!(conflict(&[(0, a), (1, b)]), None);
}

// --- Off the card ---

#[test]
fn muos_reports_off_card_saves() {
    assert_eq!(off_card("muos"), OffCard::Internal);
    assert_eq!(off_card("anbernic-stock"), OffCard::None);
    assert_eq!(off_card("retroarch"), OffCard::None);
    assert_eq!(off_card("no-such-profile"), OffCard::None);
    assert!(SAVE_LOCATIONS.iter().any(|(id, _)| *id == "muos"));
}
