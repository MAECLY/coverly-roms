//! tar, alone or inside a compressor (`.tar.gz`/`.tgz`, `.tar.bz2`,
//! `.tar.xz`, `.tar.zst`, `.tar.lz4`), read in **raw mode** (spec
//! `2026-10-23-organize-archives-and-discs.md` §4.3, §13 S6, S6b).
//!
//! **Raw walk.** Coverly walks the 512-byte blocks itself and uses
//! `tar::Header` only to decode header fields (sizes in octal or base-256,
//! the ustar prefix, the mtime). Nothing is ever unpacked, and no special
//! record is processed by the crate: its `Entries` iterator (even with
//! `raw(true)`) ends at the *first* zero block, cannot skip the extension
//! blocks of an old GNU sparse header and gives no hook between the end
//! marker and the end of the stream, all of which S6 and S6b need. The
//! header checksum (unsigned, or the signed sum old writers use) is checked
//! on every header.
//!
//! **Metadata records (S6b).** A pax `x`/`g` or GNU `L`/`K` record is read
//! only when its declared size is at most [`MAX_RECORD`] (1 MiB), else
//! `Damaged`; pax `length key=value\n` records are parsed here with checked
//! arithmetic. `path`, `size` and `linkpath` of an `x` record (and the name
//! of an `L` record) apply to the next header only; two `x` (or two `L`)
//! records for one entry are `Damaged`, and so is a global `g` record that
//! would override `path`, `size`, `linkpath` or sparse keys for every later
//! entry. At most [`MAX_RECORDS_PER_ENTRY`] records precede one entry, and
//! at most `max_entries` records (sparse extension blocks included) appear
//! in one archive, like entries.
//!
//! **Kinds.** Typeflags `0`, NUL and `7` are files (a NUL/`0` entry whose
//! name ends in `/` is a v7 directory), `5` directories, `1` hard links, `2`
//! symlinks; `3`/`4`/`6` (devices, FIFOs), sparse files (`S`, GNU sparse pax
//! keys), multi-volume continuations (`M`) and anything unknown are
//! [`EntryKind::Other`]. Only files are ever read. An entry whose path
//! goes through a symlink entry of the same archive (`d → /etc`, then
//! `d/passwd`; compared case-insensitively) gets `unsafe_name: Link`.
//!
//! **Inside a compressor (S6).** The walk reads to the end-of-archive
//! marker (two zero blocks; required inside a compressor, while a plain tar
//! may also end at EOF on a block boundary), then drains at most
//! `tar_trailer_allowance` (1 MiB) more decoded bytes, charged to the
//! budget, so the outer check (gzip CRC32/ISIZE, xz check, zstd checksum,
//! bzip2 CRC) runs at EOF. A longer trailer (zeros, a second gzip member,
//! another xz stream) is `BudgetSpent` with a byte-limited (plan) budget
//! and `Damaged { "trailing data" }` with the apply budget.
//!
//! **Listing.** A plain tar is listed by seeking from header to header,
//! with no decode budget. A compressed one is decoded through
//! [`open_compressor`] with `plan_listing_budget` (256 MiB) and the plan
//! wall time; when the budget runs out before the end marker, the entries
//! seen so far are the listing and [`ArchiveReader::listing_complete`] is
//! false (the planner may then move or copy the archive, never extract
//! it). tar stores no CRC: entries have `crc32: None`, and readers verify
//! the size.
//!
//! **Reading.** `open_entry` of a plain tar seeks to the entry's records;
//! of a compressed one it decodes from the start, charging the skipped
//! bytes. Both walk the headers again and refuse an archive that changed
//! since the listing. `stream_entries` makes one pass; inside a compressor
//! it goes on to the end marker and through the trailer drain, so a bad
//! stream check at EOF fails the whole pass. Every call runs inside the
//! panic guard (S1).

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::budget::{Budget, ChargedRead};
use super::guard::{guarded, GuardedRead, Poison};
use super::names::{name_key, RelPath, UnsafeWhy};
use super::single::{open_compressor, stream_method};
use super::verify::VerifyingReader;
use super::{
    ArchiveError, ArchiveReader, BudgetKind, Compressor, Entry, EntryKind, Format, Limits, Method,
    KIB, MIB,
};

/// Largest pax or GNU long-name/long-link record read into memory (S6b).
pub const MAX_RECORD: u64 = MIB;
/// Most metadata records before one entry (S6b).
pub const MAX_RECORDS_PER_ENTRY: u32 = 8;
/// Detail of a trailer longer than the allowance on apply (S6).
pub const TRAILING_DATA: &str = "trailing data";

const BLOCK: u64 = 512;

/// Where an entry's records start (its first metadata record, or its
/// header) and where its data starts, as offsets in the tar stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    header_at: u64,
    data_at: u64,
}

/// A tar, plain or inside a compressor, read as an archive (module docs).
pub struct TarReader<R: Read + Seek + Send = File> {
    src: R,
    outer: Option<Compressor>,
    entries: Vec<Entry>,
    spans: Vec<Span>,
    complete: bool,
    limits: Limits,
    poison: Poison,
}

impl TarReader<File> {
    /// Opens and lists `path` as a tar inside `outer` (`None`: plain).
    pub fn open(
        path: &Path,
        outer: Option<Compressor>,
        limits: &Limits,
    ) -> Result<Self, ArchiveError> {
        let file = File::open(path).map_err(ArchiveError::Io)?;
        Self::new(file, outer, limits)
    }
}

impl<R: Read + Seek + Send> TarReader<R> {
    /// Lists the tar in `src` with the plan listing budget
    /// ([`Budget::plan_listing`]).
    pub fn new(src: R, outer: Option<Compressor>, limits: &Limits) -> Result<Self, ArchiveError> {
        let mut budget = Budget::plan_listing(limits);
        Self::new_with_budget(src, outer, limits, &mut budget)
    }

    /// Lists the tar in `src` charging the decode of a compressed one to
    /// `budget`: a byte-limited budget can stop the listing early (module
    /// docs), and decides how a long trailer is reported (S6).
    pub fn new_with_budget(
        src: R,
        outer: Option<Compressor>,
        limits: &Limits,
        budget: &mut Budget,
    ) -> Result<Self, ArchiveError> {
        let limits = limits.clone();
        guarded(move || Self::list(src, outer, limits, budget))
    }

    /// The source, for tests that change the archive after its listing.
    #[cfg(any(test, feature = "test-util"))]
    pub fn source_mut(&mut self) -> &mut R {
        &mut self.src
    }

    fn list(
        mut src: R,
        outer: Option<Compressor>,
        limits: Limits,
        budget: &mut Budget,
    ) -> Result<Self, ArchiveError> {
        let (method, raws, complete) = match outer {
            None => {
                let len = src.seek(SeekFrom::End(0)).map_err(ArchiveError::Io)?;
                let mut w = Walker::new(Plain::at(&mut src, 0, len)?, &limits, 0);
                let (raws, complete) = collect(&mut w, &limits, None)?;
                (Method::Stored, raws, complete)
            }
            Some(c) => {
                let method = stream_method(&mut src, c, &limits)?;
                let trailer = Trailer {
                    allowance: limits.tar_trailer_allowance,
                    limited: budget.remaining().is_some(),
                };
                let decoder = open_compressor(c, &mut src, &limits, budget)?;
                let mut w =
                    Walker::new(Decoded::new(ChargedRead::new(decoder, budget)), &limits, 0);
                let (raws, complete) = collect(&mut w, &limits, Some(trailer))?;
                (method, raws, complete)
            }
        };
        let compressed = outer.is_some();
        let (mut entries, spans): (Vec<Entry>, Vec<Span>) = raws
            .into_iter()
            .enumerate()
            .map(|(index, raw)| raw.into_entry(index, &method, compressed))
            .unzip();
        mark_link_prefixes(&mut entries);
        Ok(Self {
            src,
            outer,
            entries,
            spans,
            complete,
            limits,
            poison: Poison::new(),
        })
    }

    /// The decoded, verified bytes of entry `index`, before the guard
    /// wrapper.
    fn open_decoded<'a>(
        &'a mut self,
        index: usize,
        budget: &'a mut Budget,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
        let Self {
            src,
            outer,
            entries,
            spans,
            limits,
            ..
        } = self;
        let entry = entries.get(index).ok_or_else(|| no_entry(index))?;
        readable(entry)?;
        let size = entry.size;
        match *outer {
            None => {
                let span = spans[index];
                let len = src.seek(SeekFrom::End(0)).map_err(ArchiveError::Io)?;
                let mut w =
                    Walker::new(Plain::at(src, span.header_at, len)?, limits, span.header_at);
                let raw = w.next()?.ok_or_else(changed)?;
                raw.matches(entry, span)?;
                let charged = ChargedRead::new(w.take(size), budget);
                Ok(Box::new(VerifyingReader::new(charged, size, None)))
            }
            Some(c) => {
                stream_method(src, c, limits)?;
                let decoder = open_compressor(c, &mut *src, limits, budget)?;
                let mut w = Walker::new(Decoded::new(ChargedRead::new(decoder, budget)), limits, 0);
                for (listed, &span) in entries.iter().zip(spans.iter()).take(index + 1) {
                    w.next()?.ok_or_else(changed)?.matches(listed, span)?;
                }
                Ok(Box::new(VerifyingReader::new(w.take(size), size, None)))
            }
        }
    }

    fn stream_inner(
        &mut self,
        wanted: &[usize],
        budget: &mut Budget,
        sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
    ) -> Result<(), ArchiveError> {
        let mut order = wanted.to_vec();
        order.sort_unstable();
        order.dedup();
        for &index in &order {
            readable(self.entries.get(index).ok_or_else(|| no_entry(index))?)?;
        }
        if order.is_empty() {
            return Ok(());
        }
        let Self {
            src,
            outer,
            entries,
            spans,
            limits,
            complete,
            ..
        } = self;
        let Some(c) = *outer else {
            let len = src.seek(SeekFrom::End(0)).map_err(ArchiveError::Io)?;
            for index in order {
                let (entry, span) = (&entries[index], spans[index]);
                let mut w = Walker::new(
                    Plain::at(&mut *src, span.header_at, len)?,
                    limits,
                    span.header_at,
                );
                w.next()?.ok_or_else(changed)?.matches(entry, span)?;
                let charged = ChargedRead::new((&mut w).take(entry.size), &mut *budget);
                let mut data = VerifyingReader::new(charged, entry.size, None);
                sink(entry, &mut data)?;
                drain(&mut data)?;
            }
            return Ok(());
        };

        let trailer = Trailer {
            allowance: limits.tar_trailer_allowance,
            limited: budget.remaining().is_some(),
        };
        stream_method(src, c, limits)?;
        let decoder = open_compressor(c, &mut *src, limits, budget)?;
        let mut w = Walker::new(Decoded::new(ChargedRead::new(decoder, budget)), limits, 0);
        let mut want = order.into_iter().peekable();
        let mut index = 0usize;
        while let Some(raw) = w.next()? {
            if let (Some(listed), Some(&span)) = (entries.get(index), spans.get(index)) {
                raw.matches(listed, span)?;
                if want.peek() == Some(&index) {
                    want.next();
                    let mut data =
                        VerifyingReader::new((&mut w).take(listed.size), listed.size, None);
                    sink(listed, &mut data)?;
                    drain(&mut data)?;
                }
            }
            index += 1;
        }
        // Fewer entries than listed, or more than a complete listing saw.
        if index < entries.len() || (*complete && index > entries.len()) {
            return Err(changed());
        }
        drain_trailer(&mut w, trailer)
    }
}

impl<R: Read + Seek + Send> ArchiveReader for TarReader<R> {
    fn format(&self) -> Format {
        Format::Tar { outer: self.outer }
    }

    fn entries(&self) -> &[Entry] {
        &self.entries
    }

    fn listing_complete(&self) -> bool {
        self.complete
    }

    fn open_entry<'a>(
        &'a mut self,
        index: usize,
        budget: &'a mut Budget,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
        let poison = self.poison.clone();
        let stream = poison.guard(|| self.open_decoded(index, budget))?;
        Ok(Box::new(GuardedRead::new(stream, poison)))
    }

    fn stream_entries(
        &mut self,
        wanted: &[usize],
        budget: &mut Budget,
        sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
    ) -> Result<(), ArchiveError> {
        let poison = self.poison.clone();
        poison.guard(|| self.stream_inner(wanted, budget, sink))
    }
}

/// A tar listed from a stream that cannot seek: bsdtar's pax transcode of
/// an archive it opens (spec §5.2, S8). It is read front to back like a
/// tar inside a compressor: the end marker is required, the trailer drain
/// is capped (S6), and every read walks the headers again and refuses a
/// stream that differs from the listing.
#[derive(Clone, Debug)]
pub(crate) struct StreamListing {
    pub(crate) entries: Vec<Entry>,
    spans: Vec<Span>,
    pub(crate) complete: bool,
}

/// Lists the tar `src` yields, every byte charged to `budget`. A spent
/// byte budget ends the listing early (incomplete) once one entry is known;
/// a complete one is read through the trailer to the stream's end.
pub(crate) fn list_stream(
    src: impl Read,
    limits: &Limits,
    budget: &mut Budget,
) -> Result<StreamListing, ArchiveError> {
    let trailer = Trailer {
        allowance: limits.tar_trailer_allowance,
        limited: budget.remaining().is_some(),
    };
    let mut w = Walker::new(Decoded::new(ChargedRead::new(src, budget)), limits, 0);
    let (raws, complete) = collect(&mut w, limits, Some(trailer))?;
    let (mut entries, spans): (Vec<Entry>, Vec<Span>) = raws
        .into_iter()
        .enumerate()
        .map(|(index, raw)| raw.into_entry(index, &Method::Stored, true))
        .unzip();
    mark_link_prefixes(&mut entries);
    Ok(StreamListing {
        entries,
        spans,
        complete,
    })
}

/// Entry `index` of `listing` from a fresh stream `src`, verified by size.
/// After the entry's last byte the reader walks on to the end of the
/// stream (the other entries, the end marker, the capped trailer) before
/// it reports EOF, so a source that checks itself at its end (a tool's
/// exit status) has done so when the entry reads as whole.
pub(crate) fn open_stream_entry<'a, R: Read + 'a>(
    src: R,
    listing: &'a StreamListing,
    index: usize,
    limits: &Limits,
    budget: &'a mut Budget,
) -> Result<Box<dyn Read + 'a>, ArchiveError> {
    let entry = listing.entries.get(index).ok_or_else(|| no_entry(index))?;
    readable(entry)?;
    let trailer = Trailer {
        allowance: limits.tar_trailer_allowance,
        limited: budget.remaining().is_some(),
    };
    let mut w = Walker::new(Decoded::new(ChargedRead::new(src, budget)), limits, 0);
    for (listed, &span) in listing.entries.iter().zip(&listing.spans).take(index + 1) {
        w.next()?.ok_or_else(changed)?.matches(listed, span)?;
    }
    let to_end = ToEnd {
        w,
        left: entry.size,
        listing,
        next: index + 1,
        trailer,
        done: false,
    };
    Ok(Box::new(VerifyingReader::new(to_end, entry.size, None)))
}

/// One pass over a fresh stream `src` handing the `wanted` entries of
/// `listing` to `sink`, then through the end marker and the trailer.
pub(crate) fn stream_stream_entries(
    src: impl Read,
    listing: &StreamListing,
    wanted: &[usize],
    limits: &Limits,
    budget: &mut Budget,
    sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
) -> Result<(), ArchiveError> {
    let mut order = wanted.to_vec();
    order.sort_unstable();
    order.dedup();
    for &index in &order {
        readable(listing.entries.get(index).ok_or_else(|| no_entry(index))?)?;
    }
    if order.is_empty() {
        return Ok(());
    }
    let trailer = Trailer {
        allowance: limits.tar_trailer_allowance,
        limited: budget.remaining().is_some(),
    };
    let mut w = Walker::new(Decoded::new(ChargedRead::new(src, budget)), limits, 0);
    let mut want = order.into_iter().peekable();
    let mut index = 0usize;
    while let Some(raw) = w.next()? {
        if let (Some(listed), Some(&span)) = (listing.entries.get(index), listing.spans.get(index))
        {
            raw.matches(listed, span)?;
            if want.peek() == Some(&index) {
                want.next();
                let mut data = VerifyingReader::new((&mut w).take(listed.size), listed.size, None);
                sink(listed, &mut data)?;
                drain(&mut data)?;
            }
        }
        index += 1;
    }
    if index < listing.entries.len() || (listing.complete && index > listing.entries.len()) {
        return Err(changed());
    }
    drain_trailer(&mut w, trailer)
}

/// The rest of one entry, then the rest of the stream (see
/// [`open_stream_entry`]).
struct ToEnd<'a, S> {
    w: Walker<S>,
    left: u64,
    listing: &'a StreamListing,
    /// The listing index of the next header.
    next: usize,
    trailer: Trailer,
    done: bool,
}

impl<S: TarSource> ToEnd<'_, S> {
    fn finish(&mut self) -> Result<(), ArchiveError> {
        while let Some(raw) = self.w.next()? {
            let at = self.next;
            if let (Some(listed), Some(&span)) =
                (self.listing.entries.get(at), self.listing.spans.get(at))
            {
                raw.matches(listed, span)?;
            }
            self.next += 1;
        }
        let n = self.listing.entries.len();
        if self.next < n || (self.listing.complete && self.next > n) {
            return Err(changed());
        }
        drain_trailer(&mut self.w, self.trailer)
    }
}

impl<S: TarSource> Read for ToEnd<'_, S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left > 0 {
            let want = usize::try_from(self.left).map_or(buf.len(), |l| l.min(buf.len()));
            let n = self.w.read(&mut buf[..want])?;
            self.left -= n as u64;
            return Ok(n);
        }
        if !self.done {
            self.finish()?;
            self.done = true;
        }
        Ok(0)
    }
}

/// How the bytes after the end marker are drained (S6).
#[derive(Clone, Copy, Debug)]
struct Trailer {
    allowance: u64,
    /// A byte-limited budget (plan): a long trailer is `BudgetSpent`.
    limited: bool,
}

/// Walks every entry (the listing). A spent budget ends the listing early
/// (incomplete) once at least one entry is known; otherwise it is the
/// error. Inside a compressor the trailer is drained after the end marker.
fn collect<S: TarSource>(
    w: &mut Walker<S>,
    limits: &Limits,
    trailer: Option<Trailer>,
) -> Result<(Vec<Raw>, bool), ArchiveError> {
    let mut raws = Vec::new();
    let mut total = 0u64;
    loop {
        match w.next() {
            Ok(Some(raw)) => {
                if raw.size > limits.max_entry_size {
                    return Err(ArchiveError::TooLarge { declared: raw.size });
                }
                if raw.kind == EntryKind::File {
                    total = total.saturating_add(raw.size);
                    if total > limits.max_total_size {
                        return Err(ArchiveError::TooLarge { declared: total });
                    }
                }
                raws.push(raw);
            }
            Ok(None) => break,
            Err(ArchiveError::BudgetSpent { what }) => {
                if raws.is_empty() {
                    return Err(ArchiveError::BudgetSpent { what });
                }
                return Ok((raws, false));
            }
            Err(e) => return Err(e),
        }
    }
    if raws.is_empty() {
        return Err(damaged("tar without entries"));
    }
    if let Some(trailer) = trailer {
        drain_trailer(w, trailer)?;
    }
    Ok((raws, true))
}

/// Reads what follows the end marker, at most `allowance` bytes plus one
/// to tell a longer trailer from EOF; the decoder's own end-of-stream
/// checks run as EOF is reached.
fn drain_trailer(r: &mut impl Read, trailer: Trailer) -> Result<(), ArchiveError> {
    let mut buf = vec![0u8; 64 * KIB as usize];
    let mut left = trailer.allowance;
    loop {
        let want = usize::try_from(left)
            .map_or(buf.len(), |l| l.min(buf.len()))
            .max(1);
        let n = read_some(r, &mut buf[..want])?;
        if n == 0 {
            return Ok(());
        }
        if left == 0 {
            return Err(if trailer.limited {
                ArchiveError::BudgetSpent {
                    what: BudgetKind::DecodedBytes,
                }
            } else {
                damaged(TRAILING_DATA)
            });
        }
        left -= n as u64;
    }
}

/// One entry as the walk found it.
#[derive(Debug)]
struct Raw {
    raw_name: Vec<u8>,
    kind: EntryKind,
    size: u64,
    mtime: Option<SystemTime>,
    span: Span,
}

impl Raw {
    fn into_entry(self, index: usize, method: &Method, compressed: bool) -> (Entry, Span) {
        let parsed = std::str::from_utf8(&self.raw_name)
            .map_err(|_| UnsafeWhy::Encoding)
            .and_then(RelPath::parse);
        let (name, unsafe_name) = match parsed {
            Ok(name) => (name, None),
            Err(why) => (RelPath::placeholder(index), Some(why)),
        };
        let entry = Entry {
            index,
            name,
            raw_name: self.raw_name,
            kind: self.kind,
            size: self.size,
            packed: (!compressed).then_some(self.size),
            crc32: None,
            encrypted: false,
            method: method.clone(),
            // One stream: every entry of a compressed tar is decoded in order.
            solid_block: compressed.then_some(0),
            mtime: self.mtime,
            unsafe_name,
        };
        (entry, self.span)
    }

    /// The walk found what the listing holds, at the same place.
    fn matches(&self, entry: &Entry, span: Span) -> Result<(), ArchiveError> {
        if self.raw_name == entry.raw_name
            && self.kind == entry.kind
            && self.size == entry.size
            && self.span == span
        {
            Ok(())
        } else {
            Err(changed())
        }
    }
}

/// What the metadata records before one header say about it.
#[derive(Debug, Default)]
struct Meta {
    pax_seen: bool,
    path: Option<Vec<u8>>,
    size: Option<u64>,
    mtime: Option<u64>,
    sparse: bool,
    long_name: Option<Vec<u8>>,
    long_link_seen: bool,
}

impl Meta {
    fn pax(&mut self, data: &[u8]) -> Result<(), ArchiveError> {
        if self.pax_seen {
            return Err(damaged("tar entry with two pax headers"));
        }
        self.pax_seen = true;
        for (key, value) in parse_pax(data)? {
            match key {
                b"path" => self.path = Some(value.to_vec()),
                b"size" => self.size = Some(decimal(value).ok_or_else(|| damaged("tar pax size"))?),
                b"mtime" => self.mtime = pax_seconds(value),
                // `linkpath` names a link target: links are never followed.
                _ if key.starts_with(b"GNU.sparse.") => self.sparse = true,
                _ => {}
            }
        }
        Ok(())
    }

    fn long_name(&mut self, mut data: Vec<u8>) -> Result<(), ArchiveError> {
        if self.long_name.is_some() {
            return Err(damaged("tar entry with two GNU long names"));
        }
        while data.last() == Some(&0) {
            data.pop();
        }
        self.long_name = Some(data);
        Ok(())
    }

    fn long_link(&mut self) -> Result<(), ArchiveError> {
        if self.long_link_seen {
            return Err(damaged("tar entry with two GNU long links"));
        }
        self.long_link_seen = true;
        Ok(())
    }
}

/// A pax global header may not change names, sizes or sparse maps of every
/// later entry: other readers would see another archive.
fn check_global(data: &[u8]) -> Result<(), ArchiveError> {
    for (key, _) in parse_pax(data)? {
        if matches!(key, b"path" | b"linkpath" | b"size") || key.starts_with(b"GNU.sparse.") {
            return Err(damaged("tar pax global override"));
        }
    }
    Ok(())
}

/// One pax record: key and value bytes.
type PaxRecord<'d> = (&'d [u8], &'d [u8]);

/// `length key=value\n` records; the length is decimal and counts the
/// whole record. Any malformed record is `Damaged`.
fn parse_pax(data: &[u8]) -> Result<Vec<PaxRecord<'_>>, ArchiveError> {
    let bad = || damaged("tar pax record");
    let mut out = Vec::new();
    let mut rest = data;
    while !rest.is_empty() {
        // u64::MAX has 20 digits.
        let space = rest
            .iter()
            .take(21)
            .position(|&b| b == b' ')
            .ok_or_else(bad)?;
        let len = decimal(&rest[..space]).ok_or_else(bad)?;
        let len = usize::try_from(len).map_err(|_| bad())?;
        if len <= space + 1 || len > rest.len() {
            return Err(bad());
        }
        let (record, tail) = rest.split_at(len);
        let body = record[space + 1..].strip_suffix(b"\n").ok_or_else(bad)?;
        let eq = body.iter().position(|&b| b == b'=').ok_or_else(bad)?;
        out.push((&body[..eq], &body[eq + 1..]));
        rest = tail;
    }
    Ok(out)
}

/// ASCII decimal digits with checked arithmetic; `None` when empty, not
/// digits, or over `u64::MAX`.
fn decimal(digits: &[u8]) -> Option<u64> {
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0u64, |acc, &d| {
        if !d.is_ascii_digit() {
            return None;
        }
        acc.checked_mul(10)?.checked_add(u64::from(d - b'0'))
    })
}

/// The whole seconds of a pax time (`1700000000.123`); `None` for a
/// negative or malformed one (the header mtime is used instead).
fn pax_seconds(value: &[u8]) -> Option<u64> {
    let whole = value.split(|&b| b == b'.').next()?;
    decimal(whole)
}

fn seconds_to_time(secs: u64) -> Option<SystemTime> {
    if secs == 0 {
        return None;
    }
    UNIX_EPOCH.checked_add(Duration::from_secs(secs))
}

fn kind_of(typeflag: u8, name: &[u8], sparse: bool) -> EntryKind {
    if sparse {
        return EntryKind::Other;
    }
    match typeflag {
        b'0' | 0 | b'7' if name.ends_with(b"/") => EntryKind::Dir,
        b'0' | 0 | b'7' => EntryKind::File,
        b'5' => EntryKind::Dir,
        b'1' => EntryKind::Hardlink,
        b'2' => EntryKind::Symlink,
        // Devices, FIFOs, sparse (`S`), multi-volume (`M`), vendor types.
        _ => EntryKind::Other,
    }
}

/// The stored checksum equals the unsigned sum of the header bytes (with
/// the checksum field as spaces), or the signed sum some old writers used.
fn check_checksum(block: &[u8; BLOCK as usize]) -> Result<(), ArchiveError> {
    let bad = || damaged("tar header checksum");
    let stored = ::tar::Header::from_byte_slice(block)
        .cksum()
        .map_err(|_| bad())?;
    let mut unsigned = 0u32;
    let mut signed = 0i64;
    for (i, &b) in block.iter().enumerate() {
        let b = if (148..156).contains(&i) { b' ' } else { b };
        unsigned += u32::from(b);
        signed += i64::from(b as i8);
    }
    if stored == unsigned || i64::from(stored) == signed {
        Ok(())
    } else {
        Err(bad())
    }
}

fn is_gnu(block: &[u8; BLOCK as usize]) -> bool {
    &block[257..265] == b"ustar  \0"
}

/// A stream of tar blocks: a plain file (seeks over data) or a decoder
/// (decodes and drops it, charged).
trait TarSource: Read {
    fn skip(&mut self, n: u64) -> Result<(), ArchiveError>;
    /// Whether the stream must end with the end-of-archive marker.
    fn needs_marker(&self) -> bool;
}

/// A plain tar file of `len` bytes.
struct Plain<'r, R> {
    r: &'r mut R,
    pos: u64,
    len: u64,
}

impl<'r, R: Read + Seek> Plain<'r, R> {
    fn at(r: &'r mut R, start: u64, len: u64) -> Result<Self, ArchiveError> {
        r.seek(SeekFrom::Start(start)).map_err(ArchiveError::Io)?;
        Ok(Self { r, pos: start, len })
    }
}

impl<R: Read> Read for Plain<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.r.read(buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek> TarSource for Plain<'_, R> {
    fn skip(&mut self, n: u64) -> Result<(), ArchiveError> {
        let to = self
            .pos
            .checked_add(n)
            .filter(|&to| to <= self.len)
            .ok_or_else(|| damaged("tar truncated"))?;
        self.r.seek(SeekFrom::Start(to)).map_err(ArchiveError::Io)?;
        self.pos = to;
        Ok(())
    }

    fn needs_marker(&self) -> bool {
        false
    }
}

/// The decoded stream of a compressor, every byte charged to the budget.
struct Decoded<'b, D> {
    inner: ChargedRead<'b, D>,
}

impl<'b, D: Read> Decoded<'b, D> {
    fn new(inner: ChargedRead<'b, D>) -> Self {
        Self { inner }
    }
}

impl<D: Read> Read for Decoded<'_, D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<D: Read> TarSource for Decoded<'_, D> {
    fn skip(&mut self, n: u64) -> Result<(), ArchiveError> {
        let copied = io::copy(&mut (&mut self.inner).take(n), &mut io::sink())
            .map_err(ArchiveError::from)?;
        if copied != n {
            return Err(damaged("tar truncated"));
        }
        Ok(())
    }

    fn needs_marker(&self) -> bool {
        true
    }
}

/// The block walk (module docs). Reading it reads entry data and keeps the
/// position, so the next header is found after a partial read too.
struct Walker<S> {
    src: S,
    /// Offset in the tar stream of the next byte `src` yields.
    pos: u64,
    /// Offset of the next header (after the current entry's data).
    next: u64,
    entries: u64,
    records: u64,
    max_entries: u64,
    ended: bool,
}

impl<S: TarSource> Walker<S> {
    fn new(src: S, limits: &Limits, start: u64) -> Self {
        Self {
            src,
            pos: start,
            next: start,
            entries: 0,
            records: 0,
            max_entries: limits.max_entries,
            ended: false,
        }
    }

    /// One block; `None` at EOF on a block boundary.
    fn block(&mut self) -> Result<Option<[u8; BLOCK as usize]>, ArchiveError> {
        let mut b = [0u8; BLOCK as usize];
        let n = read_full(&mut self.src, &mut b)?;
        self.pos += n as u64;
        match n {
            0 => Ok(None),
            512 => Ok(Some(b)),
            _ => Err(damaged("tar truncated")),
        }
    }

    fn skip_to(&mut self, at: u64) -> Result<(), ArchiveError> {
        let n = at
            .checked_sub(self.pos)
            .ok_or_else(|| damaged("tar offset"))?;
        self.src.skip(n)?;
        self.pos = at;
        Ok(())
    }

    /// The data of a metadata record of `size` bytes (S6b cap), padding
    /// skipped.
    fn record(&mut self, size: u64) -> Result<Vec<u8>, ArchiveError> {
        if size > MAX_RECORD {
            return Err(damaged("tar metadata record too large"));
        }
        let mut data = vec![0u8; usize::try_from(size).map_err(|_| damaged("tar record"))?];
        let n = read_full(&mut self.src, &mut data)?;
        self.pos += n as u64;
        if n != data.len() {
            return Err(damaged("tar truncated"));
        }
        let end = round_up(self.pos)?;
        self.skip_to(end)?;
        Ok(data)
    }

    fn count_record(&mut self, per_entry: &mut u32) -> Result<(), ArchiveError> {
        *per_entry += 1;
        if *per_entry > MAX_RECORDS_PER_ENTRY {
            return Err(damaged("tar entry with too many metadata records"));
        }
        self.count_archive_record()
    }

    fn count_archive_record(&mut self) -> Result<(), ArchiveError> {
        self.records += 1;
        if self.records > self.max_entries {
            return Err(ArchiveError::TooManyEntries {
                count: self.records,
            });
        }
        Ok(())
    }

    /// The next entry, its metadata records applied; `None` at the end.
    fn next(&mut self) -> Result<Option<Raw>, ArchiveError> {
        if self.ended {
            return Ok(None);
        }
        self.skip_to(self.next)?;
        let header_at = self.pos;
        let mut meta = Meta::default();
        let mut records = 0u32;
        loop {
            let Some(block) = self.block()? else {
                if records > 0 {
                    return Err(damaged("tar metadata without an entry"));
                }
                if self.src.needs_marker() {
                    return Err(damaged("tar end marker missing"));
                }
                self.ended = true;
                return Ok(None);
            };
            if block.iter().all(|&b| b == 0) {
                if records > 0 {
                    return Err(damaged("tar metadata without an entry"));
                }
                match self.block()? {
                    Some(second) if second.iter().any(|&b| b != 0) => {
                        return Err(damaged("tar end marker"));
                    }
                    // A lone zero block at EOF is accepted.
                    _ => {}
                }
                self.ended = true;
                return Ok(None);
            }
            check_checksum(&block)?;
            let header = ::tar::Header::from_byte_slice(&block);
            let size = header
                .entry_size()
                .map_err(|_| damaged("tar header size"))?;
            let typeflag = block[156];
            if matches!(typeflag, b'x' | b'g' | b'L' | b'K') {
                self.count_record(&mut records)?;
                let data = self.record(size)?;
                match typeflag {
                    b'x' => meta.pax(&data)?,
                    b'g' => check_global(&data)?,
                    b'L' => meta.long_name(data)?,
                    _ => meta.long_link()?,
                }
                continue;
            }

            self.entries += 1;
            if self.entries > self.max_entries {
                return Err(ArchiveError::TooManyEntries {
                    count: self.entries,
                });
            }
            // An old GNU sparse header is followed by extension blocks
            // while their `isextended` byte is set.
            if typeflag == b'S' && is_gnu(&block) && block[482] != 0 {
                loop {
                    self.count_archive_record()?;
                    let ext = self.block()?.ok_or_else(|| damaged("tar truncated"))?;
                    if ext[504] == 0 {
                        break;
                    }
                }
            }
            let size = meta.size.unwrap_or(size);
            let data_at = self.pos;
            self.next = data_at
                .checked_add(size)
                .ok_or_else(|| damaged("tar header size"))
                .and_then(round_up)?;
            let raw_name = meta
                .path
                .take()
                .or_else(|| meta.long_name.take())
                .unwrap_or_else(|| header.path_bytes().into_owned());
            let kind = kind_of(typeflag, &raw_name, meta.sparse);
            let mtime = meta
                .mtime
                .or_else(|| header.mtime().ok())
                .and_then(seconds_to_time);
            return Ok(Some(Raw {
                raw_name,
                kind,
                size,
                mtime,
                span: Span { header_at, data_at },
            }));
        }
    }
}

impl<S: TarSource> Read for Walker<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.src.read(buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

/// Entries whose path goes through a symlink entry of the archive
/// (compared by [`name_key`], so `D/x` too on a case-insensitive volume)
/// get `unsafe_name: Link`. Links are never extracted; this keeps a file
/// from being written through one a previous tool created.
fn mark_link_prefixes(entries: &mut [Entry]) {
    let links: HashSet<String> = entries
        .iter()
        .filter(|e| e.kind == EntryKind::Symlink && e.unsafe_name.is_none())
        .map(|e| name_key(e.name.as_str()))
        .collect();
    if links.is_empty() {
        return;
    }
    for e in entries.iter_mut().filter(|e| e.unsafe_name.is_none()) {
        let key = name_key(e.name.as_str());
        let through_link = key
            .match_indices('/')
            .any(|(at, _)| links.contains(&key[..at]));
        if through_link {
            e.unsafe_name = Some(UnsafeWhy::Link);
        }
    }
}

/// Only safe files are read.
fn readable(entry: &Entry) -> Result<(), ArchiveError> {
    match entry.kind {
        EntryKind::File => {}
        EntryKind::Symlink | EntryKind::Hardlink => {
            return Err(ArchiveError::UnsafeEntry {
                name: entry.name.to_string(),
                why: UnsafeWhy::Link,
            })
        }
        EntryKind::Dir | EntryKind::Other => {
            return Err(ArchiveError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("tar entry {} is not a file", entry.index),
            )))
        }
    }
    if let Some(why) = entry.unsafe_name {
        return Err(ArchiveError::UnsafeEntry {
            name: String::from_utf8_lossy(&entry.raw_name).into_owned(),
            why,
        });
    }
    Ok(())
}

fn round_up(at: u64) -> Result<u64, ArchiveError> {
    at.checked_next_multiple_of(BLOCK)
        .ok_or_else(|| damaged("tar header size"))
}

fn no_entry(index: usize) -> ArchiveError {
    ArchiveError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("no tar entry {index}"),
    ))
}

fn changed() -> ArchiveError {
    damaged("tar changed since it was listed")
}

fn damaged(detail: impl Into<String>) -> ArchiveError {
    ArchiveError::Damaged {
        detail: detail.into(),
    }
}

/// Reads the rest of an entry (checked by its verifier).
fn drain(r: &mut dyn Read) -> Result<(), ArchiveError> {
    io::copy(r, &mut io::sink()).map_err(ArchiveError::from)?;
    Ok(())
}

/// One read, retrying `Interrupted`.
fn read_some(r: &mut impl Read, buf: &mut [u8]) -> Result<usize, ArchiveError> {
    loop {
        match r.read(buf) {
            Ok(n) => return Ok(n),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(ArchiveError::from(e)),
        }
    }
}

/// Fills `buf` or reports how much there was before EOF.
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> Result<usize, ArchiveError> {
    let mut filled = 0;
    while filled < buf.len() {
        match read_some(r, &mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_is_checked() {
        assert_eq!(decimal(b"0"), Some(0));
        assert_eq!(decimal(b"18446744073709551615"), Some(u64::MAX));
        assert_eq!(decimal(b"18446744073709551616"), None);
        assert_eq!(decimal(b""), None);
        assert_eq!(decimal(b"1a"), None);
        assert_eq!(decimal(b"-1"), None);
    }

    #[test]
    fn pax_records_parse() {
        let got = parse_pax(b"12 path=a.b\n13 size=1500\n").unwrap();
        assert_eq!(
            got,
            vec![(&b"path"[..], &b"a.b"[..]), (&b"size"[..], &b"1500"[..])]
        );
        assert!(parse_pax(b"5 a=b\n").is_err(), "length too short");
        assert!(parse_pax(b" 6 a=b\n").is_err(), "no digits");
    }

    #[test]
    fn huge_mtime_is_no_time() {
        assert_eq!(seconds_to_time(0), None);
        // Must not panic on overflow.
        let _ = seconds_to_time(u64::MAX);
        assert_eq!(pax_seconds(b"1700000000.25"), Some(1_700_000_000));
        assert_eq!(pax_seconds(b"-5"), None);
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of(b'0', b"a", false), EntryKind::File);
        assert_eq!(kind_of(0, b"dir/", false), EntryKind::Dir);
        assert_eq!(kind_of(b'7', b"a", false), EntryKind::File);
        assert_eq!(kind_of(b'0', b"a", true), EntryKind::Other);
        assert_eq!(kind_of(b'S', b"a", false), EntryKind::Other);
        assert_eq!(kind_of(b'M', b"a", false), EntryKind::Other);
        assert_eq!(kind_of(b'V', b"a", false), EntryKind::Other);
    }
}
