//! Duplicates level 3: `ReaderDiscProbe` over the disc reader (duplicates
//! spec §5.4, plan Task 25). Every image is synthetic (`disc::testing`).

use std::fs;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use coverly_core::archive::testing::sevenz_copy;
use coverly_core::archive::Limits;
use coverly_core::dedupe::archive::ArchiveEntryListing;
use coverly_core::dedupe::disc::ReaderDiscProbe;
use coverly_core::dedupe::source::{DiscIdSource, DiscProbe};
use coverly_core::disc::source::MAX_WINDOW;
use coverly_core::disc::testing::{self as dt, IsoBuilder, PSX_LICENCE};
use coverly_core::disc::{Cso, DiscReader, Pbp, MAX_INFLATED};
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

// --- PBP and CSO (plan DD-3) ---

#[test]
fn pbp_reads_inner_system_cnf_not_outer_param_sfo() {
    let dir = TempDir::new().unwrap();
    let track = dt::psx_track(true, "cdrom:\\SCUS_945.70;1");
    let pbp = put(dir.path(), "Crash Bash.PBP", &dt::pbp_of(&track));
    let bin = put(dir.path(), "Crash Bash.bin", &track);

    let id = probe().probe(&pbp, "psx").unwrap().expect("a PS1 PBP");
    // The converter wrote SLUS99999 outside the disc: never the identity.
    assert_eq!(id.serials, ["SCUS-94570"]);
    assert_eq!(id.source, DiscIdSource::SystemCnf);
    assert!(id.volume.is_some());
    assert_eq!(
        Some(id),
        probe().probe(&bin, "psx").unwrap(),
        "the same disc as a bin"
    );

    // A PSP game's EBOOT (no PSISO image) is no PS1 disc.
    let mut psp = dt::pbp_of(&track);
    let psar = u32::from_le_bytes(psp[0x24..0x28].try_into().unwrap()) as usize;
    psp[psar..psar + 4].copy_from_slice(b"\0PSP");
    let psp = put(dir.path(), "EBOOT.PBP", &psp);
    assert_eq!(probe().probe(&psp, "psx").unwrap(), None);
}

#[test]
fn multi_disc_pbp_lists_serials_in_order() {
    let dir = TempDir::new().unwrap();
    let one = dt::psx_track(true, "cdrom:\\SLUS_011.61;1");
    let two = dt::psx_track(true, "cdrom:\\SLUS_013.18;1");
    let pbp = put(dir.path(), "Driver 2.pbp", &dt::pbp_with(&[&one, &two]));
    let id = probe().probe(&pbp, "psx").unwrap().expect("two discs");
    assert_eq!(id.serials, ["SLUS-01161", "SLUS-01318"]);
    assert_eq!(id.disc_no, None);
    let first = put(dir.path(), "Driver 2 (Disc 1).bin", &one);
    assert_eq!(
        id.volume,
        probe().probe(&first, "psx").unwrap().unwrap().volume
    );
    // A multi-disc PBP is a set: never one disc of a playlist.
    let m3u = put(dir.path(), "Driver 2.m3u", b"Driver 2.pbp\n");
    assert_eq!(probe().probe(&m3u, "psx").unwrap(), None);
}

#[test]
fn cso_psp_disc_id() {
    let dir = TempDir::new().unwrap();
    let iso = IsoBuilder::new("PSP GAME")
        .file("UMD_DATA.BIN;1", b"ULUS-10041|C1D2A45B4F28B1E2|0001|G")
        .build();
    let cso = put(dir.path(), "Lumines.cso", &dt::cso_of(&iso));
    let raw = put(dir.path(), "Lumines.iso", &iso);
    let id = probe().probe(&cso, "psp").unwrap().expect("a UMD");
    assert_eq!(id.serials, ["ULUS-10041"]);
    assert_eq!(id.source, DiscIdSource::UmdData);
    assert_eq!(Some(id), probe().probe(&raw, "psp").unwrap());
    assert_eq!(probe().probe(&cso, "psx").unwrap(), None);
    // Not a CISO image (a ZISO, another version): no answer, no error.
    let mut other = dt::cso_of(&iso);
    other[..4].copy_from_slice(b"ZISO");
    let other = put(dir.path(), "Other.cso", &other);
    assert_eq!(probe().probe(&other, "psp").unwrap(), None);
}

/// A reader that counts the bytes read through it.
struct Counting<R> {
    inner: R,
    read: u64,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        Ok(n)
    }
}

impl<R: Seek> Seek for Counting<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Bytes deflate cannot shrink: every block is stored plain.
fn noise(len: usize) -> Vec<u8> {
    let mut x: u32 = 0x1234_5678;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

#[test]
fn pbp_and_cso_reads_are_bounded() {
    const RAW: u64 = 64 * 1024;
    // A PS1 disc with 8 MiB of incompressible data before SYSTEM.CNF:
    // blocks are reached directly, so only the few the reads touch are
    // inflated, far below the cap.
    let late = IsoBuilder::new("PLAYSTATION")
        .raw(true)
        .licence(PSX_LICENCE)
        .file("MOVIE.STR;1", &noise(8 * 1024 * 1024))
        .file(
            "SYSTEM.CNF;1",
            b"BOOT = cdrom:\\SLUS_008.62;1\r\nTCB = 4\r\n",
        )
        .build();
    let mut pbp = Pbp::open(Counting {
        inner: Cursor::new(dt::pbp_of(&late)),
        read: 0,
    })
    .unwrap()
    .expect("a PBP");
    let id = {
        let mut disc = pbp.disc(0).unwrap();
        DiscReader::identify_with(&mut disc, None, None).unwrap()
    };
    assert!(pbp.inflated() <= 8 * 16 * 2352, "{}", pbp.inflated());
    let id = id.expect("a PlayStation disc");
    assert_eq!(id.serials, ["SLUS-00862"]);
    assert!(id.volume.is_some());
    let read = pbp.into_inner().read;
    assert!(read <= RAW + MAX_INFLATED as u64, "{read} bytes read");

    // An early SYSTEM.CNF is read within the bounds.
    let early = IsoBuilder::new("PLAYSTATION")
        .raw(true)
        .licence(PSX_LICENCE)
        .file(
            "SYSTEM.CNF;1",
            b"BOOT = cdrom:\\SLUS_008.62;1\r\nTCB = 4\r\n",
        )
        .file("MOVIE.STR;1", &noise(8 * 1024 * 1024))
        .build();
    let mut pbp = Pbp::open(Counting {
        inner: Cursor::new(dt::pbp_of(&early)),
        read: 0,
    })
    .unwrap()
    .expect("a PBP");
    let id = {
        let mut disc = pbp.disc(0).unwrap();
        DiscReader::identify_with(&mut disc, None, None).unwrap()
    };
    assert_eq!(id.expect("a PS1 disc").serials, ["SLUS-00862"]);
    assert!(pbp.inflated() <= MAX_INFLATED);
    // The blocks are stored plain: what was read is the blocks inflated
    // plus headers and index entries.
    let read = pbp.into_inner().read;
    assert!(read <= RAW + MAX_INFLATED as u64, "{read} bytes read");

    // The same for a CSO over 8 MiB of incompressible data.
    let iso = IsoBuilder::new("PSP GAME")
        .file("UMD_DATA.BIN;1", b"ULUS-10041|C1D2A45B4F28B1E2|0001|G")
        .file("DATA.BIN;1", &noise(8 * 1024 * 1024))
        .build();
    let mut cso = Cso::open(Counting {
        inner: Cursor::new(dt::cso_of(&iso)),
        read: 0,
    })
    .unwrap()
    .expect("a CSO");
    let len = cso.image_len();
    let id = DiscReader::identify_with(&mut cso, Some(len), None)
        .unwrap()
        .expect("a UMD");
    assert_eq!(id.serials, ["ULUS-10041"]);
    assert!(cso.inflated() <= MAX_INFLATED);
    let read = cso.into_inner().read;
    assert!(read <= RAW + MAX_INFLATED as u64, "{read} bytes read");
}
