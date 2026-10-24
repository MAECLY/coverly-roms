//! Contract test for dedupe phase B (spec §10, plan AP-6): an
//! `EntryListing` and a `DiscProbe` written in a few lines over the archive
//! and disc readers of this branch, with no new types. Phase B's adapters
//! replace these; the seams they rely on are pinned here. Fixtures are
//! synthetic.

use std::io::{self, Read, Write};
use std::path::Path;

use coverly_core::archive::testing::{
    lzma_props, sevenz_copy, zip_crafted, zip_lzma_payload, ZipMethod, ZipSpec,
};
use coverly_core::archive::{file_entries, is_budget_spent, open_archive, Budget, Limits};
use coverly_core::dedupe::source::{
    DiscId, DiscIdSource, DiscProbe, EntryListing, ListedEntry, ZipListing,
};
use coverly_core::disc::source::MAX_WINDOW;
use coverly_core::disc::testing as dt;
use coverly_core::disc::{DiscReader, DiscSystem, FileSource, WindowSource};
use coverly_core::identify::hash::{hash_stream, HeaderRule};

// ---- The adapters: what phase B writes (spec §10) ----------------------

/// `EntryListing` over every format `open_archive` reads.
struct TestArchiveListing {
    limits: Limits,
}

impl TestArchiveListing {
    /// The `n`-th file entry's reader, budget spent mapped to `Ok(None)`.
    fn read(&self, path: &Path, n: usize, cap: u64) -> io::Result<Option<(ListedEntry, Vec<u8>)>> {
        let mut r = open_archive(path, &self.limits)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not an archive"))?;
        let entry = file_entries(r.as_ref())
            .nth(n)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no entry {n}")))?;
        let mut budget = Budget::plan_head(&self.limits);
        let mut out = Vec::new();
        let read = r
            .open_entry(entry.index, &mut budget)
            .map_err(io::Error::from)
            .and_then(|s| s.take(cap).read_to_end(&mut out));
        match read {
            Err(e) if is_budget_spent(&e) => Ok(None),
            Err(e) => Err(e),
            Ok(_) => Ok(Some((listed(&entry), out))),
        }
    }
}

fn listed(e: &coverly_core::archive::Entry) -> ListedEntry {
    ListedEntry {
        name: e.name.as_str().to_owned(),
        size: e.size,
        crc32: e.crc32,
    }
}

impl EntryListing for TestArchiveListing {
    fn list(&self, path: &Path) -> io::Result<Option<Vec<ListedEntry>>> {
        let Some(r) = open_archive(path, &self.limits)? else {
            return Ok(None);
        };
        Ok(Some(file_entries(r.as_ref()).map(listed).collect()))
    }

    fn payload_sha1(
        &self,
        path: &Path,
        entry: usize,
        rule: HeaderRule,
        budget: u64,
    ) -> io::Result<Option<String>> {
        let Some(e) = self.list(path)?.and_then(|l| l.into_iter().nth(entry)) else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no entry"));
        };
        if e.size > budget {
            return Ok(None);
        }
        // The archive reader verifies size and CRC at EOF.
        let Some((e, bytes)) = self.read(path, entry, e.size.saturating_add(1))? else {
            return Ok(None);
        };
        Ok(hash_stream(&bytes[..], e.size, rule)?.payload_sha1)
    }

    fn head(&self, path: &Path, entry: usize, len: usize) -> io::Result<Option<Vec<u8>>> {
        Ok(self.read(path, entry, len as u64)?.map(|(_, b)| b))
    }
}

/// `DiscProbe` over raw images and disc archives.
struct TestDiscProbe {
    listing: TestArchiveListing,
}

impl DiscProbe for TestDiscProbe {
    fn probe(&self, path: &Path, system: &str) -> io::Result<Option<DiscId>> {
        let found = match self.listing.list(path)? {
            None => DiscReader::identify(&mut FileSource::open(path)?),
            Some(entries) => entries.iter().enumerate().find_map(|(n, _)| {
                let (_, head) = self.listing.read(path, n, MAX_WINDOW as u64).ok()??;
                DiscReader::identify(&mut WindowSource::new(head))
            }),
        };
        let ours = |d: &coverly_core::disc::DiscIdentity| matches!(d.system, DiscSystem::Known(id) if id == system);
        Ok(found.filter(ours).map(|d| d.to_dedupe_id()))
    }
}

// ---- Helpers ------------------------------------------------------------

fn listing() -> TestArchiveListing {
    TestArchiveListing {
        limits: Limits::default(),
    }
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
    // The same index space as today's `ZipListing`: a drop-in for phase B.
    assert_eq!(ZipListing.list(&zip).unwrap(), Some(got));

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

    let tight = TestArchiveListing {
        limits: Limits {
            plan_head_budget: 1024,
            ..Limits::default()
        },
    };
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
    let probe = TestDiscProbe { listing: listing() };
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
