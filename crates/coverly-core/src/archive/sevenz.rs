//! 7z backend of the archive reader: the bounded header walker and the
//! decoder over `sevenz-rust2` (spec `2026-10-23-organize-archives-and-discs.md` §4.3, §13 S4, S5).
//!
//! [`walk`] is the 7z integrity listing. It parses the whole header itself,
//! iteratively and with every count, size and offset bounded, **before**
//! any third-party code sees the file (S5: fail closed):
//!
//! * The start header CRC is checked, `32 + next_header_offset +
//!   next_header_size ≤ len` is computed with checked arithmetic, the next
//!   header is at most `max_header` bytes and its CRC must match.
//! * A `kEncodedHeader` (S4) must describe exactly one folder whose coders
//!   are one LZMA/LZMA2 coder reading the packed stream plus optional
//!   BCJ/Delta filters: AES is [`ArchiveError::Encrypted`], any other coder
//!   `Damaged`. Its dictionary is checked with the S3 formula, its unpack
//!   size is at most `max_header`, its pack range lies before the header,
//!   and its CRC is required. It is decoded through
//!   [`super::lzma::open_decoder`] with a [`MemoryGate`] permit, verified
//!   by size and CRC, and the result must start with `kHeader`: a second
//!   `kEncodedHeader` is `Damaged`, so there is no recursion at all.
//! * The `kHeader` parse caps folders and files at `max_entries` and coders
//!   per folder at [`MAX_CODERS`]; bind pairs must form one tree, substream
//!   sizes must sum to the folder sizes, every property's size must lie
//!   inside the header, there must be exactly `numFiles` UTF-16 names, bit
//!   vectors are sized to their counts, and the parse must consume the
//!   header to its last byte, ending on `kEnd`. Unknown properties,
//!   `kArchiveProperties`, `kAdditionalStreamsInfo`, the "external" flag
//!   and anti-items are `Damaged`.
//! * A declared count larger than the bytes left could hold is `Damaged`
//!   (a lie); one that could be real but is over `max_entries` is
//!   [`ArchiveError::TooManyEntries`]. Declared sizes over the caps are
//!   [`ArchiveError::TooLarge`]. Vectors grow by push from parsed bytes,
//!   never `with_capacity` from a declared count.
//! * Every main folder's coder memory (LZMA/LZMA2 dictionary, PPMd model)
//!   is summed and checked against `decoder_mem_cap` (S2).
//!
//! The whole walk runs inside the panic guard (S1). Decoding of the main
//! folders is the second part of this module: [`SevenZReader`] copies the
//! header bytes into memory, walks that copy again and hands the same copy
//! to `sevenz-rust2`, and decodes only once the crate's own view equals
//! the walk (S5, plan Task 10).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bzip2::read::MultiBzDecoder;
use flate2::read::DeflateDecoder;
use lzma_rust2::filter::bcj::BcjReader;
use lzma_rust2::filter::delta::DeltaReader;
use ppmd_rust::Ppmd7Decoder;
use sevenz_rust2::BlockDecoder;

use super::budget::{Budget, ChargedRead, MemoryGate, Permit};
use super::guard::{guarded, GuardedRead, Poison};
use super::lzma::{self, open_decoder, Lzma2Params, LzmaKind, LzmaParams};
use super::names::{decode_utf16_name, RelPath, UnsafeWhy};
use super::verify::VerifyingReader;
use super::{
    ArchiveError, ArchiveReader, BcjKind, BudgetKind, Entry, EntryKind, Format, Limits, Method,
    Tool, KIB, MIB,
};

/// The 7z signature.
pub const SIGNATURE: [u8; 6] = [b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C];
/// Length of the start (signature) header.
pub const START_HEADER_LEN: u64 = 32;
/// Most coders one folder may have (7-Zip writes at most 4: BCJ2 + 3).
pub const MAX_CODERS: usize = 8;
/// Most in or out streams one complex coder may have (BCJ2 has 4 in).
pub const MAX_CODER_STREAMS: u64 = 8;
/// Longest coder id (7-Zip refuses more than 8 bytes).
const MAX_CODER_ID: usize = 8;

// Property ids (7-Zip `7zFormat.txt`).
const K_END: u64 = 0x00;
const K_HEADER: u64 = 0x01;
const K_ARCHIVE_PROPERTIES: u64 = 0x02;
const K_ADDITIONAL_STREAMS_INFO: u64 = 0x03;
const K_MAIN_STREAMS_INFO: u64 = 0x04;
const K_FILES_INFO: u64 = 0x05;
const K_PACK_INFO: u64 = 0x06;
const K_UNPACK_INFO: u64 = 0x07;
const K_SUBSTREAMS_INFO: u64 = 0x08;
const K_SIZE: u64 = 0x09;
const K_CRC: u64 = 0x0A;
const K_FOLDER: u64 = 0x0B;
const K_CODERS_UNPACK_SIZE: u64 = 0x0C;
const K_NUM_UNPACK_STREAM: u64 = 0x0D;
const K_EMPTY_STREAM: u64 = 0x0E;
const K_EMPTY_FILE: u64 = 0x0F;
const K_ANTI: u64 = 0x10;
const K_NAME: u64 = 0x11;
const K_CTIME: u64 = 0x12;
const K_ATIME: u64 = 0x13;
const K_MTIME: u64 = 0x14;
const K_WIN_ATTRIBUTES: u64 = 0x15;
const K_ENCODED_HEADER: u64 = 0x17;
const K_DUMMY: u64 = 0x19;

/// Windows `FILE_ATTRIBUTE_REPARSE_POINT`.
const ATTR_REPARSE_POINT: u32 = 0x400;
/// p7zip / 7-Zip: the high 16 bits hold a Unix `st_mode`.
const ATTR_UNIX_EXTENSION: u32 = 0x8000;
const S_IFMT: u32 = 0o170_000;
const S_IFLNK: u32 = 0o120_000;

/// Decoder memory of the coders whose state does not depend on their
/// properties (filter buffers, the Deflate window, the bzip2 block).
const FILTER_MEM: u64 = 64 * KIB;
const DEFLATE_MEM: u64 = 256 * KIB;
const BZIP2_MEM: u64 = 8 * MIB;
/// PPMd7's smallest model (`PPMD7_MIN_MEM_SIZE`).
const PPMD7_MIN_MEM: u32 = 1 << 11;

/// What one coder does, from its id and properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoderKind {
    Copy,
    Lzma(LzmaParams),
    Lzma2(Lzma2Params),
    Ppmd {
        order: u8,
        mem: u32,
    },
    Bzip2,
    Deflate,
    Deflate64,
    /// A branch converter; `start` is the optional 4-byte start offset.
    Bcj {
        kind: BcjKind,
        start: u32,
    },
    Delta {
        distance: u16,
    },
    Aes,
    /// A coder we name but do not decode (zstd, Rar5 coders, …).
    Other,
}

impl CoderKind {
    fn is_filter(self) -> bool {
        matches!(self, Self::Bcj { .. } | Self::Delta { .. } | Self::Aes)
    }
}

/// One coder of a folder, validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoderInfo {
    pub id: Vec<u8>,
    pub props: Vec<u8>,
    pub num_in: u64,
    pub num_out: u64,
    pub kind: CoderKind,
    pub method: Method,
    /// Decoder memory with the S3 formula (raw dictionary, never lowered).
    pub mem_needed: u64,
}

/// One folder (solid block), validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderInfo {
    pub coders: Vec<CoderInfo>,
    /// `(in_index, out_index)`: in stream `in_index` reads out stream
    /// `out_index`.
    pub bind_pairs: Vec<(u64, u64)>,
    /// The in-stream indices fed by packed streams, in pack order.
    pub packed_streams: Vec<u64>,
    /// One size per coder out stream.
    pub unpack_sizes: Vec<u64>,
    /// The out stream that is the folder's output.
    pub main_out: u64,
    /// Size of the folder's output.
    pub unpack_size: u64,
    /// The folder CRC (unpack info), when stored.
    pub crc: Option<u32>,
    /// Absolute file offset of the folder's first packed stream.
    pub pack_pos: u64,
    /// Sum of the folder's packed stream sizes.
    pub pack_size: u64,
    /// Size of each of the folder's packed streams.
    pub pack_sizes: Vec<u64>,
    /// Number of files stored in the folder.
    pub num_substreams: u64,
    /// Sum of the coders' `mem_needed` (one gate request, spec §4.4).
    pub mem_needed: u64,
    /// Whether an AES coder is part of the folder.
    pub encrypted: bool,
}

/// One file of the listing with where its bytes are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalkedFile {
    pub entry: Entry,
    /// The folder holding the bytes; `None` for directories and empty files.
    pub folder: Option<u32>,
    /// Offset of the bytes in the folder's output.
    pub offset: u64,
    /// The stored Windows attributes.
    pub attributes: Option<u32>,
}

/// A fully validated 7z header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Walked {
    pub folders: Vec<FolderInfo>,
    pub files: Vec<WalkedFile>,
    /// File offset just past the (outer) next header.
    pub header_end: u64,
    /// `(file offset, size)` of a `kEncodedHeader`'s packed stream, at
    /// most twice `max_header` bytes.
    pub header_pack: Option<(u64, u64)>,
}

/// Walks the 7z in `src` (`len` bytes) with every bound of the module
/// docs, inside the panic guard. Nothing outside this module has read the
/// file when it returns `Ok`.
pub fn walk<R: Read + Seek>(
    src: &mut R,
    len: u64,
    limits: &Limits,
) -> Result<Walked, ArchiveError> {
    guarded(|| walk_inner(src, len, limits))
}

fn walk_inner<R: Read + Seek>(
    src: &mut R,
    len: u64,
    limits: &Limits,
) -> Result<Walked, ArchiveError> {
    if len < START_HEADER_LEN {
        return Err(damaged("7z shorter than its start header"));
    }
    let mut start = [0u8; START_HEADER_LEN as usize];
    src.seek(SeekFrom::Start(0)).map_err(ArchiveError::Io)?;
    src.read_exact(&mut start).map_err(ArchiveError::Io)?;
    if start[..6] != SIGNATURE {
        return Err(damaged("7z signature"));
    }
    if start[6] != 0 {
        return Err(ArchiveError::Unsupported {
            format: format!("7z version {}.{}", start[6], start[7]),
            need: Some(Tool::SevenZip),
        });
    }
    if crc32fast::hash(&start[12..32]) != le_u32(&start[8..12]) {
        return Err(damaged("7z start header crc"));
    }
    let next_offset = le_u64(&start[12..20]);
    let next_size = le_u64(&start[20..28]);
    let next_crc = le_u32(&start[28..32]);
    let header_start = START_HEADER_LEN
        .checked_add(next_offset)
        .ok_or_else(|| damaged("7z next header offset"))?;
    let header_end = header_start
        .checked_add(next_size)
        .filter(|&end| end <= len)
        .ok_or_else(|| damaged("7z next header outside the file"))?;
    if next_size > limits.max_header {
        return Err(ArchiveError::TooLarge {
            declared: next_size,
        });
    }
    // Bounded by `max_header` just above.
    let mut header = vec![0u8; usize::try_from(next_size).map_err(|_| damaged("7z header"))?];
    src.seek(SeekFrom::Start(header_start))
        .map_err(ArchiveError::Io)?;
    src.read_exact(&mut header).map_err(ArchiveError::Io)?;
    if crc32fast::hash(&header) != next_crc {
        return Err(damaged("7z header crc"));
    }
    if header.is_empty() {
        // 7-Zip writes an empty archive as a start header only.
        return Ok(Walked {
            folders: Vec::new(),
            files: Vec::new(),
            header_end,
            header_pack: None,
        });
    }

    let mut cur = Cur::new(&header);
    let mut header_pack = None;
    let header = match cur.number()? {
        K_HEADER => header,
        K_ENCODED_HEADER => {
            let (decoded, pack) = decode_encoded_header(src, &mut cur, header_start, limits)?;
            if decoded.first().map(|&b| u64::from(b)) != Some(K_HEADER) {
                return Err(damaged("7z encoded header does not decode to a header"));
            }
            header_pack = Some(pack);
            decoded
        }
        other => return Err(damaged(format!("7z header starts with property {other}"))),
    };
    let mut cur = Cur::new(&header);
    cur.number()?; // kHeader, checked above
    let (folders, files) = parse_header(&mut cur, header_start, limits)?;
    Ok(Walked {
        folders,
        files,
        header_end,
        header_pack,
    })
}

/// Validates the `kEncodedHeader` streams (S4) and decodes the header;
/// also returns the `(offset, size)` of its packed stream.
fn decode_encoded_header<R: Read + Seek>(
    src: &mut R,
    cur: &mut Cur<'_>,
    header_start: u64,
    limits: &Limits,
) -> Result<(Vec<u8>, (u64, u64)), ArchiveError> {
    let mut folders = parse_streams_info(cur, header_start, limits)?;
    if !cur.at_end() {
        return Err(damaged("7z bytes after the encoded header"));
    }
    if folders.len() != 1 {
        return Err(damaged(format!(
            "7z encoded header with {} folders",
            folders.len()
        )));
    }
    let (folder, sub) = folders.remove(0);
    if folder.encrypted {
        return Err(ArchiveError::Encrypted);
    }
    let chain = simple_chain(&folder)?;
    let first = &folder.coders[chain[0]];
    let lzma_kind = match first.kind {
        CoderKind::Lzma(p) => LzmaKind::Lzma(p),
        CoderKind::Lzma2(p) => LzmaKind::Lzma2(p),
        _ => return Err(damaged("7z encoded header coder")),
    };
    for &c in &chain[1..] {
        match folder.coders[c].kind {
            CoderKind::Bcj { kind, .. } if kind != BcjKind::Bcj2 => {}
            CoderKind::Delta { .. } => {}
            _ => return Err(damaged("7z encoded header coder")),
        }
    }
    if folder.unpack_size > limits.max_header {
        return Err(ArchiveError::TooLarge {
            declared: folder.unpack_size,
        });
    }
    // The packed stream is held in memory for the crate (S5). LZMA/LZMA2
    // never expand a header anywhere near twice its size.
    if folder.pack_size > limits.max_header.saturating_mul(2) {
        return Err(ArchiveError::TooLarge {
            declared: folder.pack_size,
        });
    }
    let crc = match (folder.crc, sub.crcs.as_slice()) {
        (Some(crc), _) => crc,
        (None, [Some(crc)]) if sub.sizes.len() == 1 => *crc,
        _ => return Err(damaged("7z encoded header without crc")),
    };
    lzma::check(lzma_kind.mem_needed(), limits.decoder_mem_cap)?;
    let _permit = MemoryGate::global().acquire(folder.mem_needed)?;

    src.seek(SeekFrom::Start(folder.pack_pos))
        .map_err(ArchiveError::Io)?;
    let packed = (&mut *src).take(folder.pack_size);
    let declared = folder.unpack_sizes[chain[0]];
    let mut reader = open_decoder(lzma_kind, Some(declared), limits.decoder_mem_cap, packed)?;
    for &c in &chain[1..] {
        reader = match folder.coders[c].kind {
            CoderKind::Bcj { kind, start } => bcj_reader(reader, kind, start)?,
            CoderKind::Delta { distance } => {
                Box::new(DeltaReader::new(reader, usize::from(distance)))
            }
            _ => return Err(damaged("7z encoded header coder")),
        };
    }
    let mut verified = VerifyingReader::new(reader, folder.unpack_size, Some(crc));
    // Bounded by `max_header` above; the reader stops at size + 1.
    let mut out = Vec::new();
    verified.read_to_end(&mut out).map_err(ArchiveError::from)?;
    if !verified.verified() {
        return Err(damaged("7z encoded header size"));
    }
    Ok((out, (folder.pack_pos, folder.pack_size)))
}

fn bcj_reader<'a>(
    inner: Box<dyn Read + 'a>,
    kind: BcjKind,
    start: u32,
) -> Result<Box<dyn Read + 'a>, ArchiveError> {
    let start = start as usize;
    Ok(Box::new(match kind {
        BcjKind::X86 => BcjReader::new_x86(inner, start),
        BcjKind::PowerPc => BcjReader::new_ppc(inner, start),
        BcjKind::Ia64 => BcjReader::new_ia64(inner, start),
        BcjKind::Arm => BcjReader::new_arm(inner, start),
        BcjKind::ArmThumb => BcjReader::new_arm_thumb(inner, start),
        BcjKind::Sparc => BcjReader::new_sparc(inner, start),
        BcjKind::Arm64 => BcjReader::new_arm64(inner, start),
        BcjKind::RiscV => BcjReader::new_riscv(inner, start),
        BcjKind::Bcj2 => return Err(damaged("7z encoded header coder")),
    }))
}

/// For a folder of one-in/one-out coders: the coder indices from the one
/// reading the packed stream to the one producing the folder's output.
fn simple_chain(folder: &FolderInfo) -> Result<Vec<usize>, ArchiveError> {
    if folder
        .coders
        .iter()
        .any(|c| c.num_in != 1 || c.num_out != 1)
        || folder.packed_streams.len() != 1
    {
        return Err(damaged("7z encoded header coders"));
    }
    // With simple coders, in stream i and out stream i belong to coder i.
    let mut chain = Vec::new();
    let mut coder = folder.packed_streams[0];
    loop {
        if chain.len() >= folder.coders.len() {
            return Err(damaged("7z coder cycle"));
        }
        chain.push(usize::try_from(coder).map_err(|_| damaged("7z coder"))?);
        if coder == folder.main_out {
            return Ok(chain);
        }
        coder = folder
            .bind_pairs
            .iter()
            .find(|&&(_, out)| out == coder)
            .map(|&(input, _)| input)
            .ok_or_else(|| damaged("7z coder chain"))?;
    }
}

/// Substreams of one folder: sizes and CRCs of its files, in order.
#[derive(Clone, Debug, Default)]
struct Substreams {
    sizes: Vec<u64>,
    crcs: Vec<Option<u32>>,
}

/// The `kHeader` after its id: main streams, files, `kEnd`, end of data.
fn parse_header(
    cur: &mut Cur<'_>,
    header_start: u64,
    limits: &Limits,
) -> Result<(Vec<FolderInfo>, Vec<WalkedFile>), ArchiveError> {
    let mut id = cur.number()?;
    match id {
        K_ARCHIVE_PROPERTIES => return Err(damaged("7z archive properties")),
        K_ADDITIONAL_STREAMS_INFO => return Err(damaged("7z additional streams")),
        _ => {}
    }
    let mut folders = Vec::new();
    if id == K_MAIN_STREAMS_INFO {
        folders = parse_streams_info(cur, header_start, limits)?;
        id = cur.number()?;
    }
    let mut files = Vec::new();
    if id == K_FILES_INFO {
        files = parse_files_info(cur, &folders, limits)?;
        id = cur.number()?;
    } else if folders.iter().any(|(f, _)| f.num_substreams > 0) {
        return Err(damaged("7z streams without files"));
    }
    if id != K_END {
        return Err(damaged(format!("7z header property {id}")));
    }
    if !cur.at_end() {
        return Err(damaged("7z bytes after the header end"));
    }
    let with_stream: Vec<&WalkedFile> = files.iter().filter(|f| f.folder.is_some()).collect();
    if !with_stream.is_empty() && with_stream.iter().all(|f| f.entry.encrypted) {
        return Err(ArchiveError::Encrypted);
    }
    Ok((folders.into_iter().map(|(f, _)| f).collect(), files))
}

/// A `StreamsInfo` up to and including its `kEnd`.
fn parse_streams_info(
    cur: &mut Cur<'_>,
    header_start: u64,
    limits: &Limits,
) -> Result<Vec<(FolderInfo, Substreams)>, ArchiveError> {
    let mut id = cur.number()?;
    let mut pack: Option<(u64, Vec<u64>)> = None;
    if id == K_PACK_INFO {
        pack = Some(parse_pack_info(cur, limits)?);
        id = cur.number()?;
    }
    let mut folders = Vec::new();
    if id == K_UNPACK_INFO {
        folders = parse_unpack_info(cur, limits)?;
        id = cur.number()?;
    }
    let mut subs: Vec<Substreams> = Vec::new();
    if id == K_SUBSTREAMS_INFO {
        subs = parse_substreams(cur, &mut folders, limits)?;
        id = cur.number()?;
    } else {
        for f in &mut folders {
            f.num_substreams = 1;
            subs.push(Substreams {
                sizes: vec![f.unpack_size],
                crcs: vec![f.crc],
            });
        }
    }
    if id != K_END {
        return Err(damaged(format!("7z streams property {id}")));
    }

    // Packed streams: assigned to folders in order, all before the header.
    let (pack_pos, pack_sizes) = pack.unwrap_or((0, Vec::new()));
    let needed: usize = folders.iter().map(|f| f.packed_streams.len()).sum();
    if needed != pack_sizes.len() {
        return Err(damaged("7z pack stream count"));
    }
    let mut at = START_HEADER_LEN
        .checked_add(pack_pos)
        .ok_or_else(|| damaged("7z pack position"))?;
    let mut sizes = pack_sizes.into_iter();
    for f in &mut folders {
        f.pack_pos = at;
        for _ in 0..f.packed_streams.len() {
            let size = sizes.next().ok_or_else(|| damaged("7z pack streams"))?;
            f.pack_sizes.push(size);
            f.pack_size = f
                .pack_size
                .checked_add(size)
                .ok_or_else(|| damaged("7z pack size"))?;
        }
        for (k, &input) in f.packed_streams.iter().enumerate() {
            if let Some(out) = preserved_out(&f.coders, input) {
                if f.pack_sizes[k] != f.unpack_sizes[out] {
                    return Err(damaged("7z copy coder size"));
                }
            }
        }
        at = at
            .checked_add(f.pack_size)
            .filter(|&end| end <= header_start)
            .ok_or_else(|| damaged("7z packed streams outside the file"))?;
        if f.mem_needed > limits.decoder_mem_cap {
            return Err(ArchiveError::NeedsTooMuchMemory {
                needed: f.mem_needed,
                cap: limits.decoder_mem_cap,
            });
        }
    }
    Ok(folders.into_iter().zip(subs).collect())
}

fn parse_pack_info(cur: &mut Cur<'_>, limits: &Limits) -> Result<(u64, Vec<u64>), ArchiveError> {
    let pos = cur.number()?;
    let n = cur.count(limits.max_entries.saturating_mul(4), 1, "pack stream")?;
    let mut sizes = None;
    let mut crcs = false;
    loop {
        match cur.number()? {
            K_END => break,
            K_SIZE if sizes.is_none() => {
                let mut v = Vec::new();
                for _ in 0..n {
                    v.push(cur.number()?);
                }
                sizes = Some(v);
            }
            K_CRC if sizes.is_some() && !crcs => {
                cur.digests(n)?;
                crcs = true;
            }
            other => return Err(damaged(format!("7z pack info property {other}"))),
        }
    }
    match sizes {
        Some(v) => Ok((pos, v)),
        None if n == 0 => Ok((pos, Vec::new())),
        None => Err(damaged("7z pack sizes missing")),
    }
}

fn parse_unpack_info(cur: &mut Cur<'_>, limits: &Limits) -> Result<Vec<FolderInfo>, ArchiveError> {
    if cur.number()? != K_FOLDER {
        return Err(damaged("7z unpack info without folders"));
    }
    let n = cur.count(limits.max_entries, 3, "folder")?;
    if cur.byte()? != 0 {
        return Err(damaged("7z external folders"));
    }
    let mut folders = Vec::new();
    for _ in 0..n {
        folders.push(parse_folder(cur)?);
    }
    if cur.number()? != K_CODERS_UNPACK_SIZE {
        return Err(damaged("7z coder unpack sizes missing"));
    }
    for f in &mut folders {
        let outs: u64 = f.coders.iter().map(|c| c.num_out).sum();
        for _ in 0..outs {
            f.unpack_sizes.push(cur.number()?);
        }
        f.unpack_size = f.unpack_sizes[usize::try_from(f.main_out).map_err(|_| damaged("7z"))?];
        check_preserved_sizes(f)?;
    }
    let mut crcs = false;
    loop {
        match cur.number()? {
            K_END => break,
            K_CRC if !crcs => {
                for (f, crc) in folders.iter_mut().zip(cur.digests(n)?) {
                    f.crc = crc;
                }
                crcs = true;
            }
            other => return Err(damaged(format!("7z unpack info property {other}"))),
        }
    }
    Ok(folders)
}

fn parse_folder(cur: &mut Cur<'_>) -> Result<FolderInfo, ArchiveError> {
    let num_coders = cur.number()?;
    if num_coders == 0 || num_coders > MAX_CODERS as u64 {
        return Err(damaged(format!("7z folder with {num_coders} coders")));
    }
    let mut coders = Vec::new();
    for _ in 0..num_coders {
        let flags = cur.byte()?;
        if flags & 0xC0 != 0 {
            return Err(damaged("7z coder flags"));
        }
        let id_len = usize::from(flags & 0x0F);
        if id_len == 0 || id_len > MAX_CODER_ID {
            return Err(damaged("7z coder id"));
        }
        let id = cur.bytes(id_len)?.to_vec();
        let (num_in, num_out) = if flags & 0x10 != 0 {
            (cur.number()?, cur.number()?)
        } else {
            (1, 1)
        };
        if !(1..=MAX_CODER_STREAMS).contains(&num_in) || !(1..=MAX_CODER_STREAMS).contains(&num_out)
        {
            return Err(damaged("7z coder stream count"));
        }
        let props = if flags & 0x20 != 0 {
            let size = cur.number()?;
            let size = usize::try_from(size).map_err(|_| damaged("7z coder properties"))?;
            cur.bytes(size)?.to_vec()
        } else {
            Vec::new()
        };
        let (kind, method, mem_needed) = coder_kind(&id, &props)?;
        coders.push(CoderInfo {
            id,
            props,
            num_in,
            num_out,
            kind,
            method,
            mem_needed,
        });
    }
    // At most 8 coders × 8 streams: these sums cannot overflow.
    let total_in: u64 = coders.iter().map(|c| c.num_in).sum();
    let total_out: u64 = coders.iter().map(|c| c.num_out).sum();
    let mut bind_pairs = Vec::new();
    for _ in 0..total_out - 1 {
        let input = cur.number()?;
        let output = cur.number()?;
        if input >= total_in
            || output >= total_out
            || bind_pairs
                .iter()
                .any(|&(i, o): &(u64, u64)| i == input || o == output)
        {
            return Err(damaged("7z bind pair"));
        }
        bind_pairs.push((input, output));
    }
    let num_packed = total_in
        .checked_sub(total_out - 1)
        .filter(|&n| n >= 1)
        .ok_or_else(|| damaged("7z folder without packed streams"))?;
    let is_bound = |i: u64| bind_pairs.iter().any(|&(input, _)| input == i);
    let mut packed_streams = Vec::new();
    if num_packed == 1 {
        let free = (0..total_in)
            .find(|&i| !is_bound(i))
            .ok_or_else(|| damaged("7z packed stream"))?;
        packed_streams.push(free);
    } else {
        for _ in 0..num_packed {
            let i = cur.number()?;
            if i >= total_in || is_bound(i) || packed_streams.contains(&i) {
                return Err(damaged("7z packed stream index"));
            }
            packed_streams.push(i);
        }
    }
    let main_out = (0..total_out)
        .find(|&o| !bind_pairs.iter().any(|&(_, out)| out == o))
        .ok_or_else(|| damaged("7z folder output"))?;
    check_coder_tree(&coders, &bind_pairs, main_out)?;

    let mem_needed = coders
        .iter()
        .try_fold(0u64, |sum, c| sum.checked_add(c.mem_needed))
        .unwrap_or(u64::MAX);
    let encrypted = coders.iter().any(|c| c.kind == CoderKind::Aes);
    Ok(FolderInfo {
        coders,
        bind_pairs,
        packed_streams,
        unpack_sizes: Vec::new(),
        main_out,
        unpack_size: 0,
        crc: None,
        pack_pos: 0,
        pack_size: 0,
        pack_sizes: Vec::new(),
        num_substreams: 1,
        mem_needed,
        encrypted,
    })
}

/// Whether a coder writes exactly as many bytes as it reads.
fn preserves_size(kind: CoderKind) -> bool {
    match kind {
        CoderKind::Copy | CoderKind::Delta { .. } => true,
        CoderKind::Bcj { kind, .. } => kind != BcjKind::Bcj2,
        _ => false,
    }
}

/// For in stream `input`: the out stream index of its coder when that
/// coder is a one-in/one-out size-preserving one.
fn preserved_out(coders: &[CoderInfo], input: u64) -> Option<usize> {
    let (mut first_in, mut first_out) = (0u64, 0u64);
    for c in coders {
        if input < first_in + c.num_in {
            let simple = c.num_in == 1 && c.num_out == 1 && preserves_size(c.kind);
            return simple.then(|| usize::try_from(first_out).ok()).flatten();
        }
        first_in += c.num_in;
        first_out += c.num_out;
    }
    None
}

/// Size-preserving coders (Copy, BCJ, Delta) fed by another coder must
/// declare the same size as their input (S5: an inconsistent size is a
/// lie, never passed on).
fn check_preserved_sizes(f: &FolderInfo) -> Result<(), ArchiveError> {
    for &(input, bound_out) in &f.bind_pairs {
        if let Some(out) = preserved_out(&f.coders, input) {
            let read = usize::try_from(bound_out).map_err(|_| damaged("7z bind pair"))?;
            if f.unpack_sizes[read] != f.unpack_sizes[out] {
                return Err(damaged("7z filter sizes disagree"));
            }
        }
    }
    Ok(())
}

/// Every coder is reached exactly once from the folder output, following
/// bind pairs: no cycle, no coder left over.
fn check_coder_tree(
    coders: &[CoderInfo],
    bind_pairs: &[(u64, u64)],
    main_out: u64,
) -> Result<(), ArchiveError> {
    // Stream index → coder index, from the per-coder counts.
    let coder_of = |index: u64, inputs: bool| -> Option<usize> {
        let mut first = 0u64;
        for (c, coder) in coders.iter().enumerate() {
            let n = if inputs { coder.num_in } else { coder.num_out };
            if index < first + n {
                return Some(c);
            }
            first += n;
        }
        None
    };
    let first_in = |c: usize| -> u64 { coders[..c].iter().map(|x| x.num_in).sum() };
    let mut seen = [false; MAX_CODERS];
    let mut stack = vec![coder_of(main_out, false).ok_or_else(|| damaged("7z folder output"))?];
    let mut visited = 0usize;
    while let Some(c) = stack.pop() {
        if std::mem::replace(&mut seen[c], true) {
            return Err(damaged("7z coder graph"));
        }
        visited += 1;
        let start = first_in(c);
        for input in start..start + coders[c].num_in {
            if let Some(&(_, out)) = bind_pairs.iter().find(|&&(i, _)| i == input) {
                stack.push(coder_of(out, false).ok_or_else(|| damaged("7z bind pair"))?);
            }
        }
    }
    if visited != coders.len() {
        return Err(damaged("7z coder graph"));
    }
    Ok(())
}

/// What a coder id and its properties mean, and the memory it needs.
fn coder_kind(id: &[u8], props: &[u8]) -> Result<(CoderKind, Method, u64), ArchiveError> {
    let bcj = |kind: BcjKind| -> Result<(CoderKind, Method, u64), ArchiveError> {
        let start = match props {
            [] => 0,
            [a, b, c, d] => u32::from_le_bytes([*a, *b, *c, *d]),
            _ => return Err(damaged("7z branch filter properties")),
        };
        Ok((
            CoderKind::Bcj { kind, start },
            Method::Bcj(kind),
            FILTER_MEM,
        ))
    };
    match id {
        [0x00] => {
            if !props.is_empty() {
                return Err(damaged("7z copy properties"));
            }
            Ok((CoderKind::Copy, Method::Stored, 0))
        }
        [0x03] => match props {
            [d] => Ok((
                CoderKind::Delta {
                    distance: u16::from(*d) + 1,
                },
                Method::Delta,
                FILTER_MEM,
            )),
            _ => Err(damaged("7z delta properties")),
        },
        [0x21] => match props {
            [p] => {
                let params = Lzma2Params::from_prop(*p)?;
                Ok((
                    CoderKind::Lzma2(params),
                    Method::Lzma2 {
                        dict: params.dict_raw,
                    },
                    params.mem_needed(),
                ))
            }
            _ => Err(damaged("7z lzma2 properties")),
        },
        [0x03, 0x01, 0x01] => {
            let props: [u8; 5] = props
                .try_into()
                .map_err(|_| damaged("7z lzma properties"))?;
            let params = LzmaParams::from_props(props)?;
            Ok((
                CoderKind::Lzma(params),
                Method::Lzma {
                    dict: params.dict_raw,
                },
                params.mem_needed(),
            ))
        }
        [0x03, 0x04, 0x01] => match props {
            [order, m0, m1, m2, m3] => {
                let mem = u32::from_le_bytes([*m0, *m1, *m2, *m3]);
                // A model over PPMd7's maximum (up to 4 GiB declared) is
                // left to the folder memory check, which refuses it.
                let needed = u64::from(mem) + lzma::FIXED_STATE;
                if !(2..=64).contains(order) || mem < PPMD7_MIN_MEM {
                    return Err(damaged("7z ppmd properties"));
                }
                Ok((
                    CoderKind::Ppmd { order: *order, mem },
                    Method::Ppmd { order: *order, mem },
                    needed,
                ))
            }
            _ => Err(damaged("7z ppmd properties")),
        },
        [0x04, 0x02, 0x02] => Ok((CoderKind::Bzip2, Method::Bzip2, BZIP2_MEM)),
        [0x04, 0x01, 0x08] => Ok((CoderKind::Deflate, Method::Deflate, DEFLATE_MEM)),
        [0x04, 0x01, 0x09] => Ok((CoderKind::Deflate64, Method::Deflate64, DEFLATE_MEM)),
        [0x03, 0x03, 0x01, 0x03] => bcj(BcjKind::X86),
        [0x03, 0x03, 0x01, 0x1B] => Ok((
            CoderKind::Bcj {
                kind: BcjKind::Bcj2,
                start: 0,
            },
            Method::Bcj(BcjKind::Bcj2),
            FILTER_MEM,
        )),
        [0x03, 0x03, 0x02, 0x05] => bcj(BcjKind::PowerPc),
        [0x03, 0x03, 0x04, 0x01] => bcj(BcjKind::Ia64),
        [0x03, 0x03, 0x05, 0x01] => bcj(BcjKind::Arm),
        [0x03, 0x03, 0x07, 0x01] => bcj(BcjKind::ArmThumb),
        [0x03, 0x03, 0x08, 0x05] => bcj(BcjKind::Sparc),
        [0x0A] => bcj(BcjKind::Arm64),
        [0x0B] => bcj(BcjKind::RiscV),
        [0x06, 0xF1, 0x07, 0x01] => Ok((
            CoderKind::Aes,
            Method::Other("7z AES-256".into()),
            FILTER_MEM,
        )),
        other => Ok((
            CoderKind::Other,
            Method::Other(format!("7z coder {}", hex(other))),
            0,
        )),
    }
}

fn parse_substreams(
    cur: &mut Cur<'_>,
    folders: &mut [FolderInfo],
    limits: &Limits,
) -> Result<Vec<Substreams>, ArchiveError> {
    let mut id = cur.number()?;
    if id == K_NUM_UNPACK_STREAM {
        let mut total = 0u64;
        for f in folders.iter_mut() {
            f.num_substreams = cur.number()?;
            total = total.saturating_add(f.num_substreams);
        }
        if total > limits.max_entries {
            return Err(ArchiveError::TooManyEntries { count: total });
        }
        id = cur.number()?;
    }
    let has_sizes = id == K_SIZE;
    let mut subs = Vec::new();
    for f in folders.iter() {
        let mut sizes = Vec::new();
        if f.num_substreams > 1 && !has_sizes {
            return Err(damaged("7z substream sizes missing"));
        }
        if f.num_substreams >= 1 {
            let mut sum = 0u64;
            for _ in 1..f.num_substreams {
                let size = cur.number()?;
                sum = sum
                    .checked_add(size)
                    .ok_or_else(|| damaged("7z substream sizes"))?;
                sizes.push(size);
            }
            let last = f
                .unpack_size
                .checked_sub(sum)
                .ok_or_else(|| damaged("7z substream sizes exceed the folder"))?;
            sizes.push(last);
        }
        subs.push(Substreams {
            sizes,
            crcs: Vec::new(),
        });
    }
    if has_sizes {
        id = cur.number()?;
    }
    let unknown = |f: &FolderInfo| -> u64 {
        if f.num_substreams == 1 && f.crc.is_some() {
            0
        } else {
            f.num_substreams
        }
    };
    let count: u64 = folders.iter().map(unknown).sum();
    let mut digests = Vec::new().into_iter();
    if id == K_CRC {
        let n = usize::try_from(count).map_err(|_| damaged("7z digests"))?;
        digests = cur.digests(n)?.into_iter();
        id = cur.number()?;
    }
    for (f, sub) in folders.iter().zip(subs.iter_mut()) {
        if unknown(f) == 0 {
            sub.crcs.push(f.crc);
        } else {
            for _ in 0..f.num_substreams {
                sub.crcs.push(digests.next().flatten());
            }
        }
    }
    if id != K_END {
        return Err(damaged(format!("7z substreams property {id}")));
    }
    Ok(subs)
}

fn parse_files_info(
    cur: &mut Cur<'_>,
    folders: &[(FolderInfo, Substreams)],
    limits: &Limits,
) -> Result<Vec<WalkedFile>, ArchiveError> {
    // Every file needs at least its 2-byte name terminator.
    let n = cur.count(limits.max_entries, 2, "file")?;
    let mut empty_stream: Option<Vec<bool>> = None;
    let mut empty_file: Option<Vec<bool>> = None;
    let mut names: Option<Vec<Vec<u16>>> = None;
    let mut mtimes: Option<Vec<Option<u64>>> = None;
    let mut attributes: Option<Vec<Option<u32>>> = None;
    let mut seen = 0u64;
    loop {
        let prop = cur.number()?;
        if prop == K_END {
            break;
        }
        let size = usize::try_from(cur.number()?).map_err(|_| damaged("7z property size"))?;
        let mut data = Cur::new(cur.bytes(size)?);
        // 7-Zip writes kDummy (alignment padding) as often as it needs.
        if prop < 64 && prop != K_DUMMY {
            if seen & (1 << prop) != 0 {
                return Err(damaged(format!("7z file property {prop} twice")));
            }
            seen |= 1 << prop;
        }
        let num_empty = || -> Result<usize, ArchiveError> {
            Ok(empty_stream
                .as_ref()
                .ok_or_else(|| damaged("7z empty-file vector before empty streams"))?
                .iter()
                .filter(|&&e| e)
                .count())
        };
        match prop {
            K_EMPTY_STREAM => empty_stream = Some(data.bits(n)?),
            K_EMPTY_FILE => empty_file = Some(data.bits(num_empty()?)?),
            K_ANTI => {
                if data.bits(num_empty()?)?.contains(&true) {
                    return Err(damaged("7z anti-items"));
                }
            }
            K_NAME => {
                if data.byte()? != 0 {
                    return Err(damaged("7z external names"));
                }
                let mut list = Vec::new();
                for _ in 0..n {
                    let mut units = Vec::new();
                    loop {
                        let pair = data.bytes(2)?;
                        let unit = u16::from_le_bytes([pair[0], pair[1]]);
                        if unit == 0 {
                            break;
                        }
                        units.push(unit);
                    }
                    list.push(units);
                }
                names = Some(list);
            }
            K_CTIME | K_ATIME | K_MTIME => {
                let defined = data.defined(n)?;
                if data.byte()? != 0 {
                    return Err(damaged("7z external times"));
                }
                let mut times = Vec::new();
                for d in defined {
                    times.push(if d { Some(data.u64()?) } else { None });
                }
                if prop == K_MTIME {
                    mtimes = Some(times);
                }
            }
            K_WIN_ATTRIBUTES => {
                let defined = data.defined(n)?;
                if data.byte()? != 0 {
                    return Err(damaged("7z external attributes"));
                }
                let mut attrs = Vec::new();
                for d in defined {
                    attrs.push(if d { Some(data.u32()?) } else { None });
                }
                attributes = Some(attrs);
            }
            K_DUMMY => data.skip_rest(),
            other => return Err(damaged(format!("7z file property {other}"))),
        }
        if !data.at_end() {
            return Err(damaged(format!("7z file property {prop} size")));
        }
    }
    let names = match names {
        Some(names) => names,
        None if n == 0 => Vec::new(),
        None => return Err(damaged("7z files without names")),
    };
    let empty_stream = empty_stream.unwrap_or_else(|| vec![false; n]);
    let num_empty = empty_stream.iter().filter(|&&e| e).count();
    let empty_file = empty_file.unwrap_or_else(|| vec![false; num_empty]);

    // Files with a stream take the folders' substreams in order.
    let total_streams: u64 = folders.iter().map(|(f, _)| f.num_substreams).sum();
    if total_streams != (n - num_empty) as u64 {
        return Err(damaged("7z stream count does not match the files"));
    }
    let mut slots = folders.iter().enumerate().flat_map(|(i, (f, sub))| {
        let mut offset = 0u64;
        sub.sizes.iter().zip(&sub.crcs).map(move |(&size, &crc)| {
            let at = offset;
            offset = offset.saturating_add(size);
            (i, f, size, crc, at)
        })
    });

    let mut files = Vec::new();
    let mut total = 0u64;
    let mut empty_index = 0usize;
    for (index, units) in names.into_iter().enumerate() {
        let attrib = attributes.as_ref().and_then(|a| a[index]);
        let mtime = mtimes.as_ref().and_then(|t| t[index]).and_then(filetime);
        let (mut kind, folder, size, crc32, offset, method, packed, encrypted);
        if empty_stream[index] {
            kind = if empty_file[empty_index] {
                EntryKind::File
            } else {
                EntryKind::Dir
            };
            empty_index += 1;
            (folder, size, crc32, offset) = (None, 0, None, 0);
            (method, packed, encrypted) = (Method::Stored, None, false);
        } else {
            let (i, f, s, c, at) = slots
                .next()
                .ok_or_else(|| damaged("7z stream count does not match the files"))?;
            kind = EntryKind::File;
            folder = Some(u32::try_from(i).map_err(|_| damaged("7z folder index"))?);
            (size, crc32, offset) = (s, c, at);
            method = folder_method(f);
            packed = (f.num_substreams == 1).then_some(f.pack_size);
            encrypted = f.encrypted;
        }
        if let Some(a) = attrib {
            let unix_link = a & ATTR_UNIX_EXTENSION != 0 && (a >> 16) & S_IFMT == S_IFLNK;
            if a & ATTR_REPARSE_POINT != 0 || unix_link {
                kind = EntryKind::Symlink;
            }
        }
        if size > limits.max_entry_size {
            return Err(ArchiveError::TooLarge { declared: size });
        }
        total = total
            .checked_add(size)
            .filter(|&t| t <= limits.max_total_size)
            .ok_or(ArchiveError::TooLarge {
                declared: total.saturating_add(size),
            })?;
        let raw_name: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
        let (name, unsafe_name) = entry_name(index, &units);
        files.push(WalkedFile {
            entry: Entry {
                index,
                name,
                raw_name,
                kind,
                size,
                packed,
                crc32,
                encrypted,
                method,
                solid_block: folder,
                mtime,
                unsafe_name,
            },
            folder,
            offset,
            attributes: attrib,
        });
    }
    Ok(files)
}

/// The method shown for a folder's files: its first compressor, else its
/// first coder.
fn folder_method(f: &FolderInfo) -> Method {
    f.coders
        .iter()
        .find(|c| !c.kind.is_filter())
        .or(f.coders.first())
        .map_or(Method::Stored, |c| c.method.clone())
}

fn entry_name(index: usize, units: &[u16]) -> (RelPath, Option<UnsafeWhy>) {
    match decode_utf16_name(units).and_then(|text| RelPath::parse(&text)) {
        Ok(name) => (name, None),
        Err(why) => (RelPath::placeholder(index), Some(why)),
    }
}

/// A Windows FILETIME (100 ns ticks since 1601) as a `SystemTime`.
fn filetime(ft: u64) -> Option<SystemTime> {
    const TICKS_PER_SEC: u64 = 10_000_000;
    const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
    let span = |ticks: u64| {
        Duration::new(
            ticks / TICKS_PER_SEC,
            ((ticks % TICKS_PER_SEC) * 100) as u32,
        )
    };
    if ft >= UNIX_EPOCH_TICKS {
        UNIX_EPOCH.checked_add(span(ft - UNIX_EPOCH_TICKS))
    } else {
        UNIX_EPOCH.checked_sub(span(UNIX_EPOCH_TICKS - ft))
    }
}

/// A bounded cursor over header bytes: every read past the end is
/// `Damaged`, never a panic.
struct Cur<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cur<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn at_end(&self) -> bool {
        self.pos == self.buf.len()
    }

    fn skip_rest(&mut self) {
        self.pos = self.buf.len();
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], ArchiveError> {
        if n > self.remaining() {
            return Err(damaged("7z header truncated"));
        }
        let out = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn byte(&mut self) -> Result<u8, ArchiveError> {
        Ok(self.bytes(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, ArchiveError> {
        Ok(le_u32(self.bytes(4)?))
    }

    fn u64(&mut self) -> Result<u64, ArchiveError> {
        Ok(le_u64(self.bytes(8)?))
    }

    /// A 7z number: the leading one bits of the first byte count the
    /// extra little-endian bytes; the rest of the first byte is the top.
    fn number(&mut self) -> Result<u64, ArchiveError> {
        let first = self.byte()?;
        let mut value = 0u64;
        let mut mask = 0x80u8;
        for i in 0..8 {
            if first & mask == 0 {
                let high = u64::from(first & mask.wrapping_sub(1));
                return Ok(value | (high << (8 * i)));
            }
            value |= u64::from(self.byte()?) << (8 * i);
            mask >>= 1;
        }
        Ok(value)
    }

    /// A declared count: more items than the bytes left could hold (at
    /// `min_bytes` each) is a lie (`Damaged`); a plausible count over
    /// `cap` is `TooManyEntries`.
    fn count(&mut self, cap: u64, min_bytes: u64, what: &str) -> Result<usize, ArchiveError> {
        let n = self.number()?;
        if n.saturating_mul(min_bytes) > self.remaining() as u64 {
            return Err(damaged(format!("7z {what} count {n} exceeds the header")));
        }
        if n > cap {
            return Err(ArchiveError::TooManyEntries { count: n });
        }
        usize::try_from(n).map_err(|_| damaged("7z count"))
    }

    /// `n` bits, most significant first (`n` is already bounded).
    fn bits(&mut self, n: usize) -> Result<Vec<bool>, ArchiveError> {
        let bytes = self.bytes(n.div_ceil(8))?;
        Ok((0..n)
            .map(|i| bytes[i / 8] & (0x80 >> (i % 8)) != 0)
            .collect())
    }

    /// "All defined" byte, then the bit vector when it is 0.
    fn defined(&mut self, n: usize) -> Result<Vec<bool>, ArchiveError> {
        match self.byte()? {
            0 => self.bits(n),
            1 => Ok(vec![true; n]),
            _ => Err(damaged("7z defined vector")),
        }
    }

    /// `n` optional CRCs.
    fn digests(&mut self, n: usize) -> Result<Vec<Option<u32>>, ArchiveError> {
        let defined = self.defined(n)?;
        let mut out = Vec::new();
        for d in defined {
            out.push(if d { Some(self.u32()?) } else { None });
        }
        Ok(out)
    }
}

fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn le_u64(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[..8]);
    u64::from_le_bytes(a)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn damaged(detail: impl Into<String>) -> ArchiveError {
    ArchiveError::Damaged {
        detail: detail.into(),
    }
}

// ---------------------------------------------------------------------------
// Part 2: decoding over `sevenz-rust2` (plan Task 10, spec §4.5, S5)
// ---------------------------------------------------------------------------

/// Size and modification time of a source, to tell that it changed after
/// the walk (spec §13.1 TOCTOU). `None`: the source cannot say (an
/// in-memory cursor); its length is still compared.
pub trait SourceStamp {
    fn stamp(&self) -> io::Result<Option<Stamp>>;
}

/// What [`SourceStamp`] reports.
pub type Stamp = (u64, Option<SystemTime>);

impl SourceStamp for File {
    fn stamp(&self) -> io::Result<Option<Stamp>> {
        let meta = self.metadata()?;
        Ok(Some((meta.len(), meta.modified().ok())))
    }
}

impl<T> SourceStamp for io::Cursor<T> {
    fn stamp(&self) -> io::Result<Option<Stamp>> {
        Ok(None)
    }
}

/// The archive file, remembering whether a read or seek of it failed, so
/// a decoder error can be told apart from an I/O error of the file.
struct Src<R> {
    inner: R,
    failed: Arc<AtomicBool>,
}

impl<R> Src<R> {
    fn note<T>(&self, r: io::Result<T>) -> io::Result<T> {
        if let Err(e) = &r {
            if e.kind() != io::ErrorKind::Interrupted {
                self.failed.store(true, Ordering::Relaxed);
            }
        }
        r
    }
}

impl<R: Read> Read for Src<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let r = self.inner.read(buf);
        self.note(r)
    }
}

impl<R: Seek> Seek for Src<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let r = self.inner.seek(pos);
        self.note(r)
    }
}

/// An `io::Error` out of a decoder over the file: one carrying an
/// `ArchiveError` stays itself, one after a failed file read stays I/O,
/// anything else is the data being rejected.
fn decode_error(e: io::Error, src_failed: &AtomicBool) -> ArchiveError {
    if e.get_ref().is_some_and(|inner| inner.is::<ArchiveError>()) {
        return ArchiveError::from(e);
    }
    if src_failed.load(Ordering::Relaxed) {
        return ArchiveError::Io(e);
    }
    damaged(format!("7z: {e}"))
}

/// Maps the errors of a decoder (see [`decode_error`]).
struct MapErrors<D> {
    inner: D,
    src_failed: Arc<AtomicBool>,
}

impl<D: Read> Read for MapErrors<D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner
            .read(buf)
            .map_err(|e| decode_error(e, &self.src_failed).into())
    }
}

/// A decoded stream and the gate permit for its decoders' memory. Fields
/// drop in order: the decoders first, then the permit.
struct Holding<D> {
    inner: D,
    _permit: Permit,
}

impl<D: Read> Read for Holding<D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

/// An error of `sevenz-rust2` (opening or decoding a folder).
fn crate_error(e: sevenz_rust2::Error, src_failed: &AtomicBool) -> ArchiveError {
    use sevenz_rust2::Error as E;
    match e {
        E::Io(io_err, _) | E::FileOpen(io_err, _) | E::MaybeBadPassword(io_err) => {
            decode_error(io_err, src_failed)
        }
        E::PasswordRequired => ArchiveError::Encrypted,
        E::UnsupportedCompressionMethod(m) => ArchiveError::Unsupported {
            format: format!("7z ({m})"),
            need: Some(Tool::SevenZip),
        },
        E::Unsupported(m) => ArchiveError::Unsupported {
            format: format!("7z ({m})"),
            need: Some(Tool::SevenZip),
        },
        E::ExternalUnsupported => ArchiveError::Unsupported {
            format: "7z (external)".into(),
            need: Some(Tool::SevenZip),
        },
        other => damaged(format!("7z: {other}")),
    }
}

/// A coder of `folder` that the decoders here cannot run correctly: one
/// `sevenz-rust2` is built without (zstd, Rar5 coders…), Deflate64, or a
/// branch filter with a start offset (the crate ignores it). Such a folder
/// is `Unsupported { need: 7-Zip }` (spec §5.2); the dispatch (plan Task 16)
/// streams it through `7zz` when one is found.
fn unsupported_coder(folder: &FolderInfo) -> Option<String> {
    folder
        .coders
        .iter()
        .find_map(|c| match (&c.kind, &c.method) {
            (CoderKind::Other, Method::Other(name)) => {
                Some(format!("7z ({})", name.trim_start_matches("7z ")))
            }
            (CoderKind::Other, _) => Some("7z (unknown coder)".into()),
            (CoderKind::Deflate64, _) => Some("7z (Deflate64)".into()),
            (CoderKind::Bcj { kind, start }, _) if *start != 0 => {
                Some(format!("7z ({kind:?} filter with a start offset)"))
            }
            _ => None,
        })
}

/// Test counters and hooks of the 7z decoder (plan Task 10). Thread-local,
/// so parallel tests do not see each other.
#[cfg(any(test, feature = "test-util"))]
pub mod testing_hooks {
    use std::cell::{Cell, RefCell};

    type Hook = Box<dyn FnOnce()>;

    thread_local! {
        static CRATE_OPENED: Cell<u64> = const { Cell::new(0) };
        static FOLDER_PASSES: Cell<u64> = const { Cell::new(0) };
        static BEFORE_CRATE_OPEN: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    /// How many times `sevenz-rust2` was given an archive on this thread.
    pub fn crate_opened() -> u64 {
        CRATE_OPENED.with(Cell::get)
    }

    /// How many folder decode passes ran through `sevenz-rust2` on this
    /// thread.
    pub fn folder_passes() -> u64 {
        FOLDER_PASSES.with(Cell::get)
    }

    /// Runs `f` once, on this thread, right before the next archive is
    /// given to `sevenz-rust2` (after the re-walk of the header copy): a
    /// test swaps the file there to show the crate never sees the change.
    pub fn before_crate_open(f: impl FnOnce() + 'static) {
        BEFORE_CRATE_OPEN.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }

    pub(super) fn crate_opening() {
        if let Some(f) = BEFORE_CRATE_OPEN.with(|h| h.borrow_mut().take()) {
            f();
        }
        CRATE_OPENED.with(|c| c.set(c.get() + 1));
    }

    pub(super) fn folder_pass() {
        FOLDER_PASSES.with(|c| c.set(c.get() + 1));
    }
}

#[cfg(not(any(test, feature = "test-util")))]
mod testing_hooks {
    pub(super) fn crate_opening() {}
    pub(super) fn folder_pass() {}
}

/// What [`SevenZReader::decode_folder`] calls for each file of a folder:
/// the file, the crate's stream of it and the budget; `Ok(false)` stops.
type FolderVisit<'f> =
    dyn FnMut(&WalkedFile, &mut dyn Read, &mut Budget) -> Result<bool, ArchiveError> + 'f;

/// A 7z archive: listed by [`walk`], decoded after `sevenz-rust2` agrees
/// with that walk (spec S5).
///
/// * The listing is the walk; the crate is not called to list.
/// * Before the first decoded byte, the size (and the mtime, for a file)
///   are re-checked, the header bytes are read from the **same** handle
///   into a [`Snapshot`] and walked again there; only then does the crate
///   parse that very copy, inside the guard, and its view (entry count, names, sizes, CRCs, attributes, coders, pack
///   streams, folder ↔ file map) must equal the walk, else `Damaged`.
///   Every later decode re-checks the size and mtime.
/// * Each folder takes the sum of its coders' memory from the
///   [`MemoryGate`] in one request (spec §4.5).
/// * [`ArchiveReader::open_entry`] decodes the folder up to the entry with
///   our own decoder chain (the checked LZMA/LZMA2 constructor, PPMd,
///   BZip2, Deflate, BCJ, Delta), charging the skipped bytes. A folder
///   with a multi-stream coder (BCJ2) is decoded by the crate instead, and
///   at most `plan_head_budget` bytes of the entry are buffered; past them
///   the stream ends in `BudgetSpent` (not read, never damaged).
/// * [`ArchiveReader::stream_entries`] makes one crate pass per folder in
///   storage order, single-threaded; unwanted entries before the last
///   wanted one are decoded to nowhere (charged, CRC-checked).
pub struct SevenZReader<R: Read + Seek + Send + SourceStamp = File> {
    src: Src<R>,
    len: u64,
    stamp: Option<Stamp>,
    walked: Walked,
    entries: Vec<Entry>,
    limits: Limits,
    poison: Poison,
    /// The crate's parsed view, once it matched the walk.
    view: Option<sevenz_rust2::Archive>,
}

impl SevenZReader<File> {
    /// Opens and walks the 7z at `path`.
    pub fn open(path: &Path, limits: &Limits) -> Result<Self, ArchiveError> {
        let file = File::open(path).map_err(ArchiveError::Io)?;
        Self::new(file, limits)
    }
}

impl<R: Read + Seek + Send + SourceStamp> SevenZReader<R> {
    /// Walks the 7z in `src` (the integrity listing of spec §4.3). The
    /// crate is not called.
    pub fn new(src: R, limits: &Limits) -> Result<Self, ArchiveError> {
        let stamp = src.stamp().map_err(ArchiveError::Io)?;
        let mut src = Src {
            inner: src,
            failed: Arc::new(AtomicBool::new(false)),
        };
        let len = src.seek(SeekFrom::End(0)).map_err(ArchiveError::Io)?;
        let walked = walk(&mut src, len, limits)?;
        let entries = walked.files.iter().map(|f| f.entry.clone()).collect();
        Ok(Self {
            src,
            len,
            stamp,
            walked,
            entries,
            limits: limits.clone(),
            poison: Poison::new(),
            view: None,
        })
    }

    /// The first coder no decoder here runs, as `"7z (<coder>)"`: such
    /// folders are `Unsupported { need: Some(Tool::SevenZip) }`, the rest
    /// of the archive still decodes.
    pub fn unsupported_coder(&self) -> Option<String> {
        self.walked.folders.iter().find_map(unsupported_coder)
    }

    /// The validated header walk.
    pub fn walked(&self) -> &Walked {
        &self.walked
    }

    /// The file is still the one that was walked: same length and, when
    /// the source can tell, same size and mtime.
    fn recheck(&mut self) -> Result<(), ArchiveError> {
        let len = self.src.seek(SeekFrom::End(0)).map_err(ArchiveError::Io)?;
        let stamp = match self.stamp {
            Some(_) => self.src.inner.stamp().map_err(ArchiveError::Io)?,
            None => None,
        };
        if len != self.len || stamp != self.stamp {
            return Err(damaged("7z changed since it was listed"));
        }
        Ok(())
    }

    /// Re-checks the file and, the first time, has the crate parse it and
    /// compares its view with the walk (S5).
    ///
    /// The header bytes are copied into a [`Snapshot`] and walked again
    /// there; the crate parses that same copy, so it only ever sees bytes
    /// the walk just validated, even if the file changes under the handle
    /// in between (a later change of the data is caught by the CRCs).
    fn ensure_view(&mut self) -> Result<(), ArchiveError> {
        self.recheck()?;
        if self.view.is_some() {
            return Ok(());
        }
        let changed = || damaged("7z changed since it was listed");
        let mut snap = Snapshot::capture(&mut self.src, self.len, &self.walked, &self.limits)?;
        let again = match walk(&mut snap, self.len, &self.limits) {
            Ok(again) => again,
            Err(_) if snap.strayed => return Err(changed()),
            Err(e) => return Err(e),
        };
        if again != self.walked {
            return Err(changed());
        }
        testing_hooks::crate_opening();
        let archive = guarded(|| {
            snap.seek(SeekFrom::Start(0)).map_err(ArchiveError::Io)?;
            // No read of the file happens here: an I/O error is the
            // snapshot refusing a read outside the validated bytes.
            sevenz_rust2::Archive::read(&mut snap, &sevenz_rust2::Password::empty())
                .map_err(|e| crate_error(e, &AtomicBool::new(false)))
        })?;
        cross_check(&self.walked, &archive)?;
        self.view = Some(archive);
        Ok(())
    }

    /// The walked file of entry `index`, when it can be read.
    fn readable(&self, index: usize) -> Result<&WalkedFile, ArchiveError> {
        let file = self.walked.files.get(index).ok_or_else(|| {
            ArchiveError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("no 7z entry {index}"),
            ))
        })?;
        let entry = &file.entry;
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
                    format!("7z entry {index} is not a file"),
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
        Ok(file)
    }

    /// One crate pass over folder `fi`: `each` gets every file of the
    /// folder in storage order with the crate's stream of it, and returns
    /// whether to go on.
    fn decode_folder(
        &mut self,
        fi: u32,
        budget: &mut Budget,
        each: &mut FolderVisit<'_>,
    ) -> Result<(), ArchiveError> {
        let folder = &self.walked.folders[fi as usize];
        if let Some(format) = unsupported_coder(folder) {
            return Err(ArchiveError::Unsupported {
                format,
                need: Some(Tool::SevenZip),
            });
        }
        let mem = folder.mem_needed;
        self.ensure_view()?;
        let _permit = MemoryGate::global().acquire_for(mem, budget)?;
        testing_hooks::folder_pass();
        let Self {
            src, view, walked, ..
        } = self;
        let view = view.as_ref().ok_or_else(|| damaged("7z crate view"))?;
        let files: Vec<&WalkedFile> = walked
            .files
            .iter()
            .filter(|f| f.folder == Some(fi))
            .collect();
        let failed = Arc::clone(&src.failed);
        let password = sevenz_rust2::Password::empty();
        let mut stored: Option<ArchiveError> = None;
        let mut next = 0usize;
        let mut stopped = false;
        let result = BlockDecoder::new(1, fi as usize, view, &password, src).for_each_entries(
            &mut |_, stream: &mut dyn Read| {
                let Some(file) = files.get(next) else {
                    stored = Some(damaged("7z folder has more files than listed"));
                    return Ok(false);
                };
                next += 1;
                let mut stream = MapErrors {
                    inner: stream,
                    src_failed: Arc::clone(&failed),
                };
                match each(file, &mut stream, budget) {
                    Ok(true) => Ok(true),
                    Ok(false) => {
                        stopped = true;
                        Ok(false)
                    }
                    Err(e) => {
                        stored = Some(e);
                        Ok(false)
                    }
                }
            },
        );
        if let Some(e) = stored {
            return Err(e);
        }
        result.map_err(|e| crate_error(e, &failed))?;
        if !stopped && next != files.len() {
            return Err(damaged("7z folder has fewer files than listed"));
        }
        Ok(())
    }

    /// The decoded bytes of entry `index`, verified at EOF, before the
    /// guard wrapper.
    fn open_decoded<'a>(
        &'a mut self,
        index: usize,
        budget: &'a mut Budget,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
        let file = self.readable(index)?.clone();
        let (size, crc) = (file.entry.size, file.entry.crc32);
        let Some(fi) = file.folder else {
            return Ok(Box::new(VerifyingReader::new(io::empty(), size, crc)));
        };
        let folder = self.walked.folders[fi as usize].clone();
        if let Some(format) = unsupported_coder(&folder) {
            return Err(ArchiveError::Unsupported {
                format,
                need: Some(Tool::SevenZip),
            });
        }
        let Some(chain) = simple_chain(&folder).ok() else {
            return self.open_buffered(fi, &file, budget);
        };
        self.ensure_view()?;
        let permit = MemoryGate::global().acquire_for(folder.mem_needed, budget)?;
        self.src
            .seek(SeekFrom::Start(folder.pack_pos))
            .map_err(ArchiveError::Io)?;
        let failed = Arc::clone(&self.src.failed);
        let packed: Box<dyn Read + 'a> = Box::new((&mut self.src).take(folder.pack_size));
        let decoded = build_chain(packed, &folder, &chain, self.limits.decoder_mem_cap)?;
        let mut charged = ChargedRead::new(
            MapErrors {
                inner: decoded,
                src_failed: failed,
            },
            budget,
        );
        skip(&mut charged, file.offset)?;
        Ok(Box::new(Holding {
            inner: VerifyingReader::new(charged.take(size), size, crc),
            _permit: permit,
        }))
    }

    /// `open_entry` of a folder only the crate decodes (BCJ2): the entry is
    /// buffered, at most `plan_head_budget` bytes of it.
    fn open_buffered<'a>(
        &'a mut self,
        fi: u32,
        file: &WalkedFile,
        budget: &'a mut Budget,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
        let target = file.entry.index;
        let (size, crc) = (file.entry.size, file.entry.crc32);
        let cap = self.limits.plan_head_budget.min(size);
        let mut buf = Vec::new();
        let mut complete = false;
        self.decode_folder(fi, budget, &mut |f, stream, budget| {
            let mut verified = VerifyingReader::new(
                ChargedRead::new(stream, budget),
                f.entry.size,
                f.entry.crc32,
            );
            if f.entry.index != target {
                drain(&mut verified)?;
                return Ok(true);
            }
            (&mut verified)
                .take(cap)
                .read_to_end(&mut buf)
                .map_err(ArchiveError::from)?;
            if buf.len() as u64 == size {
                drain(&mut verified)?;
                complete = true;
            }
            Ok(false)
        })?;
        let head = io::Cursor::new(buf);
        if complete {
            return Ok(Box::new(VerifyingReader::new(head, size, crc)));
        }
        Ok(Box::new(head.chain(Spent)))
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
        let mut empty = Vec::new();
        let mut by_folder: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for &index in &order {
            match self.readable(index)?.folder {
                Some(fi) => by_folder.entry(fi).or_default().push(index),
                None => empty.push(index),
            }
        }
        for index in empty {
            let entry = self.walked.files[index].entry.clone();
            let mut stream = VerifyingReader::new(io::empty(), entry.size, entry.crc32);
            sink(&entry, &mut stream)?;
            drain(&mut stream)?;
        }
        for (fi, list) in by_folder {
            let last = list.last().copied().unwrap_or(0);
            self.decode_folder(fi, budget, &mut |f, stream, budget| {
                let index = f.entry.index;
                let mut verified = VerifyingReader::new(
                    ChargedRead::new(stream, budget),
                    f.entry.size,
                    f.entry.crc32,
                );
                if list.binary_search(&index).is_ok() {
                    sink(&f.entry, &mut verified)?;
                }
                // The rest of the entry: checked, and the next one lines up.
                drain(&mut verified)?;
                Ok(index < last)
            })?;
        }
        Ok(())
    }
}

/// The bytes of a 7z that its header walk reads, copied into memory: the
/// start header, the next header and an encoded header's packed stream
/// (S5). A read anywhere else fails and sets `strayed`; seeks past the end
/// read as EOF, like a file of `len` bytes.
struct Snapshot {
    len: u64,
    pos: u64,
    ranges: Vec<(u64, Vec<u8>)>,
    strayed: bool,
}

impl Snapshot {
    /// Copies the header ranges of `src` (`len` bytes, as walked): where
    /// the start header now points, and the encoded header's packed
    /// stream of `walked`. Bounded by `max_header` (header) and twice that
    /// (packed stream); a start header pointing elsewhere is `Damaged`.
    fn capture<R: Read + Seek>(
        src: &mut R,
        len: u64,
        walked: &Walked,
        limits: &Limits,
    ) -> Result<Self, ArchiveError> {
        let changed = || damaged("7z changed since it was listed");
        let mut start = vec![0u8; START_HEADER_LEN as usize];
        read_at(src, 0, &mut start)?;
        let next_offset = le_u64(&start[12..20]);
        let next_size = le_u64(&start[20..28]);
        let mut ranges = vec![(0, start)];
        let header_start = START_HEADER_LEN
            .checked_add(next_offset)
            .filter(|&at| at.checked_add(next_size).is_some_and(|end| end <= len))
            .filter(|_| next_size <= limits.max_header)
            .ok_or_else(changed)?;
        let mut pack = None;
        if let Some((at, size)) = walked.header_pack {
            if size > limits.max_header.saturating_mul(2)
                || at.checked_add(size).is_none_or(|end| end > len)
            {
                return Err(changed());
            }
            pack = Some((at, size));
        }
        for (at, size) in std::iter::once((header_start, next_size)).chain(pack) {
            if size == 0 {
                continue;
            }
            // Both sizes are bounded just above.
            let mut buf = vec![0u8; usize::try_from(size).map_err(|_| changed())?];
            read_at(src, at, &mut buf)?;
            ranges.push((at, buf));
        }
        Ok(Self {
            len,
            pos: 0,
            ranges,
            strayed: false,
        })
    }
}

impl Read for Snapshot {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.len {
            return Ok(0);
        }
        let pos = self.pos;
        let hit = self.ranges.iter().find_map(|(at, bytes)| {
            let off = usize::try_from(pos.checked_sub(*at)?).ok()?;
            bytes.get(off..).filter(|rest| !rest.is_empty())
        });
        let Some(rest) = hit else {
            self.strayed = true;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "7z read outside the validated header",
            ));
        };
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for Snapshot {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        let pos =
            pos.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "7z seek out of range"))?;
        self.pos = pos;
        Ok(pos)
    }
}

/// Reads exactly `buf.len()` bytes of `src` at `at`.
fn read_at<R: Read + Seek>(src: &mut R, at: u64, buf: &mut [u8]) -> Result<(), ArchiveError> {
    src.seek(SeekFrom::Start(at)).map_err(ArchiveError::Io)?;
    src.read_exact(buf).map_err(ArchiveError::Io)
}

/// A stream that ends a buffered head: the entry was not fully read.
struct Spent;

impl Read for Spent {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(ArchiveError::BudgetSpent {
            what: BudgetKind::DecodedBytes,
        }
        .into())
    }
}

/// Reads `r` to its end (verifying it, for a [`VerifyingReader`]).
fn drain(r: &mut dyn Read) -> Result<(), ArchiveError> {
    io::copy(r, &mut io::sink()).map_err(ArchiveError::from)?;
    Ok(())
}

/// Decodes and drops the `n` bytes before an entry in its folder.
fn skip(r: &mut impl Read, n: u64) -> Result<(), ArchiveError> {
    let copied = io::copy(&mut r.take(n), &mut io::sink()).map_err(ArchiveError::from)?;
    if copied != n {
        return Err(damaged("7z folder ends before its file"));
    }
    Ok(())
}

/// The decoders of a folder of one-in/one-out coders, from the packed
/// stream to the folder output; each stage is bounded by its declared
/// output size. LZMA/LZMA2 go through the one checked constructor (S3).
fn build_chain<'a>(
    packed: Box<dyn Read + 'a>,
    folder: &FolderInfo,
    chain: &[usize],
    cap: u64,
) -> Result<Box<dyn Read + 'a>, ArchiveError> {
    let mut reader = packed;
    for &c in chain {
        let coder = &folder.coders[c];
        let out = folder.unpack_sizes[c];
        reader = match coder.kind {
            CoderKind::Copy => reader,
            CoderKind::Lzma(p) => open_decoder(LzmaKind::Lzma(p), Some(out), cap, reader)?,
            CoderKind::Lzma2(p) => open_decoder(LzmaKind::Lzma2(p), Some(out), cap, reader)?,
            CoderKind::Ppmd { order, mem } => {
                lzma::check(coder.mem_needed, cap)?;
                let decoder = Ppmd7Decoder::new(reader, u32::from(order), mem)
                    .map_err(|e| damaged(format!("7z ppmd: {e}")))?;
                Box::new(decoder)
            }
            CoderKind::Bzip2 => Box::new(MultiBzDecoder::new(reader)),
            CoderKind::Deflate => Box::new(DeflateDecoder::new(reader)),
            CoderKind::Bcj { kind, start } => bcj_reader(reader, kind, start)?,
            CoderKind::Delta { distance } => {
                Box::new(DeltaReader::new(reader, usize::from(distance)))
            }
            CoderKind::Aes => return Err(ArchiveError::Encrypted),
            CoderKind::Deflate64 | CoderKind::Other => {
                return Err(ArchiveError::Unsupported {
                    format: unsupported_coder(folder).unwrap_or_else(|| "7z".into()),
                    need: Some(Tool::SevenZip),
                })
            }
        };
        reader = Box::new(reader.take(out));
    }
    Ok(reader)
}

/// The crate's view of the archive must be the walk (S5, spec R17): a file
/// that changed under the open handle, or a crate that read the header
/// another way, is `Damaged` before a byte is decoded.
fn cross_check(walked: &Walked, view: &sevenz_rust2::Archive) -> Result<(), ArchiveError> {
    let differs = |what: &str| damaged(format!("7z crate view differs: {what}"));
    if view.files.len() != walked.files.len() {
        return Err(differs("file count"));
    }
    if view.blocks.len() != walked.folders.len() {
        return Err(differs("folder count"));
    }
    if let Some(first) = walked.folders.first() {
        if view.pack_pos().checked_add(START_HEADER_LEN) != Some(first.pack_pos) {
            return Err(differs("pack position"));
        }
    }
    let pack_sizes: Vec<u64> = walked
        .folders
        .iter()
        .flat_map(|f| f.pack_sizes.iter().copied())
        .collect();
    if view.pack_sizes() != pack_sizes.as_slice() {
        return Err(differs("pack sizes"));
    }
    for (block, folder) in view.blocks.iter().zip(&walked.folders) {
        let same_coders = block.coders.len() == folder.coders.len()
            && block.coders.iter().zip(&folder.coders).all(|(a, b)| {
                a.encoder_method_id() == b.id.as_slice() && a.properties() == b.props
            });
        if !same_coders {
            return Err(differs("coders"));
        }
        if block.get_unpack_size() != folder.unpack_size {
            return Err(differs("folder size"));
        }
        let crc = block.has_crc.then_some(block.crc as u32);
        if crc != folder.crc || block.crc > u64::from(u32::MAX) {
            return Err(differs("folder crc"));
        }
    }
    let map = &view.stream_map;
    for (i, (file, ours)) in view.files.iter().zip(&walked.files).enumerate() {
        let units: Vec<u16> = ours
            .entry
            .raw_name
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u16::from_le_bytes(*p))
            .collect();
        if String::from_utf16(&units).ok().as_deref() != Some(file.name.as_str()) {
            return Err(differs("name"));
        }
        if file.has_stream != ours.folder.is_some() || file.is_anti_item {
            return Err(differs("streams"));
        }
        if (ours.entry.kind == EntryKind::Dir) != (file.is_directory && !file.has_stream) {
            return Err(differs("directories"));
        }
        let attributes = file
            .has_windows_attributes
            .then_some(file.windows_attributes);
        if attributes != ours.attributes {
            return Err(differs("attributes"));
        }
        if file.has_stream {
            let crc = file.has_crc.then_some(file.crc as u32);
            if file.size != ours.entry.size || crc != ours.entry.crc32 {
                return Err(differs("size or crc"));
            }
            let block = map.file_block_index.get(i).copied().flatten();
            if block.map(|b| b as u64) != ours.folder.map(u64::from) {
                return Err(differs("folder of a file"));
            }
        }
    }
    // The crate decodes a folder's files as one run from its first file;
    // an empty file inside that run would shift every later one.
    for (fi, folder) in walked.folders.iter().enumerate() {
        let mut ours = walked
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.folder == Some(fi as u32))
            .map(|(i, _)| i);
        let Some(first) = ours.next() else {
            continue;
        };
        if map.block_first_file_index.get(fi) != Some(&first) {
            return Err(differs("first file of a folder"));
        }
        let mut count = 1u64;
        for (k, i) in ours.enumerate() {
            if i != first + k + 1 {
                return Err(ArchiveError::Unsupported {
                    format: "7z (empty file inside a solid block)".into(),
                    need: Some(Tool::SevenZip),
                });
            }
            count += 1;
        }
        if count != folder.num_substreams {
            return Err(differs("files of a folder"));
        }
    }
    Ok(())
}

impl<R: Read + Seek + Send + SourceStamp> ArchiveReader for SevenZReader<R> {
    fn format(&self) -> Format {
        Format::SevenZip
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

    /// One pass per folder in storage order. The decoding runs inside the
    /// guard, and so does `sink` (it is called from inside the crate's
    /// pass): a panic in either poisons the reader.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_round_trips() {
        for v in [
            0u64,
            1,
            0x7F,
            0x80,
            0x3FFF,
            0x4000,
            1 << 20,
            1 << 35,
            u64::MAX - 1,
            u64::MAX,
        ] {
            let mut buf = Vec::new();
            crate::archive::testing::sz_number(&mut buf, v);
            let mut cur = Cur::new(&buf);
            assert_eq!(cur.number().expect("number"), v, "{v:#x}");
            assert!(cur.at_end(), "{v:#x}");
        }
    }

    #[test]
    fn truncated_number_damaged() {
        let mut cur = Cur::new(&[0xFF, 1, 2]);
        assert!(matches!(cur.number(), Err(ArchiveError::Damaged { .. })));
    }

    #[test]
    fn count_tells_lies_from_big_archives() {
        let mut buf = Vec::new();
        crate::archive::testing::sz_number(&mut buf, 1_000_000);
        assert!(matches!(
            Cur::new(&buf).count(10, 2, "file"),
            Err(ArchiveError::Damaged { .. })
        ));
        let mut buf = Vec::new();
        crate::archive::testing::sz_number(&mut buf, 20);
        buf.extend_from_slice(&[0; 40]);
        assert!(matches!(
            Cur::new(&buf).count(10, 2, "file"),
            Err(ArchiveError::TooManyEntries { count: 20 })
        ));
    }

    #[test]
    fn cross_check_refuses_a_crate_view_of_other_bytes() {
        use crate::archive::testing::sevenz_copy;
        // Same shape, another name: only the names differ.
        let ours = sevenz_copy(&[("Game (Track 01).bin", b"data")]);
        let theirs = sevenz_copy(&[("Game (Track 02).bin", b"data")]);
        let walked = walk(
            &mut io::Cursor::new(&ours),
            ours.len() as u64,
            &Limits::default(),
        )
        .expect("walk");
        let same = sevenz_rust2::Archive::read(
            &mut io::Cursor::new(&ours),
            &sevenz_rust2::Password::empty(),
        )
        .expect("crate");
        cross_check(&walked, &same).expect("same bytes agree");
        let other = sevenz_rust2::Archive::read(
            &mut io::Cursor::new(&theirs),
            &sevenz_rust2::Password::empty(),
        )
        .expect("crate");
        let err = cross_check(&walked, &other).expect_err("names differ");
        assert!(
            matches!(&err, ArchiveError::Damaged { detail } if detail.contains("crate view differs: name")),
            "{err:?}"
        );
    }

    #[test]
    fn snapshot_serves_only_the_walked_header() {
        use crate::archive::testing::sevenz_copy;
        let bytes = sevenz_copy(&[("a.bin", b"payload")]);
        let len = bytes.len() as u64;
        let limits = Limits::default();
        let walked = walk(&mut io::Cursor::new(&bytes), len, &limits).expect("walk");
        let mut snap =
            Snapshot::capture(&mut io::Cursor::new(&bytes), len, &walked, &limits).expect("snap");
        assert_eq!(walk(&mut snap, len, &limits).expect("walk again"), walked);
        assert!(!snap.strayed);
        // The payload sits right after the start header: not in the copy.
        snap.seek(SeekFrom::Start(START_HEADER_LEN)).expect("seek");
        let mut buf = [0u8; 4];
        assert!(snap.read(&mut buf).is_err());
        assert!(snap.strayed);
        // Past the end is EOF; before the start is refused.
        snap.seek(SeekFrom::End(0)).expect("seek end");
        assert_eq!(snap.read(&mut buf).expect("eof"), 0);
        assert!(snap.seek(SeekFrom::Current(-(len as i64) - 1)).is_err());
    }

    #[test]
    fn snapshot_refuses_a_start_header_pointing_elsewhere() {
        use crate::archive::testing::sevenz_copy;
        let bytes = sevenz_copy(&[("a.bin", b"payload")]);
        let len = bytes.len() as u64;
        let limits = Limits::default();
        let walked = walk(&mut io::Cursor::new(&bytes), len, &limits).expect("walk");
        let mut bad = bytes.clone();
        bad[12..20].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            Snapshot::capture(&mut io::Cursor::new(&bad), len, &walked, &limits),
            Err(ArchiveError::Damaged { .. })
        ));
    }

    #[test]
    fn filetime_converts_both_sides_of_the_epoch() {
        let t = filetime(116_444_736_000_000_000 + 10_000_000).expect("time");
        assert_eq!(
            t.duration_since(UNIX_EPOCH).expect("after"),
            Duration::from_secs(1)
        );
        assert!(filetime(0).is_some_and(|t| t < UNIX_EPOCH));
    }
}
