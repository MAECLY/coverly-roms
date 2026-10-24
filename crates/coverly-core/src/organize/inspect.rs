//! Opening a candidate (organise spec §7.2; archives spec §3, §6.1): a raw
//! file, or any container through [`archive::open_archive`]. The container
//! is decided by **magic first**; the extension is only a fallback for
//! bytes without magic, and a mismatch between the two is a note, never
//! damage (D1). Entries are listed from the archive's validated listing,
//! ROM-like entries are sniffed from at most [`STREAM_CAP`] decoded bytes,
//! and a missing CRC comes from a bounded decode of entries up to
//! [`CRC_DECODE_CAP`]. Every decode is charged to one plan-head
//! [`Budget`] per archive; running out is "not fully read", never damage.
//! Never extracts.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::archive::detect::{by_extension, detect_container, multi_volume_part, EOCD_TAIL};
use crate::archive::{
    self, file_entries, ArchiveError, ArchiveReader, Budget, Compressor, Entry, Format, Limits,
    Tool, ToolsOnDemand,
};
use crate::identify::sniff::{
    sniff, ContainerMagic, FileAt, NotARom, SliceAt, Sniff, StreamAt, PREFIX, STREAM_CAP,
};
use crate::scan::classify::{extension, is_appledouble, is_junk_file};
use crate::systems::{ExtClass, SystemDb};

/// Archives with more entries than this are too big to open safely (the
/// archive reader's entry cap).
pub const MAX_ZIP_ENTRIES: usize = archive::MAX_ENTRIES as usize;
/// Archives with more ROM-like entries than this are collections, not games.
pub const MAX_ROM_ENTRIES: usize = 8;
/// ROM-like entries without a stored CRC (tar, a compressor without a
/// trailer CRC) are decoded whole for one when at most this large (archives
/// spec §6.1: every cartridge).
pub const CRC_DECODE_CAP: u64 = 64 * 1024 * 1024;

/// Note: an entry is encrypted (its CRC is known, its bytes are not).
pub const NOTE_ENCRYPTED: &str = "encrypted";
/// Note: an entry uses a compression method this build cannot decode.
pub const NOTE_UNSUPPORTED_COMPRESSION: &str = "unsupported-compression";

const MIB: u64 = 1024 * 1024;

/// How the candidate is packaged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Container {
    Raw,
    Zip,
    SevenZip,
    Chd,
    Rar,
    /// A tar, plain or inside a compressor.
    Tar,
    /// A single-file compressor around one ROM (`game.gba.gz`).
    Gzip,
    Bzip2,
    Xz,
    Zstd,
    Lz4,
    /// The legacy `.lzma` ("LZMA alone") format.
    Lzma,
    /// One part of a multi-volume archive.
    MultiVolume,
}

impl Container {
    /// An archive whose entries are listed and read (not a raw file, a
    /// CHD disc image or one part of a multi-volume set).
    pub fn is_archive(self) -> bool {
        !matches!(self, Self::Raw | Self::Chd | Self::MultiVolume)
    }

    /// The container a magic (or an extension's proposed magic) names.
    /// Formats recognised but never opened (cab, arj, lzh, compress, ECM)
    /// are raw here: the archive reader reports them unsupported.
    fn of_magic(m: ContainerMagic) -> Container {
        use ContainerMagic as M;
        match m {
            M::Zip | M::ZipEocdOnly => Self::Zip,
            M::ZipSpanned => Self::MultiVolume,
            M::SevenZip => Self::SevenZip,
            M::Rar4 | M::Rar5 => Self::Rar,
            M::Gzip => Self::Gzip,
            M::Bzip2 => Self::Bzip2,
            M::Xz => Self::Xz,
            M::Zstd => Self::Zstd,
            M::Lz4 => Self::Lz4,
            M::Tar => Self::Tar,
            M::LzmaAlone => Self::Lzma,
            M::Chd => Self::Chd,
            M::Cab | M::Arj | M::Lzh | M::Compress | M::Ecm => Self::Raw,
        }
    }

    /// The container of an opened archive.
    fn of_format(f: Format) -> Container {
        match f {
            Format::Zip => Self::Zip,
            Format::SevenZip => Self::SevenZip,
            Format::Rar { .. } => Self::Rar,
            Format::Tar { .. } => Self::Tar,
            Format::Single(c) => match c {
                Compressor::Gzip => Self::Gzip,
                Compressor::Bzip2 => Self::Bzip2,
                Compressor::Xz => Self::Xz,
                Compressor::Zstd => Self::Zstd,
                Compressor::Lz4 => Self::Lz4,
                Compressor::LzmaAlone => Self::Lzma,
            },
        }
    }
}

/// A ROM-like content of the candidate (the file itself when raw).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Payload {
    /// Entry name inside the archive (file name when raw).
    pub name: String,
    /// Lowercase extension of `name`, without the dot.
    pub ext: String,
    /// Uncompressed size.
    pub size: u64,
    /// CRC32 as 8 upper-case hex digits (archive entries only: raw files
    /// are hashed later, only when a DAT needs it).
    pub crc32: Option<String>,
    pub sniff: Sniff,
}

/// One file entry of an archive, as its listing states it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ZipEntryInfo {
    pub name: String,
    /// CRC32 as 8 upper-case hex digits: the stored one, or one decoded
    /// for a ROM-like entry of a format that stores none; `None` otherwise.
    pub crc32: Option<String>,
    pub size: u64,
}

/// Why a candidate cannot be used as it is (mapped to a `LeftReason` by
/// `organize::plan::left_reason_for`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ArchiveIssue {
    /// The format's integrity listing failed (archives spec §4.3), a
    /// decoder failed or panicked, or a CRC/size did not match.
    Damaged,
    /// A container Coverly recognises but cannot open; `need` names the
    /// external tool that would (fields absent before archives).
    Unsupported {
        #[serde(default)]
        format: Option<String>,
        #[serde(default)]
        need: Option<Tool>,
    },
    /// The archive's header or every payload is password protected.
    Encrypted,
    /// Decoding needs more memory than the decoder cap (S2, S3).
    NeedsMemory { mib: u64 },
    /// Too many entries or declared bytes to open safely: never damage.
    TooBig {
        entries: Option<u64>,
        bytes: Option<u64>,
    },
    /// One part of a multi-volume set (`parts` found next to it).
    MultiVolume { parts: usize },
    /// The only payload is another archive.
    Nested,
    /// The file is a web page or text, whatever its name says.
    NotARom { what: NotARom },
    /// An archive with more than [`MAX_ROM_ENTRIES`] ROM-like entries.
    TooManyRoms { count: usize },
}

fn yes() -> bool {
    true
}

/// What [`inspect_file`] found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Inspection {
    pub container: Container,
    /// ROM-like contents, sniffed (at most [`MAX_ROM_ENTRIES`]).
    pub payloads: Vec<Payload>,
    /// Every file entry of an archive (arcade sets are confirmed by them).
    pub entries: Vec<ZipEntryInfo>,
    pub issue: Option<ArchiveIssue>,
    /// Machine-readable notes ([`NOTE_ENCRYPTED`], …), not prose.
    pub notes: Vec<String>,
    /// CRC32 (upper-case hex) of the prefix read from a raw file: two raw
    /// files of one size with different prefixes cannot be duplicates, so
    /// only files sharing size and prefix are hashed in full.
    #[serde(default)]
    pub head_crc: Option<String>,
    /// The format the archive reader opened.
    #[serde(default)]
    pub format: Option<Format>,
    /// The extension names another archive format than the magic: the
    /// extension (lower-case) and the format found (archives spec §3.2
    /// rule 2, a note).
    #[serde(default)]
    pub mismatch: Option<(String, Format)>,
    /// The listing reached the archive's end (a tar inside a compressor
    /// may stop at the listing budget).
    #[serde(default = "yes")]
    pub listing_complete: bool,
    /// A decode budget ran out: the item is not fully read (Review, never
    /// damage).
    #[serde(default)]
    pub budget_spent: bool,
}

impl Inspection {
    fn new(container: Container) -> Inspection {
        Inspection {
            container,
            payloads: Vec::new(),
            entries: Vec::new(),
            issue: None,
            notes: Vec::new(),
            head_crc: None,
            format: None,
            mismatch: None,
            listing_complete: true,
            budget_spent: false,
        }
    }

    fn note(&mut self, note: &str) {
        if !self.notes.iter().any(|n| n == note) {
            self.notes.push(note.to_string());
        }
    }

    /// Stops with `issue`: nothing listed or sniffed is kept.
    fn fail(&mut self, issue: ArchiveIssue) {
        self.payloads.clear();
        self.entries.clear();
        self.issue = Some(issue);
    }

    fn damaged(&mut self) {
        self.fail(ArchiveIssue::Damaged);
    }

    fn spent(&mut self) {
        self.budget_spent = true;
    }
}

/// Opens the candidate at `path` (`len` bytes) under the default archive
/// [`Limits`] and reports its container, its sniffed ROM-like contents and
/// any issue. See [`inspect_file_with`].
pub fn inspect_file(path: &Path, len: u64, db: &SystemDb) -> io::Result<Inspection> {
    inspect_file_with(path, len, db, &Limits::default(), None)
}

/// [`inspect_file`] under `limits` (the plan's bounds; `external_tools`
/// follows the setting) with the plan's external `tools` (discovered only
/// when a RAR or a 7z needs one, never inside the plan's roots); `None`
/// uses the process-wide discovery. Errors only when the file cannot be
/// opened or read, or the plan is cancelled; every archive problem is an
/// [`ArchiveIssue`].
pub fn inspect_file_with(
    path: &Path,
    len: u64,
    db: &SystemDb,
    limits: &Limits,
    tools: Option<&ToolsOnDemand>,
) -> io::Result<Inspection> {
    let ext = extension(path);
    let mut file = File::open(path)?;
    let mut head = vec![0u8; PREFIX.min(usize::try_from(len).unwrap_or(PREFIX))];
    let n = read_prefix(&mut file, &mut head)?;
    head.truncate(n);
    let tail = read_tail(&mut file, len, head.len())?;
    let first = sniff(&mut SliceAt(&head), head.len() as u64, &ext);
    let magic = detect_container(&head, len, Some(tail.as_deref().unwrap_or(&head)));
    let mut out = Inspection::new(container_for(&ext, magic));
    match out.container {
        Container::Chd => {
            if magic != Some(ContainerMagic::Chd) {
                out.issue = Some(ArchiveIssue::Damaged);
            }
        }
        Container::Raw => {
            if let Some(m) = magic {
                // Recognised, never opened (cab, arj, lzh, compress, ECM).
                out.issue = Some(ArchiveIssue::Unsupported {
                    format: Some(unopened_name(m).to_string()),
                    need: None,
                });
                return Ok(out);
            }
            if multi_volume_part(&file_name(path)).is_some() {
                // A later part of a set has no magic of its own.
                if let Err(ArchiveError::MultiVolume { parts }) =
                    archive::open_archive_with(path, limits, Some(&[]))
                {
                    out.container = Container::MultiVolume;
                    out.issue = Some(ArchiveIssue::MultiVolume { parts: parts.len() });
                    return Ok(out);
                }
            }
            let sniffed = if len <= head.len() as u64 {
                first
            } else {
                sniff(&mut FileAt(file), len, &ext)
            };
            out.head_crc = Some(format!("{:08X}", crc32fast::hash(&head)));
            if let Some(what) = sniffed.not_a_rom {
                out.issue = Some(ArchiveIssue::NotARom { what });
            }
            out.payloads.push(Payload {
                name: file_name(path),
                ext,
                size: len,
                crc32: None,
                sniff: sniffed,
            });
        }
        _ => {
            drop(file);
            inspect_archive(path, &ext, &first, (db, limits, tools), &mut out)?;
        }
    }
    Ok(out)
}

/// Header magic of the first data track of a cue/gdi set.
pub fn inspect_set_primary(track: &Path) -> io::Result<Sniff> {
    let file = File::open(track)?;
    let len = file.metadata()?.len();
    Ok(sniff(&mut FileAt(file), len, &extension(track)))
}

/// Reads the start of `file` into `buf` (fewer bytes at end of file).
fn read_prefix(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match file.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

/// The end of a file longer than its head (a prefixed zip's end record
/// lies there); `None` when the head is the whole file.
fn read_tail(file: &mut File, len: u64, head_len: usize) -> io::Result<Option<Vec<u8>>> {
    if len <= head_len as u64 {
        return Ok(None);
    }
    let n = len.min(EOCD_TAIL as u64);
    file.seek(SeekFrom::Start(len - n))?;
    let mut tail = vec![0u8; usize::try_from(n).unwrap_or(EOCD_TAIL)];
    let got = read_prefix(file, &mut tail)?;
    tail.truncate(got);
    file.seek(SeekFrom::Start(0))?;
    Ok(Some(tail))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Magic first (a 7z named `.zip` is a 7z; a zip named `.bin` is a zip),
/// the extension only when the bytes carry no magic (archives spec §3.2
/// rule 1): an HTML page named `.zip` is judged as a zip that is not one.
fn container_for(ext: &str, magic: Option<ContainerMagic>) -> Container {
    magic
        .or_else(|| by_extension(ext))
        .map_or(Container::Raw, Container::of_magic)
}

/// The name of a format recognised but never opened (the same names the
/// archive reader reports).
fn unopened_name(m: ContainerMagic) -> &'static str {
    use ContainerMagic as M;
    match m {
        M::Cab => "CAB",
        M::Arj => "ARJ",
        M::Lzh => "LZH",
        M::Compress => "Unix compress",
        M::Ecm => "ECM",
        _ => "archive",
    }
}

/// Whether `ext` (lower-case) names a format the archive reader opens.
fn names_an_archive(ext: &str) -> bool {
    by_extension(ext).is_some_and(|m| Container::of_magic(m).is_archive())
}

// The end-record precheck lives with the zip backend now.
pub use crate::archive::zip::{central_directory_claim, CentralDirectoryClaim};

/// macOS/Windows metadata stored in an archive (`__MACOSX/…`, `._x`,
/// `.DS_Store`, `Thumbs.db`): listed, never a payload.
fn is_metadata_entry(name: &str) -> bool {
    let mut parts = name.split(['/', '\\']).filter(|p| !p.is_empty());
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    parts.any(|p| p.eq_ignore_ascii_case("__MACOSX")) || is_appledouble(base) || is_junk_file(base)
}

/// Files that ride along with a game in a zip (readmes, scene `.nfo`/
/// `.diz`, checksums, scans): they do not make a zip arcade-shaped.
const README_EXTS: [&str; 17] = [
    "txt", "nfo", "diz", "md", "doc", "pdf", "htm", "html", "url", "sfv", "md5", "sha1", "jpg",
    "jpeg", "png", "gif", "bmp",
];

/// An archive entry that is read and sniffed.
struct RomEntry {
    index: usize,
    info: ZipEntryInfo,
    encrypted: bool,
}

/// The issue an archive error stands for, or what else it means: a spent
/// budget marks the inspection; cancellation and plain I/O errors are the
/// caller's.
fn record_error(e: ArchiveError, out: &mut Inspection) -> io::Result<()> {
    match e {
        ArchiveError::Damaged { .. }
        | ArchiveError::ToolFailed { .. }
        | ArchiveError::UnsafeEntry { .. } => out.damaged(),
        ArchiveError::Unsupported { format, need } => out.fail(ArchiveIssue::Unsupported {
            format: Some(format),
            need,
        }),
        ArchiveError::Encrypted => out.fail(ArchiveIssue::Encrypted),
        ArchiveError::NeedsTooMuchMemory { needed, .. } => out.fail(ArchiveIssue::NeedsMemory {
            mib: needed.div_ceil(MIB),
        }),
        ArchiveError::TooManyEntries { count } => out.fail(ArchiveIssue::TooBig {
            entries: Some(count),
            bytes: None,
        }),
        ArchiveError::TooLarge { declared } => out.fail(ArchiveIssue::TooBig {
            entries: None,
            bytes: Some(declared),
        }),
        ArchiveError::MultiVolume { parts } => {
            out.container = Container::MultiVolume;
            out.fail(ArchiveIssue::MultiVolume { parts: parts.len() });
        }
        ArchiveError::BudgetSpent { .. } => {
            out.listing_complete = false;
            out.spent();
        }
        ArchiveError::Io(e)
            if matches!(
                e.kind(),
                io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData
            ) =>
        {
            out.damaged()
        }
        ArchiveError::Io(e) => return Err(e),
        ArchiveError::Cancelled => return Err(ArchiveError::Cancelled.into()),
    }
    Ok(())
}

/// The name an entry is shown and judged by: the validated path, or the
/// stored bytes when the name is unsafe (never extracted under it).
fn entry_name(e: &Entry) -> String {
    match e.unsafe_name {
        Some(_) => String::from_utf8_lossy(&e.raw_name).into_owned(),
        None => e.name.as_str().to_string(),
    }
}

fn inspect_archive(
    path: &Path,
    ext: &str,
    first: &Sniff,
    (db, limits, tools): (&SystemDb, &Limits, Option<&ToolsOnDemand>),
    out: &mut Inspection,
) -> io::Result<()> {
    // Only a RAR, or a 7z with a coder the pure-Rust reader lacks, runs a
    // tool: nothing is discovered for the other formats.
    let may_need_tool = matches!(out.container, Container::Rar | Container::SevenZip);
    let found = match tools {
        Some(t) if may_need_tool && limits.external_tools => Some(t.get()),
        Some(_) => Some(&[][..]),
        None => None,
    };
    let mut reader = match archive::open_archive_with(path, limits, found) {
        Ok(Some(reader)) => reader,
        // No container after all: text under an archive name is not a
        // ROM (`3-D Tetris (USA).zip`), anything else is damaged.
        Ok(None) => {
            match first.not_a_rom {
                Some(what) => out.issue = Some(ArchiveIssue::NotARom { what }),
                None => out.damaged(),
            }
            return Ok(());
        }
        Err(e) => return record_error(e, out),
    };
    let format = reader.format();
    out.format = Some(format);
    out.container = Container::of_format(format);
    if names_an_archive(ext) && !format.named_by(ext) {
        out.mismatch = Some((ext.to_string(), format));
    }
    if !reader.listing_complete() {
        out.listing_complete = false;
        out.spent();
    }
    let entries: Vec<Entry> = file_entries(reader.as_ref()).cloned().collect();
    let mut candidates: Vec<(RomEntry, ExtClass)> = Vec::new();
    // File entries that are neither metadata nor readme-like, and how many
    // of them are archives themselves.
    let (mut substantive, mut nested) = (0usize, 0usize);
    for entry in &entries {
        let info = ZipEntryInfo {
            name: entry_name(entry),
            crc32: entry.crc32.map(|c| format!("{c:08X}")),
            size: entry.size,
        };
        let entry_ext = extension(Path::new(&info.name));
        if is_metadata_entry(&info.name) {
            out.entries.push(info);
            continue;
        }
        if !README_EXTS.contains(&entry_ext.as_str()) {
            substantive += 1;
            if names_an_archive(&entry_ext) {
                nested += 1;
            }
        }
        let class = db.extension_class(&entry_ext);
        if matches!(
            class,
            ExtClass::Unique | ExtClass::Shared | ExtClass::Generic
        ) {
            candidates.push((
                RomEntry {
                    index: entry.index,
                    info: info.clone(),
                    encrypted: entry.encrypted,
                },
                class,
            ));
        }
        out.entries.push(info);
    }
    // A generic extension (`bin`, `iso`…) is a payload only when it is the
    // archive's only file besides readmes and OS metadata: next to other
    // data files it is arcade data or a track.
    let lone = substantive == 1;
    let roms: Vec<RomEntry> = candidates
        .into_iter()
        .filter(|(_, class)| lone || *class != ExtClass::Generic)
        .map(|(rom, _)| rom)
        .collect();
    if roms.len() > MAX_ROM_ENTRIES {
        out.issue = Some(ArchiveIssue::TooManyRoms { count: roms.len() });
        return Ok(());
    }
    if roms.is_empty() && substantive > 0 && nested == substantive {
        out.fail(ArchiveIssue::Nested);
        return Ok(());
    }
    let mut budget = Budget::plan_head(limits);
    for rom in roms {
        let ext = extension(Path::new(&rom.info.name));
        let mut crc32 = rom.info.crc32.clone();
        let sniffed = if rom.encrypted {
            out.note(NOTE_ENCRYPTED);
            Sniff::default()
        } else {
            let want_crc = crc32.is_none() && rom.info.size <= CRC_DECODE_CAP;
            match read_payload(
                reader.as_mut(),
                rom.index,
                rom.info.size,
                want_crc,
                &ext,
                &mut budget,
            ) {
                Ok(read) => {
                    if read.crc_cut_short {
                        out.spent();
                    }
                    if let Some(crc) = read.crc {
                        crc32 = Some(format!("{crc:08X}"));
                        if let Some(listed) = out.entries.iter_mut().find(|e| {
                            e.name == rom.info.name && e.size == rom.info.size && e.crc32.is_none()
                        }) {
                            listed.crc32.clone_from(&crc32);
                        }
                    }
                    read.sniff
                }
                Err(ArchiveError::Encrypted) => {
                    out.note(NOTE_ENCRYPTED);
                    Sniff::default()
                }
                Err(ArchiveError::Unsupported { .. }) => {
                    out.note(NOTE_UNSUPPORTED_COMPRESSION);
                    Sniff::default()
                }
                // An unsafe name or a link is not read: judged by name.
                Err(ArchiveError::UnsafeEntry { .. }) => Sniff::default(),
                Err(ArchiveError::BudgetSpent { .. }) => {
                    out.spent();
                    Sniff::default()
                }
                Err(e) => return record_error(e, out),
            }
        };
        out.payloads.push(Payload {
            name: rom.info.name,
            ext,
            size: rom.info.size,
            crc32,
            sniff: sniffed,
        });
    }
    if reader.is_poisoned() {
        out.damaged();
    }
    Ok(())
}

/// A reader that hashes what passes through it and keeps the first error
/// as an [`ArchiveError`] (the sniffer only sees "no more bytes").
struct Tee<R> {
    inner: R,
    hasher: crc32fast::Hasher,
    error: Option<ArchiveError>,
}

impl<R: Read> Read for Tee<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.error.is_some() {
            return Err(io::Error::other("archive entry failed"));
        }
        match self.inner.read(buf) {
            Ok(n) => {
                self.hasher.update(&buf[..n]);
                Ok(n)
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => Err(e),
            Err(e) => {
                let kind = e.kind();
                self.error = Some(ArchiveError::from(e));
                Err(kind.into())
            }
        }
    }
}

/// What [`read_payload`] got from one entry.
struct PayloadRead {
    sniff: Sniff,
    /// The CRC32 of the whole decoded entry, when one was wanted and the
    /// budget lasted.
    crc: Option<u32>,
    /// The budget ran out while decoding for the CRC (the sniff is whole).
    crc_cut_short: bool,
}

/// Sniffs entry `index` from at most [`STREAM_CAP`] decoded bytes and,
/// with `want_crc`, decodes the rest for its CRC32. Every byte is charged
/// to `budget`; the stream verifies size (and stored CRC) at its end. A
/// budget spent after the sniff keeps the sniff (the header evidence).
fn read_payload(
    reader: &mut dyn ArchiveReader,
    index: usize,
    size: u64,
    want_crc: bool,
    ext: &str,
    budget: &mut Budget,
) -> Result<PayloadRead, ArchiveError> {
    let stream = reader.open_entry(index, budget)?;
    let mut tee = Tee {
        inner: stream,
        hasher: crc32fast::Hasher::new(),
        error: None,
    };
    let sniffed = sniff(
        &mut StreamAt::new((&mut tee).take(STREAM_CAP), STREAM_CAP),
        size,
        ext,
    );
    if let Some(e) = tee.error.take() {
        return Err(e);
    }
    let mut read = PayloadRead {
        sniff: sniffed,
        crc: None,
        crc_cut_short: false,
    };
    if !want_crc {
        return Ok(read);
    }
    match io::copy(&mut tee, &mut io::sink()) {
        Ok(_) => read.crc = Some(tee.hasher.finalize()),
        Err(e) => match tee.error.take().unwrap_or_else(|| ArchiveError::from(e)) {
            ArchiveError::BudgetSpent { .. } => read.crc_cut_short = true,
            other => return Err(other),
        },
    }
    Ok(read)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_decides_before_extension() {
        use ContainerMagic as M;
        assert_eq!(container_for("zip", Some(M::SevenZip)), Container::SevenZip);
        assert_eq!(container_for("bin", Some(M::Zip)), Container::Zip);
        assert_eq!(container_for("rar", Some(M::Zip)), Container::Zip);
        assert_eq!(container_for("gba", Some(M::ZipEocdOnly)), Container::Zip);
        assert_eq!(
            container_for("zip", Some(M::ZipSpanned)),
            Container::MultiVolume
        );
        assert_eq!(container_for("gz", Some(M::Xz)), Container::Xz);
        // No magic: the extension is the fallback.
        assert_eq!(container_for("zip", None), Container::Zip);
        assert_eq!(container_for("7z", None), Container::SevenZip);
        assert_eq!(container_for("chd", None), Container::Chd);
        assert_eq!(container_for("rar", None), Container::Rar);
        assert_eq!(container_for("tgz", None), Container::Gzip);
        assert_eq!(container_for("lzma", None), Container::Lzma);
        assert_eq!(container_for("gba", None), Container::Raw);
        // Recognised, never opened: raw, reported unsupported.
        assert_eq!(container_for("bin", Some(M::Ecm)), Container::Raw);
    }

    #[test]
    fn only_openable_formats_are_archives() {
        assert!(Container::SevenZip.is_archive());
        assert!(Container::Gzip.is_archive());
        assert!(!Container::Raw.is_archive());
        assert!(!Container::Chd.is_archive());
        assert!(!Container::MultiVolume.is_archive());
        assert!(names_an_archive("zip") && names_an_archive("tgz"));
        assert!(!names_an_archive("chd") && !names_an_archive("bin"));
    }
}
