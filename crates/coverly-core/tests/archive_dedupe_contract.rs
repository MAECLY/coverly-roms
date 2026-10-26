//! Contract test for dedupe phase B (spec §10, plan AP-6): the
//! `EntryListing` is `dedupe::archive::ArchiveEntryListing` over the archive
//! reader and the `DiscProbe` is `dedupe::disc::ReaderDiscProbe` over it and
//! the disc reader. The seams they rely on are pinned here. Fixtures are
//! synthetic.

use std::io::{self, Write};
use std::path::Path;

use coverly_core::archive::testing::{
    lzma_props, sevenz_copy, zip_crafted, zip_lzma_payload, ZipMethod, ZipSpec,
};
use coverly_core::archive::{is_budget_spent, Limits};
use coverly_core::dedupe::archive::ArchiveEntryListing;
use coverly_core::dedupe::disc::ReaderDiscProbe;
use coverly_core::dedupe::source::{DiscIdSource, DiscProbe, EntryListing, ListedEntry};
use coverly_core::disc::testing as dt;
use coverly_core::identify::hash::{hash_stream, HeaderRule};

// ---- Helpers ------------------------------------------------------------

fn listing() -> ArchiveEntryListing {
    ArchiveEntryListing::new(Limits::default())
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn sample(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 13 % 251) as u8).collect()
}

// ---- Tests --------------------------------------------------------------

#[test]
fn listing_index_space_is_file_entries_in_archive_order() {
    let dir = tempfile::tempdir().unwrap();
    let a = sample(300);
    let b = sample(500);
    let bytes = zip_crafted(&[
        ZipSpec::new("set/", b"", ZipMethod::Stored),
        ZipSpec::new("set/b.gba", &b, ZipMethod::Deflate),
        ZipSpec::new("set/a.gba", &a, ZipMethod::Stored),
    ]);
    let zip = write(dir.path(), "set.zip", &bytes);

    let got = listing().list(&zip).unwrap().expect("an archive");
    let names: Vec<&str> = got.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        ["set/b.gba", "set/a.gba"],
        "archive order, dirs left out"
    );
    assert_eq!(got[0].size, 500);
    assert_eq!(got[0].crc32, Some(crc32fast::hash(&b)));
    // The index space phase A's zip listing had: file entries only.
    assert_eq!(
        got,
        [
            ListedEntry {
                name: "set/b.gba".into(),
                size: 500,
                crc32: Some(crc32fast::hash(&b)),
            },
            ListedEntry {
                name: "set/a.gba".into(),
                size: 300,
                crc32: Some(crc32fast::hash(&a)),
            },
        ]
    );

    // Entry n of the listing is the n-th file entry.
    let head = listing().head(&zip, 1, 16).unwrap().expect("decoded");
    assert_eq!(head, a[..16]);

    // A 7z misnamed .zip is opened by magic, in its own order.
    let sz = write(
        dir.path(),
        "disc.zip",
        &sevenz_copy(&[("x.bin", &a), ("y.bin", &b)]),
    );
    let got = listing().list(&sz).unwrap().expect("7z by magic");
    assert_eq!(
        got.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        ["x.bin", "y.bin"]
    );

    // Not an archive at all (an HTML page saved as .zip).
    let html = write(dir.path(), "page.zip", b"<!doctype html><html></html>");
    assert_eq!(listing().list(&html).unwrap(), None);
}

#[test]
fn payload_sha1_verifies_crc() {
    let dir = tempfile::tempdir().unwrap();
    let data = sample(4096);
    let good = write(
        dir.path(),
        "good.zip",
        &zip_crafted(&[ZipSpec::new("game.gba", &data, ZipMethod::Deflate)]),
    );
    let want = hash_stream(&data[..], data.len() as u64, HeaderRule::Plain).unwrap();
    let got = listing()
        .payload_sha1(&good, 0, HeaderRule::Plain, 1 << 20)
        .unwrap();
    assert_eq!(got, want.payload_sha1);
    assert!(got.is_some());

    // Over the caller's budget: not read.
    assert_eq!(
        listing()
            .payload_sha1(&good, 0, HeaderRule::Plain, 100)
            .unwrap(),
        None
    );

    // A CRC that does not match the bytes is an error, never a hash.
    let bad = write(
        dir.path(),
        "bad.zip",
        &zip_crafted(&[
            ZipSpec::new("game.gba", &data, ZipMethod::Deflate).declared_crc(0xDEAD_BEEF)
        ]),
    );
    let err = listing()
        .payload_sha1(&bad, 0, HeaderRule::Plain, 1 << 20)
        .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{err}");
}

#[test]
fn head_budget_spent_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let data = sample(64 * 1024);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&data).unwrap();
    let path = write(dir.path(), "game.gba.gz", &gz.finish().unwrap());

    let tight = ArchiveEntryListing::new(Limits {
        plan_head_budget: 1024,
        ..Limits::default()
    });
    assert_eq!(tight.list(&path).unwrap().map(|l| l.len()), Some(1));
    // Headerless (not read), never an error and never a spin.
    assert_eq!(tight.head(&path, 0, 8192).unwrap(), None);
    // Within the budget the head is served.
    assert_eq!(
        tight.head(&path, 0, 512).unwrap().as_deref(),
        Some(&data[..512])
    );
}

#[test]
fn probe_returns_serial_and_fingerprint() {
    let dir = tempfile::tempdir().unwrap();
    let probe = ReaderDiscProbe::new(listing());
    let track = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");

    // A raw image on disk.
    let bin = write(dir.path(), "Next Tetris, The (USA).bin", &track);
    let id = probe.probe(&bin, "psx").unwrap().expect("a PS1 disc");
    assert_eq!(id.serials, ["SLUS-00862"]);
    assert_eq!(id.source, DiscIdSource::SystemCnf);
    let fp = id.volume.clone().expect("volume fingerprint");
    assert_eq!(fp.boot_sha1.len(), 40);

    // The same disc inside a 7z misnamed .zip, after its sheet.
    let cue = b"FILE \"Next Tetris, The (USA).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n";
    let sz = write(
        dir.path(),
        "Next Tetris, The.zip",
        &sevenz_copy(&[
            ("Next Tetris, The (USA).cue", cue),
            ("Next Tetris, The (USA).bin", &track),
        ]),
    );
    assert_eq!(probe.probe(&sz, "psx").unwrap(), Some(id));
    // Another system's question gets no answer.
    assert_eq!(probe.probe(&sz, "saturn").unwrap(), None);

    // A Saturn disc: IP.BIN product number, disc number when present.
    let sat = write(
        dir.path(),
        "Tetris Plus.zip",
        &sevenz_copy(&[("t.bin", &dt::saturn_track("T-5704H"))]),
    );
    let id = probe.probe(&sat, "saturn").unwrap().expect("a Saturn disc");
    assert_eq!(id.serials, ["T-5704H"]);
    assert_eq!(id.source, DiscIdSource::IpBin);
    assert!(probe.reads_discs());
}

#[test]
fn memory_cap_maps_to_out_of_memory() {
    let dir = tempfile::tempdir().unwrap();
    // A 1.5 GiB LZMA dictionary over 64 bytes (S2, S3).
    let payload = zip_lzma_payload(lzma_props(3, 0, 2, 3 << 29), &[0u8; 64]);
    let path = write(
        dir.path(),
        "game.zip",
        &zip_crafted(&[ZipSpec::new(
            "game.bin",
            &[0u8; 1024],
            ZipMethod::Raw {
                method: 14,
                payload,
            },
        )]),
    );
    let err = listing().list(&path).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::OutOfMemory, "{err}");
    // Dedupe's "unreadable unit" path sees a plain io::Error.
    assert!(!is_budget_spent(&err));
}

#[test]
fn payload_sha1_honours_the_callers_budget() {
    // The caller's `budget` bounds the decode, not the plan head budget:
    // an entry over `plan_head_budget` but within `budget` is hashed.
    let dir = tempfile::tempdir().unwrap();
    let data = sample(8192);
    let path = write(
        dir.path(),
        "game.zip",
        &zip_crafted(&[ZipSpec::new("game.gba", &data, ZipMethod::Deflate)]),
    );
    let tight = ArchiveEntryListing::new(Limits {
        plan_head_budget: 1024,
        ..Limits::default()
    });
    let want = hash_stream(&data[..], data.len() as u64, HeaderRule::Plain).unwrap();
    let got = tight
        .payload_sha1(&path, 0, HeaderRule::Plain, 1 << 20)
        .unwrap();
    assert!(got.is_some());
    assert_eq!(got, want.payload_sha1);
}

#[test]
fn probe_follows_the_sheet_to_the_primary_track() {
    let dir = tempfile::tempdir().unwrap();
    let probe = ReaderDiscProbe::new(listing());

    // A cue set on disk: dedupe's `DiscSet` unit probes its sheet.
    let track = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");
    write(dir.path(), "Next Tetris, The (USA).bin", &track);
    let cue = write(
        dir.path(),
        "Next Tetris, The (USA).cue",
        b"FILE \"Next Tetris, The (USA).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
    );
    let id = probe.probe(&cue, "psx").unwrap().expect("a PS1 cue set");
    assert_eq!(id.serials, ["SLUS-00862"]);

    // A GD-ROM whose low-density track 01 repeats an IP.BIN: the identity
    // is track 03's, as the gdi names it, never the first match.
    let gdi_text: &[u8] =
        b"3\n1 0 4 2352 track01.bin 0\n2 756 0 2352 track02.raw 0\n3 45000 4 2352 track03.bin 0\n";
    let decoy = dt::dreamcast_track03("T-99999N  ", "1/1");
    let real = dt::dreamcast_track03("T-40214N  ", "1/1");
    let gd = dir.path().join("gd");
    std::fs::create_dir(&gd).unwrap();
    write(&gd, "track01.bin", &decoy);
    write(&gd, "track02.raw", &[0u8; 2352]);
    write(&gd, "track03.bin", &real);
    let gdi = write(&gd, "Tetris Online.gdi", gdi_text);
    let id = probe.probe(&gdi, "dreamcast").unwrap().expect("a GD-ROM");
    assert_eq!(id.serials, ["T-40214N"]);

    // The same set inside a 7z misnamed .zip.
    let sz = write(
        dir.path(),
        "Next Tetris, The_ On-Line Edition.zip",
        &sevenz_copy(&[
            ("Tetris Online.gdi", gdi_text),
            ("track01.bin", &decoy),
            ("track02.raw", &[0u8; 2352]),
            ("track03.bin", &real),
        ]),
    );
    let id = probe.probe(&sz, "dreamcast").unwrap().expect("a GD-ROM");
    assert_eq!(id.serials, ["T-40214N"]);
}
