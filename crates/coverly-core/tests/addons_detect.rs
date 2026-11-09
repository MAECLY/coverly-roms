//! Mods spec D2 / §14: patches are told by their bytes, not their
//! extension, and only a bounded part of a patch is ever read.

use std::fs;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::sync::Mutex;

use coverly_core::addons::patch::{read_patch, read_patch_from};
use coverly_core::addons::testing as t;
use coverly_core::addons::{AddonFormat, AddonKey};
use coverly_core::consoles::{start_open_log, take_open_log};
use tempfile::TempDir;

/// The open log is one per process: the tests of this file take turns.
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn a_renamed_patch_is_told_by_its_bytes() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let renamed = dir.path().join("Game (USA).ips");
    fs::write(&renamed, t::bps(524_288, 0xDEAD_BEEF)).unwrap();
    assert_eq!(
        read_patch(&renamed).unwrap(),
        Some((
            AddonFormat::Bps,
            AddonKey::SourceCrc {
                crc32: 0xDEAD_BEEF,
                size: 524_288
            }
        ))
    );
    let text = dir.path().join("Readme.ips");
    fs::write(&text, "This is not a patch, only a note.\n").unwrap();
    assert_eq!(read_patch(&text).unwrap(), None);
    let notes = dir.path().join("Notes.ips");
    fs::write(&notes, "PATCH NOTES\nFixed the intro text.\n").unwrap();
    assert_eq!(read_patch(&notes).unwrap(), None, "PATCH without EOF");
    let ips = dir.path().join("Game (Japan).ips");
    fs::write(&ips, t::ips()).unwrap();
    assert_eq!(
        read_patch(&ips).unwrap(),
        Some((AddonFormat::Ips, AddonKey::None))
    );
    let bare = dir.path().join("Empty.ips");
    fs::write(&bare, b"PATCHEOF").unwrap();
    assert_eq!(
        read_patch(&bare).unwrap(),
        Some((AddonFormat::Ips, AddonKey::None)),
        "an IPS shorter than its 12-byte footer window"
    );
    let empty = dir.path().join("Empty.bps");
    fs::write(&empty, b"").unwrap();
    assert_eq!(read_patch(&empty).unwrap(), None);
    let xdelta = dir.path().join("Hack.vcdiff");
    fs::write(&xdelta, t::xdelta(Some("Hack.iso//Game (Europe).iso/"))).unwrap();
    assert_eq!(
        read_patch(&xdelta).unwrap(),
        Some((
            AddonFormat::Xdelta,
            AddonKey::SourceName {
                name: "Game (Europe).iso".into()
            }
        ))
    );
}

/// Counts every byte read through it.
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

#[test]
fn a_big_patch_is_read_only_at_its_head_and_footer() {
    // A 5 MB BPS: the head (64 bytes) and the footer (12) are all read.
    let mut bytes = t::bps(4_194_304, 0x0102_0304);
    let footer = bytes.split_off(bytes.len() - 12);
    bytes.resize(5_000_000 - 12, 0);
    bytes.extend(&footer);
    let mut r = Counting {
        inner: Cursor::new(bytes),
        read: 0,
    };
    assert_eq!(
        read_patch_from(&mut r).unwrap(),
        Some((
            AddonFormat::Bps,
            AddonKey::SourceCrc {
                crc32: 0x0102_0304,
                size: 4_194_304
            }
        ))
    );
    assert!(r.read <= 64 + 12, "read {} bytes", r.read);

    // A 5 MB xdelta: the head and at most 4 KiB of header.
    let mut bytes = t::xdelta(Some("a.iso//b.iso/"));
    bytes.resize(5_000_000, 0);
    let mut r = Counting {
        inner: Cursor::new(bytes),
        read: 0,
    };
    assert_eq!(
        read_patch_from(&mut r).unwrap(),
        Some((
            AddonFormat::Xdelta,
            AddonKey::SourceName {
                name: "b.iso".into()
            }
        ))
    );
    assert!(r.read <= 64 + 4096, "read {} bytes", r.read);

    // A 5 MB IPS: the head and the last 12 bytes (its `EOF`).
    let mut bytes = t::ips();
    bytes.truncate(bytes.len() - 3);
    bytes.resize(5_000_000 - 3, 0);
    bytes.extend(b"EOF");
    let mut r = Counting {
        inner: Cursor::new(bytes),
        read: 0,
    };
    assert_eq!(
        read_patch_from(&mut r).unwrap(),
        Some((AddonFormat::Ips, AddonKey::None))
    );
    assert!(r.read <= 64 + 12, "read {} bytes", r.read);
}

/// Unix only: a 4 GiB `set_len` is sparse there; on NTFS it may reserve
/// the space on the runner's disk. The byte bound itself is checked on
/// every OS by the counting test above.
#[cfg(unix)]
#[test]
fn a_sparse_4_gib_patch_is_sniffed_without_reading_it() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Huge.bps");
    fs::write(&path, t::bps(1_048_576, 7)).unwrap();
    let f = fs::OpenOptions::new().write(true).open(&path).unwrap();
    f.set_len(4 << 30).unwrap();
    drop(f);
    // The footer is now zeros: the key carries CRC 0, read from the end.
    assert_eq!(
        read_patch(&path).unwrap(),
        Some((
            AddonFormat::Bps,
            AddonKey::SourceCrc {
                crc32: 0,
                size: 1_048_576
            }
        ))
    );
}

#[test]
fn key_material_is_never_opened() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let patch = dir.path().join("Game.bps");
    fs::write(&patch, t::bps(16, 1)).unwrap();
    let mut keys = Vec::new();
    for name in ["prod.keys", "title.keys", "Game.tik", "Game.rap"] {
        let p = dir.path().join(name);
        fs::write(&p, t::bps(16, 1)).unwrap();
        keys.push(p);
    }
    start_open_log();
    assert!(read_patch(&patch).unwrap().is_some());
    for key in &keys {
        let err = read_patch(key).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{key:?}");
    }
    let opened = take_open_log();
    assert_eq!(opened.len(), 1, "{opened:?}");
    assert_eq!(opened[0].file_name(), patch.file_name());
    for key in &keys {
        assert!(!opened.iter().any(|p| p == key), "{key:?} was opened");
    }
}
