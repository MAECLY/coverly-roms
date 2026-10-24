//! LZMA, LZMA2 and xz: one memory formula for the cap check and for the
//! decoder (spec `2026-10-23-organize-archives-and-discs.md` §4, S2, S3).
//!
//! * [`LzmaParams`] / [`Lzma2Params`] keep the **raw** dictionary size of
//!   the stream's own property header. [`LzmaParams::mem_needed`] never
//!   lowers it with a declared unpacked size (S3): a tiny declared size and
//!   a 1.5 GiB dictionary is refused, not "clamped".
//! * [`open_decoder`] is the only place in the crate that builds an
//!   `lzma-rust2` reader (`clippy.toml` bans the constructors everywhere
//!   else). It checks [`LzmaKind::mem_needed`] against the cap and then
//!   hands the decoder exactly the checked dictionary.
//! * [`XzWalker`] decodes `.xz` block by block. Every block header (of
//!   every stream of a multi-stream file) is parsed and its LZMA2
//!   dictionary checked **before** that block's decoder exists (S2); the
//!   block check, the index and the footer are verified as it goes.
//!
//! Callers wrap what these return in `GuardedRead` (S1), `ChargedRead`
//! and `VerifyingReader`, as for every other decoder.

use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use lzma_rust2::filter::bcj::BcjReader;
use lzma_rust2::filter::delta::DeltaReader;
use lzma_rust2::{Lzma2Reader, LzmaReader};

use super::budget::{MemoryGate, Permit};
use super::guard::guarded;
use super::{ArchiveError, BcjKind, KIB};

/// Highest valid LZMA properties byte: `(pb * 5 + lp) * 9 + lc` with
/// `lc ≤ 8`, `lp ≤ 4`, `pb ≤ 4`.
pub const LZMA_PROPS_MAX: u8 = 224;
/// Highest valid LZMA2 dictionary property (40 = 4 GiB − 1).
pub const LZMA2_DICT_PROP_MAX: u8 = 40;
/// LZMA2 limits `lc + lp` to 4 in every chunk.
pub const LZMA2_LC_LP_MAX: u8 = 4;
/// Decoder state besides the dictionary and the literal coder: the
/// range-decoder buffer (64 KiB for LZMA2), the probability tables of the
/// match coders and the BCJ/Delta filter buffers. Generous on purpose:
/// the check must never be below what the decoder allocates.
pub const FIXED_STATE: u64 = 128 * KIB;
/// Size of the "LZMA alone" (`.lzma`) header: props, dict, size.
pub const LZMA_ALONE_HEADER: usize = 13;

/// The parameters of an LZMA stream, from its 5-byte properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LzmaParams {
    pub lc: u8,
    pub lp: u8,
    pub pb: u8,
    /// The dictionary size exactly as declared (S3: never lowered).
    pub dict_raw: u32,
}

impl LzmaParams {
    /// Parses the props byte and the little-endian dictionary size. A
    /// props byte above [`LZMA_PROPS_MAX`] is `Damaged`.
    pub fn from_props(props: [u8; 5]) -> Result<Self, ArchiveError> {
        let byte = props[0];
        if byte > LZMA_PROPS_MAX {
            return Err(damaged(format!("lzma props byte {byte}")));
        }
        let pb = byte / 45;
        let lp = (byte % 45) / 9;
        let lc = byte % 9;
        let dict_raw = u32::from_le_bytes([props[1], props[2], props[3], props[4]]);
        Ok(Self {
            lc,
            lp,
            pb,
            dict_raw,
        })
    }

    /// The properties byte these parameters encode.
    pub fn props_byte(&self) -> u8 {
        (self.pb * 5 + self.lp) * 9 + self.lc
    }

    /// Decoder memory: the raw dictionary, the literal coder
    /// (`0x300 << (lc + lp)` probabilities of 2 bytes) and the fixed state.
    /// Never `min(dict, declared size)` (S3).
    pub fn mem_needed(&self) -> u64 {
        dict_alloc(self.dict_raw) + literal_coder(self.lc + self.lp) + FIXED_STATE
    }
}

/// The parameters of an LZMA2 stream, from its 1-byte dictionary property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Lzma2Params {
    /// The dictionary size exactly as declared.
    pub dict_raw: u32,
}

impl Lzma2Params {
    /// Decodes the dictionary property (`(2 | (b & 1)) << (b / 2 + 11)`,
    /// 40 = `u32::MAX`). Above 40 is `Damaged`.
    pub fn from_prop(prop: u8) -> Result<Self, ArchiveError> {
        if prop > LZMA2_DICT_PROP_MAX {
            return Err(damaged(format!("lzma2 dictionary property {prop}")));
        }
        let dict_raw = if prop == LZMA2_DICT_PROP_MAX {
            u32::MAX
        } else {
            (2 | u32::from(prop & 1)) << (prop / 2 + 11)
        };
        Ok(Self { dict_raw })
    }

    /// Decoder memory with the same formula as [`LzmaParams::mem_needed`],
    /// counting the literal coder at the largest `lc + lp` LZMA2 allows, so
    /// that every chunk's own properties fit the checked value.
    pub fn mem_needed(&self) -> u64 {
        dict_alloc(self.dict_raw) + literal_coder(LZMA2_LC_LP_MAX) + FIXED_STATE
    }
}

/// Which raw decoder [`open_decoder`] builds, with the parameters read from
/// the stream's own header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LzmaKind {
    /// Raw LZMA (zip method 14 after its 4-byte prefix, 7z LZMA coders,
    /// `.lzma` after its 13-byte header).
    Lzma(LzmaParams),
    /// Raw LZMA2 (7z LZMA2 coders, xz blocks).
    Lzma2(Lzma2Params),
}

impl LzmaKind {
    pub fn mem_needed(&self) -> u64 {
        match self {
            Self::Lzma(p) => p.mem_needed(),
            Self::Lzma2(p) => p.mem_needed(),
        }
    }

    pub fn dict_raw(&self) -> u32 {
        match self {
            Self::Lzma(p) => p.dict_raw,
            Self::Lzma2(p) => p.dict_raw,
        }
    }
}

/// `needed > cap` → `NeedsTooMuchMemory`.
pub fn check(needed: u64, cap: u64) -> Result<(), ArchiveError> {
    if needed > cap {
        return Err(ArchiveError::NeedsTooMuchMemory { needed, cap });
    }
    Ok(())
}

/// Parses an "LZMA alone" (`.lzma`) header: the parameters and the
/// declared unpacked size (`u64::MAX` = unknown, `None`). The declared size
/// never takes part in the memory check (S3).
pub fn parse_lzma_alone(
    header: [u8; LZMA_ALONE_HEADER],
) -> Result<(LzmaParams, Option<u64>), ArchiveError> {
    let params = LzmaParams::from_props([header[0], header[1], header[2], header[3], header[4]])?;
    let mut size = [0u8; 8];
    size.copy_from_slice(&header[5..]);
    let size = u64::from_le_bytes(size);
    Ok((params, (size != u64::MAX).then_some(size)))
}

/// The only constructor of `lzma-rust2` readers in the crate (S3).
///
/// Checks `kind.mem_needed()` against `cap` before anything is built or
/// read, then builds the decoder from the same raw dictionary. `declared`
/// is the unpacked size the container states (`None` = unknown, the stream
/// must carry an end marker); it bounds the output (`take(declared + 1)`,
/// so an overrun reaches the caller's `VerifyingReader`) and never the
/// memory. Construction runs inside the panic guard (S1); decoding errors
/// come back as `Damaged`, errors of `src` itself unchanged.
///
/// The caller holds a `MemoryGate` permit for `kind.mem_needed()`.
pub fn open_decoder<'a, R: Read + 'a>(
    kind: LzmaKind,
    declared: Option<u64>,
    cap: u64,
    src: R,
) -> Result<Box<dyn Read + 'a>, ArchiveError> {
    check(kind.mem_needed(), cap)?;
    let src = Tracked::new(src);
    let src_failed = src.flag();
    guarded(move || {
        let decoder: Box<dyn Read + 'a> = match kind {
            LzmaKind::Lzma(params) => Box::new(build_lzma(src, params, declared)?),
            LzmaKind::Lzma2(params) => Box::new(build_lzma2(src, params, cap)?),
        };
        let decoder = DecodeErrors {
            inner: decoder,
            src_failed,
        };
        let bounded: Box<dyn Read + 'a> = match declared {
            Some(n) => Box::new(decoder.take(n.saturating_add(1))),
            None => Box::new(decoder),
        };
        Ok(bounded)
    })
}

/// Builds a raw LZMA reader from checked parameters. The crate may use a
/// smaller window when the declared size is small; it never uses a larger
/// one than `dict_raw`, which is what was checked.
#[allow(clippy::disallowed_methods)] // the single constructor (S3)
fn build_lzma<R: Read>(
    src: Tracked<R>,
    params: LzmaParams,
    declared: Option<u64>,
) -> Result<LzmaReader<Tracked<R>>, ArchiveError> {
    let failed = src.flag();
    record_decoder_dict(params.dict_raw);
    LzmaReader::new_with_props(
        src,
        declared.unwrap_or(u64::MAX),
        params.props_byte(),
        params.dict_raw,
        None,
    )
    .map_err(|e| decode_error(e, &failed, "lzma"))
}

/// Builds a raw LZMA2 reader from checked parameters, with the crate's own
/// memory limit set to the cap as a second line, over a chunk checker
/// that refuses chunk properties outside LZMA2's bounds.
#[allow(clippy::disallowed_methods)] // the single constructor (S3)
fn build_lzma2<R: Read>(
    src: Tracked<R>,
    params: Lzma2Params,
    cap: u64,
) -> Result<Lzma2Reader<Lzma2ChunkCheck<Tracked<R>>>, ArchiveError> {
    record_decoder_dict(params.dict_raw);
    let limit_kb = u32::try_from(cap / KIB).unwrap_or(u32::MAX);
    Lzma2Reader::new_mem_limit(Lzma2ChunkCheck::new(src), params.dict_raw, limit_kb, None).map_err(
        |_| ArchiveError::NeedsTooMuchMemory {
            needed: params.mem_needed(),
            cap,
        },
    )
}

/// Bytes the dictionary buffer takes (the crate rounds up to 16 and
/// allocates at least 4 KiB).
fn dict_alloc(dict: u32) -> u64 {
    (u64::from(dict).max(4096) + 15) & !15
}

/// Bytes of the literal coder: `0x300 << (lc + lp)` 16-bit probabilities.
fn literal_coder(lc_lp: u8) -> u64 {
    2 * (0x300u64 << lc_lp)
}

fn damaged(detail: impl Into<String>) -> ArchiveError {
    ArchiveError::Damaged {
        detail: detail.into(),
    }
}

fn io_damaged(detail: impl Into<String>) -> io::Error {
    damaged(detail).into()
}

/// Maps an error out of a decoder: one that carries an `ArchiveError`
/// (budget, chunk check) stays itself, one from the source stays I/O, and
/// anything else is the decoder rejecting the data: `Damaged`.
fn decode_error(e: io::Error, src_failed: &AtomicBool, what: &str) -> ArchiveError {
    if e.get_ref().is_some_and(|inner| inner.is::<ArchiveError>()) {
        return ArchiveError::from(e);
    }
    if src_failed.load(Ordering::Relaxed) {
        return ArchiveError::Io(e);
    }
    damaged(format!("{what}: {e}"))
}

#[cfg(any(test, feature = "test-util"))]
thread_local! {
    static LAST_DECODER_DICT: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

fn record_decoder_dict(dict: u32) {
    #[cfg(any(test, feature = "test-util"))]
    LAST_DECODER_DICT.with(|d| d.set(Some(dict)));
    #[cfg(not(any(test, feature = "test-util")))]
    let _ = dict;
}

/// The dictionary size the last LZMA/LZMA2 decoder built on this thread
/// was given, and clears it (`None` when none was built since the last
/// call). Proves S3: it equals the value the cap check used.
#[cfg(any(test, feature = "test-util"))]
pub fn last_decoder_dict() -> Option<u32> {
    LAST_DECODER_DICT.with(std::cell::Cell::take)
}

/// A source that counts its bytes and remembers whether it failed, so a
/// decoder error can be told apart from an I/O error of the file.
struct Tracked<R> {
    inner: R,
    count: u64,
    failed: Arc<AtomicBool>,
}

impl<R: Read> Tracked<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            count: 0,
            failed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.failed)
    }

    /// Fills `buf` or reports how much there was before EOF.
    fn read_full(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut filled = 0;
        while filled < buf.len() {
            match self.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(filled)
    }
}

impl<R: Read> Read for Tracked<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.inner.read(buf) {
            Ok(n) => {
                self.count += n as u64;
                Ok(n)
            }
            Err(e) => {
                if e.kind() != io::ErrorKind::Interrupted {
                    self.failed.store(true, Ordering::Relaxed);
                }
                Err(e)
            }
        }
    }
}

/// Maps the errors of a built decoder (see [`decode_error`]).
struct DecodeErrors<D> {
    inner: D,
    src_failed: Arc<AtomicBool>,
}

impl<D: Read> Read for DecodeErrors<D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner
            .read(buf)
            .map_err(|e| decode_error(e, &self.src_failed, "lzma").into())
    }
}

/// Where [`Lzma2ChunkCheck`] is in the LZMA2 chunk sequence.
#[derive(Clone, Copy, Debug)]
enum ChunkState {
    /// The next byte is a control byte.
    Control,
    /// Inside a chunk header: bytes still to come, and whether its last
    /// byte is a properties byte.
    Header {
        left: u8,
        props: bool,
        packed: u32,
        uncompressed: bool,
    },
    /// Inside a chunk's payload.
    Data { left: u32 },
    /// The end marker was seen.
    End,
}

/// A pass-through over a raw LZMA2 stream that follows its chunk headers
/// and refuses, as `Damaged`, any chunk whose new properties fall outside
/// what [`Lzma2Params::mem_needed`] counted (`lc + lp ≤ 4`, props ≤ 224).
struct Lzma2ChunkCheck<R> {
    inner: R,
    state: ChunkState,
    header: [u8; 5],
    header_len: u8,
}

impl<R> Lzma2ChunkCheck<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            state: ChunkState::Control,
            header: [0; 5],
            header_len: 0,
        }
    }

    fn into_inner(self) -> R {
        self.inner
    }

    fn observe(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut i = 0;
        while i < bytes.len() {
            match self.state {
                ChunkState::Control => {
                    let control = bytes[i];
                    i += 1;
                    self.header_len = 0;
                    self.state = match control {
                        0x00 => ChunkState::End,
                        0x01 | 0x02 => ChunkState::Header {
                            left: 2,
                            props: false,
                            packed: 0,
                            uncompressed: true,
                        },
                        0x80..=0xFF => ChunkState::Header {
                            left: if control >= 0xC0 { 5 } else { 4 },
                            props: control >= 0xC0,
                            packed: 0,
                            uncompressed: false,
                        },
                        _ => return Err(io_damaged(format!("lzma2 control byte {control:#04x}"))),
                    };
                }
                ChunkState::Header {
                    left,
                    props,
                    packed,
                    uncompressed,
                } => {
                    let take = usize::from(left).min(bytes.len() - i);
                    for &b in &bytes[i..i + take] {
                        self.header[usize::from(self.header_len)] = b;
                        self.header_len += 1;
                    }
                    i += take;
                    let left = left - take as u8;
                    if left > 0 {
                        self.state = ChunkState::Header {
                            left,
                            props,
                            packed,
                            uncompressed,
                        };
                        continue;
                    }
                    let h = self.header;
                    let payload = if uncompressed {
                        u32::from(u16::from_be_bytes([h[0], h[1]])) + 1
                    } else {
                        u32::from(u16::from_be_bytes([h[2], h[3]])) + 1
                    };
                    if props {
                        let byte = h[4];
                        if byte > LZMA_PROPS_MAX {
                            return Err(io_damaged(format!("lzma2 chunk props {byte}")));
                        }
                        let lc = byte % 9;
                        let lp = (byte % 45) / 9;
                        if lc + lp > LZMA2_LC_LP_MAX {
                            return Err(io_damaged(format!("lzma2 chunk lc {lc} + lp {lp} > 4")));
                        }
                    }
                    self.state = ChunkState::Data { left: payload };
                }
                ChunkState::Data { left } => {
                    let avail = u32::try_from(bytes.len() - i).unwrap_or(u32::MAX);
                    let take = left.min(avail);
                    i += take as usize;
                    self.state = if take == left {
                        ChunkState::Control
                    } else {
                        ChunkState::Data { left: left - take }
                    };
                }
                ChunkState::End => {
                    return Err(io_damaged("lzma2 data after the end marker"));
                }
            }
        }
        Ok(())
    }
}

impl<R: Read> Read for Lzma2ChunkCheck<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.observe(&buf[..n])?;
        Ok(n)
    }
}

// ---------------------------------------------------------------------------
// xz

const XZ_MAGIC: [u8; 6] = [0xFD, b'7', b'z', b'X', b'Z', 0x00];
const XZ_FOOTER_MAGIC: [u8; 2] = *b"YZ";
const XZ_FILTER_DELTA: u64 = 0x03;
const XZ_FILTER_LZMA2: u64 = 0x21;
/// Largest xz variable-length integer (`2^63 − 1`, 9 bytes).
const XZ_VLI_MAX: u64 = u64::MAX / 2;

/// One filter before LZMA2 in an xz block's chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XzPreFilter {
    Bcj { kind: BcjKind, start: u32 },
    Delta { distance: u32 },
}

/// A parsed, CRC-checked xz block header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XzBlockHeader {
    /// Header size in bytes (including the size byte and the CRC32).
    pub size: u64,
    pub compressed: Option<u64>,
    pub uncompressed: Option<u64>,
    /// Filters before LZMA2, in the order the header lists them.
    pub pre_filters: Vec<XzPreFilter>,
    pub lzma2: Lzma2Params,
}

/// Parses one xz block header (`bytes` = the whole header, size byte
/// first). Checks its CRC32, flags, filter chain (BCJ/Delta then LZMA2
/// last) and padding. Does not check memory: the caller does, with
/// [`Lzma2Params::mem_needed`], before building the block's decoder.
pub fn parse_xz_block_header(bytes: &[u8]) -> Result<XzBlockHeader, ArchiveError> {
    let Some(&size_byte) = bytes.first() else {
        return Err(damaged("xz block header empty"));
    };
    if size_byte == 0 {
        return Err(damaged("xz index where a block header was expected"));
    }
    let size = (usize::from(size_byte) + 1) * 4;
    if bytes.len() != size {
        return Err(damaged("xz block header size"));
    }
    let (body, crc) = bytes.split_at(size - 4);
    if crc32fast::hash(body) != u32::from_le_bytes([crc[0], crc[1], crc[2], crc[3]]) {
        return Err(damaged("xz block header crc"));
    }
    let flags = body[1];
    if flags & 0x3C != 0 {
        return Err(damaged("xz block header reserved flags"));
    }
    let filters = usize::from(flags & 0x03) + 1;
    let mut pos = 2;
    let compressed = if flags & 0x40 != 0 {
        let v = vli(body, &mut pos)?;
        if v == 0 {
            return Err(damaged("xz block compressed size 0"));
        }
        Some(v)
    } else {
        None
    };
    let uncompressed = if flags & 0x80 != 0 {
        Some(vli(body, &mut pos)?)
    } else {
        None
    };
    let mut pre_filters = Vec::with_capacity(3);
    let mut lzma2 = None;
    for n in 0..filters {
        let id = vli(body, &mut pos)?;
        let props_len = vli(body, &mut pos)?;
        let props_len = usize::try_from(props_len).map_err(|_| damaged("xz filter props"))?;
        let end = pos
            .checked_add(props_len)
            .filter(|&e| e <= body.len())
            .ok_or_else(|| damaged("xz filter props past the header"))?;
        let props = &body[pos..end];
        pos = end;
        let last = n + 1 == filters;
        match id {
            XZ_FILTER_LZMA2 => {
                if !last {
                    return Err(damaged("xz lzma2 filter not last"));
                }
                let [prop] = props else {
                    return Err(damaged("xz lzma2 props size"));
                };
                lzma2 = Some(Lzma2Params::from_prop(*prop)?);
            }
            _ if last => return Err(damaged("xz last filter is not lzma2")),
            XZ_FILTER_DELTA => {
                let [prop] = props else {
                    return Err(damaged("xz delta props size"));
                };
                pre_filters.push(XzPreFilter::Delta {
                    distance: u32::from(*prop) + 1,
                });
            }
            0x04..=0x0B => {
                let kind = match id {
                    0x04 => BcjKind::X86,
                    0x05 => BcjKind::PowerPc,
                    0x06 => BcjKind::Ia64,
                    0x07 => BcjKind::Arm,
                    0x08 => BcjKind::ArmThumb,
                    0x09 => BcjKind::Sparc,
                    0x0A => BcjKind::Arm64,
                    _ => BcjKind::RiscV,
                };
                let start = match props {
                    [] => 0,
                    [a, b, c, d] => u32::from_le_bytes([*a, *b, *c, *d]),
                    _ => return Err(damaged("xz bcj props size")),
                };
                pre_filters.push(XzPreFilter::Bcj { kind, start });
            }
            other => {
                return Err(ArchiveError::Unsupported {
                    format: format!("xz filter {other:#x}"),
                    need: None,
                })
            }
        }
    }
    if body[pos..].iter().any(|&b| b != 0) {
        return Err(damaged("xz block header padding"));
    }
    let lzma2 = lzma2.ok_or_else(|| damaged("xz block without lzma2"))?;
    Ok(XzBlockHeader {
        size: size as u64,
        compressed,
        uncompressed,
        pre_filters,
        lzma2,
    })
}

/// Reads one xz variable-length integer (minimal encoding, ≤ 9 bytes).
fn vli(bytes: &[u8], pos: &mut usize) -> Result<u64, ArchiveError> {
    let mut value = 0u64;
    for i in 0..9 {
        let Some(&b) = bytes.get(*pos) else {
            return Err(damaged("xz integer past the header"));
        };
        *pos += 1;
        if i > 0 && b == 0 {
            return Err(damaged("xz integer not minimal"));
        }
        value |= u64::from(b & 0x7F) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(damaged("xz integer too long"))
}

/// The integrity check of an xz stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum XzCheck {
    None,
    Crc32,
    Crc64,
    /// SHA-256 and the reserved ids: read past, not verified (no SHA-256
    /// crate in the graph); [`XzWalker::checks_verified`] says so.
    Skipped(usize),
}

impl XzCheck {
    fn from_id(id: u8) -> Self {
        match id {
            0x00 => Self::None,
            0x01 => Self::Crc32,
            0x04 => Self::Crc64,
            // Sizes per the xz format: 0x01–03 → 4, 0x04–06 → 8,
            // 0x07–09 → 16, 0x0A–0C → 32, 0x0D–0F → 64.
            _ => Self::Skipped(4 << ((id - 1) / 3)),
        }
    }

    fn size(self) -> usize {
        match self {
            Self::None => 0,
            Self::Crc32 => 4,
            Self::Crc64 => 8,
            Self::Skipped(n) => n,
        }
    }
}

enum CheckState {
    None,
    Crc32(crc32fast::Hasher),
    Crc64(Crc64),
    Skipped,
}

impl CheckState {
    fn new(check: XzCheck) -> Self {
        match check {
            XzCheck::None => Self::None,
            XzCheck::Crc32 => Self::Crc32(crc32fast::Hasher::new()),
            XzCheck::Crc64 => Self::Crc64(Crc64::new()),
            XzCheck::Skipped(_) => Self::Skipped,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Crc32(h) => h.update(bytes),
            Self::Crc64(h) => h.update(bytes),
            Self::None | Self::Skipped => {}
        }
    }

    /// Whether `stored` matches; `None` when this check is not verified.
    fn matches(self, stored: &[u8]) -> Option<bool> {
        match self {
            Self::None => Some(true),
            Self::Crc32(h) => Some(stored == h.finalize().to_le_bytes()),
            Self::Crc64(h) => Some(stored == h.finish().to_le_bytes()),
            Self::Skipped => None,
        }
    }
}

/// CRC-64/XZ (ECMA-182 polynomial, reflected), the xz default check.
struct Crc64(u64);

const CRC64_TABLE: [u64; 256] = {
    let mut table = [0u64; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u64;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xC96C_5795_D787_0F42
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

impl Crc64 {
    fn new() -> Self {
        Self(!0)
    }

    fn update(&mut self, bytes: &[u8]) {
        let mut c = self.0;
        for &b in bytes {
            c = CRC64_TABLE[((c ^ u64::from(b)) & 0xFF) as usize] ^ (c >> 8);
        }
        self.0 = c;
    }

    fn finish(self) -> u64 {
        !self.0
    }
}

/// The filter chain decoding one xz block. Typed (not boxed) so the
/// source comes back out when the block ends.
enum Stage<R: Read> {
    Lzma2(Box<Lzma2Reader<Lzma2ChunkCheck<Tracked<R>>>>),
    Bcj(BcjReader<Box<Stage<R>>>),
    Delta(DeltaReader<Box<Stage<R>>>),
}

impl<R: Read> Read for Stage<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Lzma2(r) => r.read(buf),
            Self::Bcj(r) => r.read(buf),
            Self::Delta(r) => r.read(buf),
        }
    }
}

impl<R: Read> Stage<R> {
    fn into_source(self) -> Tracked<R> {
        match self {
            Self::Lzma2(r) => (*r).into_inner().into_inner(),
            Self::Bcj(r) => r.into_inner().into_source(),
            Self::Delta(r) => r.into_inner().into_source(),
        }
    }

    fn wrap(self, filter: XzPreFilter) -> Result<Self, ArchiveError> {
        let inner = Box::new(self);
        Ok(match filter {
            XzPreFilter::Delta { distance } => {
                Self::Delta(DeltaReader::new(inner, distance as usize))
            }
            XzPreFilter::Bcj { kind, start } => {
                let start = start as usize;
                Self::Bcj(match kind {
                    BcjKind::X86 => BcjReader::new_x86(inner, start),
                    BcjKind::PowerPc => BcjReader::new_ppc(inner, start),
                    BcjKind::Ia64 => BcjReader::new_ia64(inner, start),
                    BcjKind::Arm => BcjReader::new_arm(inner, start),
                    BcjKind::ArmThumb => BcjReader::new_arm_thumb(inner, start),
                    BcjKind::Sparc => BcjReader::new_sparc(inner, start),
                    BcjKind::Arm64 => BcjReader::new_arm64(inner, start),
                    BcjKind::RiscV => BcjReader::new_riscv(inner, start),
                    // Not an xz filter (7z only).
                    BcjKind::Bcj2 => {
                        return Err(ArchiveError::Unsupported {
                            format: "xz filter bcj2".into(),
                            need: None,
                        })
                    }
                })
            }
        })
    }
}

/// The block being decoded.
struct ActiveBlock<R: Read> {
    chain: Stage<R>,
    header: XzBlockHeader,
    /// Source position where the compressed data starts.
    start: u64,
    out: u64,
    check: CheckState,
    permit: Option<Permit>,
}

/// Bookkeeping of the stream being read, compared with its index.
struct StreamState {
    flags: [u8; 2],
    check: XzCheck,
    blocks: u64,
    /// CRC32 over every block's (unpadded, uncompressed) sizes; the index
    /// records must hash to the same value (no per-block allocation).
    records: crc32fast::Hasher,
}

/// An `.xz` decoder that walks the container itself (S2): every block
/// header of every stream is parsed, and its LZMA2 dictionary checked
/// against the cap (and taken from the memory gate, when one is set),
/// before that block's decoder is built through the same constructor as
/// [`open_decoder`]. Block sizes, checks (CRC32, CRC64), the index, the
/// footer and stream padding are verified; data after the last stream
/// that is neither padding nor a stream is `Damaged`.
pub struct XzWalker<R: Read> {
    src: Option<Tracked<R>>,
    src_failed: Arc<AtomicBool>,
    block: Option<ActiveBlock<R>>,
    stream: Option<StreamState>,
    cap: u64,
    gate: Option<&'static MemoryGate>,
    streams: u64,
    finished: bool,
    failed: bool,
    checks_verified: bool,
}

impl<R: Read> XzWalker<R> {
    /// A walker over `src` (positioned at the stream header) whose block
    /// decoders may need at most `cap` bytes each.
    pub fn new(src: R, cap: u64) -> Self {
        let src = Tracked::new(src);
        Self {
            src_failed: src.flag(),
            src: Some(src),
            block: None,
            stream: None,
            cap,
            gate: None,
            streams: 0,
            finished: false,
            failed: false,
            checks_verified: true,
        }
    }

    /// Takes each block's memory from `gate` while that block decodes.
    pub fn with_gate(mut self, gate: &'static MemoryGate) -> Self {
        self.gate = Some(gate);
        self
    }

    /// False once a stream with a check we do not verify (SHA-256, a
    /// reserved id) was read: its blocks were decoded unchecked.
    pub fn checks_verified(&self) -> bool {
        self.checks_verified
    }

    /// Streams fully read so far.
    pub fn streams(&self) -> u64 {
        self.streams
    }

    fn src(&mut self) -> Result<&mut Tracked<R>, ArchiveError> {
        self.src
            .as_mut()
            .ok_or_else(|| damaged("xz walker lost its source"))
    }

    fn read_exact_src(&mut self, buf: &mut [u8]) -> Result<(), ArchiveError> {
        let n = self.src()?.read_full(buf)?;
        if n < buf.len() {
            return Err(damaged("xz truncated"));
        }
        Ok(())
    }

    fn step(&mut self, buf: &mut [u8]) -> Result<usize, ArchiveError> {
        loop {
            if self.finished {
                return Ok(0);
            }
            if let Some(block) = self.block.as_mut() {
                let n = block
                    .chain
                    .read(buf)
                    .map_err(|e| decode_error(e, &self.src_failed, "xz block"))?;
                if n > 0 {
                    block.out += n as u64;
                    if block.header.uncompressed.is_some_and(|u| block.out > u) {
                        return Err(damaged("xz block larger than its header says"));
                    }
                    block.check.update(&buf[..n]);
                    return Ok(n);
                }
                let block = self.block.take().expect("checked above");
                self.finish_block(block)?;
                continue;
            }
            if self.stream.is_none() {
                if !self.start_stream()? {
                    self.finished = true;
                    return Ok(0);
                }
                continue;
            }
            self.next_block_or_index()?;
        }
    }

    /// Reads a stream header (after any stream padding). `false` at a
    /// clean end after at least one stream.
    fn start_stream(&mut self) -> Result<bool, ArchiveError> {
        let mut header = [0u8; 12];
        if self.streams == 0 {
            self.read_exact_src(&mut header)?;
        } else {
            loop {
                let mut word = [0u8; 4];
                let n = self.src()?.read_full(&mut word)?;
                match n {
                    0 => return Ok(false),
                    4 if word == [0; 4] => continue,
                    4 if word != XZ_MAGIC[..4] => return Err(damaged("xz trailing data")),
                    4 => {
                        header[..4].copy_from_slice(&word);
                        self.read_exact_src(&mut header[4..])?;
                        break;
                    }
                    _ => return Err(damaged("xz stream padding not a multiple of 4")),
                }
            }
        }
        if header[..6] != XZ_MAGIC {
            return Err(damaged(if self.streams == 0 {
                "xz magic"
            } else {
                "xz trailing data"
            }));
        }
        let flags = [header[6], header[7]];
        if crc32fast::hash(&flags).to_le_bytes() != header[8..12] {
            return Err(damaged("xz stream header crc"));
        }
        if flags[0] != 0 || flags[1] & 0xF0 != 0 {
            return Err(damaged("xz stream flags"));
        }
        let check = XzCheck::from_id(flags[1]);
        if matches!(check, XzCheck::Skipped(_)) {
            self.checks_verified = false;
        }
        self.stream = Some(StreamState {
            flags,
            check,
            blocks: 0,
            records: crc32fast::Hasher::new(),
        });
        Ok(true)
    }

    fn next_block_or_index(&mut self) -> Result<(), ArchiveError> {
        let mut first = [0u8; 1];
        self.read_exact_src(&mut first)?;
        if first[0] == 0 {
            return self.read_index_and_footer();
        }
        let size = (usize::from(first[0]) + 1) * 4;
        let mut bytes = vec![0u8; size];
        bytes[0] = first[0];
        self.read_exact_src(&mut bytes[1..])?;
        let header = parse_xz_block_header(&bytes)?;

        // S2: this block's memory is checked (and taken) before its
        // decoder exists.
        let needed = header.lzma2.mem_needed();
        check(needed, self.cap)?;
        let permit = match self.gate {
            Some(gate) => Some(gate.acquire(needed)?),
            None => None,
        };
        let src = self
            .src
            .take()
            .ok_or_else(|| damaged("xz walker lost its source"))?;
        let start = src.count;
        let mut chain = Stage::Lzma2(Box::new(build_lzma2(src, header.lzma2, self.cap)?));
        for &filter in header.pre_filters.iter().rev() {
            chain = chain.wrap(filter)?;
        }
        let check = CheckState::new(self.stream.as_ref().map_or(XzCheck::None, |s| s.check));
        self.block = Some(ActiveBlock {
            chain,
            header,
            start,
            out: 0,
            check,
            permit,
        });
        Ok(())
    }

    fn finish_block(&mut self, block: ActiveBlock<R>) -> Result<(), ArchiveError> {
        let ActiveBlock {
            chain,
            header,
            start,
            out,
            check: state,
            permit,
        } = block;
        let src = chain.into_source();
        let compressed = src.count - start;
        self.src = Some(src);
        drop(permit);
        if header.compressed.is_some_and(|c| c != compressed) {
            return Err(damaged("xz block compressed size"));
        }
        if header.uncompressed.is_some_and(|u| u != out) {
            return Err(damaged("xz block uncompressed size"));
        }
        let pad = ((4 - compressed % 4) % 4) as usize;
        let mut padding = [0u8; 3];
        self.read_exact_src(&mut padding[..pad])?;
        if padding[..pad].iter().any(|&b| b != 0) {
            return Err(damaged("xz block padding"));
        }
        let stream_check = self.stream.as_ref().map_or(XzCheck::None, |s| s.check);
        let mut stored = vec![0u8; stream_check.size()];
        self.read_exact_src(&mut stored)?;
        if state.matches(&stored) == Some(false) {
            return Err(damaged("xz block check"));
        }
        let unpadded = header.size + compressed + stream_check.size() as u64;
        if unpadded > XZ_VLI_MAX || out > XZ_VLI_MAX {
            return Err(damaged("xz block too large"));
        }
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| damaged("xz block outside a stream"))?;
        stream.blocks += 1;
        stream.records.update(&unpadded.to_le_bytes());
        stream.records.update(&out.to_le_bytes());
        Ok(())
    }

    /// Reads one byte of the index into `crc`.
    fn index_byte(&mut self, crc: &mut crc32fast::Hasher) -> Result<u8, ArchiveError> {
        let mut b = [0u8; 1];
        self.read_exact_src(&mut b)?;
        crc.update(&b);
        Ok(b[0])
    }

    fn index_vli(
        &mut self,
        crc: &mut crc32fast::Hasher,
        len: &mut u64,
    ) -> Result<u64, ArchiveError> {
        let mut value = 0u64;
        for i in 0..9 {
            let b = self.index_byte(crc)?;
            *len += 1;
            if i > 0 && b == 0 {
                return Err(damaged("xz index integer not minimal"));
            }
            value |= u64::from(b & 0x7F) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(damaged("xz index integer too long"))
    }

    /// The index (its indicator byte already read) and the stream footer.
    fn read_index_and_footer(&mut self) -> Result<(), ArchiveError> {
        let stream = self
            .stream
            .take()
            .ok_or_else(|| damaged("xz index outside a stream"))?;
        let mut crc = crc32fast::Hasher::new();
        crc.update(&[0]);
        let mut len = 1u64;
        let count = self.index_vli(&mut crc, &mut len)?;
        if count != stream.blocks {
            return Err(damaged("xz index record count"));
        }
        let mut records = crc32fast::Hasher::new();
        for _ in 0..count {
            let unpadded = self.index_vli(&mut crc, &mut len)?;
            let uncompressed = self.index_vli(&mut crc, &mut len)?;
            records.update(&unpadded.to_le_bytes());
            records.update(&uncompressed.to_le_bytes());
        }
        if records.finalize() != stream.records.finalize() {
            return Err(damaged("xz index records"));
        }
        while !len.is_multiple_of(4) {
            if self.index_byte(&mut crc)? != 0 {
                return Err(damaged("xz index padding"));
            }
            len += 1;
        }
        let mut stored = [0u8; 4];
        self.read_exact_src(&mut stored)?;
        if stored != crc.finalize().to_le_bytes() {
            return Err(damaged("xz index crc"));
        }
        let index_size = len + 4;

        let mut footer = [0u8; 12];
        self.read_exact_src(&mut footer)?;
        if footer[10..] != XZ_FOOTER_MAGIC {
            return Err(damaged("xz footer magic"));
        }
        if crc32fast::hash(&footer[4..10]).to_le_bytes() != footer[..4] {
            return Err(damaged("xz footer crc"));
        }
        let backward = u32::from_le_bytes([footer[4], footer[5], footer[6], footer[7]]);
        if (u64::from(backward) + 1) * 4 != index_size {
            return Err(damaged("xz footer backward size"));
        }
        if footer[8..10] != stream.flags {
            return Err(damaged("xz footer flags"));
        }
        self.streams += 1;
        Ok(())
    }
}

impl<R: Read> Read for XzWalker<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io_damaged("xz stream already failed"));
        }
        if buf.is_empty() {
            return Ok(0);
        }
        self.step(buf).map_err(|e| {
            self.failed = true;
            // A dropped block gives its gate permit back at once.
            self.block = None;
            e.into()
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};
    use std::num::NonZeroU64;

    use lzma_rust2::{CheckType, FilterType, LzmaOptions, LzmaWriter, XzOptions, XzWriter};

    use super::*;
    use crate::archive::testing::{lzma2_stored, lzma_props, xz_stream, XzBlock};
    use crate::archive::{DECODER_MEM_CAP, MIB};

    /// 1.5 GiB, the S3 example.
    const HUGE_DICT: u32 = 3 << 29;
    /// LZMA2 property for 1.5 GiB: (2 | 1) << (37 / 2 + 11).
    const HUGE_DICT_PROP: u8 = 37;

    fn sample(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (i * 7 % 251) as u8 ^ (i / 300) as u8)
            .collect()
    }

    fn lzma_raw(data: &[u8], dict: u32) -> (LzmaParams, Vec<u8>) {
        let mut options = LzmaOptions::with_preset(1);
        options.dict_size = dict;
        let mut w = LzmaWriter::new_no_header(Vec::new(), &options, true).unwrap();
        w.write_all(data).unwrap();
        let out = w.finish().unwrap();
        let params = LzmaParams {
            lc: options.lc as u8,
            lp: options.lp as u8,
            pb: options.pb as u8,
            dict_raw: dict,
        };
        (params, out)
    }

    fn archive_err(e: io::Error) -> ArchiveError {
        ArchiveError::from(e)
    }

    fn read_all(r: &mut dyn Read) -> (Vec<u8>, Option<ArchiveError>) {
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match r.read(&mut buf) {
                Ok(0) => return (out, None),
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(e) => return (out, Some(archive_err(e))),
            }
        }
    }

    /// A source that fails the test if anything reads it.
    struct Untouched;
    impl Read for Untouched {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("the source was read although the decoder must not exist");
        }
    }

    // ----- parameters -----

    #[test]
    fn lzma_params_from_props() {
        let p = LzmaParams::from_props(lzma_props(3, 0, 2, 1 << 20)).unwrap();
        assert_eq!((p.lc, p.lp, p.pb, p.dict_raw), (3, 0, 2, 1 << 20));
        assert_eq!(p.props_byte(), 93);
        let p = LzmaParams::from_props(lzma_props(8, 4, 4, 0)).unwrap();
        assert_eq!(p.props_byte(), LZMA_PROPS_MAX);
        let err = LzmaParams::from_props([225, 0, 0, 1, 0]).unwrap_err();
        assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
    }

    #[test]
    fn lzma2_dict_prop_decoding() {
        assert_eq!(Lzma2Params::from_prop(0).unwrap().dict_raw, 4096);
        assert_eq!(Lzma2Params::from_prop(1).unwrap().dict_raw, 6144);
        assert_eq!(
            Lzma2Params::from_prop(24).unwrap().dict_raw,
            16 * MIB as u32
        );
        assert_eq!(
            Lzma2Params::from_prop(HUGE_DICT_PROP).unwrap().dict_raw,
            HUGE_DICT
        );
        assert_eq!(Lzma2Params::from_prop(40).unwrap().dict_raw, u32::MAX);
        assert!(matches!(
            Lzma2Params::from_prop(41),
            Err(ArchiveError::Damaged { .. })
        ));
    }

    #[test]
    fn lzma_lc_lp_term_counted() {
        let small = LzmaParams {
            lc: 0,
            lp: 0,
            pb: 0,
            dict_raw: 4096,
        };
        let wide = LzmaParams {
            lc: 8,
            lp: 4,
            ..small
        };
        assert_eq!(small.mem_needed(), 4096 + 2 * 0x300 + FIXED_STATE);
        // 2 bytes × 0x300 << 12 = 6 MiB of literal probabilities.
        assert_eq!(wide.mem_needed(), 4096 + 6 * MIB + FIXED_STATE);
        let cap = 4 * MIB;
        check(small.mem_needed(), cap).unwrap();
        let err = open_decoder(LzmaKind::Lzma(wide), None, cap, Untouched)
            .err()
            .unwrap();
        assert!(
            matches!(err, ArchiveError::NeedsTooMuchMemory { needed, cap: c } if needed == wide.mem_needed() && c == cap),
            "{err:?}"
        );
    }

    #[test]
    fn lzma2_mem_counts_widest_literal_coder() {
        let p = Lzma2Params { dict_raw: 1 << 20 };
        assert_eq!(p.mem_needed(), (1 << 20) + 2 * (0x300 << 4) + FIXED_STATE);
        let lzma = LzmaParams {
            lc: 4,
            lp: 0,
            pb: 2,
            dict_raw: 1 << 20,
        };
        assert_eq!(p.mem_needed(), lzma.mem_needed(), "one formula for both");
    }

    // ----- S3 -----

    #[test]
    fn tiny_declared_size_huge_dict_refused() {
        let params = LzmaParams {
            lc: 3,
            lp: 0,
            pb: 2,
            dict_raw: HUGE_DICT,
        };
        let _ = last_decoder_dict();
        let err = open_decoder(
            LzmaKind::Lzma(params),
            Some(1024),
            DECODER_MEM_CAP,
            Untouched,
        )
        .err()
        .unwrap();
        match err {
            ArchiveError::NeedsTooMuchMemory { needed, cap } => {
                assert!(
                    needed > u64::from(HUGE_DICT),
                    "the raw dict is counted: {needed}"
                );
                assert_eq!(cap, DECODER_MEM_CAP);
            }
            other => panic!("expected NeedsTooMuchMemory, got {other:?}"),
        }
        assert_eq!(last_decoder_dict(), None, "no decoder was built");
    }

    #[test]
    fn cap_check_and_decoder_use_same_dict() {
        let data = sample(10 * 1024);
        let (params, raw) = lzma_raw(&data, 8 * MIB as u32);
        let _ = last_decoder_dict();
        // A declared size far below the dictionary changes neither the
        // check nor the dictionary handed to the decoder.
        let kind = LzmaKind::Lzma(params);
        check(kind.mem_needed(), DECODER_MEM_CAP).unwrap();
        let mut r = open_decoder(
            kind,
            Some(data.len() as u64),
            DECODER_MEM_CAP,
            raw.as_slice(),
        )
        .unwrap();
        assert_eq!(last_decoder_dict(), Some(params.dict_raw));
        assert_eq!(last_decoder_dict(), None, "the hook is cleared on read");
        let (out, err) = read_all(&mut r);
        assert!(err.is_none(), "{err:?}");
        assert_eq!(out, data);

        // Same for LZMA2.
        let p2 = Lzma2Params::from_prop(20).unwrap();
        let stored = lzma2_stored(&data);
        let mut r = open_decoder(
            LzmaKind::Lzma2(p2),
            None,
            DECODER_MEM_CAP,
            stored.as_slice(),
        )
        .unwrap();
        assert_eq!(last_decoder_dict(), Some(p2.dict_raw));
        let (out, err) = read_all(&mut r);
        assert!(err.is_none(), "{err:?}");
        assert_eq!(out, data);
    }

    #[test]
    fn lzma_alone_declared_size_ignored_for_memory() {
        let mut header = [0u8; LZMA_ALONE_HEADER];
        header[..5].copy_from_slice(&lzma_props(3, 0, 2, HUGE_DICT));
        header[5..].copy_from_slice(&16u64.to_le_bytes());
        let (params, declared) = parse_lzma_alone(header).unwrap();
        assert_eq!(declared, Some(16));
        assert_eq!(params.dict_raw, HUGE_DICT);
        assert!(params.mem_needed() > u64::from(HUGE_DICT));
        let _ = last_decoder_dict();
        let err = open_decoder(LzmaKind::Lzma(params), declared, DECODER_MEM_CAP, Untouched)
            .err()
            .unwrap();
        assert!(
            matches!(err, ArchiveError::NeedsTooMuchMemory { .. }),
            "{err:?}"
        );
        assert_eq!(last_decoder_dict(), None);
    }

    #[test]
    fn lzma_alone_unknown_size_uses_dict() {
        let data = sample(40 * 1024);
        let mut options = LzmaOptions::with_preset(1);
        options.dict_size = 64 * 1024;
        let mut w = LzmaWriter::new_use_header(Vec::new(), &options, None).unwrap();
        w.write_all(&data).unwrap();
        let file = w.finish().unwrap();
        let mut header = [0u8; LZMA_ALONE_HEADER];
        header.copy_from_slice(&file[..LZMA_ALONE_HEADER]);
        let (params, declared) = parse_lzma_alone(header).unwrap();
        assert_eq!(declared, None, "u64::MAX means unknown");
        assert_eq!(params.dict_raw, 64 * 1024);
        assert_eq!(
            LzmaKind::Lzma(params).mem_needed(),
            dict_alloc(64 * 1024) + literal_coder(params.lc + params.lp) + FIXED_STATE
        );
        let _ = last_decoder_dict();
        let mut r = open_decoder(
            LzmaKind::Lzma(params),
            declared,
            DECODER_MEM_CAP,
            &file[LZMA_ALONE_HEADER..],
        )
        .unwrap();
        assert_eq!(last_decoder_dict(), Some(64 * 1024));
        let (out, err) = read_all(&mut r);
        assert!(err.is_none(), "{err:?}");
        assert_eq!(out, data);

        // An unknown size with a huge dictionary is refused as well.
        header[1..5].copy_from_slice(&HUGE_DICT.to_le_bytes());
        let (params, declared) = parse_lzma_alone(header).unwrap();
        let err = open_decoder(LzmaKind::Lzma(params), declared, DECODER_MEM_CAP, Untouched)
            .err()
            .unwrap();
        assert!(
            matches!(err, ArchiveError::NeedsTooMuchMemory { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn lzma_round_trip_small() {
        let data = b"Coverly LZMA round trip ".repeat(200);
        let (params, raw) = lzma_raw(&data, 1 << 16);
        let mut r = open_decoder(
            LzmaKind::Lzma(params),
            Some(data.len() as u64),
            DECODER_MEM_CAP,
            raw.as_slice(),
        )
        .unwrap();
        let (out, err) = read_all(&mut r);
        assert!(err.is_none(), "{err:?}");
        assert_eq!(out, data);
    }

    #[test]
    fn lzma_garbage_is_damaged_not_io() {
        let params = LzmaParams {
            lc: 3,
            lp: 0,
            pb: 2,
            dict_raw: 1 << 16,
        };
        // The range coder's first byte must be 0.
        let garbage = [0xFFu8; 64];
        let result = open_decoder(
            LzmaKind::Lzma(params),
            Some(100),
            DECODER_MEM_CAP,
            &garbage[..],
        )
        .and_then(|mut r| match read_all(&mut r) {
            (_, Some(e)) => Err(e),
            (_, None) => Ok(()),
        });
        assert!(
            matches!(result, Err(ArchiveError::Damaged { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn source_io_error_stays_io() {
        struct Failing;
        impl Read for Failing {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "nope"))
            }
        }
        let p = Lzma2Params::from_prop(10).unwrap();
        let result =
            open_decoder(LzmaKind::Lzma2(p), None, DECODER_MEM_CAP, Failing).and_then(|mut r| {
                match read_all(&mut r) {
                    (_, Some(e)) => Err(e),
                    (_, None) => Ok(()),
                }
            });
        match result {
            Err(ArchiveError::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::PermissionDenied),
            other => panic!("expected Io, got {other:?}"),
        }
    }

    #[test]
    fn lzma2_chunk_props_outside_bounds_damaged() {
        // An LZMA chunk with dict reset + new props (control 0xE0) whose
        // props byte says lc 4 + lp 1.
        let props = (2 * 5 + 1) * 9 + 4;
        let stream = [0xE0u8, 0x00, 0x00, 0x00, 0x04, props, 0, 0, 0, 0, 0, 0x00];
        let p = Lzma2Params::from_prop(10).unwrap();
        let mut r = open_decoder(LzmaKind::Lzma2(p), None, DECODER_MEM_CAP, &stream[..]).unwrap();
        let (_, err) = read_all(&mut r);
        match err {
            Some(ArchiveError::Damaged { detail }) => assert!(detail.contains("lc"), "{detail}"),
            other => panic!("expected Damaged, got {other:?}"),
        }
    }

    #[test]
    fn declared_size_bounds_output() {
        let data = sample(4096);
        let p = Lzma2Params::from_prop(10).unwrap();
        let stored = lzma2_stored(&data);
        let mut r = open_decoder(
            LzmaKind::Lzma2(p),
            Some(1000),
            DECODER_MEM_CAP,
            stored.as_slice(),
        )
        .unwrap();
        let (out, err) = read_all(&mut r);
        assert!(err.is_none(), "{err:?}");
        assert_eq!(
            out.len(),
            1001,
            "declared + 1, so an overrun reaches the verifier"
        );
    }

    // ----- xz -----

    fn small_block(data: &[u8]) -> XzBlock {
        XzBlock {
            data: data.to_vec(),
            dict_prop: 16,
        }
    }

    #[test]
    fn crc64_matches_xz_check_value() {
        let mut c = Crc64::new();
        c.update(b"123456789");
        assert_eq!(c.finish(), 0x995D_C9BB_DF19_39FA);
    }

    #[test]
    fn xz_round_trip_two_blocks() {
        let data = sample(600 * 1024);
        let mut options = XzOptions::with_preset(1);
        options.lzma_options.dict_size = 64 * 1024;
        options.set_block_size(NonZeroU64::new(256 * 1024));
        options.set_check_sum_type(CheckType::Crc64);
        options.prepend_pre_filter(FilterType::BcjX86, 0);
        let mut w = XzWriter::new(Vec::new(), options).unwrap();
        w.write_all(&data).unwrap();
        let file = w.finish().unwrap();

        let gate = MemoryGate::leaked(DECODER_MEM_CAP);
        let mut r = XzWalker::new(file.as_slice(), DECODER_MEM_CAP).with_gate(gate);
        let (out, err) = read_all(&mut r);
        assert!(err.is_none(), "{err:?}");
        assert_eq!(out, data);
        assert!(r.checks_verified());
        assert_eq!(r.streams(), 1);
        assert_eq!(gate.in_use(), 0, "block permits are released");
    }

    #[test]
    fn xz_stored_fixture_multi_stream_with_padding() {
        let a = sample(70_000);
        let b = sample(10);
        let mut file = xz_stream(&[small_block(&a), small_block(&b)]);
        file.extend_from_slice(&[0; 8]);
        file.extend_from_slice(&xz_stream(&[small_block(b"second stream")]));
        file.extend_from_slice(&[0; 4]);
        let mut r = XzWalker::new(file.as_slice(), DECODER_MEM_CAP);
        let (out, err) = read_all(&mut r);
        assert!(err.is_none(), "{err:?}");
        let mut want = a.clone();
        want.extend_from_slice(&b);
        want.extend_from_slice(b"second stream");
        assert_eq!(out, want);
        assert_eq!(r.streams(), 2);
    }

    #[test]
    fn xz_second_block_huge_dict_refused() {
        let first = sample(5000);
        let file = xz_stream(&[
            small_block(&first),
            XzBlock {
                data: b"never decoded".to_vec(),
                dict_prop: HUGE_DICT_PROP,
            },
        ]);
        let gate = MemoryGate::leaked(512 * MIB);
        let mut r = XzWalker::new(file.as_slice(), DECODER_MEM_CAP).with_gate(gate);
        let _ = last_decoder_dict();
        let (out, err) = read_all(&mut r);
        assert_eq!(out, first, "the first block decodes");
        assert!(
            matches!(err, Some(ArchiveError::NeedsTooMuchMemory { needed, .. }) if needed > u64::from(HUGE_DICT)),
            "{err:?}"
        );
        assert_eq!(
            last_decoder_dict(),
            Some(Lzma2Params::from_prop(16).unwrap().dict_raw),
            "the second block's decoder was never built"
        );
        assert_eq!(gate.in_use(), 0);
        // The walker stays failed.
        let mut buf = [0u8; 16];
        assert!(r.read(&mut buf).is_err());
    }

    #[test]
    fn xz_second_stream_huge_dict_refused() {
        let first = sample(3000);
        let mut file = xz_stream(&[small_block(&first)]);
        file.extend_from_slice(&xz_stream(&[XzBlock {
            data: b"never decoded".to_vec(),
            dict_prop: HUGE_DICT_PROP,
        }]));
        let mut r = XzWalker::new(file.as_slice(), DECODER_MEM_CAP);
        let (out, err) = read_all(&mut r);
        assert_eq!(out, first);
        assert!(
            matches!(err, Some(ArchiveError::NeedsTooMuchMemory { .. })),
            "{err:?}"
        );
    }

    #[test]
    fn xz_gate_smaller_than_block_refused() {
        let file = xz_stream(&[small_block(b"abc")]);
        let gate = MemoryGate::leaked(64 * 1024);
        let mut r = XzWalker::new(file.as_slice(), DECODER_MEM_CAP).with_gate(gate);
        let (_, err) = read_all(&mut r);
        assert!(
            matches!(err, Some(ArchiveError::NeedsTooMuchMemory { .. })),
            "{err:?}"
        );
    }

    #[test]
    fn xz_sha256_check_decodes_unverified() {
        let data = sample(2000);
        let mut options = XzOptions::with_preset(0);
        options.set_check_sum_type(CheckType::Sha256);
        let mut w = XzWriter::new(Vec::new(), options).unwrap();
        w.write_all(&data).unwrap();
        let file = w.finish().unwrap();
        let mut r = XzWalker::new(file.as_slice(), DECODER_MEM_CAP);
        let (out, err) = read_all(&mut r);
        assert!(err.is_none(), "{err:?}");
        assert_eq!(out, data);
        assert!(!r.checks_verified());
    }

    /// Offset of the first block header in a fixture (after the 12-byte
    /// stream header).
    const FIRST_BLOCK: usize = 12;

    fn expect_damaged(file: &[u8], what: &str) {
        let mut r = XzWalker::new(Cursor::new(file), DECODER_MEM_CAP);
        let (_, err) = read_all(&mut r);
        match err {
            Some(ArchiveError::Damaged { detail }) => {
                assert!(detail.contains(what), "expected {what:?} in {detail:?}")
            }
            other => panic!("expected Damaged({what}), got {other:?}"),
        }
    }

    #[test]
    fn xz_corruptions_are_damaged() {
        let data = sample(3000);
        let good = xz_stream(&[small_block(&data)]);

        let mut bad = good.clone();
        bad[0] = b'X';
        expect_damaged(&bad, "magic");

        let mut bad = good.clone();
        bad[FIRST_BLOCK + 2] ^= 0x01; // inside the block header
        expect_damaged(&bad, "block header crc");

        let mut bad = good.clone();
        let payload = FIRST_BLOCK + 16;
        bad[payload + 10] ^= 0xFF; // a stored byte: the CRC32 check catches it
        expect_damaged(&bad, "block check");

        let mut bad = good.clone();
        let len = bad.len();
        bad[len - 1] = b'Q';
        expect_damaged(&bad, "footer magic");

        let mut bad = good.clone();
        bad.truncate(bad.len() - 5);
        expect_damaged(&bad, "truncated");

        let mut bad = good.clone();
        bad.extend_from_slice(b"junk");
        expect_damaged(&bad, "trailing data");

        let mut bad = good.clone();
        bad.extend_from_slice(&[0, 0]);
        expect_damaged(&bad, "padding");
    }

    #[test]
    fn xz_index_must_match_blocks() {
        let data = sample(100);
        let mut file = xz_stream(&[small_block(&data)]);
        // Index: 00 <count=1> <unpadded> <uncompressed=100 → 0x64> pad crc.
        // Find the record's uncompressed size byte (0x64) right after the
        // indicator, count and unpadded size, and change it.
        let footer = file.len() - 12;
        let backward = u32::from_le_bytes(file[footer + 4..footer + 8].try_into().unwrap());
        let index = footer - (backward as usize + 1) * 4;
        assert_eq!(file[index], 0, "index indicator");
        assert_eq!(file[index + 1], 1, "one record");
        let uncompressed = index + 3;
        assert_eq!(file[uncompressed], 100);
        file[uncompressed] = 99;
        // Fix the index CRC so only the record comparison fails.
        let crc_at = footer - 4;
        let crc = crc32fast::hash(&file[index..crc_at]);
        file[crc_at..footer].copy_from_slice(&crc.to_le_bytes());
        expect_damaged(&file, "index records");
    }

    #[test]
    fn xz_block_header_parser() {
        // size byte, flags (1 filter, both sizes), sizes, LZMA2 prop 16, pad, crc
        let mut h = vec![0x02u8, 0xC0, 0x05, 0x64, 0x21, 0x01, 16, 0x00];
        let crc = crc32fast::hash(&h);
        h.extend_from_slice(&crc.to_le_bytes());
        let parsed = parse_xz_block_header(&h).unwrap();
        assert_eq!(parsed.size, 12);
        assert_eq!(parsed.compressed, Some(5));
        assert_eq!(parsed.uncompressed, Some(100));
        assert!(parsed.pre_filters.is_empty());
        assert_eq!(parsed.lzma2, Lzma2Params::from_prop(16).unwrap());

        // An unknown filter id before LZMA2 is unsupported, not damaged.
        let mut h = vec![0x02u8, 0x01, 0x30, 0x00, 0x21, 0x01, 16, 0x00];
        let crc = crc32fast::hash(&h);
        h.extend_from_slice(&crc.to_le_bytes());
        assert!(matches!(
            parse_xz_block_header(&h),
            Err(ArchiveError::Unsupported { .. })
        ));

        // LZMA2 not last.
        let mut h = vec![0x02u8, 0x01, 0x21, 0x01, 16, 0x04, 0x00, 0x00];
        let crc = crc32fast::hash(&h);
        h.extend_from_slice(&crc.to_le_bytes());
        assert!(matches!(
            parse_xz_block_header(&h),
            Err(ArchiveError::Damaged { .. })
        ));

        // Non-minimal integer.
        let mut h = vec![0x02u8, 0x40, 0x85, 0x00, 0x21, 0x01, 16, 0x00];
        let crc = crc32fast::hash(&h);
        h.extend_from_slice(&crc.to_le_bytes());
        assert!(matches!(
            parse_xz_block_header(&h),
            Err(ArchiveError::Damaged { .. })
        ));
    }

    #[test]
    fn xz_walker_survives_garbled_fixtures() {
        // A quick sweep of single-byte corruptions: every one ends in Ok or
        // Err, never in a panic or a hang. The full S1 suite is plan Task 17.
        let good = xz_stream(&[small_block(&sample(300)), small_block(b"tail")]);
        for i in 0..good.len() {
            for flip in [0x01u8, 0x80, 0xFF] {
                let mut bad = good.clone();
                bad[i] ^= flip;
                let result = guarded(|| {
                    let mut r = XzWalker::new(bad.as_slice(), DECODER_MEM_CAP);
                    let _ = read_all(&mut r);
                    Ok(())
                });
                assert!(result.is_ok(), "byte {i} ^ {flip:#x} panicked");
            }
        }
    }
}
