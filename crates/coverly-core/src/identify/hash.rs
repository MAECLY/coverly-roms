//! Content hashing of ROM files (spec §16.2): CRC32, MD5 and SHA1 in one
//! streaming pass, with an optional copier/emulator header skipped for a
//! second set of digests, and the inner CRC of a single-file zip read from
//! the central directory without decompressing. Version 2 adds the payload
//! SHA1 (headerless, N64 byte order normalised) that duplicate detection
//! keys on, in the same pass. Read-only.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use sha1::Sha1;

/// Files larger than this are not hashed by default (spec §9 "max file size to hash").
pub const DEFAULT_HASH_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Formats identified by name: multi-file sets, compressed discs, 7z (deferred).
const NAME_ONLY_EXTS: [&str; 10] = [
    "cue", "gdi", "m3u", "ccd", "chd", "7z", "pbp", "cso", "rvz", "zso",
];
const CHUNK: usize = 64 * 1024;
/// A zip with more entries than this is not a single ROM; do not walk it.
const MAX_ZIP_ENTRIES: usize = 4096;
/// Version of the [`HashOutcome`] shape stored in the `quick_ids` memo.
/// Rows written under another version are a miss and are re-hashed lazily.
pub const HASH_OUTCOME_VERSION: u32 = 2;

/// How much work [`hash_rom_mode`] does for a single-entry zip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HashMode {
    /// Central directory only (fill and scan): nothing is decompressed.
    #[default]
    Quick,
    /// Also decode the entry, when it fits the size cap, for its payload SHA1.
    WithPayload,
}

/// Byte order of an N64 image, from its first four bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum N64Order {
    /// Big-endian (`.z64`), the reference order.
    Z64,
    /// Byte-swapped in 16-bit pairs (`.v64`).
    V64,
    /// Little-endian 32-bit words (`.n64`).
    N64,
}

/// The order of an N64 image starting with `head`; `None` when it does not
/// start with the N64 boot magic in any order.
pub fn n64_to_z64(head: &[u8]) -> Option<N64Order> {
    match head.get(..4)? {
        [0x80, 0x37, 0x12, 0x40] => Some(N64Order::Z64),
        [0x37, 0x80, 0x40, 0x12] => Some(N64Order::V64),
        [0x40, 0x12, 0x37, 0x80] => Some(N64Order::N64),
        _ => None,
    }
}

impl N64Order {
    /// Rewrites the whole 4-byte words of `bytes` in big-endian order; a
    /// trailing partial word is left as is.
    pub(crate) fn normalise(self, bytes: &mut [u8]) {
        match self {
            N64Order::Z64 => {}
            N64Order::V64 => bytes
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .for_each(|c| c.swap(0, 1)),
            N64Order::N64 => bytes
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|c| c.reverse()),
        }
    }
}

/// Header present in some dumps but not in the DAT's headerless entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeaderRule {
    /// No header.
    #[default]
    Plain,
    /// iNES: `NES\x1a`, 16 bytes.
    Nes,
    /// fwNES: `FDS\x1a`, 16 bytes.
    Fds,
    /// SNES copier header: 512 bytes when `len % 1024 == 512`.
    SnesCopier,
    /// SMS/Game Gear copier header: 512 bytes when `len % 16384 == 512`.
    SmsCopier,
    /// Lynx: `LYNX`, 64 bytes.
    Lynx,
    /// Atari 7800: `ATARI7800` at offset 1, 128 bytes.
    A7800,
    /// Nintendo 64: no header, but a `.v64`/`.n64` image is rewritten in
    /// big-endian order for its payload SHA1. Only this rule does that, so a
    /// file of another system is never byte-swapped.
    #[serde(rename = "n64-order")]
    N64,
}

impl HeaderRule {
    /// Parses a `systems.toml` value.
    pub fn parse(s: &str) -> Option<HeaderRule> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "" | "plain" => HeaderRule::Plain,
            "nes" => HeaderRule::Nes,
            "fds" => HeaderRule::Fds,
            "snes-copier" => HeaderRule::SnesCopier,
            "sms-copier" => HeaderRule::SmsCopier,
            "lynx" => HeaderRule::Lynx,
            "a7800" => HeaderRule::A7800,
            "n64-order" => HeaderRule::N64,
            _ => return None,
        })
    }

    /// Length of the header at the start of a file whose first bytes are
    /// `head` and whose size is `file_len`; 0 when absent.
    pub fn header_len(self, head: &[u8], file_len: u64) -> usize {
        match self {
            HeaderRule::Nes if head.starts_with(b"NES\x1a") => 16,
            HeaderRule::Fds if head.starts_with(b"FDS\x1a") => 16,
            HeaderRule::Lynx if head.starts_with(b"LYNX") => 64,
            HeaderRule::A7800 if head.get(1..10) == Some(b"ATARI7800".as_slice()) => 128,
            HeaderRule::SnesCopier if file_len % 1024 == 512 => 512,
            HeaderRule::SmsCopier if file_len % 16384 == 512 => 512,
            _ => 0,
        }
    }
}

/// Digests of a byte stream, upper-case hex.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digests {
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
    pub size: u64,
}

/// Digests of the whole file and, when a header was found, of the rest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RomHashes {
    pub full: Digests,
    pub headerless: Option<Digests>,
    /// SHA1 of the headerless bytes in big-endian N64 order (v2): equal for
    /// the same game whatever its copier header or N64 byte order.
    #[serde(default)]
    pub payload_sha1: Option<String>,
}

/// What hashing a ROM produced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum HashOutcome {
    /// The file contents were hashed.
    Content(RomHashes),
    /// A zip holding one file: its CRC from the central directory.
    ZipEntry {
        crc32: String,
        size: u64,
        inner_name: String,
        /// Payload SHA1 of the entry (v2), set only by
        /// [`HashMode::WithPayload`] when it decoded within the size cap.
        #[serde(default)]
        payload_sha1: Option<String>,
    },
    /// Larger than the cap: identify by name.
    TooLarge { size: u64 },
    /// A format identified by name.
    NameOnly,
}

/// Hashes `path` (extension `ext`, any case) with `rule`, reading nothing
/// when the format is name-only and only the central directory of a zip.
pub fn hash_rom(
    path: &Path,
    ext: &str,
    rule: HeaderRule,
    max_bytes: u64,
) -> io::Result<HashOutcome> {
    hash_rom_mode(path, ext, rule, max_bytes, HashMode::Quick)
}

/// [`hash_rom`] with a choice of `mode`: [`HashMode::WithPayload`] also
/// decodes a single-entry zip of at most `max_bytes` for its payload SHA1.
pub fn hash_rom_mode(
    path: &Path,
    ext: &str,
    rule: HeaderRule,
    max_bytes: u64,
    mode: HashMode,
) -> io::Result<HashOutcome> {
    let ext = ext.to_ascii_lowercase();
    if NAME_ONLY_EXTS.contains(&ext.as_str()) {
        return Ok(HashOutcome::NameOnly);
    }
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if ext == "zip" {
        if let Some((mut outcome, index)) = zip_outcome(&mut file) {
            if let (HashMode::WithPayload, Some(index)) = (mode, index) {
                if let HashOutcome::ZipEntry {
                    crc32,
                    size,
                    payload_sha1,
                    ..
                } = &mut outcome
                {
                    if *size <= max_bytes {
                        *payload_sha1 = zip_payload(&mut file, index, crc32, *size, rule);
                    }
                }
            }
            return Ok(outcome);
        }
        file.seek(SeekFrom::Start(0))?;
    }
    if size > max_bytes {
        return Ok(HashOutcome::TooLarge { size });
    }
    hash_stream(file, size, rule).map(HashOutcome::Content)
}

/// Payload SHA1 of entry `index`, decoding at most `size` bytes; `None` when
/// it does not decode to `size` bytes with the central-directory `crc32`.
fn zip_payload(
    file: &mut File,
    index: usize,
    crc32: &str,
    size: u64,
    rule: HeaderRule,
) -> Option<String> {
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut archive = zip::ZipArchive::new(&mut *file).ok()?;
    let entry = archive.by_index(index).ok()?;
    // One byte past `size` shows an entry that inflates to more than it says.
    let h = hash_stream(entry.take(size.saturating_add(1)), size, rule).ok()?;
    (h.full.size == size && h.full.crc32.eq_ignore_ascii_case(crc32))
        .then_some(h.payload_sha1)
        .flatten()
}

/// `None` when the file is not a readable zip (then it is hashed as is);
/// otherwise the outcome and, for a single entry, its index.
fn zip_outcome(file: &mut File) -> Option<(HashOutcome, Option<usize>)> {
    let name_only = Some((HashOutcome::NameOnly, None));
    let mut archive = zip::ZipArchive::new(&mut *file).ok()?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return name_only;
    }
    let mut found = None;
    for i in 0..archive.len() {
        let Ok(entry) = archive.by_index_raw(i) else {
            return name_only;
        };
        if entry.is_dir() {
            continue;
        }
        if found.is_some() {
            return name_only;
        }
        let Ok(name) = entry.name() else {
            return name_only;
        };
        found = Some((i, entry.crc32(), entry.size(), name.into_owned()));
    }
    match found {
        Some((index, crc, size, inner_name)) => Some((
            HashOutcome::ZipEntry {
                crc32: format!("{crc:08X}"),
                size,
                inner_name,
                payload_sha1: None,
            },
            Some(index),
        )),
        None => name_only,
    }
}

struct Hasher {
    crc: crc32fast::Hasher,
    md5: Md5,
    sha1: Sha1,
    size: u64,
}

impl Hasher {
    fn new() -> Hasher {
        Hasher {
            crc: crc32fast::Hasher::new(),
            md5: Md5::new(),
            sha1: Sha1::new(),
            size: 0,
        }
    }
    fn update(&mut self, bytes: &[u8]) {
        self.crc.update(bytes);
        self.md5.update(bytes);
        self.sha1.update(bytes);
        self.size += bytes.len() as u64;
    }
    fn finish(self) -> Digests {
        Digests {
            crc32: format!("{:08X}", self.crc.finalize()),
            md5: hex_upper(&self.md5.finalize()),
            sha1: hex_upper(&self.sha1.finalize()),
            size: self.size,
        }
    }
}

pub(crate) fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// Hashes `size` bytes of `reader` with `rule` in one pass: every byte feeds
/// the full digests, bytes after the header feed the headerless digests and
/// the payload SHA1 (through a byte-order fix when `rule` is
/// [`HeaderRule::N64`] and the bytes are a byte-swapped or little-endian
/// image).
pub fn hash_stream<R: Read>(mut reader: R, size: u64, rule: HeaderRule) -> io::Result<RomHashes> {
    let mut buf = vec![0u8; CHUNK];
    let mut full = Hasher::new();
    let mut body = Hasher::new();
    // Only a swapped N64 image needs a payload digest of its own.
    let mut n64: Option<(N64Order, Sha1)> = None;
    let mut header = None;
    let mut to_skip = 0usize;
    loop {
        let n = read_full(&mut reader, &mut buf)?;
        if n == 0 {
            break;
        }
        if header.is_none() {
            let len = rule.header_len(&buf[..n], size);
            header = Some(len);
            to_skip = len;
            if rule == HeaderRule::N64 {
                n64 = n64_to_z64(&buf[..n])
                    .filter(|o| *o != N64Order::Z64)
                    .map(|o| (o, Sha1::new()));
            }
        }
        full.update(&buf[..n]);
        let skip = to_skip.min(n);
        to_skip -= skip;
        body.update(&buf[skip..n]);
        if let Some((order, sha)) = &mut n64 {
            // Every chunk but the last is a whole number of words.
            order.normalise(&mut buf[..n]);
            sha.update(&buf[..n]);
        }
    }
    let header = header.unwrap_or(0) as u64;
    let full = full.finish();
    let headerless = (header > 0 && size > header).then(|| body.finish());
    let payload_sha1 = match n64 {
        Some((_, sha)) => hex_upper(&sha.finalize()),
        None => headerless.as_ref().unwrap_or(&full).sha1.clone(),
    };
    Ok(RomHashes {
        full,
        headerless,
        payload_sha1: Some(payload_sha1),
    })
}

/// Reads until `buf` is full or EOF, so the first chunk holds the header.
fn read_full<R: Read>(file: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}
