//! Disc identification from synthetic sectors (spec §7): ISO 9660, the
//! PlayStation rules over it, and serial normalisation. Every fixture is
//! built here from crafted bytes; none comes from a real disc.

use coverly_core::disc::iso9660::{
    find_root_file, identify_playstation, identify_playstation_with, read_extent, read_pvd,
    FileExtent, PlayStationDisc, MAX_ROOT_RECORDS, SYSTEM_CNF_CAP,
};
use coverly_core::disc::serial::{boot_file_serial, normalise_serial};
use coverly_core::disc::source::MAX_WINDOW;
use coverly_core::disc::testing::{self as dt, IsoBuilder};
use coverly_core::disc::{DiscSystem, FileSource, SectorLayout, SectorSource, WindowSource};
use coverly_core::identify::sniff::fixtures as fx;

fn ps(bytes: Vec<u8>, len: Option<u64>) -> Option<PlayStationDisc> {
    identify_playstation(&mut WindowSource::new(bytes), len).unwrap()
}

#[test]
fn psx_boot_serial_raw_2352() {
    let bytes = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");
    let len = bytes.len() as u64;
    let mut src = WindowSource::new(bytes.clone());
    assert_eq!(src.layout(), SectorLayout::Raw2352Mode2);
    let disc = identify_playstation(&mut src, Some(len)).unwrap().unwrap();
    assert_eq!(disc.system, DiscSystem::Known("psx"));
    assert_eq!(disc.serial.as_deref(), Some("SLUS-00862"));
    assert_eq!(disc.confidence, 0.95);
    assert_eq!(disc.detail, "SYSTEM.CNF BOOT + licence text");
    assert!(disc.licence);
    assert!(disc.volume.is_some());

    // The rule sniff already had keeps working on its own fixtures.
    for raw in [false, true] {
        let disc = ps(fx::psx_iso(raw, false), None).unwrap();
        assert_eq!(disc.system, DiscSystem::Known("psx"));
        assert_eq!(disc.serial.as_deref(), Some("SLUS-00001"));
        assert_eq!(disc.confidence, 0.95);
    }
}

#[test]
fn psx_boot_serial_iso_2048() {
    let bytes = dt::psx_track(false, "cdrom:\\SCES_123.45;1");
    let disc = ps(bytes.clone(), None).unwrap();
    assert_eq!(disc.system, DiscSystem::Known("psx"));
    assert_eq!(disc.serial.as_deref(), Some("SCES-12345"));
    assert_eq!(disc.confidence, 0.95);

    // The same image read from a file on disk.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.iso");
    std::fs::write(&path, &bytes).unwrap();
    let mut src = FileSource::open(&path).unwrap();
    let from_file = identify_playstation(&mut src, Some(bytes.len() as u64))
        .unwrap()
        .unwrap();
    assert_eq!(from_file, disc);

    // A BOOT line without a serial-shaped file name still boots a PS1.
    let disc = ps(dt::psx_track(false, "cdrom:\\PSX.EXE;1"), None).unwrap();
    assert_eq!(disc.system, DiscSystem::Known("psx"));
    assert_eq!(disc.serial, None);
    assert_eq!(disc.confidence, 0.95);

    // Another ISO 9660 disc, or no ISO 9660 at all, says nothing.
    let other = IsoBuilder::new("LINUX").build();
    assert_eq!(ps(other, None), None);
    assert_eq!(ps(vec![0; 40 * 2048], None), None);
    assert_eq!(ps(Vec::new(), None), None);
}

#[test]
fn psx_pvd_only_0_80() {
    let bytes = IsoBuilder::new("PLAYSTATION").raw(true).build();
    let disc = ps(bytes, None).unwrap();
    assert_eq!(disc.system, DiscSystem::Known("psx"));
    assert_eq!(disc.serial, None);
    assert_eq!(disc.confidence, 0.80);
    assert_eq!(disc.detail, "PLAYSTATION");
    // No boot file, no fingerprint (spec §5.4 needs its SHA1).
    assert_eq!(disc.volume, None);

    let mut renamed = fx::psx_iso(false, false);
    fx::drop_system_cnf(&mut renamed);
    let disc = ps(renamed, None).unwrap();
    assert_eq!(disc.confidence, 0.80);
    assert_eq!(disc.serial, None);
}

#[test]
fn psx_licence_text_is_extra_evidence() {
    // The licence string of raw sector 4, with its odd real-world spacing.
    let with = IsoBuilder::new("PLAYSTATION")
        .raw(true)
        .licence(dt::PSX_LICENCE)
        .build();
    let disc = ps(with, None).unwrap();
    assert!(disc.licence);
    assert_eq!(disc.confidence, 0.80, "evidence, not a stronger verdict");
    assert_eq!(disc.detail, "PLAYSTATION + licence text");

    let without = dt::psx_track_with(false, "cdrom:\\SLUS_008.62;1", None);
    let disc = ps(without, None).unwrap();
    assert!(!disc.licence);
    assert_eq!(disc.detail, "SYSTEM.CNF BOOT");

    // The licence text alone, without a PLAYSTATION volume, is not a disc.
    let lone = IsoBuilder::new("CDROM").licence(dt::PSX_LICENCE).build();
    assert_eq!(ps(lone, None), None);
}

#[test]
fn ps2_boot2_unsupported_system() {
    let disc = ps(dt::ps2_iso(), None).unwrap();
    assert_eq!(disc.system, DiscSystem::Unsupported("PlayStation 2"));
    assert_eq!(disc.serial.as_deref(), Some("SLUS-20062"));
    assert_eq!(disc.detail, "SYSTEM.CNF BOOT2");
    assert!(disc.volume.is_some());

    // BOOT2 wins over a BOOT line in the same file.
    let both = IsoBuilder::new("PLAYSTATION")
        .file(
            "SYSTEM.CNF;1",
            b"BOOT = cdrom:\\SLUS_008.62;1\r\nBOOT2 = cdrom0:\\SLES_500.03;1\r\n",
        )
        .build();
    let disc = ps(both, None).unwrap();
    assert_eq!(disc.system, DiscSystem::Unsupported("PlayStation 2"));
    assert_eq!(disc.serial.as_deref(), Some("SLES-50003"));

    // sniff's PS2 fixture agrees.
    let disc = ps(fx::psx_iso(false, true), None).unwrap();
    assert_eq!(disc.system, DiscSystem::Unsupported("PlayStation 2"));
}

#[test]
fn ps2_by_size_unsupported_system() {
    // No SYSTEM.CNF found, but no CD holds this much: a PS2 DVD.
    let pvd_only = IsoBuilder::new("PLAYSTATION").build();
    let dvd_len = 4_700_000_000;
    let disc = ps(pvd_only.clone(), Some(dvd_len)).unwrap();
    assert_eq!(disc.system, DiscSystem::Unsupported("PlayStation 2"));
    assert_eq!(disc.detail, "PLAYSTATION on a DVD-size image");

    // The volume itself may say so: a window over the head of a DVD.
    let big = IsoBuilder::new("PLAYSTATION").space_size(2_000_000).build();
    let disc = ps(big, None).unwrap();
    assert_eq!(disc.system, DiscSystem::Unsupported("PlayStation 2"));

    // CD-sized stays PS1, and a found BOOT line beats the size.
    assert_eq!(
        ps(pvd_only, Some(700_000_000)).unwrap().system,
        DiscSystem::Known("psx")
    );
    let boot = dt::psx_track(false, "cdrom:\\SLUS_008.62;1");
    assert_eq!(
        ps(boot, Some(dvd_len)).unwrap().system,
        DiscSystem::Known("psx")
    );
}

#[test]
fn serial_normalisation_table() {
    let table: &[(&str, Option<&str>)] = &[
        // PlayStation boot names and the shapes DATs and users write.
        ("SLUS_008.62", Some("SLUS-00862")),
        ("SLUS_008.62;1", Some("SLUS-00862")),
        ("slus_008.62", Some("SLUS-00862")),
        ("SLUS-00862", Some("SLUS-00862")),
        ("SLUS00862", Some("SLUS-00862")),
        ("SLUS 00862", Some("SLUS-00862")),
        ("SCES_123.45", Some("SCES-12345")),
        ("SLPS_012.34", Some("SLPS-01234")),
        ("SLUS_200.62", Some("SLUS-20062")),
        // Sega product numbers, padded to their field.
        ("T-5704H   ", Some("T-5704H")),
        ("T-40214N  ", Some("T-40214N")),
        ("T-40214N", Some("T-40214N")),
        ("MK-81086  ", Some("MK-81086")),
        ("t-5704h\0\0\0", Some("T-5704H")),
        ("GS-9076   ", Some("GS-9076")),
        // Sega CD: `GM <8-char serial>-<version>`.
        ("GM MK-4407 -00", Some("MK-4407")),
        ("GM T-93185 -00  ", Some("T-93185")),
        ("GM G-6017", Some("G-6017")),
        // Not a serial.
        ("", None),
        ("          ", None),
        ("PSX.EXE", None),
        ("SLUS_008.6", None),
        ("SLUS_0086.2", None),
        ("T 5704H", None),
        ("-T5704H", None),
        ("T-5704H-", None),
        ("ABCDEFGH", None),
        ("T-57\u{e9}4H", None),
        ("T-5704\nH", None),
        ("T-0123456789ABCDEF0", None),
    ];
    for (raw, want) in table {
        assert_eq!(normalise_serial(raw).as_deref(), *want, "{raw:?}");
    }

    let boots: &[(&str, Option<&str>)] = &[
        ("cdrom:\\SLUS_008.62;1", Some("SLUS-00862")),
        ("cdrom0:\\SLUS_200.62;1", Some("SLUS-20062")),
        ("cdrom:SLUS_008.62;1", Some("SLUS-00862")),
        ("cdrom:\\GAME\\SCES_123.45;1", Some("SCES-12345")),
        ("cdrom:/SLUS_008.62;1", Some("SLUS-00862")),
        ("  cdrom:\\SLUS_008.62;1  ", Some("SLUS-00862")),
        ("cdrom:\\PSX.EXE;1", None),
        ("", None),
    ];
    for (boot, want) in boots {
        assert_eq!(boot_file_serial(boot).as_deref(), *want, "{boot:?}");
    }
}

#[test]
fn root_directory_capped_at_64_records() {
    let cnf = b"BOOT = cdrom:\\SLUS_008.62;1\r\n";
    // `.` and `..` count: the boot file is record number 64, the last read.
    let last = IsoBuilder::new("PLAYSTATION")
        .filler(MAX_ROOT_RECORDS - 3)
        .file("SYSTEM.CNF;1", cnf)
        .build();
    let mut src = WindowSource::new(last.clone());
    let pvd = read_pvd(&mut src).unwrap().unwrap();
    assert_eq!(pvd.system_id, "PLAYSTATION");
    assert!(pvd.root.is_dir());
    let found = find_root_file(&mut src, &pvd, "SYSTEM.CNF", 4096).unwrap();
    assert_eq!(found.as_deref(), Some(&cnf[..]));
    assert_eq!(ps(last, None).unwrap().confidence, 0.95);

    // One more record before it and it is out of reach.
    let past = IsoBuilder::new("PLAYSTATION")
        .filler(MAX_ROOT_RECORDS - 2)
        .file("SYSTEM.CNF;1", cnf)
        .build();
    let mut src = WindowSource::new(past.clone());
    let pvd = read_pvd(&mut src).unwrap().unwrap();
    assert_eq!(
        find_root_file(&mut src, &pvd, "SYSTEM.CNF", 4096).unwrap(),
        None
    );
    assert_eq!(ps(past, None).unwrap().confidence, 0.80);

    // The cap on the file's bytes holds, and names match without `;1`.
    let mut src = WindowSource::new(dt::psx_track(false, "cdrom:\\SLUS_008.62;1"));
    let pvd = read_pvd(&mut src).unwrap().unwrap();
    let head = find_root_file(&mut src, &pvd, "system.cnf;1", 4).unwrap();
    assert_eq!(head.as_deref(), Some(&b"BOOT"[..]));

    // A directory is not a file, and depth is 1: no subdirectory is read.
    let nested = IsoBuilder::new("PLAYSTATION").dir("SYSTEM.CNF").build();
    let mut src = WindowSource::new(nested);
    let pvd = read_pvd(&mut src).unwrap().unwrap();
    assert_eq!(
        find_root_file(&mut src, &pvd, "SYSTEM.CNF", 4096).unwrap(),
        None
    );
}

#[test]
fn hostile_volume_descriptors_stay_bounded() {
    // A root directory that claims 4 GiB at the end of the address space.
    let mut bytes = IsoBuilder::new("PLAYSTATION").build();
    let pvd_at = 16 * 2048;
    bytes[pvd_at + 156 + 2..pvd_at + 156 + 6].copy_from_slice(&u32::MAX.to_le_bytes());
    bytes[pvd_at + 156 + 10..pvd_at + 156 + 14].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut src = WindowSource::new(bytes.clone());
    let pvd = read_pvd(&mut src).unwrap().unwrap();
    assert_eq!(
        find_root_file(&mut src, &pvd, "SYSTEM.CNF", 4096).unwrap(),
        None
    );
    assert_eq!(ps(bytes, None).unwrap().confidence, 0.80);

    // Garbage records (too short, names past the record) end the walk.
    let mut garbled = dt::psx_track(false, "cdrom:\\SLUS_008.62;1");
    let root = 20 * 2048;
    garbled[root] = 7; // shorter than any record
    assert_eq!(ps(garbled.clone(), None).unwrap().confidence, 0.80);
    garbled[root] = 40;
    garbled[root + 32] = 200; // name longer than the record
    assert_eq!(ps(garbled, None).unwrap().confidence, 0.80);

    // A file record pointing past the image reads as missing.
    let mut far = dt::psx_track(false, "cdrom:\\SLUS_008.62;1");
    let mut src = WindowSource::new(far.clone());
    let pvd = read_pvd(&mut src).unwrap().unwrap();
    assert!(find_root_file(&mut src, &pvd, "SYSTEM.CNF", 4096)
        .unwrap()
        .is_some());
    let pos = far.windows(12).position(|w| w == b"SYSTEM.CNF;1").unwrap() - 33;
    far[pos + 2..pos + 6].copy_from_slice(&0x00FF_FFFFu32.to_le_bytes());
    assert_eq!(ps(far, None).unwrap().confidence, 0.80);

    // Not a primary volume descriptor.
    let mut wrong = IsoBuilder::new("PLAYSTATION").build();
    wrong[pvd_at] = 2;
    assert!(read_pvd(&mut WindowSource::new(wrong)).unwrap().is_none());
}

#[test]
fn fingerprint_equal_across_bin_and_iso() {
    let cnf = b"BOOT = cdrom:\\SLUS_008.62;1\r\nTCB = 4\r\n";
    let disc = |raw: bool| {
        IsoBuilder::new("PLAYSTATION")
            .raw(raw)
            .dates(*b"1999073012000000\x24", *b"1999080114300000\x24")
            .licence(dt::PSX_LICENCE)
            .file("SYSTEM.CNF;1", cnf)
            .build()
    };
    let bin = ps(disc(true), None).unwrap();
    let iso = ps(disc(false), None).unwrap();
    let (a, b) = (bin.volume.unwrap(), iso.volume.unwrap());
    assert_eq!(a, b);
    assert_eq!(&a.created, b"1999073012000000\x24");
    assert_eq!(&a.modified, b"1999080114300000\x24");
    assert_eq!(a.boot_sha1.len(), 40);
    assert!(a
        .boot_sha1
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase()));

    // Another master of the same serial: other dates, other fingerprint.
    let remaster = IsoBuilder::new("PLAYSTATION")
        .dates(*b"2001010100000000\x24", *b"2001010100000000\x24")
        .file("SYSTEM.CNF;1", cnf)
        .build();
    let c = ps(remaster, None).unwrap().volume.unwrap();
    assert_ne!(c, a);
    assert_eq!(c.boot_sha1, a.boot_sha1);

    // A patched SYSTEM.CNF changes the boot SHA1.
    let patched = IsoBuilder::new("PLAYSTATION")
        .dates(*b"1999073012000000\x24", *b"1999080114300000\x24")
        .file(
            "SYSTEM.CNF;1",
            b"BOOT = cdrom:\\SLUS_008.62;1\r\nTCB = 5\r\n",
        )
        .build();
    let d = ps(patched, None).unwrap().volume.unwrap();
    assert_ne!(d.boot_sha1, a.boot_sha1);
    assert_eq!(d.space_size, a.space_size);
}

/// The real shape of the PS1 disc of spec §1 (`Next Tetris, The (USA)`):
/// a 4 MiB `PSX.BSE` before `SYSTEM.CNF` puts the boot file at LBA ~2089,
/// past the 4 MiB head window the plan reads from an archive.
fn psx_with_boot_past_window(cnf: &[u8]) -> Vec<u8> {
    IsoBuilder::new("PLAYSTATION")
        .raw(true)
        .licence(dt::PSX_LICENCE)
        .file("PSX.BSE;1", &vec![0x55; 4_227_072])
        .file("SLUS_008.62;1", b"PS-X EXE")
        .file("SYSTEM.CNF;1", cnf)
        .build()
}

#[test]
fn psx_system_cnf_past_the_window_is_pending() {
    let cnf = b"BOOT = cdrom:\\SLUS_008.62;1\r\nTCB = 4\r\n";
    let full = psx_with_boot_past_window(cnf);
    assert!(full.len() > MAX_WINDOW);
    let len = full.len() as u64;

    // The head alone: the root directory is there, SYSTEM.CNF is not.
    let mut head = WindowSource::new(full.clone());
    let disc = identify_playstation(&mut head, Some(len)).unwrap().unwrap();
    assert_eq!(disc.system, DiscSystem::Known("psx"));
    assert_eq!(disc.confidence, 0.80, "no boot line read yet");
    assert_eq!(disc.serial.as_deref(), Some("SLUS-00862"), "root file name");
    assert_eq!(disc.detail, "PLAYSTATION + boot file name + licence text");
    assert_eq!(disc.volume, None);
    let ext = disc
        .pending_boot
        .expect("SYSTEM.CNF listed past the window");
    assert_eq!(ext.size as usize, cnf.len());

    // The caller fetches exactly those stored sectors and finishes.
    let range = ext.stored_range(head.layout(), SYSTEM_CNF_CAP).unwrap();
    assert!(range.start > MAX_WINDOW as u64);
    assert!(range.end - range.start <= 2 * 2352);
    let stored = full[range.start as usize..range.end as usize].to_vec();
    let mut tail = WindowSource::at(ext.lba, stored, head.layout());
    let bytes = read_extent(&mut tail, ext, SYSTEM_CNF_CAP)
        .unwrap()
        .unwrap();
    assert_eq!(bytes, cnf);
    let done = identify_playstation_with(&mut head, Some(len), Some(&bytes))
        .unwrap()
        .unwrap();

    // The same verdict as reading the whole image from disk.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("track01.bin");
    std::fs::write(&path, &full).unwrap();
    let whole = identify_playstation(&mut FileSource::open(&path).unwrap(), Some(len))
        .unwrap()
        .unwrap();
    assert_eq!(done, whole);
    assert_eq!(whole.serial.as_deref(), Some("SLUS-00862"));
    assert_eq!(whole.confidence, 0.95);
    assert_eq!(whole.detail, "SYSTEM.CNF BOOT + licence text");
    assert_eq!(whole.pending_boot, None);
    assert!(whole.volume.is_some());
}

#[test]
fn psx_root_name_serial_needs_one_candidate() {
    // Two serial-named root files: no guess.
    let two = IsoBuilder::new("PLAYSTATION")
        .file("SLUS_008.62;1", b"x")
        .file("SLUS_008.63;1", b"y")
        .build();
    let disc = ps(two, None).unwrap();
    assert_eq!(disc.serial, None);
    assert_eq!(disc.detail, "PLAYSTATION");
    assert_eq!(disc.pending_boot, None, "no SYSTEM.CNF listed at all");

    // A directory named like a serial is not a boot file.
    let dir = IsoBuilder::new("PLAYSTATION").dir("SLUS_008.62").build();
    assert_eq!(ps(dir, None).unwrap().serial, None);

    // A read boot line wins over the root names.
    let both = IsoBuilder::new("PLAYSTATION")
        .file("SCES_123.45;1", b"x")
        .file("SYSTEM.CNF;1", b"BOOT = cdrom:\\SLUS_008.62;1\r\n")
        .build();
    let disc = ps(both, None).unwrap();
    assert_eq!(disc.serial.as_deref(), Some("SLUS-00862"));
    assert_eq!(disc.confidence, 0.95);
}

#[test]
fn pending_boot_extents_stay_bounded() {
    // An extent at the end of the address space has no stored range.
    let far = FileExtent {
        lba: u32::MAX,
        size: u32::MAX,
    };
    for layout in [SectorLayout::Iso2048, SectorLayout::Raw2352Mode2] {
        let r = far.stored_range(layout, SYSTEM_CNF_CAP);
        assert!(
            r.as_ref().is_none_or(|r| r.end - r.start <= 2 * 2352),
            "{r:?}"
        );
    }
    // The cap bounds the range whatever the declared size.
    let big = FileExtent {
        lba: 3000,
        size: u32::MAX,
    };
    let r = big
        .stored_range(SectorLayout::Raw2352Mode2, SYSTEM_CNF_CAP)
        .unwrap();
    assert_eq!(r, 3000 * 2352..3002 * 2352);
    // A window that starts later reads nothing before its first sector.
    let mut w = WindowSource::at(10, vec![0; 2048 * 2], SectorLayout::Iso2048);
    let mut out = [0u8; 2048];
    assert!(!w.read_user(9, &mut out).unwrap());
    assert!(w.read_user(10, &mut out).unwrap());
    assert!(w.read_user(11, &mut out).unwrap());
    assert!(!w.read_user(12, &mut out).unwrap());
    assert_eq!(w.raw_sector(9).unwrap(), None);
    assert!(w.raw_sector(11).unwrap().is_some());
    // An extent the window does not hold reads as missing, never panics.
    let ext = FileExtent {
        lba: 11,
        size: 3 * 2048,
    };
    assert_eq!(read_extent(&mut w, ext, usize::MAX).unwrap(), None);
}

// ---- Task 21: Sega IP.BIN, PC Engine CD, Neo Geo CD, 3DO, DiscReader ----

use coverly_core::disc::{DiscIdSource, DiscIdentity, DiscReader};

fn id(bytes: Vec<u8>) -> Option<DiscIdentity> {
    DiscReader::identify(&mut WindowSource::new(bytes))
}

#[test]
fn saturn_product_t5704h() {
    let bytes = dt::saturn_track("T-5704H   ");
    let mut src = WindowSource::new(bytes.clone());
    assert_eq!(src.layout(), SectorLayout::Raw2352Mode1);
    let disc = DiscReader::identify(&mut src).unwrap();
    assert_eq!(disc.system, DiscSystem::Known("saturn"));
    assert_eq!(disc.serials, ["T-5704H"]);
    assert_eq!(disc.disc_no, Some(1));
    assert_eq!(disc.source, DiscIdSource::IpBin);
    assert_eq!(disc.confidence, 0.95);
    assert_eq!(disc.detail, "SEGA SEGASATURN");
    // The fingerprint hashes IP.BIN, the first 32 KiB of the track.
    let volume = disc.volume.clone().expect("PVD and IP.BIN in the window");
    assert_eq!(volume.boot_sha1.len(), 40);

    // The same track from a file on disk gives the same identity.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Tetris Plus (USA) (Track 01).bin");
    std::fs::write(&path, &bytes).unwrap();
    let from_file = DiscReader::identify(&mut FileSource::open(&path).unwrap()).unwrap();
    assert_eq!(from_file, disc);

    // A head too short for the fingerprint still identifies the disc.
    let short = id(bytes[..4 * 2352].to_vec()).unwrap();
    assert_eq!(short.serials, ["T-5704H"]);
    assert_eq!(short.volume, None);
}

#[test]
fn dreamcast_track03_t40214n_disc_1_of_1() {
    let disc = id(dt::dreamcast_track03("T-40214N  ", "1/1")).unwrap();
    assert_eq!(disc.system, DiscSystem::Known("dreamcast"));
    assert_eq!(disc.serials, ["T-40214N"]);
    assert_eq!(disc.disc_no, Some(1));
    assert_eq!(disc.source, DiscIdSource::IpBin);
    assert_eq!(disc.confidence, 0.95);
    assert_eq!(disc.detail, "SEGA SEGAKATANA");
    assert!(disc.volume.is_some());

    // Disc 2 of 2; a device field that makes no sense gives no number.
    let two = id(dt::dreamcast_track03("T-40214N  ", "2/2")).unwrap();
    assert_eq!(two.disc_no, Some(2));
    assert_ne!(two.volume, disc.volume, "IP.BIN differs");
    let odd = id(dt::dreamcast_track03("T-40214N  ", "3/2")).unwrap();
    assert_eq!(odd.disc_no, None);
    assert_eq!(odd.serials, ["T-40214N"]);
}

#[test]
fn dreamcast_track01_alone_is_not_enough() {
    // The low-density track has a volume but no IP.BIN: even with the
    // Katana name in its system id, it is no verdict.
    let track01 = dt::dreamcast_track01();
    assert!(read_pvd(&mut WindowSource::new(track01.clone()))
        .unwrap()
        .is_some());
    assert_eq!(id(track01), None);
    // The magic away from offset 0 of sector 0 is not IP.BIN either.
    let mut moved = dt::dreamcast_track03("T-40214N  ", "1/1");
    moved.copy_within(16..16 + 0x200, 2352 + 16);
    moved[16..16 + 0x200].fill(0);
    assert_eq!(id(moved).map(|d| d.system), None);
}

#[test]
fn segacd_never_megadrive() {
    let bytes = dt::segacd_track();
    let disc = id(bytes.clone()).unwrap();
    assert_eq!(disc.system, DiscSystem::Known("segacd"));
    assert_eq!(disc.serials, ["MK-4407"]);
    assert_eq!(disc.source, DiscIdSource::IpBin);
    assert_eq!(disc.confidence, 0.95);
    assert_eq!(disc.detail, "SEGADISCSYSTEM + SEGA MEGA DRIVE");
    // The header sniff reads agrees: Sega CD, never a Mega Drive cart,
    // and the serial comes from the same reader.
    let s = coverly_core::identify::sniff::sniff(
        &mut coverly_core::identify::sniff::SliceAt(&bytes),
        bytes.len() as u64,
        "bin",
    );
    assert!(s.hits.iter().all(|e| e.system == "segacd"), "{:?}", s.hits);
    assert!(!s.hits.is_empty());
    assert_eq!(s.serial.as_deref(), Some("MK-4407"));
}

#[test]
fn pce_cd_sector1() {
    let bytes = dt::pce_cd_track();
    let disc = id(bytes.clone()).unwrap();
    assert_eq!(disc.system, DiscSystem::Known("pcenginecd"));
    assert!(disc.serials.is_empty(), "no serial on the disc");
    assert_eq!(disc.source, DiscIdSource::Signature);
    assert_eq!(disc.confidence, 0.90);
    assert_eq!(disc.detail, "PC Engine CD-ROM SYSTEM");
    assert_eq!(disc.volume, None);
    // The text in sector 0 instead of sector 1 is not the signature.
    let mut wrong = bytes[2352..2 * 2352].to_vec();
    wrong.extend(vec![0u8; 22 * 2352]);
    assert_eq!(id(wrong), None);
}

#[test]
fn neocd_ipl_txt() {
    let disc = id(dt::neocd_iso()).unwrap();
    assert_eq!(disc.system, DiscSystem::Known("neocd"));
    assert!(disc.serials.is_empty());
    assert_eq!(disc.source, DiscIdSource::Signature);
    assert_eq!(disc.confidence, 0.85);
    assert_eq!(disc.detail, "IPL.TXT");
    // With a SYSTEM.CNF beside it, it is not a Neo Geo CD.
    let both = IsoBuilder::new("")
        .file("IPL.TXT;1", b"PROG.PRG,0,0\r\n")
        .file("SYSTEM.CNF;1", b"BOOT = cdrom:\\X.EXE;1\r\n")
        .build();
    assert_eq!(id(both), None);
    // A directory named IPL.TXT is not the boot list.
    assert_eq!(id(IsoBuilder::new("").dir("IPL.TXT").build()), None);
    // A Sega disc with an IPL.TXT stays a Sega disc.
    let ip = dt::ip_bin(b"SEGA SEGASATURN ", &[(0x20, b"T-1234    ")]);
    let saturn = IsoBuilder::new("")
        .system_area(&ip)
        .file("IPL.TXT;1", b"x")
        .build();
    assert_eq!(id(saturn).unwrap().system, DiscSystem::Known("saturn"));
}

#[test]
fn three_do_unsupported() {
    for raw in [false, true] {
        let disc = id(dt::opera_3do(raw)).unwrap();
        assert_eq!(disc.system, DiscSystem::Unsupported("3DO"), "raw={raw}");
        assert!(disc.serials.is_empty());
        assert_eq!(disc.source, DiscIdSource::Signature);
        assert_eq!(disc.detail, "Opera volume header");
    }
    // Five sync bytes are needed, not four.
    let mut near = dt::opera_3do(false);
    near[5] = 0;
    assert_eq!(id(near), None);
}

#[test]
fn psp_iso_unchanged() {
    // Today's sniff rule keeps its verdict.
    let iso = fx::psp_iso();
    let s = coverly_core::identify::sniff::sniff(
        &mut coverly_core::identify::sniff::SliceAt(&iso),
        iso.len() as u64,
        "iso",
    );
    assert_eq!(s.hits.len(), 1);
    assert_eq!((s.hits[0].system.as_str(), s.hits[0].weight), ("psp", 0.85));
    // DiscReader says the same, with no serial when UMD_DATA.BIN is absent.
    let disc = id(iso).unwrap();
    assert_eq!(disc.system, DiscSystem::Known("psp"));
    assert_eq!(disc.confidence, 0.85);
    assert_eq!(disc.detail, "PSP GAME");
    assert!(disc.serials.is_empty());
    assert_eq!(disc.source, DiscIdSource::Signature);
    // UMD_DATA.BIN names the serial.
    let umd = IsoBuilder::new("PSP GAME")
        .file("UMD_DATA.BIN;1", b"ULUS-10041|C1D2A45B4F28B1E2|0001|G")
        .build();
    let disc = id(umd).unwrap();
    assert_eq!(disc.serials, ["ULUS-10041"]);
    assert_eq!(disc.source, DiscIdSource::UmdData);
}

#[test]
fn identity_detail_strings_are_data() {
    // Hostile header fields never reach the detail, and only pass as a
    // serial when they are one.
    let hostile = dt::ip_bin(
        b"SEGA SEGASATURN ",
        &[(0x20, b"\x1b[31mT-1\n"), (0x38, b"CD-9/1  ")],
    );
    let disc = id(IsoBuilder::new("SEGA SEGASATURN")
        .system_area(&hostile)
        .build())
    .unwrap();
    assert_eq!(disc.detail, "SEGA SEGASATURN");
    assert!(disc.serials.is_empty(), "{:?}", disc.serials);
    assert_eq!(disc.disc_no, None);

    let katana = dt::ip_bin(b"SEGA SEGAKATANA ", &[(0x40, "T-4021\u{e9}".as_bytes())]);
    let disc = id(IsoBuilder::new("").system_area(&katana).build()).unwrap();
    assert!(disc.serials.is_empty());

    // Every detail of every rule is plain ASCII from a fixed vocabulary.
    let known = [
        "SEGA SEGASATURN",
        "SEGA SEGAKATANA",
        "SEGADISCSYSTEM + SEGA MEGA DRIVE",
        "PC Engine CD-ROM SYSTEM",
        "IPL.TXT",
        "Opera volume header",
        "PSP GAME",
        "SYSTEM.CNF BOOT + licence text",
        "PLAYSTATION",
    ];
    let all = [
        dt::saturn_track("T-5704H   "),
        dt::dreamcast_track03("T-40214N  ", "1/1"),
        dt::segacd_track(),
        dt::pce_cd_track(),
        dt::neocd_iso(),
        dt::opera_3do(false),
        fx::psp_iso(),
        dt::psx_track(true, "cdrom:\\SLUS_008.62;1"),
        IsoBuilder::new("PLAYSTATION").build(),
    ];
    for (bytes, want) in all.into_iter().zip(known) {
        let disc = id(bytes).unwrap();
        assert_eq!(disc.detail, want);
        assert!(disc
            .detail
            .bytes()
            .all(|b| b.is_ascii_graphic() || b == b' '));
        assert!(disc
            .serials
            .iter()
            .all(|s| normalise_serial(s).as_ref() == Some(s)));
    }
}

#[test]
fn disc_reader_fetches_a_pending_boot_file() {
    let cnf = b"BOOT = cdrom:\\SLUS_008.62;1\r\nTCB = 4\r\n";
    let full = psx_with_boot_past_window(cnf);
    let len = full.len() as u64;
    let mut head = WindowSource::new(full[..MAX_WINDOW].to_vec());

    // Without a fetch: the boot file name at 0.80, no fingerprint.
    let first = DiscReader::identify(&mut head).unwrap();
    assert_eq!(first.serials, ["SLUS-00862"]);
    assert_eq!(first.confidence, 0.80);
    assert_eq!(first.source, DiscIdSource::Signature);
    assert_eq!(first.volume, None);

    // The fetch is asked for exactly the stored sectors of SYSTEM.CNF.
    let mut asked = Vec::new();
    let mut fetch = |r: std::ops::Range<u64>| {
        asked.push(r.clone());
        Ok(Some(full[r.start as usize..r.end as usize].to_vec()))
    };
    let done = DiscReader::identify_with(&mut head, Some(len), Some(&mut fetch))
        .unwrap()
        .unwrap();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].start >= MAX_WINDOW as u64);
    assert!(asked[0].end - asked[0].start <= 2 * 2352);
    assert_eq!(done.confidence, 0.95);
    assert_eq!(done.source, DiscIdSource::SystemCnf);
    assert_eq!(done.detail, "SYSTEM.CNF BOOT + licence text");
    assert!(done.volume.is_some());
    // The same as reading the whole track.
    let whole = DiscReader::identify_with(&mut WindowSource::new(full.clone()), Some(len), None);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.bin");
    std::fs::write(&path, &full).unwrap();
    let from_file =
        DiscReader::identify_with(&mut FileSource::open(&path).unwrap(), Some(len), None)
            .unwrap()
            .unwrap();
    assert_eq!(from_file, done);
    assert!(whole.unwrap().is_some());

    // A spent budget keeps the first verdict; a fetch error is returned.
    let mut spent = |_r: std::ops::Range<u64>| Ok(None);
    let kept = DiscReader::identify_with(&mut head, Some(len), Some(&mut spent))
        .unwrap()
        .unwrap();
    assert_eq!(kept, first);
    let mut short = |_r: std::ops::Range<u64>| Ok(Some(vec![0u8; 10]));
    let kept = DiscReader::identify_with(&mut head, Some(len), Some(&mut short))
        .unwrap()
        .unwrap();
    assert_eq!(kept, first);
    let mut broken = |_r: std::ops::Range<u64>| Err(std::io::Error::other("disk gone"));
    assert!(DiscReader::identify_with(&mut head, Some(len), Some(&mut broken)).is_err());
}
