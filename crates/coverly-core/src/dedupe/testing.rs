//! Fakes of the duplicates seams and synthetic builders, for tests of this
//! crate and of its users (feature `test-util`).

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::identify::hash::HeaderRule;

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
