//! Duplicates level 3: `ReaderDiscProbe` over the disc reader (duplicates
//! spec §5.4, plan Task 25). Every image is synthetic (`disc::testing`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use coverly_core::archive::testing::sevenz_copy;
use coverly_core::archive::Limits;
use coverly_core::dedupe::archive::ArchiveEntryListing;
use coverly_core::dedupe::disc::ReaderDiscProbe;
use coverly_core::dedupe::source::{DiscIdSource, DiscProbe};
use coverly_core::disc::source::MAX_WINDOW;
use coverly_core::disc::testing::{self as dt, IsoBuilder, PSX_LICENCE};
use tempfile::TempDir;

fn probe() -> ReaderDiscProbe {
    ReaderDiscProbe::new(ArchiveEntryListing::new(Limits::default()))
}

fn put(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, bytes).unwrap();
    path
}

/// A deflated zip of `entries`: small on disk, large once decoded.
fn deflated_zip(dir: &Path, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
    let path = dir.join(name);
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (inner, bytes) in entries {
        zip.start_file(*inner, opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    path
}

fn cue_for(bin: &str) -> String {
    format!("FILE \"{bin}\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n")
}

/// A Saturn track 01 whose `IP.BIN` device field says disc `n` of `of`.
fn saturn_disc(product: &str, n: u8, of: u8) -> Vec<u8> {
    let device = format!("CD-{n}/{of}  ");
    let ip = dt::ip_bin(
        b"SEGA SEGASATURN ",
        &[
            (0x10, b"SEGA TP T-57    "),
            (0x20, product.as_bytes()),
            (0x2A, b"V1.000"),
            (0x30, b"19961122"),
            (0x38, device.as_bytes()),
        ],
    );
    IsoBuilder::new("SEGA SEGASATURN")
        .raw_mode1()
        .system_area(&ip)
        .build()
}

#[test]
fn ps1_bin_serial_and_fingerprint() {
    let dir = TempDir::new().unwrap();
    let bin = put(
        dir.path(),
        "Crash Bash (USA).bin",
        &dt::psx_track(true, "cdrom:\\SCUS_945.70;1"),
    );
    let id = probe().probe(&bin, "psx").unwrap().expect("a PS1 disc");
    assert_eq!(id.serials, ["SCUS-94570"]);
    assert_eq!(id.source, DiscIdSource::SystemCnf);
    let fp = id.volume.expect("volume fingerprint");
    assert_eq!(fp.boot_sha1.len(), 40);
    assert!(fp.boot_sha1.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(fp.space_size > 0);
}

#[test]
fn iso_2048_and_bin_2352_of_the_same_disc_share_fingerprint() {
    let dir = TempDir::new().unwrap();
    let boot = "cdrom:\\SCUS_941.94;1";
    let iso = put(dir.path(), "Gran Turismo.iso", &dt::psx_track(false, boot));
    let bin = put(dir.path(), "Gran Turismo.bin", &dt::psx_track(true, boot));
    let cue = put(
        dir.path(),
        "Gran Turismo.cue",
        cue_for("Gran Turismo.bin").as_bytes(),
    );
    let a = probe().probe(&iso, "psx").unwrap().expect("iso");
    let b = probe().probe(&bin, "psx").unwrap().expect("bin");
    let c = probe().probe(&cue, "psx").unwrap().expect("cue set");
    assert_eq!(a.serials, ["SCUS-94194"]);
    assert!(a.volume.is_some());
    assert_eq!(a, b, "sector layout never changes the identity");
    assert_eq!(b, c, "a cue set is its primary track");

    // Another build of the same serial: another fingerprint.
    let other = IsoBuilder::new("PLAYSTATION")
        .raw(true)
        .licence(PSX_LICENCE)
        .dates([b'1'; 17], [b'2'; 17])
        .file(
            "SYSTEM.CNF;1",
            b"BOOT = cdrom:\\SCUS_941.94;1\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFFF0\r\n",
        )
        .build();
    let rev = put(dir.path(), "Gran Turismo (Rev 1).bin", &other);
    let d = probe().probe(&rev, "psx").unwrap().expect("rev");
    assert_eq!(d.serials, a.serials);
    assert_ne!(d.volume, a.volume);
}

/// A PS1 track whose `SYSTEM.CNF` is stored after a large file, past the
/// decoded window an archive probe holds.
fn psx_track_with_late_cnf() -> Vec<u8> {
    IsoBuilder::new("PLAYSTATION")
        .raw(true)
        .licence(PSX_LICENCE)
        .file("MOVIE.STR;1", &vec![0x5A; MAX_WINDOW + 512 * 1024])
        .file(
            "SYSTEM.CNF;1",
            b"BOOT = cdrom:\\SLUS_008.62;1\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFFF0\r\n",
        )
        .build()
}

#[test]
fn system_cnf_past_the_window_is_fetched_within_the_budget() {
    let dir = TempDir::new().unwrap();
    let track = psx_track_with_late_cnf();
    assert!(track.len() > MAX_WINDOW + 512 * 1024);
    let zip = deflated_zip(
        dir.path(),
        "Next Tetris.zip",
        &[("Next Tetris.bin", &track)],
    );
    let on_disk = put(dir.path(), "Next Tetris.bin", &track);

    let id = probe().probe(&zip, "psx").unwrap().expect("a PS1 disc");
    assert_eq!(id.serials, ["SLUS-00862"]);
    assert_eq!(
        id.source,
        DiscIdSource::SystemCnf,
        "fetched past the window"
    );
    assert!(id.volume.is_some());
    assert_eq!(Some(id), probe().probe(&on_disk, "psx").unwrap());

    // A budget that holds the window but not the skip to `SYSTEM.CNF`:
    // the window's verdict alone, never a fingerprint from a partial read.
    let tight = ReaderDiscProbe::new(ArchiveEntryListing::new(Limits {
        plan_head_budget: MAX_WINDOW as u64 + 64 * 1024,
        ..Limits::default()
    }));
    let other = deflated_zip(dir.path(), "Other.zip", &[("Other.bin", &track)]);
    let found = tight.probe(&other, "psx").unwrap();
    assert!(
        found
            .as_ref()
            .is_none_or(|d| d.source != DiscIdSource::SystemCnf && d.volume.is_none()),
        "{found:?}"
    );
}

#[test]
fn saturn_disc_no_from_device_field() {
    let dir = TempDir::new().unwrap();
    let one = put(
        dir.path(),
        "Game (Disc 1).bin",
        &saturn_disc("T-1234H   ", 1, 2),
    );
    let two = sevenz_copy(&[("Game (Disc 2).bin", &saturn_disc("T-1234H   ", 2, 2))]);
    let two = put(dir.path(), "Game (Disc 2).7z", &two);

    let a = probe().probe(&one, "saturn").unwrap().expect("disc 1");
    let b = probe().probe(&two, "saturn").unwrap().expect("disc 2");
    assert_eq!(a.serials, ["T-1234H"]);
    assert_eq!(a.serials, b.serials, "one product number on every disc");
    assert_eq!(a.source, DiscIdSource::IpBin);
    assert_eq!(a.disc_no, Some(1));
    assert_eq!(b.disc_no, Some(2));
}

#[test]
fn m3u_lists_serials_in_order() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    put(
        d,
        "discs/Driver 2 (Disc 1).bin",
        &dt::psx_track(true, "cdrom:\\SLUS_011.61;1"),
    );
    put(
        d,
        "discs/Driver 2 (Disc 1).cue",
        cue_for("Driver 2 (Disc 1).bin").as_bytes(),
    );
    put(
        d,
        "Driver 2 (Disc 2).bin",
        &dt::psx_track(true, "cdrom:\\SLUS_013.18;1"),
    );
    let m3u = put(
        d,
        "Driver 2.m3u",
        b"#EXTM3U\r\ndiscs\\Driver 2 (Disc 1).cue\r\n\r\nDriver 2 (Disc 2).bin\r\n",
    );
    let id = probe().probe(&m3u, "psx").unwrap().expect("a playlist");
    assert_eq!(id.serials, ["SLUS-01161", "SLUS-01318"]);
    assert_eq!(id.source, DiscIdSource::M3uPosition);
    assert_eq!(id.disc_no, None);
    let first = probe()
        .probe(&d.join("discs/Driver 2 (Disc 1).cue"), "psx")
        .unwrap()
        .unwrap();
    assert_eq!(id.volume, first.volume, "disc 1's fingerprint");
    assert!(id.volume.is_some());

    // The other order is another list.
    let back = put(
        d,
        "Driver 2 (back).m3u",
        b"Driver 2 (Disc 2).bin\ndiscs/Driver 2 (Disc 1).cue\n",
    );
    let rev = probe().probe(&back, "psx").unwrap().unwrap();
    assert_eq!(rev.serials, ["SLUS-01318", "SLUS-01161"]);
}

#[test]
fn m3u_with_an_unreadable_disc_is_none() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    put(
        d,
        "A (Disc 1).bin",
        &dt::psx_track(true, "cdrom:\\SLUS_011.61;1"),
    );
    put(d, "A (Disc 2).bin", &[0u8; 4096]);
    let missing = put(d, "A.m3u", b"A (Disc 1).bin\nA (Disc 3).bin\n");
    let blank = put(d, "B.m3u", b"A (Disc 1).bin\nA (Disc 2).bin\n");
    let nested = put(d, "C.m3u", b"A (Disc 1).bin\nA.m3u\n");
    let climbs = put(d, "D.m3u", b"A (Disc 1).bin\n../A (Disc 1).bin\n");
    let empty = put(d, "E.m3u", b"#EXTM3U\n");
    for m3u in [&missing, &blank, &nested, &climbs, &empty] {
        assert_eq!(
            probe().probe(m3u, "psx").unwrap(),
            None,
            "{}",
            m3u.display()
        );
    }
}

#[test]
fn chd_returns_none() {
    let dir = TempDir::new().unwrap();
    let mut chd = b"MComprHD".to_vec();
    chd.extend(dt::psx_track(true, "cdrom:\\SLUS_008.62;1"));
    let path = put(dir.path(), "Game.chd", &chd);
    assert_eq!(probe().probe(&path, "psx").unwrap(), None);
    assert!(probe().reads_discs());
}

#[test]
fn probe_of_another_system_is_none() {
    let dir = TempDir::new().unwrap();
    let ps = put(
        dir.path(),
        "Game.bin",
        &dt::psx_track(true, "cdrom:\\SLUS_008.62;1"),
    );
    let sat = put(dir.path(), "Sat.bin", &dt::saturn_track("T-5704H   "));
    let m3u = put(dir.path(), "Game.m3u", b"Game.bin\n");
    assert!(probe().probe(&ps, "psx").unwrap().is_some());
    assert_eq!(probe().probe(&ps, "saturn").unwrap(), None);
    assert_eq!(probe().probe(&sat, "psx").unwrap(), None);
    assert_eq!(probe().probe(&m3u, "saturn").unwrap(), None);
    // Not a disc at all.
    let rom = put(dir.path(), "Tetris.gb", &[0x11; 32 * 1024]);
    assert_eq!(probe().probe(&rom, "psx").unwrap(), None);
}

#[test]
fn reads_are_bounded() {
    // A disc archive is read through one plan head budget: a probe with a
    // budget just over its window still answers, and one under it reads
    // nothing past the budget (no answer, never an error).
    let dir = TempDir::new().unwrap();
    let track = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");
    let mut big = track.clone();
    big.resize(64 * 1024 * 1024, 0);
    let zip = deflated_zip(dir.path(), "Big.zip", &[("Big.bin", &big)]);
    let enough = ReaderDiscProbe::new(ArchiveEntryListing::new(Limits {
        plan_head_budget: MAX_WINDOW as u64 + 1024 * 1024,
        ..Limits::default()
    }));
    let id = enough
        .probe(&zip, "psx")
        .unwrap()
        .expect("within the window");
    assert_eq!(id.serials, ["SLUS-00862"]);

    let starved = ReaderDiscProbe::new(ArchiveEntryListing::new(Limits {
        plan_head_budget: 4096,
        ..Limits::default()
    }));
    let other = deflated_zip(dir.path(), "Other.zip", &[("Other.bin", &big)]);
    assert_eq!(starved.probe(&other, "psx").unwrap(), None);
}
