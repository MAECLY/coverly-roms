//! [`ArchiveEntryListing`]: the [`EntryListing`] seam over the archive
//! reader (`crate::archive`), so a 7z, a rar, a tar or a single-file
//! compressor is compared by its entries like a zip (plan DD-13).
//!
//! Listings and heads go through the process-wide
//! [`Cache`] the organise planner shares; a
//! payload hash opens a fresh reader under the caller's budget, its CRC
//! checked by the reader at the end of the entry. A spent budget is "not
//! read" (`Ok(None)`), never an error. External tools (RAR, 7z coders the
//! pure-Rust reader lacks) run only for a file whose magic is RAR or 7z,
//! only when [`Limits::external_tools`] allows them, and are discovered on
//! first need outside the library and the quarantine: a zip never starts
//! a discovery. Read-only.

use std::fs::File;
use std::io::{self, Read};
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use crate::archive::cache::{Cache, CacheCaps, Listing};
use crate::archive::{
    is_budget_spent, open_archive_with, ArchiveReader, Budget, Entry, EntryKind, Limits,
    ToolsOnDemand,
};
use crate::identify::hash::{hash_stream, HeaderRule};

use super::source::{EntryListing, ListedEntry};

/// [`EntryListing`] over every format the archive reader opens.
pub struct ArchiveEntryListing {
    limits: Limits,
    /// Listings and heads of archives that need no tool.
    cache: &'static Cache,
    /// The tools a RAR or a 7z may run, and the cache of what they list.
    tools: Option<(Arc<ToolsOnDemand>, Cache)>,
}

impl std::fmt::Debug for ArchiveEntryListing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiveEntryListing")
            .field("limits", &self.limits)
            .field("tools", &self.tools.as_ref().map(|(t, _)| t))
            .finish()
    }
}

impl ArchiveEntryListing {
    /// A listing that runs no external tool: a RAR is unreadable.
    pub fn new(limits: Limits) -> Self {
        ArchiveEntryListing {
            limits,
            cache: Cache::global(),
            tools: None,
        }
    }

    /// A listing whose RAR and 7z files may run the external tools found
    /// on first need, never from a `PATH` entry inside `roots_to_avoid`
    /// (the library root and the quarantine).
    pub fn with_tools(limits: Limits, roots_to_avoid: &[&Path]) -> Self {
        Self::with_tools_on_demand(limits, Arc::new(ToolsOnDemand::new(roots_to_avoid)))
    }

    /// [`ArchiveEntryListing::with_tools`] over an explicit discovery
    /// (tests pass one whose search they control).
    pub fn with_tools_on_demand(limits: Limits, tools: Arc<ToolsOnDemand>) -> Self {
        let found = Arc::clone(&tools);
        let cache = Cache::with_opener(
            CacheCaps::default(),
            Box::new(move |path, limits| open_archive_with(path, limits, Some(found.get()))),
        );
        ArchiveEntryListing {
            limits,
            cache: Cache::global(),
            tools: Some((tools, cache)),
        }
    }

    /// The bounds archives are opened under.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// The tools and their cache when `path` may need one: a RAR or a 7z
    /// by magic, with tools allowed.
    fn tools_for(&self, path: &Path) -> Option<&(Arc<ToolsOnDemand>, Cache)> {
        let tools = self.tools.as_ref().filter(|_| self.limits.external_tools)?;
        may_need_tool(path).then_some(tools)
    }

    fn cache_for(&self, path: &Path) -> &Cache {
        self.tools_for(path).map_or(self.cache, |(_, c)| c)
    }

    /// A fresh reader of `path` (no cache), with tools when it may need
    /// them.
    fn open(&self, path: &Path) -> io::Result<Option<Box<dyn ArchiveReader>>> {
        let tools = self.tools_for(path).map(|(t, _)| t.get());
        Ok(open_archive_with(path, &self.limits, tools)?)
    }

    fn listing(&self, path: &Path) -> io::Result<Option<Arc<Listing>>> {
        Ok(self.cache_for(path).listing(path, &self.limits)?)
    }

    /// File entry `n` (dedupe's index space) of `path`.
    fn entry(&self, path: &Path, n: usize) -> io::Result<Entry> {
        let listing = self
            .listing(path)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not an archive"))?;
        let entry = files(&listing).nth(n).cloned();
        entry.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no entry {n}")))
    }

    /// Decoded bytes it costs to reach the start of file entry `n`: the
    /// earlier entries of its solid block (a 7z folder, a RAR solid group,
    /// a tar inside a compressor); nothing for an independent entry.
    pub(crate) fn decode_cost(&self, path: &Path, n: usize) -> io::Result<u64> {
        let listing = self
            .listing(path)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not an archive"))?;
        let entry = files(&listing)
            .nth(n)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no entry {n}")))?;
        let Some(block) = entry.solid_block else {
            return Ok(0);
        };
        Ok(listing
            .entries
            .iter()
            .take_while(|e| e.index != entry.index)
            .filter(|e| e.solid_block == Some(block))
            .map(|e| e.size)
            .fold(0u64, u64::saturating_add))
    }

    /// [`EntryListing::head`] with the bytes decoded charged to `budget`
    /// (one budget for every read of a disc probe). A head already cached
    /// costs nothing.
    pub(crate) fn head_with(
        &self,
        path: &Path,
        n: usize,
        len: usize,
        budget: &mut Budget,
    ) -> io::Result<Option<Vec<u8>>> {
        let e = self.entry(path, n)?;
        let head = self
            .cache_for(path)
            .head(path, e.index, len, &self.limits, budget)
            .map_err(io::Error::from);
        Ok(not_read_if_spent(head)?.flatten())
    }

    /// Bytes `range` of file entry `n` (a skip, then a read) on a fresh
    /// reader, every decoded byte charged to `budget`. `Ok(None)` when the
    /// budget runs out first; fewer bytes when the entry is shorter.
    pub(crate) fn read_range(
        &self,
        path: &Path,
        n: usize,
        range: Range<u64>,
        budget: &mut Budget,
    ) -> io::Result<Option<Vec<u8>>> {
        let entry = self.entry(path, n)?;
        let mut reader = self
            .open(path)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not an archive"))?;
        let read = reader
            .open_entry(entry.index, budget)
            .map_err(io::Error::from)
            .and_then(|mut stream| {
                io::copy(&mut (&mut stream).take(range.start), &mut io::sink())?;
                let mut out = Vec::new();
                stream
                    .take(range.end.saturating_sub(range.start))
                    .read_to_end(&mut out)?;
                Ok(out)
            });
        not_read_if_spent(read)
    }
}

/// Whether `path` starts with a RAR or a 7z signature: the only formats an
/// external tool may open (the `may_need_tool` rule of organise).
fn may_need_tool(path: &Path) -> bool {
    let mut magic = [0u8; 6];
    let read = File::open(path).and_then(|mut f| f.read_exact(&mut magic));
    read.is_ok() && (magic == *b"7z\xBC\xAF\x27\x1C" || magic == *b"Rar!\x1A\x07")
}

/// The file entries of `listing`, in archive order.
fn files(listing: &Listing) -> impl Iterator<Item = &Entry> {
    listing.entries.iter().filter(|e| e.kind == EntryKind::File)
}

fn listed(e: &Entry) -> ListedEntry {
    ListedEntry {
        name: e.name.as_str().to_owned(),
        size: e.size,
        crc32: e.crc32,
    }
}

/// A spent decode budget is "not read".
fn not_read_if_spent<T>(r: io::Result<T>) -> io::Result<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) if is_budget_spent(&e) => Ok(None),
        Err(e) => Err(e),
    }
}

impl EntryListing for ArchiveEntryListing {
    fn list(&self, path: &Path) -> io::Result<Option<Vec<ListedEntry>>> {
        Ok(self.listing(path)?.map(|l| files(&l).map(listed).collect()))
    }

    fn payload_sha1(
        &self,
        path: &Path,
        entry: usize,
        rule: HeaderRule,
        budget: u64,
    ) -> io::Result<Option<String>> {
        let e = self.entry(path, entry)?;
        if e.size > budget {
            return Ok(None);
        }
        let mut reader = self
            .open(path)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not an archive"))?;
        // The caller's byte budget with the plan's wall time.
        let mut budget = Budget::plan_head(&Limits {
            plan_head_budget: budget,
            ..self.limits.clone()
        });
        let hashed = reader
            .open_entry(e.index, &mut budget)
            .map_err(io::Error::from)
            // One byte past the size reaches the end, where the reader
            // checks the size and CRC its listing states.
            .and_then(|s| hash_stream(s.take(e.size.saturating_add(1)), e.size, rule));
        let Some(h) = not_read_if_spent(hashed)? else {
            return Ok(None);
        };
        if h.full.size != e.size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("entry {} does not match its listing", e.name.as_str()),
            ));
        }
        Ok(h.payload_sha1)
    }

    fn head(&self, path: &Path, entry: usize, len: usize) -> io::Result<Option<Vec<u8>>> {
        self.head_with(path, entry, len, &mut Budget::plan_head(&self.limits))
    }
}
