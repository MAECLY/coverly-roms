//! Listing and head-bytes cache (spec `2026-10-23-organize-archives-and-discs.md`
//! §4.5).
//!
//! Planning opens one archive several times: its listing, the head of the
//! primary track, dedupe's `head` and `payload_sha1`. [`Cache`] keeps up to
//! eight archives keyed by `(path, size, mtime_ns)`, so a changed file
//! misses. A slot holds **only** data: the validated listing (format,
//! entries, completeness) and already-decoded head bytes (at most 4 MiB
//! per archive and 32 MiB per cache). It never holds a reader, a decoder,
//! a file handle or a [`MemoryGate`](super::MemoryGate) permit: every miss
//! opens a fresh reader, which re-validates the file (S2, S5) and is
//! dropped, with its decoder's permit, before the call returns. So the
//! cache can never pin decoder memory, a worker waiting on the gate never
//! waits on an idle cached decoder, and no handle keeps an archive open
//! (Windows could not move it to the Trash).
//!
//! A reader that panicked (poisoned, S1) or failed is never cached, and
//! drops what was cached for its path. The lock is never held while an
//! archive is opened or decoded.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::UNIX_EPOCH;
use std::{fs, io, io::Read};

use super::{
    open_archive, open_archive_with, ArchiveError, ArchiveReader, Budget, Entry, Format, FoundTool,
    Limits,
};

const MIB: usize = 1024 * 1024;

/// Bounds of a [`Cache`] (spec §4.5). `Default` is the spec's values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheCaps {
    /// Archives kept.
    pub entries: usize,
    /// Head bytes kept per archive (all its entries together).
    pub head_per_archive: usize,
    /// Head bytes kept by the whole cache.
    pub head_total: usize,
}

impl Default for CacheCaps {
    fn default() -> Self {
        Self {
            entries: 8,
            head_per_archive: 4 * MIB,
            head_total: 32 * MIB,
        }
    }
}

/// What identifies one version of a file: a change of size or mtime is a
/// different archive.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub path: PathBuf,
    pub size: u64,
    pub mtime_ns: i128,
}

impl CacheKey {
    /// The key of `path` as it is now; `None` when the file system gives
    /// no modification time (such a file is never cached).
    pub fn of(path: &Path) -> Result<Option<CacheKey>, ArchiveError> {
        let meta = fs::metadata(path).map_err(ArchiveError::Io)?;
        let Ok(modified) = meta.modified() else {
            return Ok(None);
        };
        let mtime_ns = match modified.duration_since(UNIX_EPOCH) {
            Ok(after) => i128::try_from(after.as_nanos()).unwrap_or(i128::MAX),
            Err(before) => -i128::try_from(before.duration().as_nanos()).unwrap_or(i128::MAX),
        };
        Ok(Some(CacheKey {
            path: path.to_path_buf(),
            size: meta.len(),
            mtime_ns,
        }))
    }
}

/// An archive's validated listing, as cached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub format: Format,
    pub entries: Vec<Entry>,
    /// [`ArchiveReader::listing_complete`].
    pub complete: bool,
}

impl Listing {
    fn of(reader: &dyn ArchiveReader) -> Listing {
        Listing {
            format: reader.format(),
            entries: reader.entries().to_vec(),
            complete: reader.listing_complete(),
        }
    }
}

/// How the cache opens an archive: [`open_archive`], [`open_archive_with`]
/// its tools, or a test double.
pub type Opener =
    dyn Fn(&Path, &Limits) -> Result<Option<Box<dyn ArchiveReader>>, ArchiveError> + Send + Sync;

/// Decoded bytes from the start of one entry.
#[derive(Clone, Debug)]
struct Head {
    index: usize,
    bytes: Vec<u8>,
    /// The bytes are the whole entry (its stream ended).
    whole: bool,
}

#[derive(Debug)]
struct Slot {
    key: CacheKey,
    listing: Arc<Listing>,
    heads: Vec<Head>,
    used: u64,
}

impl Slot {
    fn head_bytes(&self) -> usize {
        self.heads.iter().map(|h| h.bytes.len()).sum()
    }
}

#[derive(Debug, Default)]
struct State {
    slots: Vec<Slot>,
    tick: u64,
}

/// The listing and head-bytes cache (module docs).
pub struct Cache {
    caps: CacheCaps,
    opener: Box<Opener>,
    state: Mutex<State>,
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cache")
            .field("caps", &self.caps)
            .field("archives", &self.len())
            .field("head_bytes", &self.head_bytes())
            .finish()
    }
}

impl Cache {
    /// A cache with the spec's caps over [`open_archive`]: no external tool
    /// runs.
    pub fn new() -> Self {
        Self::with_caps(CacheCaps::default())
    }

    pub fn with_caps(caps: CacheCaps) -> Self {
        Self::with_opener(caps, Box::new(open_archive))
    }

    /// A cache whose archives may run `tools`: the discovery for the job's
    /// source and target roots (`discover(&[src, dst])`, spec §5.2).
    pub fn with_tools(caps: CacheCaps, tools: Vec<FoundTool>) -> Self {
        Self::with_opener(
            caps,
            Box::new(move |path, limits| open_archive_with(path, limits, Some(&tools))),
        )
    }

    /// A cache that opens archives with `opener` (tests).
    pub fn with_opener(caps: CacheCaps, opener: Box<Opener>) -> Self {
        Self {
            caps,
            opener,
            state: Mutex::new(State::default()),
        }
    }

    /// The process-wide cache the planner and dedupe share.
    pub fn global() -> &'static Cache {
        static GLOBAL: OnceLock<Cache> = OnceLock::new();
        GLOBAL.get_or_init(Cache::new)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic while holding the lock leaves only data behind: recover.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Archives cached.
    pub fn len(&self) -> usize {
        self.lock().slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Head bytes cached, over every archive.
    pub fn head_bytes(&self) -> usize {
        self.lock().slots.iter().map(Slot::head_bytes).sum()
    }

    /// Drops whatever is cached for `path`.
    pub fn forget(&self, path: &Path) {
        self.lock().slots.retain(|s| s.key.path != path);
    }

    /// The validated listing of `path`, cached or by opening it. `Ok(None)`
    /// when it is no archive (not cached).
    pub fn listing(
        &self,
        path: &Path,
        limits: &Limits,
    ) -> Result<Option<Arc<Listing>>, ArchiveError> {
        let key = CacheKey::of(path)?;
        if let Some(listing) = key.as_ref().and_then(|k| self.cached_listing(k)) {
            return Ok(Some(listing));
        }
        let Some(reader) = self.open(path, limits)? else {
            return Ok(None);
        };
        let listing = Arc::new(Listing::of(reader.as_ref()));
        let poisoned = reader.is_poisoned();
        drop(reader);
        self.store(key, path, poisoned, &listing, None);
        Ok(Some(listing))
    }

    /// The first `n` decoded bytes of entry `index` of `path` (fewer when
    /// the entry is shorter), cached or decoded under `budget`. Of the
    /// bytes decoded, at most the per-archive head cap is kept. `Ok(None)`
    /// when `path` is no archive.
    pub fn head(
        &self,
        path: &Path,
        index: usize,
        n: usize,
        limits: &Limits,
        budget: &mut Budget,
    ) -> Result<Option<Vec<u8>>, ArchiveError> {
        let key = CacheKey::of(path)?;
        if let Some(bytes) = key.as_ref().and_then(|k| self.cached_head(k, index, n)) {
            return Ok(Some(bytes));
        }
        let Some(mut reader) = self.open(path, limits)? else {
            return Ok(None);
        };
        let listing = Arc::new(Listing::of(reader.as_ref()));
        if index >= listing.entries.len() {
            return Err(ArchiveError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("no entry {index}"),
            )));
        }
        let read = read_head(reader.as_mut(), index, n, budget);
        let poisoned = reader.is_poisoned();
        // The reader, its decoder and its gate permit end here, before
        // anything is cached.
        drop(reader);
        match read {
            Ok(bytes) => {
                let whole = bytes.len() < n;
                let head = Head {
                    index,
                    bytes,
                    whole,
                };
                self.store(key, path, poisoned, &listing, Some(&head));
                Ok(Some(head.bytes))
            }
            Err(e) => {
                if poisoned || matches!(e, ArchiveError::Damaged { .. }) {
                    self.forget(path);
                }
                Err(e)
            }
        }
    }

    fn open(
        &self,
        path: &Path,
        limits: &Limits,
    ) -> Result<Option<Box<dyn ArchiveReader>>, ArchiveError> {
        match (self.opener)(path, limits) {
            Ok(r) => Ok(r),
            Err(e) => {
                self.forget(path);
                Err(e)
            }
        }
    }

    fn cached_listing(&self, key: &CacheKey) -> Option<Arc<Listing>> {
        let mut state = self.lock();
        let tick = state.next_tick();
        let slot = state.slots.iter_mut().find(|s| &s.key == key)?;
        slot.used = tick;
        Some(Arc::clone(&slot.listing))
    }

    fn cached_head(&self, key: &CacheKey, index: usize, n: usize) -> Option<Vec<u8>> {
        let mut state = self.lock();
        let tick = state.next_tick();
        let slot = state.slots.iter_mut().find(|s| &s.key == key)?;
        let head = slot.heads.iter().find(|h| h.index == index)?;
        if head.bytes.len() < n && !head.whole {
            return None;
        }
        let bytes = head.bytes[..n.min(head.bytes.len())].to_vec();
        slot.used = tick;
        Some(bytes)
    }

    /// Caches `listing` (and `head`) for the version of `path` that was
    /// read: only when the file is unchanged since `key` was taken and the
    /// reader is not poisoned.
    fn store(
        &self,
        key: Option<CacheKey>,
        path: &Path,
        poisoned: bool,
        listing: &Arc<Listing>,
        head: Option<&Head>,
    ) {
        if poisoned {
            self.forget(path);
            return;
        }
        let Some(key) = key else {
            return;
        };
        if !matches!(CacheKey::of(path), Ok(Some(now)) if now == key) {
            self.forget(path);
            return;
        }
        let caps = self.caps;
        let mut state = self.lock();
        let tick = state.next_tick();
        let at = match state.slots.iter().position(|s| s.key.path == key.path) {
            Some(i) if state.slots[i].key == key => i,
            found => {
                if let Some(i) = found {
                    state.slots.remove(i);
                }
                state.slots.push(Slot {
                    key,
                    listing: Arc::clone(listing),
                    heads: Vec::new(),
                    used: tick,
                });
                state.slots.len() - 1
            }
        };
        let slot = &mut state.slots[at];
        slot.used = tick;
        if let Some(head) = head {
            slot.heads.retain(|h| h.index != head.index);
            let room = caps.head_per_archive.saturating_sub(slot.head_bytes());
            let keep = head.bytes.len().min(room);
            if keep > 0 {
                slot.heads.push(Head {
                    index: head.index,
                    bytes: head.bytes[..keep].to_vec(),
                    whole: head.whole && keep == head.bytes.len(),
                });
            }
        }
        state.evict(caps, tick);
    }
}

impl State {
    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    /// Drops least recently used archives until both caps hold; the one
    /// used at `keep` (just stored) goes last, and then only its heads.
    fn evict(&mut self, caps: CacheCaps, keep: u64) {
        let total = |s: &State| s.slots.iter().map(Slot::head_bytes).sum::<usize>();
        while self.slots.len() > caps.entries.max(1) || total(self) > caps.head_total {
            let victim = self
                .slots
                .iter()
                .enumerate()
                .filter(|(_, s)| s.used != keep)
                .min_by_key(|(_, s)| s.used)
                .map(|(i, _)| i);
            match victim {
                Some(i) => {
                    self.slots.remove(i);
                }
                None => {
                    // Only the new slot is left and its heads alone are
                    // over the total cap.
                    for slot in &mut self.slots {
                        slot.heads.clear();
                    }
                    break;
                }
            }
        }
    }
}

/// Up to `n` bytes from the start of entry `index`; the stream (and its
/// decoder) is dropped before this returns.
fn read_head(
    reader: &mut dyn ArchiveReader,
    index: usize,
    n: usize,
    budget: &mut Budget,
) -> Result<Vec<u8>, ArchiveError> {
    let stream = reader.open_entry(index, budget)?;
    let mut bytes = Vec::new();
    stream
        .take(n as u64)
        .read_to_end(&mut bytes)
        .map_err(ArchiveError::from)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::budget::with_gate_cap;
    use crate::archive::testing::{xz_stream, zip_with, XzBlock, ZipMethod};
    use crate::archive::EntryKind;
    use crate::archive::{GuardedArchive, MemoryGate, Method, RelPath};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, SystemTime};

    /// A cache over `open_archive` that counts the archives it opens.
    fn counting(caps: CacheCaps) -> (Cache, Arc<AtomicUsize>) {
        let opened = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&opened);
        let cache = Cache::with_opener(
            caps,
            Box::new(move |path, limits| {
                count.fetch_add(1, Ordering::SeqCst);
                open_archive(path, limits)
            }),
        );
        (cache, opened)
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    fn budget() -> Budget {
        Budget::plan_head(&Limits::default())
    }

    #[test]
    fn listing_and_head_served_from_cache() {
        let dir = tempfile::tempdir().unwrap();
        let rom: Vec<u8> = (0..=255u8).cycle().take(8192).collect();
        let path = write(
            dir.path(),
            "a.zip",
            &zip_with(&[("Tetris.gb", &rom, ZipMethod::Deflate)]),
        );
        let (cache, opened) = counting(CacheCaps::default());
        let l = Limits::default();
        let first = cache.listing(&path, &l).unwrap().unwrap();
        let again = cache.listing(&path, &l).unwrap().unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        assert_eq!(opened.load(Ordering::SeqCst), 1);

        let head = cache
            .head(&path, 0, 1000, &l, &mut budget())
            .unwrap()
            .unwrap();
        assert_eq!(head, rom[..1000]);
        assert_eq!(opened.load(Ordering::SeqCst), 2);
        // A shorter head comes from the cached bytes.
        let short = cache
            .head(&path, 0, 10, &l, &mut budget())
            .unwrap()
            .unwrap();
        assert_eq!(short, rom[..10]);
        assert_eq!(opened.load(Ordering::SeqCst), 2);
        // A longer one decodes again; one past the end is the whole entry
        // and is then served from the cache at any length.
        let all = cache
            .head(&path, 0, 1 << 20, &l, &mut budget())
            .unwrap()
            .unwrap();
        assert_eq!(all, rom);
        assert_eq!(opened.load(Ordering::SeqCst), 3);
        let all = cache
            .head(&path, 0, 1 << 21, &l, &mut budget())
            .unwrap()
            .unwrap();
        assert_eq!(all, rom);
        assert_eq!(opened.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn cache_invalidated_by_mtime_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "a.zip",
            &zip_with(&[("a.gba", b"same", ZipMethod::Stored)]),
        );
        let when = SystemTime::now() - Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(when)
            .unwrap();
        let (cache, opened) = counting(CacheCaps::default());
        let l = Limits::default();
        let names = |listing: &Listing| listing.entries[0].name.as_str().to_string();
        assert_eq!(names(&cache.listing(&path, &l).unwrap().unwrap()), "a.gba");

        // Same size, other content, a later mtime (by more than a FAT
        // 2-second tick).
        fs::write(&path, zip_with(&[("b.gba", b"same", ZipMethod::Stored)])).unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(when + Duration::from_secs(10))
            .unwrap();
        assert_eq!(names(&cache.listing(&path, &l).unwrap().unwrap()), "b.gba");
        assert_eq!(opened.load(Ordering::SeqCst), 2);
        assert_eq!(cache.len(), 1, "the old version's slot is replaced");
    }

    #[test]
    fn cache_never_holds_decoder() {
        // Nine xz archives through a cache of eight: every call decodes
        // (taking gate memory for its LZMA2 decoder), and once it returns
        // the gate is fully free, whatever the cache holds.
        with_gate_cap(1 << 30, || {
            let dir = tempfile::tempdir().unwrap();
            let paths: Vec<PathBuf> = (0..9)
                .map(|i| {
                    let data = vec![i as u8; 4096];
                    write(
                        dir.path(),
                        &format!("game{i}.gba.xz"),
                        &xz_stream(&[XzBlock { data, dict_prop: 8 }]),
                    )
                })
                .collect();
            let gate = MemoryGate::global();
            let l = Limits::default();
            // The decoder really takes gate memory while it runs.
            {
                let mut reader = open_archive(&paths[0], &l).unwrap().unwrap();
                let mut b = budget();
                let mut stream = reader.open_entry(0, &mut b).unwrap();
                let mut byte = [0u8; 1];
                stream.read_exact(&mut byte).unwrap();
                assert!(gate.in_use() > 0, "the xz decoder holds a permit");
            }
            assert_eq!(gate.in_use(), 0);

            let cache = Cache::new();
            for path in &paths {
                let head = cache
                    .head(path, 0, 100, &l, &mut budget())
                    .unwrap()
                    .unwrap();
                assert_eq!(head.len(), 100);
                assert_eq!(gate.in_use(), 0, "nothing cached holds decoder memory");
            }
            assert_eq!(cache.len(), 8);
            for path in &paths {
                cache.listing(path, &l).unwrap().unwrap();
                assert_eq!(gate.in_use(), 0);
            }
        });
    }

    #[test]
    fn cache_head_bytes_capped() {
        let dir = tempfile::tempdir().unwrap();
        let caps = CacheCaps {
            entries: 8,
            head_per_archive: 1024,
            head_total: 2048,
        };
        let (cache, opened) = counting(caps);
        let l = Limits::default();
        let rom = vec![0x5Au8; 4096];
        let paths: Vec<PathBuf> = (0..3)
            .map(|i| {
                write(
                    dir.path(),
                    &format!("{i}.zip"),
                    &zip_with(&[
                        ("a.bin", &rom, ZipMethod::Stored),
                        ("b.bin", &rom, ZipMethod::Stored),
                    ]),
                )
            })
            .collect();
        // All 4 KiB are returned; 1 KiB is kept.
        let head = cache
            .head(&paths[0], 0, 4096, &l, &mut budget())
            .unwrap()
            .unwrap();
        assert_eq!(head.len(), 4096);
        assert_eq!(cache.head_bytes(), 1024);
        // The archive's cap is shared by its entries.
        cache
            .head(&paths[0], 1, 512, &l, &mut budget())
            .unwrap()
            .unwrap();
        assert_eq!(cache.head_bytes(), 1024);
        // A kept prefix still serves shorter heads; a longer one decodes.
        let before = opened.load(Ordering::SeqCst);
        cache
            .head(&paths[0], 0, 1024, &l, &mut budget())
            .unwrap()
            .unwrap();
        assert_eq!(opened.load(Ordering::SeqCst), before);
        cache
            .head(&paths[0], 0, 1025, &l, &mut budget())
            .unwrap()
            .unwrap();
        assert_eq!(opened.load(Ordering::SeqCst), before + 1);
        // The total cap evicts the least recently used archive.
        for path in &paths[1..] {
            cache
                .head(path, 0, 4096, &l, &mut budget())
                .unwrap()
                .unwrap();
            assert!(cache.head_bytes() <= 2048, "{}", cache.head_bytes());
        }
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn cache_keeps_at_most_eight_archives() {
        let dir = tempfile::tempdir().unwrap();
        let (cache, opened) = counting(CacheCaps::default());
        let l = Limits::default();
        let paths: Vec<PathBuf> = (0..10)
            .map(|i| {
                write(
                    dir.path(),
                    &format!("{i}.zip"),
                    &zip_with(&[("x.gb", b"rom", ZipMethod::Stored)]),
                )
            })
            .collect();
        for path in &paths {
            cache.listing(path, &l).unwrap().unwrap();
        }
        assert_eq!(cache.len(), 8);
        // The two oldest went; the newest are still cached.
        cache.listing(&paths[9], &l).unwrap().unwrap();
        assert_eq!(opened.load(Ordering::SeqCst), 10);
        cache.listing(&paths[0], &l).unwrap().unwrap();
        assert_eq!(opened.load(Ordering::SeqCst), 11);
    }

    #[test]
    fn not_an_archive_is_none_and_not_cached() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "Tetris.gba", &[0xA5u8; 512]);
        let cache = Cache::new();
        assert!(cache.listing(&path, &Limits::default()).unwrap().is_none());
        assert!(cache
            .head(&path, 0, 16, &Limits::default(), &mut budget())
            .unwrap()
            .is_none());
        assert!(cache.is_empty());
    }

    /// A backend whose entry reads panic (S1).
    struct Panicking(Vec<Entry>);

    impl ArchiveReader for Panicking {
        fn format(&self) -> Format {
            Format::Zip
        }
        fn entries(&self) -> &[Entry] {
            &self.0
        }
        fn open_entry<'a>(
            &'a mut self,
            _index: usize,
            _budget: &'a mut Budget,
        ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
            panic!("decoder bug");
        }
        fn stream_entries(
            &mut self,
            _wanted: &[usize],
            _budget: &mut Budget,
            _sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
        ) -> Result<(), ArchiveError> {
            Ok(())
        }
    }

    #[test]
    fn poisoned_reader_not_cached() {
        crate::archive::guard::install_panic_hook();
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "a.zip", b"anything");
        let entry = Entry {
            index: 0,
            name: RelPath::parse("a.bin").unwrap(),
            raw_name: b"a.bin".to_vec(),
            kind: EntryKind::File,
            size: 10,
            packed: None,
            crc32: None,
            encrypted: false,
            method: Method::Stored,
            solid_block: None,
            mtime: None,
            unsafe_name: None,
        };
        let cache = Cache::with_opener(
            CacheCaps::default(),
            Box::new(move |_, _| {
                Ok(Some(Box::new(GuardedArchive::new(Panicking(vec![
                    entry.clone()
                ])))))
            }),
        );
        let l = Limits::default();
        // The listing alone is fine and cached…
        cache.listing(&path, &l).unwrap().unwrap();
        assert_eq!(cache.len(), 1);
        // …until a read of the same archive panics: damaged, and dropped.
        let err = cache
            .head(&path, 0, 4, &l, &mut budget())
            .expect_err("panic caught");
        assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
        assert!(cache.is_empty());
    }
}
