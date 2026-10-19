//! Fakes of the duplicates seams and synthetic builders, for tests of this
//! crate and of its users (feature `test-util`).

use std::collections::{HashMap, HashSet};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::identify::hash::HeaderRule;

use super::keys::{DiskBytes, FileBytes};

use super::source::{
    DiscId, DiscIdSource, DiscProbe, EntryListing, ListedEntry, VolumeFingerprint,
};

/// A [`DiscProbe`] that answers from a script and records every call.
#[derive(Debug, Default)]
pub struct FakeDiscProbe {
    ids: HashMap<PathBuf, DiscId>,
    failing: HashSet<PathBuf>,
    calls: Mutex<Vec<(PathBuf, String)>>,
}

impl FakeDiscProbe {
    pub fn new() -> FakeDiscProbe {
        FakeDiscProbe::default()
    }

    /// `path` probes as `id`.
    pub fn with(mut self, path: impl Into<PathBuf>, id: DiscId) -> FakeDiscProbe {
        self.ids.insert(path.into(), id);
        self
    }

    /// Probing `path` is an I/O error.
    pub fn failing(mut self, path: impl Into<PathBuf>) -> FakeDiscProbe {
        self.failing.insert(path.into());
        self
    }

    /// Every `(path, system)` probed so far, in order.
    pub fn calls(&self) -> Vec<(PathBuf, String)> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl DiscProbe for FakeDiscProbe {
    fn probe(&self, path: &Path, system: &str) -> io::Result<Option<DiscId>> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((path.to_path_buf(), system.to_string()));
        if self.failing.contains(path) {
            return Err(io::Error::other("scripted disc read failure"));
        }
        Ok(self.ids.get(path).cloned())
    }
}

/// An [`EntryListing`] that answers from a script. A payload is returned
/// only when its entry fits the budget, like a real lister.
#[derive(Debug, Default)]
pub struct FakeListing {
    entries: HashMap<PathBuf, Vec<ListedEntry>>,
    payloads: HashMap<(PathBuf, usize), String>,
}

impl FakeListing {
    pub fn new() -> FakeListing {
        FakeListing::default()
    }

    /// `path` is an archive with `entries`.
    pub fn with_entries(
        mut self,
        path: impl Into<PathBuf>,
        entries: Vec<ListedEntry>,
    ) -> FakeListing {
        self.entries.insert(path.into(), entries);
        self
    }

    /// Entry `entry` of `path` has payload SHA1 `sha1`.
    pub fn with_payload(
        mut self,
        path: impl Into<PathBuf>,
        entry: usize,
        sha1: &str,
    ) -> FakeListing {
        self.payloads.insert((path.into(), entry), sha1.to_string());
        self
    }
}

impl EntryListing for FakeListing {
    fn list(&self, path: &Path) -> io::Result<Option<Vec<ListedEntry>>> {
        Ok(self.entries.get(path).cloned())
    }

    fn payload_sha1(
        &self,
        path: &Path,
        entry: usize,
        _rule: HeaderRule,
        budget: u64,
    ) -> io::Result<Option<String>> {
        let listed = self
            .entries
            .get(path)
            .and_then(|e| e.get(entry))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such entry"))?;
        if listed.size > budget {
            return Ok(None);
        }
        Ok(self.payloads.get(&(path.to_path_buf(), entry)).cloned())
    }
}

/// A `SYSTEM.CNF` disc identity.
pub fn disc_id(serials: &[&str], disc_no: Option<u8>, volume: Option<VolumeFingerprint>) -> DiscId {
    DiscId {
        serials: serials.iter().map(|s| s.to_string()).collect(),
        disc_no,
        volume,
        source: DiscIdSource::SystemCnf,
    }
}

/// A volume fingerprint that differs for every `seed`.
pub fn fingerprint(seed: u8) -> VolumeFingerprint {
    let mut created = *b"1999100112000000\0";
    created[15] = b'0' + seed % 10;
    VolumeFingerprint {
        space_size: 200_000 + u32::from(seed),
        created,
        modified: created,
        boot_sha1: format!("{seed:040X}"),
    }
}

/// An iNES header for a 16 KiB PRG ROM with `mapper` (low nibble in byte
/// 6, high nibble in byte 7).
pub fn nes_header(mapper: u8) -> [u8; 16] {
    let mut h = [0u8; 16];
    h[..4].copy_from_slice(b"NES\x1a");
    h[4] = 1;
    h[6] = (mapper & 0x0F) << 4;
    h[7] = mapper & 0xF0;
    h
}

/// A CHD header of `version` (3, 4 or 5) carrying `sha1` and, from v4 on,
/// `rawsha1`; every other field is zero.
pub fn chd_header(version: u32, sha1: [u8; 20], rawsha1: [u8; 20]) -> Vec<u8> {
    let (len, sha1_at, raw_at) = match version {
        3 => (120, 80, None),
        4 => (108, 48, Some(88)),
        5 => (124, 84, Some(64)),
        other => panic!("no CHD v{other} layout"),
    };
    let mut h = vec![0u8; len];
    h[..8].copy_from_slice(b"MComprHD");
    h[8..12].copy_from_slice(&(len as u32).to_be_bytes());
    h[12..16].copy_from_slice(&version.to_be_bytes());
    h[sha1_at..sha1_at + 20].copy_from_slice(&sha1);
    if let Some(at) = raw_at {
        h[at..at + 20].copy_from_slice(&rawsha1);
    }
    h
}

/// A [`FileBytes`] over the disk that counts the files it opens and the
/// bytes read from them.
#[derive(Debug, Default)]
pub struct CountingBytes {
    opens: AtomicUsize,
    read: Arc<AtomicU64>,
}

impl CountingBytes {
    pub fn new() -> CountingBytes {
        CountingBytes::default()
    }

    /// Files opened so far.
    pub fn opens(&self) -> usize {
        self.opens.load(Ordering::SeqCst)
    }

    /// Bytes read so far.
    pub fn bytes_read(&self) -> u64 {
        self.read.load(Ordering::SeqCst)
    }
}

struct CountingReader<R> {
    inner: R,
    read: Arc<AtomicU64>,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read.fetch_add(n as u64, Ordering::SeqCst);
        Ok(n)
    }
}

impl FileBytes for CountingBytes {
    fn open(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        self.opens.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(CountingReader {
            inner: DiskBytes.open(path)?,
            read: Arc::clone(&self.read),
        }))
    }
}

/// An [`EntryListing`] that counts every call before passing it on.
#[derive(Debug, Default)]
pub struct CountingListing<L> {
    inner: L,
    calls: AtomicUsize,
}

impl<L: EntryListing> CountingListing<L> {
    pub fn new(inner: L) -> CountingListing<L> {
        CountingListing {
            inner,
            calls: AtomicUsize::new(0),
        }
    }

    /// Calls of any method so far.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn count(&self) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

impl<L: EntryListing> EntryListing for CountingListing<L> {
    fn list(&self, path: &Path) -> io::Result<Option<Vec<ListedEntry>>> {
        self.count();
        self.inner.list(path)
    }

    fn payload_sha1(
        &self,
        path: &Path,
        entry: usize,
        rule: HeaderRule,
        budget: u64,
    ) -> io::Result<Option<String>> {
        self.count();
        self.inner.payload_sha1(path, entry, rule, budget)
    }

    fn head(&self, path: &Path, entry: usize, len: usize) -> io::Result<Option<Vec<u8>>> {
        self.count();
        self.inner.head(path, entry, len)
    }
}
