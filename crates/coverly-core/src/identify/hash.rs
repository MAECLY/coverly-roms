//! Content hashing of ROM files (spec §16.2): CRC32, MD5 and SHA1 in one
//! streaming pass, with an optional copier/emulator header skipped for a
//! second set of digests, and the inner CRC of a single-file zip read from
//! the central directory without decompressing. Read-only.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use sha1::Sha1;

/// Files larger than this are not hashed by default (spec §9 "max file size to hash").
pub const DEFAULT_HASH_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Formats identified by name: multi-file sets, compressed discs, 7z (deferred).
const NAME_ONLY_EXTS: [&str; 9] = ["cue", "gdi", "m3u", "ccd", "chd", "7z", "pbp", "cso", "rvz"];
const CHUNK: usize = 64 * 1024;
/// A zip with more entries than this is not a single ROM; do not walk it.
const MAX_ZIP_ENTRIES: usize = 4096;

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
    /// Lynx: `LYNX`, 64 bytes.
    Lynx,
    /// Atari 7800: `ATARI7800` at offset 1, 128 bytes.
    A7800,
}

impl HeaderRule {
    /// Parses a `systems.toml` value.
    pub fn parse(s: &str) -> Option<HeaderRule> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "" | "plain" => HeaderRule::Plain,
            "nes" => HeaderRule::Nes,
            "fds" => HeaderRule::Fds,
            "snes-copier" => HeaderRule::SnesCopier,
            "lynx" => HeaderRule::Lynx,
            "a7800" => HeaderRule::A7800,
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
    let ext = ext.to_ascii_lowercase();
    if NAME_ONLY_EXTS.contains(&ext.as_str()) {
        return Ok(HashOutcome::NameOnly);
    }
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if ext == "zip" {
        if let Some(outcome) = zip_outcome(&mut file) {
            return Ok(outcome);
        }
        file.seek(SeekFrom::Start(0))?;
    }
    if size > max_bytes {
        return Ok(HashOutcome::TooLarge { size });
    }
    stream(file, size, rule).map(HashOutcome::Content)
}

/// `None` when the file is not a readable zip (then it is hashed as is).
fn zip_outcome(file: &mut File) -> Option<HashOutcome> {
    let mut archive = zip::ZipArchive::new(&mut *file).ok()?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Some(HashOutcome::NameOnly);
    }
    let mut found = None;
    for i in 0..archive.len() {
        let Ok(entry) = archive.by_index_raw(i) else {
            return Some(HashOutcome::NameOnly);
        };
        if entry.is_dir() {
            continue;
        }
        if found.is_some() {
            return Some(HashOutcome::NameOnly);
        }
        let Ok(name) = entry.name() else {
            return Some(HashOutcome::NameOnly);
        };
        found = Some((entry.crc32(), entry.size(), name.into_owned()));
    }
    Some(match found {
        Some((crc, size, inner_name)) => HashOutcome::ZipEntry {
            crc32: format!("{crc:08X}"),
            size,
            inner_name,
        },
        None => HashOutcome::NameOnly,
    })
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

/// One pass: every byte feeds `full`, bytes after the header feed `body`.
fn stream(mut file: File, size: u64, rule: HeaderRule) -> io::Result<RomHashes> {
    let mut buf = vec![0u8; CHUNK];
    let mut full = Hasher::new();
    let mut body = Hasher::new();
    let mut header = None;
    let mut to_skip = 0usize;
    loop {
        let n = read_full(&mut file, &mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        if header.is_none() {
            let len = rule.header_len(chunk, size);
            header = Some(len);
            to_skip = len;
        }
        full.update(chunk);
        let skip = to_skip.min(n);
        to_skip -= skip;
        body.update(&chunk[skip..]);
    }
    let header = header.unwrap_or(0) as u64;
    Ok(RomHashes {
        full: full.finish(),
        headerless: (header > 0 && size > header).then(|| body.finish()),
    })
}

/// Reads until `buf` is full or EOF, so the first chunk holds the header.
fn read_full(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
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
