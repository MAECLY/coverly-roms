//! Single-file compressors: `.gz`, `.bz2`, `.xz`, `.zst`, `.lz4` and
//! `.lzma` around one file (`game.gba.gz`), read as an archive with **one
//! virtual entry** (spec `2026-10-23-organize-archives-and-discs.md` §4.3,
//! §6.1).
//!
//! **Name.** Gzip's `FNAME`, validated like any entry name ([`RelPath`]);
//! when it is missing or unsafe, the archive's own stem (`game.gba.gz` →
//! `game.gba`). An unsafe stem (`CON.gz`) keeps the entry flagged in
//! `unsafe_name`, so it is never extracted.
//!
//! **Listing** (the integrity listing of §4.3) checks the stream header and
//! the decoder memory it implies (S2): the xz first block dictionary, the
//! zstd first frame window, the `.lzma` dictionary. Then a **bounded
//! decode** of at most `plan_head_budget` bytes (and the plan wall time)
//! measures the size and the CRC32 exactly; the compressor's own checks run
//! as it goes. When the stream is longer than that, the size comes from
//! what the format declares: the xz index, the zstd frame content sizes,
//! the lz4 frame content size, the `.lzma` header. bzip2 declares
//! nothing, and the gzip trailer is not a size: `ISIZE` is the size mod
//! 4 GiB (a 4.5 GiB DVD image would claim 0.5 GiB and pass a FAT32 check)
//! and, with several members, only the last member's size and CRC32. Such
//! an entry has [`SizeSource::Unknown`], its listing is not complete
//! ([`ArchiveReader::listing_complete`] is false, so it is never
//! extracted), and its stream is bounded by `max_entry_size` only (gzip
//! and bzip2 still check every member's own CRC as they decode).
//!
//! **Decoding** goes through [`open_compressor`], shared with tar (plan
//! Task 12): `flate2`'s `MultiGzDecoder` (concatenated members), `bzip2`'s
//! `MultiBzDecoder` (pure-Rust backend), [`XzWalker`] (every block checked
//! before its decoder exists), [`ZstdFrames`] (every frame's window
//! checked; skippable frames drained against the budget and `max_header`),
//! the `lz4_flex` frame decoder (blocks of at most 4 MiB; 8 MiB for legacy
//! frames), and LZMA alone through [`open_decoder`] with the raw header
//! dictionary (S3). lz4 frames are also walked by their block headers
//! first, because `lz4_flex` reports a frame cut between two blocks as a
//! clean end. Every stream is `GuardedRead<VerifyingReader<ChargedRead<…>>>`
//! and the listing runs inside the panic guard (S1).

use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::budget::{Budget, ChargedRead, MemoryGate, Permit};
use super::guard::{guarded, GuardedRead, Poison};
use super::lzma::{
    self, open_decoder, parse_lzma_alone, read_xz_first_dict, LzmaKind, Tracked, XzWalker,
    LZMA_ALONE_HEADER,
};
use super::names::{RelPath, UnsafeWhy};
use super::verify::VerifyingReader;
use super::zstd::{parse_frame_header, read_zstd_first_window, ZstdFrames, ZSTD_MAGIC};
use super::{
    ArchiveError, ArchiveReader, Compressor, Entry, EntryKind, Format, Limits, Method, KIB,
};

/// How the entry's size (and CRC32) was learnt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SizeSource {
    /// The bounded listing decode reached the end: size and CRC32 exact.
    Measured,
    /// From the format's own fields (xz index, zstd/lz4 frame content
    /// size, `.lzma` header); verified at EOF. Never gzip (module docs).
    Declared,
    /// Neither: the entry's `size` is only the bytes the bounded decode
    /// reached (a lower bound) and its `crc32` is `None`.
    Unknown,
}

/// One compressed file read as an archive of one entry (module docs).
pub struct SingleReader<R: Read + Seek + Send = File> {
    src: R,
    compressor: Compressor,
    entries: Vec<Entry>,
    size_source: SizeSource,
    limits: Limits,
    poison: Poison,
}

impl SingleReader<File> {
    /// Opens and lists `path` as a `compressor` stream; the entry is named
    /// from the gzip `FNAME` or from `path`'s stem.
    pub fn open(
        path: &Path,
        compressor: Compressor,
        limits: &Limits,
    ) -> Result<Self, ArchiveError> {
        let file = crate::consoles::open_rom(path).map_err(ArchiveError::Io)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self::new(file, compressor, &name, limits)
    }
}

impl<R: Read + Seek + Send> SingleReader<R> {
    /// Lists the `compressor` stream in `src` (the integrity listing of
    /// spec §4.3) inside the panic guard. `file_name` is the archive's own
    /// name, whose stem names the entry when the stream does not.
    pub fn new(
        src: R,
        compressor: Compressor,
        file_name: &str,
        limits: &Limits,
    ) -> Result<Self, ArchiveError> {
        let limits = limits.clone();
        let file_name = file_name.to_owned();
        guarded(move || Self::list(src, compressor, &file_name, limits))
    }

    /// How the entry's size was learnt.
    pub fn size_source(&self) -> SizeSource {
        self.size_source
    }

    fn list(
        mut src: R,
        compressor: Compressor,
        file_name: &str,
        limits: Limits,
    ) -> Result<Self, ArchiveError> {
        let len = src.seek(SeekFrom::End(0)).map_err(ArchiveError::Io)?;
        rewind(&mut src)?;
        let cap = limits.decoder_mem_cap;
        let mut stored_name = None;
        let mut mtime = None;
        let method = match compressor {
            Compressor::Gzip => {
                let header = gzip_header(&mut src)?;
                stored_name = header.name;
                mtime = header.mtime;
                Method::Deflate
            }
            Compressor::Bzip2 => Method::Bzip2,
            Compressor::Xz => Method::Xz {
                dict: read_xz_first_dict(&mut src, cap)?,
            },
            Compressor::Zstd => {
                let mut allowance = limits.max_header;
                Method::Zstd {
                    window: read_zstd_first_window(&mut src, cap, &mut allowance)?,
                }
            }
            Compressor::Lz4 => {
                lz4_walk(&mut src, len)?;
                Method::Lz4
            }
            Compressor::LzmaAlone => {
                let (params, _) = read_lzma_alone_header(&mut src)?;
                lzma::check(LzmaKind::Lzma(params).mem_needed(), cap)?;
                Method::Lzma {
                    dict: params.dict_raw,
                }
            }
        };

        let (size, crc32, size_source) = match measure(&mut src, compressor, &limits)? {
            Measure::Done { size, crc } => (size, Some(crc), SizeSource::Measured),
            Measure::Partial { reached } => {
                // A declaration below what was already decoded is wrong.
                match declared_size(&mut src, compressor, len, &limits)?
                    .filter(|&size| size >= reached)
                {
                    Some(size) => (size, None, SizeSource::Declared),
                    None => (reached, None, SizeSource::Unknown),
                }
            }
        };
        if size > limits.max_entry_size || size > limits.max_total_size {
            return Err(ArchiveError::TooLarge { declared: size });
        }

        let (name, unsafe_name) = entry_name(stored_name.as_deref(), file_name);
        let raw_name = stored_name.unwrap_or_else(|| name.as_str().as_bytes().to_vec());
        let entry = Entry {
            index: 0,
            name,
            raw_name,
            kind: EntryKind::File,
            size,
            packed: Some(len),
            crc32,
            encrypted: false,
            method,
            solid_block: None,
            mtime,
            unsafe_name,
        };
        Ok(Self {
            src,
            compressor,
            entries: vec![entry],
            size_source,
            limits,
            poison: Poison::new(),
        })
    }

    /// The decoded, verified bytes of the entry, before the guard wrapper.
    fn open_decoded<'a>(
        &'a mut self,
        index: usize,
        budget: &'a mut Budget,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
        let entry = self.entries.get(index).ok_or_else(|| {
            ArchiveError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("no entry {index} in a single-file stream"),
            ))
        })?;
        if let Some(why) = entry.unsafe_name {
            return Err(ArchiveError::UnsafeEntry {
                name: String::from_utf8_lossy(&entry.raw_name).into_owned(),
                why,
            });
        }
        let (size, crc) = (entry.size, entry.crc32);
        if self.compressor == Compressor::Lz4 {
            // The file may have changed since the listing.
            let len = self.src.seek(SeekFrom::End(0)).map_err(ArchiveError::Io)?;
            lz4_walk(&mut self.src, len)?;
        }
        rewind(&mut self.src)?;
        let decoder = open_compressor(self.compressor, &mut self.src, &self.limits, budget)?;
        let charged = ChargedRead::new(decoder, budget);
        Ok(match self.size_source {
            SizeSource::Measured | SizeSource::Declared => {
                Box::new(VerifyingReader::new(charged, size, crc))
            }
            SizeSource::Unknown => Box::new(SizeCap {
                inner: charged,
                left: self.limits.max_entry_size,
            }),
        })
    }
}

impl<R: Read + Seek + Send> ArchiveReader for SingleReader<R> {
    fn format(&self) -> Format {
        Format::Single(self.compressor)
    }

    fn entries(&self) -> &[Entry] {
        &self.entries
    }

    fn listing_complete(&self) -> bool {
        self.size_source != SizeSource::Unknown
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
        if wanted.is_empty() {
            return Ok(());
        }
        if let Some(&bad) = wanted.iter().find(|&&i| i != 0) {
            return Err(ArchiveError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("no entry {bad} in a single-file stream"),
            )));
        }
        let entry = self.entries[0].clone();
        let mut stream = self.open_entry(0, budget)?;
        sink(&entry, &mut stream)?;
        drop(stream);
        self.poison.check()
    }
}

/// The decoded stream of compressor `c` over `src` (positioned at its
/// first byte). Every decoder memory is checked against
/// `limits.decoder_mem_cap` before it exists and taken from the global
/// [`MemoryGate`] while it decodes, waiting at most as `budget` allows.
/// zstd skippable frames are drained against `budget`'s remaining bytes and
/// `max_header`. Errors come back as `io::Error`s carrying an
/// [`ArchiveError`]: `Damaged` for rejected data, `Io` for the source's own
/// failures. The caller charges the output (`ChargedRead`) and verifies it.
pub fn open_compressor<'a, R: Read + 'a>(
    c: Compressor,
    src: R,
    limits: &Limits,
    budget: &Budget,
) -> Result<Box<dyn Read + 'a>, ArchiveError> {
    let cap = limits.decoder_mem_cap;
    let gate = MemoryGate::global();
    Ok(match c {
        Compressor::Gzip => {
            let src = Tracked::new(src);
            let src_failed = src.flag();
            Box::new(DecodeErrors {
                inner: flate2::read::MultiGzDecoder::new(src),
                src_failed,
                what: "gzip",
            })
        }
        Compressor::Bzip2 => {
            let src = Tracked::new(src);
            let src_failed = src.flag();
            Box::new(DecodeErrors {
                inner: bzip2::read::MultiBzDecoder::new(src),
                src_failed,
                what: "bzip2",
            })
        }
        Compressor::Xz => Box::new(XzWalker::new(src, cap).with_gate(gate, budget.gate_wait())),
        Compressor::Zstd => {
            let skippable = budget
                .remaining()
                .map_or(limits.max_header, |left| left.min(limits.max_header));
            Box::new(
                ZstdFrames::new(src, cap)
                    .with_gate(gate, budget.gate_wait())
                    .with_skippable_limit(skippable),
            )
        }
        Compressor::Lz4 => {
            let src = Tracked::new(src);
            let src_failed = src.flag();
            Box::new(DecodeErrors {
                inner: Lz4Frames::new(src),
                src_failed,
                what: "lz4",
            })
        }
        Compressor::LzmaAlone => {
            let mut src = src;
            let (params, declared) = read_lzma_alone_header(&mut src)?;
            let kind = LzmaKind::Lzma(params);
            lzma::check(kind.mem_needed(), cap)?;
            let permit = gate.acquire_for(kind.mem_needed(), budget)?;
            let inner = open_decoder(kind, declared, cap, src)?;
            Box::new(Holding {
                inner,
                _permit: permit,
            })
        }
    })
}

/// The method of the `c` stream in `src` with the decoder memory its
/// header implies checked against the cap (S2): what a tar inside a
/// compressor reports for its entries (plan Task 12). The header is
/// validated (gzip header, xz first block, zstd first window, every lz4
/// block header, `.lzma` properties); `src` is left at its start.
pub(super) fn stream_method<R: Read + Seek>(
    src: &mut R,
    c: Compressor,
    limits: &Limits,
) -> Result<Method, ArchiveError> {
    rewind(src)?;
    let cap = limits.decoder_mem_cap;
    let method = match c {
        Compressor::Gzip => {
            gzip_header(src)?;
            Method::Deflate
        }
        Compressor::Bzip2 => Method::Bzip2,
        Compressor::Xz => Method::Xz {
            dict: read_xz_first_dict(src, cap)?,
        },
        Compressor::Zstd => {
            let mut allowance = limits.max_header;
            Method::Zstd {
                window: read_zstd_first_window(src, cap, &mut allowance)?,
            }
        }
        Compressor::Lz4 => {
            let len = src.seek(SeekFrom::End(0)).map_err(ArchiveError::Io)?;
            lz4_walk(src, len)?;
            Method::Lz4
        }
        Compressor::LzmaAlone => {
            let (params, _) = read_lzma_alone_header(src)?;
            lzma::check(LzmaKind::Lzma(params).mem_needed(), cap)?;
            Method::Lzma {
                dict: params.dict_raw,
            }
        }
    };
    rewind(src)?;
    Ok(method)
}

/// What the bounded listing decode reached.
enum Measure {
    Done {
        size: u64,
        crc: u32,
    },
    /// The budget (bytes or wall time) ran out after `reached` bytes.
    Partial {
        reached: u64,
    },
}

/// Decodes at most `plan_head_budget` bytes (and the plan wall time) to
/// measure the size and CRC32. Every other error is the listing's.
fn measure<R: Read + Seek>(
    src: &mut R,
    c: Compressor,
    limits: &Limits,
) -> Result<Measure, ArchiveError> {
    rewind(src)?;
    let mut budget = Budget::plan_head(limits);
    let decoder = open_compressor(c, &mut *src, limits, &budget)?;
    let mut stream = ChargedRead::new(decoder, &mut budget);
    let mut hasher = crc32fast::Hasher::new();
    let mut size = 0u64;
    let mut buf = vec![0u8; 64 * KIB as usize];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                size += n as u64;
                hasher.update(&buf[..n]);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => {
                return match ArchiveError::from(e) {
                    ArchiveError::BudgetSpent { .. } => Ok(Measure::Partial { reached: size }),
                    other => Err(other),
                }
            }
        }
    }
    Ok(Measure::Done {
        size,
        crc: hasher.finalize(),
    })
}

/// The size the format declares, when it does. `None` also when the
/// declaration cannot be trusted (an xz file of several streams, a zstd
/// frame without a content size, a malformed structure the decoder will
/// report) and always for gzip, whose `ISIZE` wraps at 4 GiB and covers
/// the last member only (module docs).
fn declared_size<R: Read + Seek>(
    src: &mut R,
    c: Compressor,
    len: u64,
    limits: &Limits,
) -> Result<Option<u64>, ArchiveError> {
    Ok(match c {
        Compressor::Gzip | Compressor::Bzip2 => None,
        Compressor::Xz => xz_index_size(src, len, limits.max_header)?,
        Compressor::Zstd => zstd_content_size(src, len)?,
        Compressor::Lz4 => lz4_walk(src, len)?,
        Compressor::LzmaAlone => {
            rewind(src)?;
            read_lzma_alone_header(src)?.1
        }
    })
}

/// The entry name: `stored` (gzip `FNAME`) when it is a safe path, else
/// the stem of `file_name`; an unsafe stem keeps its reason.
fn entry_name(stored: Option<&[u8]>, file_name: &str) -> (RelPath, Option<UnsafeWhy>) {
    if let Some(name) = stored.and_then(|raw| RelPath::parse(&latin1_or_utf8(raw)).ok()) {
        return (name, None);
    }
    let stem = match file_name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => file_name,
    };
    match RelPath::parse(stem) {
        Ok(name) => (name, None),
        Err(why) => (RelPath::placeholder(0), Some(why)),
    }
}

/// RFC 1952 says ISO 8859-1; tools write UTF-8 too. Valid UTF-8 wins.
fn latin1_or_utf8(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(s) => s.to_owned(),
        Err(_) => raw.iter().map(|&b| char::from(b)).collect(),
    }
}

/// What the gzip header of the first member says.
struct GzipHeader {
    name: Option<Vec<u8>>,
    mtime: Option<SystemTime>,
}

/// Parses the first gzip member header (`flate2` validates the method,
/// the reserved flag bits and caps `FNAME`/`FCOMMENT` at 64 KiB).
fn gzip_header<R: Read + Seek>(src: &mut R) -> Result<GzipHeader, ArchiveError> {
    rewind(src)?;
    let tracked = Tracked::new(&mut *src);
    let failed = tracked.flag();
    let mut decoder = flate2::read::GzDecoder::new(tracked);
    let Some(header) = decoder.header() else {
        // The header did not parse: reading reports why.
        let err = decoder
            .read(&mut [0u8; 1])
            .err()
            .unwrap_or_else(|| io::Error::other("gzip header"));
        return Err(map_decode_error(err, &failed, "gzip header"));
    };
    let mtime =
        (header.mtime() != 0).then(|| UNIX_EPOCH + Duration::from_secs(u64::from(header.mtime())));
    Ok(GzipHeader {
        name: header.filename().map(<[u8]>::to_vec),
        mtime,
    })
}

fn read_lzma_alone_header(
    src: &mut impl Read,
) -> Result<(lzma::LzmaParams, Option<u64>), ArchiveError> {
    let mut header = [0u8; LZMA_ALONE_HEADER];
    if read_full(src, &mut header).map_err(ArchiveError::Io)? != LZMA_ALONE_HEADER {
        return Err(damaged("lzma header truncated"));
    }
    parse_lzma_alone(header)
}

const LZ4_MAGIC: u32 = 0x184D_2204;
const LZ4_LEGACY_MAGIC: u32 = 0x184C_2102;
const SKIPPABLE: std::ops::RangeInclusive<u32> = 0x184D_2A50..=0x184D_2A5F;
/// Block headers walked before giving up on a declared size (bounds the
/// time a file of millions of tiny blocks can take).
const MAX_WALK_STEPS: u64 = 1 << 22;

/// Walks lz4 frames by their headers and block sizes without decoding:
/// `Damaged` when a frame is cut (`lz4_flex` would end cleanly between two
/// blocks), the content size of every frame summed when each declares it.
/// A legacy frame or a file of more than [`MAX_WALK_STEPS`] blocks ends
/// the walk with `None`.
fn lz4_walk<R: Read + Seek>(src: &mut R, len: u64) -> Result<Option<u64>, ArchiveError> {
    rewind(src)?;
    let mut r = Walk::new(src, len);
    let mut total = Some(0u64);
    let mut frames = 0u64;
    while let Some(magic) = r.u32_or_end("lz4 magic")? {
        frames += 1;
        if SKIPPABLE.contains(&magic) {
            // lz4_flex refuses skippable frames; the decoder reports it.
            return Ok(None);
        }
        if magic == LZ4_LEGACY_MAGIC {
            return Ok(None);
        }
        if magic != LZ4_MAGIC {
            return Err(damaged("lz4 magic"));
        }
        let [flg, _bd] = r.array::<2>("lz4 frame header")?;
        if flg >> 6 != 0b01 {
            return Err(damaged("lz4 frame version"));
        }
        let content_size = if flg & 0x08 != 0 {
            Some(u64::from_le_bytes(r.array::<8>("lz4 frame header")?))
        } else {
            None
        };
        if flg & 0x01 != 0 {
            r.skip(4, "lz4 frame header")?;
        }
        r.skip(1, "lz4 frame header")?;
        loop {
            if !r.step() {
                return Ok(None);
            }
            let word = r
                .u32_or_end("lz4 frame truncated")?
                .ok_or_else(|| damaged("lz4 frame truncated"))?;
            if word == 0 {
                break;
            }
            let block = u64::from(word & 0x7FFF_FFFF) + if flg & 0x10 != 0 { 4 } else { 0 };
            r.skip(block, "lz4 frame truncated")?;
        }
        if flg & 0x04 != 0 {
            r.skip(4, "lz4 frame truncated")?;
        }
        total = match (total, content_size) {
            (Some(t), Some(c)) => t.checked_add(c),
            _ => None,
        };
    }
    if frames == 0 {
        return Err(damaged("lz4 stream without a frame"));
    }
    Ok(total)
}

/// The sum of the content sizes every zstd frame declares, walking frame
/// headers and block headers without decoding. `None` when a frame has no
/// content size or anything looks wrong (the decoder reports damage).
fn zstd_content_size<R: Read + Seek>(src: &mut R, len: u64) -> Result<Option<u64>, ArchiveError> {
    rewind(src)?;
    let mut r = Walk::new(src, len);
    let mut total = 0u64;
    let mut frames = 0u64;
    loop {
        let magic = match r.u32_or_end("zstd magic") {
            Ok(Some(m)) => m,
            Ok(None) => break,
            Err(ArchiveError::Io(e)) => return Err(ArchiveError::Io(e)),
            Err(_) => return Ok(None),
        };
        let parsed = (|| -> Result<Option<u64>, ArchiveError> {
            if SKIPPABLE.contains(&magic) {
                let n = u32::from_le_bytes(r.array::<4>("zstd")?);
                r.skip(u64::from(n), "zstd")?;
                return Ok(Some(0));
            }
            if magic != ZSTD_MAGIC {
                return Ok(None);
            }
            let [desc] = r.array::<1>("zstd")?;
            let mut header = Vec::with_capacity(18);
            header.extend_from_slice(&magic.to_le_bytes());
            header.push(desc);
            // At most 1 + 4 + 8 bytes follow the descriptor.
            let rest = usize::from(desc & 0x20 == 0)
                + [0, 1, 2, 4][usize::from(desc & 0x03)]
                + match desc >> 6 {
                    0 => usize::from(desc & 0x20 != 0),
                    1 => 2,
                    2 => 4,
                    _ => 8,
                };
            for _ in 0..rest {
                header.push(r.array::<1>("zstd")?[0]);
            }
            let (frame, _) = parse_frame_header(&header)?;
            let Some(content) = frame.content_size else {
                return Ok(None);
            };
            loop {
                if !r.step() {
                    return Ok(None);
                }
                let b = r.array::<3>("zstd")?;
                let word = u32::from_le_bytes([b[0], b[1], b[2], 0]);
                let last = word & 1 != 0;
                let size = u64::from(word >> 3);
                match (word >> 1) & 0x03 {
                    0 | 2 => r.skip(size, "zstd")?,
                    1 => r.skip(1, "zstd")?,
                    _ => return Ok(None),
                }
                if last {
                    break;
                }
            }
            if frame.checksum {
                r.skip(4, "zstd")?;
            }
            Ok(Some(content))
        })();
        match parsed {
            Ok(Some(n)) => total = total.saturating_add(n),
            Ok(None) => return Ok(None),
            Err(ArchiveError::Io(e)) => return Err(ArchiveError::Io(e)),
            Err(_) => return Ok(None),
        }
        if SKIPPABLE.contains(&magic) {
            continue;
        }
        frames += 1;
    }
    Ok((frames > 0).then_some(total))
}

/// The uncompressed size from the index of a **single-stream** `.xz` file
/// (footer, backward size, index records and both CRC32s checked). `None`
/// when the file is more than that stream (several streams, padding) or
/// the index is malformed (the walker reports damage when it decodes).
fn xz_index_size<R: Read + Seek>(
    src: &mut R,
    len: u64,
    max_header: u64,
) -> Result<Option<u64>, ArchiveError> {
    const HEADER: u64 = 12;
    const FOOTER: u64 = 12;
    if len < HEADER + FOOTER + 8 {
        return Ok(None);
    }
    let Some(footer) = read_at(src, len - FOOTER, FOOTER, len)? else {
        return Ok(None);
    };
    if &footer[10..12] != b"YZ"
        || crc32fast::hash(&footer[4..10])
            != u32::from_le_bytes([footer[0], footer[1], footer[2], footer[3]])
    {
        return Ok(None);
    }
    let backward = u32::from_le_bytes([footer[4], footer[5], footer[6], footer[7]]);
    let index_size = (u64::from(backward) + 1) * 4;
    if index_size > max_header || index_size > len - HEADER - FOOTER {
        return Ok(None);
    }
    let Some(index) = read_at(src, len - FOOTER - index_size, index_size, len)? else {
        return Ok(None);
    };
    let (body, crc) = index.split_at(index.len() - 4);
    if body.first() != Some(&0)
        || crc32fast::hash(body) != u32::from_le_bytes([crc[0], crc[1], crc[2], crc[3]])
    {
        return Ok(None);
    }
    let mut pos = 1;
    let Some(count) = vli(body, &mut pos) else {
        return Ok(None);
    };
    if count > index_size / 2 {
        return Ok(None);
    }
    let mut blocks = 0u64;
    let mut total = 0u64;
    for _ in 0..count {
        let (Some(unpadded), Some(uncompressed)) = (vli(body, &mut pos), vli(body, &mut pos))
        else {
            return Ok(None);
        };
        let padded = unpadded.checked_add(3).map(|v| v & !3);
        match (
            padded.and_then(|p| blocks.checked_add(p)),
            total.checked_add(uncompressed),
        ) {
            (Some(b), Some(t)) => (blocks, total) = (b, t),
            _ => return Ok(None),
        }
    }
    let stream = HEADER
        .checked_add(blocks)
        .and_then(|v| v.checked_add(index_size))
        .and_then(|v| v.checked_add(FOOTER));
    Ok((stream == Some(len)).then_some(total))
}

/// An xz variable-length integer (at most 9 bytes).
fn vli(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for i in 0..9 {
        let b = *bytes.get(*pos)?;
        *pos += 1;
        value |= u64::from(b & 0x7F) << (7 * i);
        if b & 0x80 == 0 {
            return (i == 0 || b != 0).then_some(value);
        }
    }
    None
}

/// A buffered forward walk over a file of `len` bytes that never reads
/// past the end: skips are checked against `len`, not performed by reads.
struct Walk<'s, R> {
    r: BufReader<&'s mut R>,
    pos: u64,
    len: u64,
    steps: u64,
}

impl<'s, R: Read + Seek> Walk<'s, R> {
    fn new(src: &'s mut R, len: u64) -> Self {
        Self {
            r: BufReader::with_capacity(64 * KIB as usize, src),
            pos: 0,
            len,
            steps: 0,
        }
    }

    /// Counts one block; false once [`MAX_WALK_STEPS`] is reached.
    fn step(&mut self) -> bool {
        self.steps += 1;
        self.steps <= MAX_WALK_STEPS
    }

    fn array<const N: usize>(&mut self, what: &str) -> Result<[u8; N], ArchiveError> {
        let mut out = [0u8; N];
        if read_full(&mut self.r, &mut out).map_err(ArchiveError::Io)? != N {
            return Err(damaged(what));
        }
        self.pos += N as u64;
        Ok(out)
    }

    /// A little-endian u32, `None` at a clean end, `Damaged` when cut.
    fn u32_or_end(&mut self, what: &str) -> Result<Option<u32>, ArchiveError> {
        let mut out = [0u8; 4];
        match read_full(&mut self.r, &mut out).map_err(ArchiveError::Io)? {
            0 => Ok(None),
            4 => {
                self.pos += 4;
                Ok(Some(u32::from_le_bytes(out)))
            }
            _ => Err(damaged(what)),
        }
    }

    fn skip(&mut self, n: u64, what: &str) -> Result<(), ArchiveError> {
        let to = self.pos.checked_add(n).filter(|&to| to <= self.len);
        let Some(to) = to else {
            return Err(damaged(what));
        };
        let offset = i64::try_from(n).map_err(|_| damaged(what))?;
        self.r.seek_relative(offset).map_err(ArchiveError::Io)?;
        self.pos = to;
        Ok(())
    }
}

/// `n` bytes at `offset`, `None` when they are not all inside `len`.
fn read_at<R: Read + Seek>(
    src: &mut R,
    offset: u64,
    n: u64,
    len: u64,
) -> Result<Option<Vec<u8>>, ArchiveError> {
    if offset.checked_add(n).is_none_or(|end| end > len) {
        return Ok(None);
    }
    let n = usize::try_from(n).map_err(|_| damaged("offset"))?;
    src.seek(SeekFrom::Start(offset))
        .map_err(ArchiveError::Io)?;
    let mut buf = vec![0u8; n];
    if read_full(src, &mut buf).map_err(ArchiveError::Io)? != n {
        return Ok(None);
    }
    Ok(Some(buf))
}

fn rewind(src: &mut impl Seek) -> Result<(), ArchiveError> {
    src.seek(SeekFrom::Start(0))
        .map(drop)
        .map_err(ArchiveError::Io)
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

fn damaged(detail: impl Into<String>) -> ArchiveError {
    ArchiveError::Damaged {
        detail: detail.into(),
    }
}

/// An error out of a third-party decoder: one carrying an `ArchiveError`
/// stays itself, one of the source stays I/O, anything else is the
/// decoder rejecting the data (`Damaged`).
fn map_decode_error(e: io::Error, src_failed: &AtomicBool, what: &str) -> ArchiveError {
    if e.get_ref().is_some_and(|inner| inner.is::<ArchiveError>()) {
        return ArchiveError::from(e);
    }
    if src_failed.load(Ordering::Relaxed) {
        return ArchiveError::Io(e);
    }
    damaged(format!("{what}: {e}"))
}

/// Maps the errors of a decoder (see [`map_decode_error`]).
struct DecodeErrors<D> {
    inner: D,
    src_failed: Arc<AtomicBool>,
    what: &'static str,
}

impl<D: Read> Read for DecodeErrors<D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner
            .read(buf)
            .map_err(|e| map_decode_error(e, &self.src_failed, self.what).into())
    }
}

/// Reports, through a shared flag, that a read of the source hit its end.
struct AtEof<R> {
    inner: R,
    eof: Arc<AtomicBool>,
}

impl<R: Read> Read for AtEof<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n == 0 && !buf.is_empty() {
            self.eof.store(true, Ordering::Relaxed);
        }
        Ok(n)
    }
}

/// Every lz4 frame of the source as one stream, as the `lz4` tool reads
/// concatenated frames. `lz4_flex` returns `Ok(0)` at each frame's end
/// mark and starts the next frame on the following read, so a bare
/// decoder would end the stream after the first frame: later frames would
/// be lost, and a tar's trailer drain (S6) would never see them. The
/// stream ends only when the source itself is exhausted; each frame
/// consumes at least its header, so the loop always ends.
struct Lz4Frames<R: Read> {
    inner: lz4_flex::frame::FrameDecoder<AtEof<R>>,
    eof: Arc<AtomicBool>,
}

impl<R: Read> Lz4Frames<R> {
    fn new(src: R) -> Self {
        let eof = Arc::new(AtomicBool::new(false));
        let src = AtEof {
            inner: src,
            eof: Arc::clone(&eof),
        };
        Self {
            inner: lz4_flex::frame::FrameDecoder::new(src),
            eof,
        }
    }
}

impl<R: Read> Read for Lz4Frames<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            let n = self.inner.read(buf)?;
            if n > 0 || self.eof.load(Ordering::Relaxed) {
                return Ok(n);
            }
        }
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

/// An entry of unknown size: no size or CRC to verify (the compressor's
/// own checks still run), but never more than `max_entry_size` bytes.
struct SizeCap<R> {
    inner: R,
    left: u64,
}

impl<R: Read> Read for SizeCap<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.left == 0 {
            // One byte more than allowed is an error, an exact end is not.
            let mut probe = [0u8; 1];
            return match self.inner.read(&mut probe)? {
                0 => Ok(0),
                _ => Err(ArchiveError::TooLarge { declared: u64::MAX }.into()),
            };
        }
        let want = usize::try_from(self.left).map_or(buf.len(), |l| l.min(buf.len()));
        let n = self.inner.read(&mut buf[..want])?;
        self.left -= n as u64;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_rules() {
        let name = |f: &str| entry_name(None, f);
        assert_eq!(name("game.gba.gz").0.as_str(), "game.gba");
        assert_eq!(name("game.gz").0.as_str(), "game");
        assert_eq!(name("noext").0.as_str(), "noext");
        assert_eq!(name("CON.gz").1, Some(UnsafeWhy::Device));
    }

    #[test]
    fn vli_rejects_overlong_and_truncated() {
        let mut pos = 0;
        assert_eq!(vli(&[0x80, 0x01], &mut pos), Some(128));
        let mut pos = 0;
        assert_eq!(vli(&[0x80, 0x00], &mut pos), None, "non-minimal");
        let mut pos = 0;
        assert_eq!(vli(&[0x80], &mut pos), None, "truncated");
    }

    #[test]
    fn size_cap_stops_one_byte_past() {
        let mut ok = SizeCap {
            inner: &b"abcd"[..],
            left: 4,
        };
        let mut out = Vec::new();
        ok.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"abcd");
        let mut over = SizeCap {
            inner: &b"abcde"[..],
            left: 4,
        };
        let err = over.read_to_end(&mut Vec::new()).unwrap_err();
        assert!(matches!(
            ArchiveError::from(err),
            ArchiveError::TooLarge { .. }
        ));
    }
}
