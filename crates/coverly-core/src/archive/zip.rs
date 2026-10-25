//! Zip and zip64 backend of the archive reader (spec
//! `2026-10-23-organize-archives-and-discs.md` §4.3, §5.1, S1, S2, S3).
//!
//! **Listing** (the integrity listing of §4.3) is, in order:
//! 1. [`central_directory_claim`] reads what the end records claim, from
//!    the last 64 KiB and the zip64 record only. More than `max_entries`
//!    is [`ArchiveError::TooManyEntries`] and a central directory over
//!    `max_header` is [`ArchiveError::TooLarge`], before anything is
//!    allocated from those counts (never `Damaged`). An end record whose
//!    comment does not end at EOF, or a second one that does, is `Damaged`.
//! 2. The central directory bytes are parsed here as well (methods, flags,
//!    attributes, the Unicode Path and AES extra fields), then
//!    `zip::ZipArchive::new` and `by_index_raw` run for every entry. The
//!    two views must agree (count, the prefix, and every name, method,
//!    packed size, local header and record offset) or the zip is `Damaged`:
//!    nothing is trusted that only one parser saw. Two entries with the
//!    same name are `Damaged` too (each tool would pick another one).
//! 3. Methods 14 (LZMA), 95 (XZ), 98 (PPMd) and 93 (zstd) are never decoded
//!    by the `zip` crate (its features are off, `scripts/check-zip-features.sh`).
//!    Their property header is read from the raw stream and the decoder
//!    memory it implies is checked against `decoder_mem_cap` (S2):
//!    LZMA 2 + 2 + 5 bytes, PPMd8 2 bytes, XZ stream header + first block
//!    header, zstd first frame header. A file entry over the cap makes the
//!    listing fail with [`ArchiveError::NeedsTooMuchMemory`].
//!
//! **Open** re-parses the header from the bytes actually fed to the
//! decoder (the file may have changed since the listing), checks it again,
//! takes the memory from the [`MemoryGate`] and builds the decoder through
//! [`super::lzma::open_decoder`], `ppmd_rust` or [`ZstdFrames`]; XZ goes
//! through [`XzWalker`], which checks every block. Methods 0/8/9/12 are
//! decoded by the crate. Every stream is
//! `GuardedRead<VerifyingReader<ChargedRead<…>>>`, and the whole listing
//! runs inside the panic guard (S1).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ::zip::result::ZipError;
use ::zip::{ZipArchive, ZipReadOptions};
use ppmd_rust::{Ppmd8Decoder, RestoreMethod};

use super::budget::{Budget, ChargedRead, MemoryGate, Permit};
use super::guard::{guarded, GuardedRead, Poison};
use super::lzma::{self, open_decoder, read_xz_first_dict, LzmaKind, LzmaParams, XzWalker};
use super::names::{decode_zip_name, RelPath, UnsafeWhy};
use super::verify::VerifyingReader;
use super::zstd::{read_zstd_first_window, ZstdFrames};
use super::{ArchiveError, ArchiveReader, Entry, EntryKind, Format, Limits, Method};

/// What a zip's end-of-central-directory record claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CentralDirectoryClaim {
    /// Total number of entries (directories included).
    pub entries: u64,
    /// Size of the central directory in bytes.
    pub size: u64,
    /// Offset where the central directory ends: the zip64 end record, or
    /// the end record when there is none. The directory starts `size`
    /// bytes before it, also in a zip with a prefix (a self-extractor).
    pub cd_end: u64,
    /// Offset of the end record that was read (the last one in the file).
    pub eocd_at: u64,
    /// Offset of the central directory the end record declares, counted
    /// from the start of the zip: `size` bytes before `cd_end` minus this
    /// is the length of the prefix.
    pub cd_offset: u64,
}

const EOCD_SIG: [u8; 4] = *b"PK\x05\x06";
const EOCD_LEN: usize = 22;
const ZIP64_LOCATOR_SIG: [u8; 4] = *b"PK\x06\x07";
const ZIP64_LOCATOR_LEN: u64 = 20;
const ZIP64_EOCD_SIG: [u8; 4] = *b"PK\x06\x06";
/// End record plus the longest comment.
const EOCD_SEARCH: u64 = EOCD_LEN as u64 + u16::MAX as u64;
const CDH_SIG: [u8; 4] = *b"PK\x01\x02";
const CDH_LEN: usize = 46;

/// Zip method numbers this backend knows.
const M_STORED: u16 = 0;
const M_DEFLATE: u16 = 8;
const M_DEFLATE64: u16 = 9;
const M_BZIP2: u16 = 12;
const M_LZMA: u16 = 14;
const M_ZSTD: u16 = 93;
const M_XZ: u16 = 95;
const M_PPMD: u16 = 98;
const M_AES: u16 = 99;

const EXTRA_ZIP64: u16 = 0x0001;
const EXTRA_UNICODE_PATH: u16 = 0x7075;
const EXTRA_AES: u16 = 0x9901;

/// Reads the counts a zip's end record (zip64 included) claims, from at
/// most the last 64 KiB of `src` plus the zip64 record. `None` when no
/// end record is found or it cannot be read (the zip reader decides).
///
/// The end record is the last one, and its comment must end exactly at
/// EOF: bytes after it, or a second end record whose comment also ends
/// there (one hidden in the other's comment), are where another parser
/// finds another directory, so both are `None` too.
pub fn central_directory_claim<R: Read + Seek>(
    src: &mut R,
    len: u64,
) -> Option<CentralDirectoryClaim> {
    let tail_len = len.min(EOCD_SEARCH);
    let tail_start = len - tail_len;
    let mut tail = vec![0u8; usize::try_from(tail_len).ok()?];
    src.seek(SeekFrom::Start(tail_start)).ok()?;
    src.read_exact(&mut tail).ok()?;
    let reaches_eof = |i: usize| {
        tail[i..i + 4] == EOCD_SIG && i + EOCD_LEN + u16_at(&tail[i..], 20) as usize == tail.len()
    };
    let mut candidates = (0..=tail.len().checked_sub(EOCD_LEN)?).rev();
    let pos = candidates.find(|&i| tail[i..i + 4] == EOCD_SIG)?;
    if !reaches_eof(pos) || candidates.any(reaches_eof) {
        return None;
    }
    let eocd = &tail[pos..pos + EOCD_LEN];
    let eocd_at = tail_start + pos as u64;
    let mut claim = CentralDirectoryClaim {
        entries: u16_at(eocd, 10),
        size: u32_at(eocd, 12),
        cd_end: eocd_at,
        eocd_at,
        cd_offset: u32_at(eocd, 16),
    };
    if claim.entries == u64::from(u16::MAX) || claim.size == u64::from(u32::MAX) {
        // Zip64: the locator sits right before the end record.
        let locator_at = eocd_at.checked_sub(ZIP64_LOCATOR_LEN)?;
        let mut locator = [0u8; ZIP64_LOCATOR_LEN as usize];
        src.seek(SeekFrom::Start(locator_at)).ok()?;
        src.read_exact(&mut locator).ok()?;
        if locator[..4] != ZIP64_LOCATOR_SIG {
            return Some(claim);
        }
        // The zip64 record ends right before the locator (its size field
        // is not trusted for that); with a prefix its stored offset is off
        // by the prefix length, so look for it at the declared offset first
        // and then right before the locator.
        let declared_at = u64_at(&locator, 8);
        let mut record = [0u8; 56];
        let mut found = None;
        for at in [declared_at, locator_at.saturating_sub(56)] {
            if src.seek(SeekFrom::Start(at)).is_ok()
                && src.read_exact(&mut record).is_ok()
                && record[..4] == ZIP64_EOCD_SIG
            {
                found = Some(at);
                break;
            }
        }
        let Some(record_at) = found else {
            return Some(claim);
        };
        claim = CentralDirectoryClaim {
            entries: u64_at(&record, 32),
            size: u64_at(&record, 40),
            cd_end: record_at,
            eocd_at,
            cd_offset: u64_at(&record, 48),
        };
    }
    Some(claim)
}

fn u16_at(b: &[u8], o: usize) -> u64 {
    u64::from(u16::from_le_bytes([b[o], b[o + 1]]))
}

fn u32_at(b: &[u8], o: usize) -> u64 {
    u64::from(u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]))
}

fn u64_at(b: &[u8], o: usize) -> u64 {
    let mut v = [0u8; 8];
    v.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(v)
}

/// The archive file, remembering whether a read or seek of it failed, so
/// a decoder error can be told apart from an I/O error of the file.
///
/// While `hide_below` is non-zero, the bytes before that offset read as
/// zeros without touching the file. `ZipArchive::new` runs that way: when
/// the end record the precheck vouched for fails the crate's own checks,
/// the crate searches the whole file backwards for another one, which no
/// cap was checked against (its entry count sizes an allocation). Hidden,
/// there is none to find, and the scan never reads the file.
struct Source<R> {
    inner: R,
    failed: Arc<AtomicBool>,
    hide_below: Arc<AtomicU64>,
    /// The position the reader shows; `inner` is there unless `stale`.
    pos: u64,
    stale: bool,
}

impl<R> Source<R> {
    fn note<T>(&self, r: io::Result<T>) -> io::Result<T> {
        if let Err(e) = &r {
            if e.kind() != io::ErrorKind::Interrupted {
                self.failed.store(true, Ordering::Relaxed);
            }
        }
        r
    }
}

impl<R: Read + Seek> Read for Source<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let hide = self.hide_below.load(Ordering::Relaxed);
        if self.pos < hide {
            let n = usize::try_from(hide - self.pos).map_or(buf.len(), |h| h.min(buf.len()));
            buf[..n].fill(0);
            self.pos += n as u64;
            self.stale = true;
            return Ok(n);
        }
        if self.stale {
            let r = self.inner.seek(SeekFrom::Start(self.pos));
            self.note(r)?;
            self.stale = false;
        }
        let r = self.inner.read(buf);
        if let Ok(n) = r {
            self.pos += n as u64;
        }
        self.note(r)
    }
}

impl<R: Seek> Seek for Source<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let pos = match pos {
            SeekFrom::Current(d) if self.stale => {
                SeekFrom::Start(self.pos.checked_add_signed(d).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "seek before the start")
                })?)
            }
            other => other,
        };
        let r = self.inner.seek(pos);
        if let Ok(at) = r {
            self.pos = at;
            self.stale = false;
        }
        self.note(r)
    }
}

/// What the central directory says about one entry, beyond what the
/// `zip` crate exposes.
#[derive(Clone, Debug)]
struct RawRecord {
    name: Vec<u8>,
    /// The method that decodes the data (for AES, the one inside).
    method: u16,
    /// The method field as stored (99 for AES).
    stored_method: u16,
    /// Compressed size and local header offset (zip64 extra applied), the
    /// offset counted from the start of the zip, before any prefix.
    packed: u64,
    local_at: u64,
    /// Where the record starts in the central directory.
    header_at: usize,
    flags: u16,
    made_by: u16,
    external_attr: u32,
    unicode_name: Option<(u32, Vec<u8>)>,
    aes: bool,
}

/// Parses `count` central directory records from `cd`. Lengths are checked
/// against the buffer; a record that does not fit is `Damaged`.
fn parse_central_directory(cd: &[u8], count: usize) -> Result<Vec<RawRecord>, ArchiveError> {
    // `count` is bounded by `max_entries` and by the crate's own parse.
    let mut out = Vec::with_capacity(count);
    let mut pos = 0usize;
    for _ in 0..count {
        let fixed = cd
            .get(pos..pos.checked_add(CDH_LEN).ok_or_else(cd_damaged)?)
            .ok_or_else(cd_damaged)?;
        if fixed[..4] != CDH_SIG {
            return Err(cd_damaged());
        }
        let field = |o: usize| u16::from_le_bytes([fixed[o], fixed[o + 1]]);
        let made_by = field(4);
        let flags = field(8);
        let mut method = field(10);
        let stored_method = method;
        let wide =
            |o: usize| u32::from_le_bytes([fixed[o], fixed[o + 1], fixed[o + 2], fixed[o + 3]]);
        let (size32, packed32, local32) = (wide(24), wide(20), wide(42));
        let mut packed = u64::from(packed32);
        let mut local_at = u64::from(local32);
        let name_len = usize::from(field(28));
        let extra_len = usize::from(field(30));
        let comment_len = usize::from(field(32));
        let external_attr = wide(38);
        let name_at = pos + CDH_LEN;
        let extra_at = name_at + name_len;
        let end = extra_at + extra_len + comment_len;
        if end > cd.len() {
            return Err(cd_damaged());
        }
        let name = cd[name_at..extra_at].to_vec();
        let mut unicode_name = None;
        let mut aes = false;
        for (id, data) in extra_fields(&cd[extra_at..extra_at + extra_len]) {
            match id {
                EXTRA_UNICODE_PATH if data.len() >= 5 && data[0] == 1 => {
                    let crc = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
                    unicode_name = Some((crc, data[5..].to_vec()));
                }
                // APPNOTE 4.5.3: only the fields stored as 0xFFFFFFFF, in
                // this order (the disk number is never read).
                EXTRA_ZIP64 => {
                    let mut slots = data
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|c| u64::from_le_bytes(*c));
                    let mut next = |v32: u32| match v32 {
                        u32::MAX => slots.next().ok_or_else(cd_damaged).map(Some),
                        _ => Ok(None),
                    };
                    next(size32)?;
                    if let Some(v) = next(packed32)? {
                        packed = v;
                    }
                    if let Some(v) = next(local32)? {
                        local_at = v;
                    }
                }
                EXTRA_AES if data.len() >= 7 && method == M_AES => {
                    aes = true;
                    method = u16::from_le_bytes([data[5], data[6]]);
                }
                _ => {}
            }
        }
        out.push(RawRecord {
            name,
            method,
            stored_method,
            packed,
            local_at,
            header_at: pos,
            flags,
            made_by,
            external_attr,
            unicode_name,
            aes,
        });
        pos = end;
    }
    Ok(out)
}

/// The `(id, data)` extra fields of a header. A field whose length runs
/// past the end stops the walk (some writers pad the extra area).
fn extra_fields(extra: &[u8]) -> impl Iterator<Item = (u16, &[u8])> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        let head = extra.get(pos..pos + 4)?;
        let id = u16::from_le_bytes([head[0], head[1]]);
        let len = usize::from(u16::from_le_bytes([head[2], head[3]]));
        let data = extra.get(pos + 4..pos + 4 + len)?;
        pos += 4 + len;
        Some((id, data))
    })
}

fn cd_damaged() -> ArchiveError {
    damaged("zip central directory")
}

fn damaged(detail: impl Into<String>) -> ArchiveError {
    ArchiveError::Damaged {
        detail: detail.into(),
    }
}

/// Maps an error of the `zip` crate: a failed read of the file stays I/O,
/// a missing password is `Encrypted`, everything else the crate rejects is
/// `Damaged` (or `Unsupported` for what it says it does not support).
fn zip_error(e: ZipError, src_failed: &AtomicBool) -> ArchiveError {
    match e {
        ZipError::Io(io_err) => io_to_archive(io_err, src_failed, "zip"),
        ZipError::UnsupportedArchive(msg) if msg == ZipError::PASSWORD_REQUIRED => {
            ArchiveError::Encrypted
        }
        ZipError::UnsupportedArchive(msg) => ArchiveError::Unsupported {
            format: format!("zip: {msg}"),
            need: None,
        },
        ZipError::CompressionMethodNotSupported(method) => ArchiveError::Unsupported {
            format: format!("zip method {method}"),
            need: None,
        },
        other => damaged(format!("zip: {other}")),
    }
}

/// An `io::Error` out of a reader or decoder over the file: one carrying
/// an `ArchiveError` stays itself, one after a failed file read stays I/O,
/// anything else is the data being rejected.
fn io_to_archive(e: io::Error, src_failed: &AtomicBool, what: &str) -> ArchiveError {
    if e.get_ref().is_some_and(|inner| inner.is::<ArchiveError>()) {
        return ArchiveError::from(e);
    }
    if src_failed.load(Ordering::Relaxed) {
        return ArchiveError::Io(e);
    }
    damaged(format!("{what}: {e}"))
}

/// Maps the errors of a decoder (see [`io_to_archive`]).
struct DecodeErrors<D> {
    inner: D,
    src_failed: Arc<AtomicBool>,
    what: &'static str,
}

impl<D: Read> Read for DecodeErrors<D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner
            .read(buf)
            .map_err(|e| io_to_archive(e, &self.src_failed, self.what).into())
    }
}

/// A decoder and the gate permit for its memory. Fields drop in order: the
/// decoder first, then the permit.
struct Holding<D> {
    inner: D,
    _permit: Permit,
}

impl<D: Read> Read for Holding<D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

/// Fills `buf` or reports how much there was before EOF.
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// The parameters of a zip PPMd (method 98) entry, from its 2-byte header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PpmdParams {
    pub order: u8,
    /// Model memory in bytes (1–256 MiB).
    pub mem: u32,
    pub restore: u8,
}

impl PpmdParams {
    /// Parses the header: order − 1 (4 bits), memory in MiB − 1 (8 bits),
    /// restore method (4 bits). Order 1 and restore methods above 1 are
    /// `Damaged`.
    pub fn from_header(header: [u8; 2]) -> Result<Self, ArchiveError> {
        let v = u16::from_le_bytes(header);
        let order = (v & 0x0F) as u8 + 1;
        let mem_mib = u32::from((v >> 4) & 0xFF) + 1;
        let restore = (v >> 12) as u8;
        if order < 2 {
            return Err(damaged("zip ppmd order"));
        }
        if restore > 1 {
            return Err(damaged("zip ppmd restore method"));
        }
        Ok(Self {
            order,
            mem: mem_mib << 20,
            restore,
        })
    }

    /// Decoder memory: the model size the header declares (spec S2).
    pub fn mem_needed(&self) -> u64 {
        u64::from(self.mem)
    }
}

/// Reads and checks the 9-byte LZMA header of a zip method 14 entry.
fn read_lzma_header(raw: &mut impl Read) -> Result<LzmaParams, ArchiveError> {
    let mut head = [0u8; 9];
    if read_full(raw, &mut head)? != head.len() {
        return Err(damaged("zip lzma header truncated"));
    }
    if u16::from_le_bytes([head[2], head[3]]) != 5 {
        return Err(damaged("zip lzma properties size"));
    }
    LzmaParams::from_props([head[4], head[5], head[6], head[7], head[8]])
}

fn read_ppmd_header(raw: &mut impl Read) -> Result<PpmdParams, ArchiveError> {
    let mut head = [0u8; 2];
    if read_full(raw, &mut head)? != head.len() {
        return Err(damaged("zip ppmd header truncated"));
    }
    PpmdParams::from_header(head)
}

/// A zip or zip64 archive (see the module docs).
pub struct ZipReader<R: Read + Seek + Send = File> {
    archive: ZipArchive<Source<R>>,
    entries: Vec<Entry>,
    /// The decoding method of each entry (AES unwrapped), by index.
    methods: Vec<u16>,
    limits: Limits,
    poison: Poison,
    src_failed: Arc<AtomicBool>,
}

impl ZipReader<File> {
    /// Opens and lists the zip at `path`.
    pub fn open(path: &Path, limits: &Limits) -> Result<Self, ArchiveError> {
        let file = File::open(path).map_err(ArchiveError::Io)?;
        Self::new(file, limits)
    }
}

impl<R: Read + Seek + Send> ZipReader<R> {
    /// Lists the zip in `src` (the integrity listing of spec §4.3) inside
    /// the panic guard.
    pub fn new(src: R, limits: &Limits) -> Result<Self, ArchiveError> {
        let failed = Arc::new(AtomicBool::new(false));
        let src = Source {
            inner: src,
            failed: Arc::clone(&failed),
            hide_below: Arc::new(AtomicU64::new(0)),
            pos: 0,
            stale: false,
        };
        let limits = limits.clone();
        guarded(move || Self::list(src, failed, limits))
    }

    fn list(
        mut src: Source<R>,
        failed: Arc<AtomicBool>,
        limits: Limits,
    ) -> Result<Self, ArchiveError> {
        let len = src.seek(SeekFrom::End(0)).map_err(ArchiveError::Io)?;
        let claim = central_directory_claim(&mut src, len)
            .ok_or_else(|| io_or(&failed, "zip end of central directory"))?;
        if claim.entries > limits.max_entries {
            return Err(ArchiveError::TooManyEntries {
                count: claim.entries,
            });
        }
        if claim.size > limits.max_header {
            return Err(ArchiveError::TooLarge {
                declared: claim.size,
            });
        }
        let cd_start = claim
            .cd_end
            .checked_sub(claim.size)
            .ok_or_else(cd_damaged)?;
        // Bounded by `max_header` just above.
        let mut cd = vec![0u8; usize::try_from(claim.size).map_err(|_| cd_damaged())?];
        src.seek(SeekFrom::Start(cd_start))
            .map_err(ArchiveError::Io)?;
        if read_full(&mut src, &mut cd).map_err(ArchiveError::Io)? != cd.len() {
            return Err(cd_damaged());
        }
        no_other_end_records(&mut src, &cd, &claim)?;

        // The crate sees nothing before the checked directory while it
        // parses (see [`Source`]); everything after it, as usual.
        let hide = Arc::clone(&src.hide_below);
        hide.store(cd_start, Ordering::Relaxed);
        let archive = ZipArchive::new(src);
        hide.store(0, Ordering::Relaxed);
        let mut archive = archive.map_err(|e| zip_error(e, &failed))?;
        let count = archive.len();
        if count as u64 > limits.max_entries {
            return Err(ArchiveError::TooManyEntries {
                count: count as u64,
            });
        }
        if count == 0 {
            return Err(damaged("zip without entries"));
        }
        // The crate keys entries by name, so two entries with one name
        // (a classic way to show two tools two different files) leave it
        // fewer entries than the directory claims: ambiguous, so damaged.
        if claim.entries != count as u64 {
            return Err(damaged("zip duplicate or missing directory entries"));
        }
        let records = parse_central_directory(&cd, count)?;
        drop(cd);
        // Both views must also agree on where the zip starts and where
        // each record and its data are (see the loop below).
        let prefix = cd_start
            .checked_sub(claim.cd_offset)
            .ok_or_else(cd_damaged)?;
        if archive.offset() != prefix || archive.central_directory_start() != cd_start {
            return Err(cd_damaged());
        }

        let mut entries = Vec::with_capacity(count);
        let mut methods = Vec::with_capacity(count);
        let mut total = 0u64;
        let mut skip_allowance = limits.max_header;
        for (index, record) in records.into_iter().enumerate() {
            let mut file = archive
                .by_index_raw(index)
                .map_err(|e| zip_error(e, &failed))?;
            #[allow(deprecated)] // the only public way to the method number
            let crate_method = file.compression().to_u16();
            let same_method = crate_method == record.stored_method
                || (record.aes && crate_method == record.method);
            if file.name_raw() != record.name.as_slice()
                || !same_method
                || file.compressed_size() != record.packed
                || prefix.checked_add(record.local_at) != Some(file.header_start())
                || cd_start.checked_add(record.header_at as u64)
                    != Some(file.central_header_start())
            {
                return Err(cd_damaged());
            }
            let size = file.size();
            let packed = file.compressed_size();
            let crc32 = file.crc32();
            let encrypted = file.encrypted() || record.aes;
            let mtime = file.last_modified().and_then(dos_time);
            if size > limits.max_entry_size {
                return Err(ArchiveError::TooLarge { declared: size });
            }
            total = total
                .checked_add(size)
                .filter(|&t| t <= limits.max_total_size)
                .ok_or(ArchiveError::TooLarge {
                    declared: total.saturating_add(size),
                })?;

            let (name, unsafe_name) = entry_name(index, &record);
            let kind = entry_kind(&record);
            let method = if encrypted || kind != EntryKind::File || (size == 0 && packed == 0) {
                list_method(record.method, None)
            } else {
                let parsed = list_params(&mut file, record.method, &limits, &mut skip_allowance)?;
                list_method(record.method, parsed)
            };
            drop(file);
            entries.push(Entry {
                index,
                name,
                raw_name: record.name,
                kind,
                size,
                packed: Some(packed),
                crc32: Some(crc32),
                encrypted,
                method,
                solid_block: None,
                mtime,
                unsafe_name,
            });
            methods.push(record.method);
        }
        let files: Vec<&Entry> = entries
            .iter()
            .filter(|e| e.kind == EntryKind::File)
            .collect();
        if !files.is_empty() && files.iter().all(|e| e.encrypted) {
            return Err(ArchiveError::Encrypted);
        }
        Ok(Self {
            archive,
            entries,
            methods,
            limits,
            poison: Poison::new(),
            src_failed: failed,
        })
    }

    /// The decoded bytes of entry `index`, verified at EOF, before the
    /// guard wrapper.
    fn open_decoded<'a>(
        &'a mut self,
        index: usize,
        budget: &'a mut Budget,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
        let entry = self.entries.get(index).ok_or_else(|| {
            ArchiveError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("no zip entry {index}"),
            ))
        })?;
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
                    format!("zip entry {index} is not a file"),
                )))
            }
        }
        if let Some(why) = entry.unsafe_name {
            return Err(ArchiveError::UnsafeEntry {
                name: String::from_utf8_lossy(&entry.raw_name).into_owned(),
                why,
            });
        }
        if entry.encrypted {
            return Err(ArchiveError::Encrypted);
        }
        let size = entry.size;
        let crc = entry.crc32;
        let empty = size == 0 && entry.packed == Some(0);
        let method = self.methods[index];
        let cap = self.limits.decoder_mem_cap;
        let flag = Arc::clone(&self.src_failed);
        let decoder: Box<dyn Read + 'a> = if empty {
            Box::new(io::empty())
        } else {
            match method {
                M_STORED | M_DEFLATE | M_DEFLATE64 | M_BZIP2 => {
                    let file = self
                        .archive
                        .by_index_with_options(index, ZipReadOptions::new().ignore_crc32(true))
                        .map_err(|e| zip_error(e, &flag))?;
                    Box::new(DecodeErrors {
                        inner: file,
                        src_failed: flag,
                        what: "zip",
                    })
                }
                M_LZMA => {
                    let mut raw = self
                        .archive
                        .by_index_raw(index)
                        .map_err(|e| zip_error(e, &flag))?;
                    // Parsed again from the bytes the decoder gets (S2).
                    let params = read_lzma_header(&mut raw)?;
                    let kind = LzmaKind::Lzma(params);
                    lzma::check(kind.mem_needed(), cap)?;
                    let permit = MemoryGate::global().acquire_for(kind.mem_needed(), budget)?;
                    let inner = open_decoder(kind, Some(size), cap, raw)?;
                    Box::new(Holding {
                        inner,
                        _permit: permit,
                    })
                }
                M_PPMD => {
                    let mut raw = self
                        .archive
                        .by_index_raw(index)
                        .map_err(|e| zip_error(e, &flag))?;
                    let params = read_ppmd_header(&mut raw)?;
                    lzma::check(params.mem_needed(), cap)?;
                    let permit = MemoryGate::global().acquire_for(params.mem_needed(), budget)?;
                    let restore = if params.restore == 0 {
                        RestoreMethod::Restart
                    } else {
                        RestoreMethod::CutOff
                    };
                    let decoder =
                        Ppmd8Decoder::new(raw, u32::from(params.order), params.mem, restore)
                            .map_err(|e| {
                                if flag.load(Ordering::Relaxed) {
                                    ArchiveError::Io(io::Error::other(e.to_string()))
                                } else {
                                    damaged(format!("zip ppmd: {e}"))
                                }
                            })?;
                    // PPMd has no reliable end of its own: the output is
                    // bounded by the declared size, and the CRC decides.
                    Box::new(Holding {
                        inner: DecodeErrors {
                            inner: decoder,
                            src_failed: flag,
                            what: "zip ppmd",
                        }
                        .take(size),
                        _permit: permit,
                    })
                }
                M_XZ => {
                    let raw = self
                        .archive
                        .by_index_raw(index)
                        .map_err(|e| zip_error(e, &flag))?;
                    Box::new(
                        XzWalker::new(raw, cap).with_gate(MemoryGate::global(), budget.gate_wait()),
                    )
                }
                M_ZSTD => {
                    let raw = self
                        .archive
                        .by_index_raw(index)
                        .map_err(|e| zip_error(e, &flag))?;
                    Box::new(
                        ZstdFrames::new(raw, cap)
                            .with_gate(MemoryGate::global(), budget.gate_wait())
                            .with_skippable_limit(self.limits.max_header),
                    )
                }
                other => {
                    return Err(ArchiveError::Unsupported {
                        format: format!("zip method {other}"),
                        need: None,
                    })
                }
            }
        };
        Ok(Box::new(VerifyingReader::new(
            ChargedRead::new(decoder, budget),
            size,
            crc,
        )))
    }
}

/// Reads the property header of an LZMA/XZ/PPMd/zstd entry at list time
/// and checks its decoder memory against the cap (S2). `None` for the
/// methods the crate decodes.
fn list_params(
    raw: &mut impl Read,
    method: u16,
    limits: &Limits,
    skip_allowance: &mut u64,
) -> Result<Option<Method>, ArchiveError> {
    let cap = limits.decoder_mem_cap;
    Ok(Some(match method {
        M_LZMA => {
            let params = read_lzma_header(raw)?;
            lzma::check(params.mem_needed(), cap)?;
            Method::Lzma {
                dict: params.dict_raw,
            }
        }
        M_PPMD => {
            let params = read_ppmd_header(raw)?;
            lzma::check(params.mem_needed(), cap)?;
            Method::Ppmd {
                order: params.order,
                mem: params.mem,
            }
        }
        M_XZ => Method::Xz {
            dict: read_xz_first_dict(raw, cap)?,
        },
        M_ZSTD => Method::Zstd {
            window: read_zstd_first_window(raw, cap, skip_allowance)?,
        },
        _ => return Ok(None),
    }))
}

/// The listed [`Method`] of a zip method number; `parsed` carries what the
/// property header said (none for encrypted or empty entries).
fn list_method(method: u16, parsed: Option<Method>) -> Method {
    if let Some(m) = parsed {
        return m;
    }
    match method {
        M_STORED => Method::Stored,
        M_DEFLATE => Method::Deflate,
        M_DEFLATE64 => Method::Deflate64,
        M_BZIP2 => Method::Bzip2,
        M_LZMA => Method::Lzma { dict: 0 },
        M_XZ => Method::Xz { dict: 0 },
        M_PPMD => Method::Ppmd { order: 0, mem: 0 },
        M_ZSTD => Method::Zstd { window: 0 },
        other => Method::Other(format!("zip method {other}")),
    }
}

/// The validated name of an entry, or a stand-in and why the stored one
/// cannot be extracted.
fn entry_name(index: usize, record: &RawRecord) -> (RelPath, Option<UnsafeWhy>) {
    let unicode = record
        .unicode_name
        .as_ref()
        .map(|(crc, name)| (*crc, name.as_slice()));
    let parsed = decode_zip_name(&record.name, record.flags & 0x0800 != 0, unicode)
        .and_then(|text| RelPath::parse(&text));
    match parsed {
        Ok(name) => (name, None),
        Err(why) => (RelPath::placeholder(index), Some(why)),
    }
}

/// Unix hosts store `st_mode` in the high 16 bits of the external
/// attributes (APPNOTE 4.4.2: 3 = Unix, 19 = OS X).
fn entry_kind(record: &RawRecord) -> EntryKind {
    const S_IFMT: u32 = 0o170_000;
    const S_IFLNK: u32 = 0o120_000;
    const S_IFDIR: u32 = 0o040_000;
    const S_IFREG: u32 = 0o100_000;
    const DOS_DIR: u32 = 0x10;
    let host = record.made_by >> 8;
    if matches!(host, 3 | 19) {
        match (record.external_attr >> 16) & S_IFMT {
            S_IFLNK => return EntryKind::Symlink,
            S_IFDIR => return EntryKind::Dir,
            0 | S_IFREG => {}
            _ => return EntryKind::Other,
        }
    }
    if record.name.ends_with(b"/") || record.name.ends_with(b"\\") {
        return EntryKind::Dir;
    }
    if host == 0 && record.external_attr & DOS_DIR != 0 && record.name.is_empty() {
        return EntryKind::Dir;
    }
    EntryKind::File
}

/// A DOS date and time (no time zone; read as UTC).
fn dos_time(dt: ::zip::DateTime) -> Option<SystemTime> {
    let (month, day) = (dt.month(), dt.day());
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(i64::from(dt.year()), month, day);
    let secs = days * 86_400
        + i64::from(dt.hour()) * 3600
        + i64::from(dt.minute()) * 60
        + i64::from(dt.second());
    UNIX_EPOCH.checked_add(Duration::from_secs(u64::try_from(secs).ok()?))
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(year: i64, month: u8, day: u8) -> i64 {
    let (m, d) = (i64::from(month), i64::from(day));
    let y = if m <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The longest gap between the central directory and the end record (the
/// zip64 end record with its extensible data, and the locator).
const MAX_END_GAP: u64 = 64 * 1024;

/// Refuses an end record or zip64 end record anywhere between the start of
/// the checked central directory and the end record, other than the zip64
/// one the claim was read from. The crate looks for the zip64 record from
/// the locator's offset onwards and falls back to an earlier end record
/// when one fails; a second one there (in a name, an extra field, the
/// zip64 record's data) would be parsed with counts no cap saw.
fn no_other_end_records<R: Read + Seek>(
    src: &mut Source<R>,
    cd: &[u8],
    claim: &CentralDirectoryClaim,
) -> Result<(), ArchiveError> {
    let gap_len = claim
        .eocd_at
        .checked_sub(claim.cd_end)
        .filter(|&g| g <= MAX_END_GAP)
        .ok_or_else(|| damaged("zip end records"))?;
    // Bounded by `MAX_END_GAP` just above.
    let mut gap = vec![0u8; gap_len as usize];
    src.seek(SeekFrom::Start(claim.cd_end))
        .map_err(ArchiveError::Io)?;
    if read_full(src, &mut gap).map_err(ArchiveError::Io)? != gap.len() {
        return Err(damaged("zip end records"));
    }
    let zip64 = claim.cd_end != claim.eocd_at;
    let has = |hay: &[u8], sig: &[u8; 4], skip: Option<usize>| {
        hay.windows(4)
            .enumerate()
            .any(|(i, w)| w == sig && Some(i) != skip)
    };
    // A signature may straddle the directory and the gap.
    let seam_from = cd.len().saturating_sub(3);
    let mut seam = cd[seam_from..].to_vec();
    seam.extend_from_slice(&gap[..gap.len().min(3)]);
    let record_in_seam = zip64.then_some(cd.len() - seam_from);
    if has(cd, &EOCD_SIG, None)
        || has(&gap, &EOCD_SIG, None)
        || has(&seam, &EOCD_SIG, None)
        || has(cd, &ZIP64_EOCD_SIG, None)
        || has(&gap, &ZIP64_EOCD_SIG, zip64.then_some(0))
        || has(&seam, &ZIP64_EOCD_SIG, record_in_seam)
    {
        return Err(damaged("zip end record inside the central directory"));
    }
    Ok(())
}

/// `Io` when the file itself failed, else `Damaged { detail }`.
fn io_or(failed: &AtomicBool, detail: &str) -> ArchiveError {
    if failed.load(Ordering::Relaxed) {
        ArchiveError::Io(io::Error::other(detail.to_owned()))
    } else {
        damaged(detail)
    }
}

impl<R: Read + Seek + Send> ArchiveReader for ZipReader<R> {
    fn format(&self) -> Format {
        Format::Zip
    }

    fn entries(&self) -> &[Entry] {
        &self.entries
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

    /// `open_entry` per wanted entry, in central-directory order (zip
    /// entries are independent, so one pass is one entry at a time).
    fn stream_entries(
        &mut self,
        wanted: &[usize],
        budget: &mut Budget,
        sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
    ) -> Result<(), ArchiveError> {
        let mut order = wanted.to_vec();
        order.sort_unstable();
        order.dedup();
        for index in order {
            let entry = self.entries.get(index).cloned().ok_or_else(|| {
                ArchiveError::Io(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("no zip entry {index}"),
                ))
            })?;
            let mut stream = self.open_entry(index, budget)?;
            sink(&entry, &mut stream)?;
        }
        self.poison.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::MIB;

    #[test]
    fn days_from_civil_known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(1980, 1, 1), 3_652);
    }

    #[test]
    fn ppmd_header_fields() {
        let p = PpmdParams::from_header([0x75, 0x00]).unwrap(); // order 6, 8 MiB
        assert_eq!(p.order, 6);
        assert_eq!(p.mem, 8 << 20);
        assert!(p.mem_needed().is_multiple_of(MIB));
        assert!(PpmdParams::from_header([0x70, 0x00]).is_err(), "order 1");
        assert!(PpmdParams::from_header([0x75, 0x20]).is_err(), "restore 2");
        let max = PpmdParams::from_header([0xF5, 0x0F]).unwrap();
        assert_eq!(max.mem_needed(), 256 * MIB);
    }

    #[test]
    fn extra_fields_stop_at_a_short_field() {
        let extra = [0x01, 0x00, 0x02, 0x00, 0xAA, 0xBB, 0x75, 0x70, 0x09];
        let got: Vec<(u16, Vec<u8>)> = extra_fields(&extra)
            .map(|(id, d)| (id, d.to_vec()))
            .collect();
        assert_eq!(got, vec![(1, vec![0xAA, 0xBB])]);
    }
}
