//! Opening a candidate (organise spec §7.2; archives spec §3, §6.1): a raw
//! file, or any container through [`archive::open_archive`]. The container
//! is decided by **magic first**; the extension is only a fallback for
//! bytes without magic, and a mismatch between the two is a note, never
//! damage (D1). Entries are listed from the archive's validated listing,
//! ROM-like entries are sniffed from at most [`STREAM_CAP`] decoded bytes,
//! and a missing CRC comes from a bounded decode of entries up to
//! [`CRC_DECODE_CAP`]. Every decode is charged to one plan-head
//! [`Budget`] per archive; running out is "not fully read", never damage.
//! A disc archive (archives spec §6.2) is judged as one disc set instead:
//! its sheets are parsed when they are reachable within the budget (else
//! the set is inferred from the listing), its references resolve inside
//! the archive, and the primary data track's head goes to
//! [`DiscReader`]. Never extracts.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::archive::detect::{by_extension, detect_container, multi_volume_part, EOCD_TAIL};
use crate::archive::names::{name_key, UnsafeWhy};
use crate::archive::{
    self, file_entries, ArchiveError, ArchiveReader, Budget, Compressor, Entry, Format, Limits,
    Tool, ToolsOnDemand,
};
use crate::dedupe::source::DiscId;
use crate::disc::sheets::MAX_SHEET_BYTES;
use crate::disc::source::MAX_WINDOW;
use crate::disc::{
    infer_redump_set, parse_sheet, primary_track, DiscIdentity, DiscReader, DiscSystem, Sheet,
    SheetKind, WindowSource,
};
use crate::identify::sniff::{
    sniff, ContainerMagic, FileAt, NotARom, SliceAt, Sniff, StreamAt, PREFIX, STREAM_CAP,
};
use crate::organize::plan::SheetCheck;
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
/// Note: an external tool failed (non-zero exit, or killed at its
/// deadline). The archive is reported damaged for this plan only: a
/// slow disk or another tool version may open it next time, so the facts
/// are never memoised.
pub const NOTE_TOOL_FAILED: &str = "tool-failed";

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
    /// Position in the archive's own order (what an extraction names);
    /// absent in facts from before extraction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    /// Why the stored name failed validation: such an entry is never
    /// extracted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsafe_why: Option<UnsafeWhy>,
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
    /// A needed entry (a disc set member) has an unsafe stored name.
    UnsafeEntry { name: String, why: UnsafeWhy },
    /// A disc sheet references files the archive does not hold.
    SetIncomplete { missing: Vec<String> },
    /// A disc sheet references an absolute path or one with `..`.
    SetOutsideFolder,
    /// A track referenced by two discs that are not twins.
    SharedTrack,
    /// The discs of several different games.
    SeveralGames { count: usize },
}

/// The system a disc archive's primary track names.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FoundSystem {
    /// A system of `systems.toml`, by id.
    Known { id: String },
    /// Recognised but not organised (`PlayStation 2`, `3DO`).
    Unsupported { name: String },
}

/// Who the disc of a disc archive is (archives spec §7), as data that is
/// memoised with the facts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiscFound {
    pub system: FoundSystem,
    /// Serials, disc number and volume fingerprint (dedupe level 3).
    pub id: DiscId,
    /// Header confidence (spec §7 table).
    pub confidence: f32,
    /// What decided, from a fixed vocabulary (`SEGA SEGASATURN`).
    pub detail: String,
}

impl DiscFound {
    fn of(identity: DiscIdentity) -> DiscFound {
        let system = match identity.system {
            DiscSystem::Known(id) => FoundSystem::Known { id: id.to_owned() },
            DiscSystem::Unsupported(name) => FoundSystem::Unsupported {
                name: name.to_owned(),
            },
        };
        DiscFound {
            system,
            id: identity.to_dedupe_id(),
            confidence: identity.confidence,
            detail: identity.detail,
        }
    }
}

/// The disc set of a disc archive (archives spec §6.2): one game, one or
/// more discs. Names are entry names as listed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiscSetInfo {
    /// The set's `.cue`, `.gdi` and `.m3u` entries, in archive order.
    pub sheets: Vec<String>,
    /// Every file of the set (sheets, tracks, images), in archive order.
    pub members: Vec<String>,
    /// The primary data track of the first disc.
    pub primary: Option<String>,
    /// `Deferred` when a sheet lay beyond the plan budget.
    pub sheet_check: SheetCheck,
    /// `None` when the primary track was not reached or matched no rule.
    pub identity: Option<DiscFound>,
    /// Several discs of one game.
    pub multi_disc: bool,
    /// The `.cue` and `.gdi` of one disc, both kept (Dreamcast dumps).
    pub twins: Vec<String>,
    /// The members are known: every sheet was read, or a sheet beyond the
    /// budget fits the Redump track pattern. Otherwise the set is a guess
    /// and goes to Review, never to extraction.
    pub extractable: bool,
    /// One entry per disc, in disc order: its sheet (the `.gdi` of a
    /// `.cue`/`.gdi` pair) or its image. A generated `.m3u` lists these.
    #[serde(default)]
    pub discs: Vec<String>,
    /// The first disc's stem without its disc tag (`Final Game (USA)`):
    /// the name of a multi-disc game's folder and playlist.
    #[serde(default)]
    pub title: String,
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
    /// The disc set, when the archive holds disc games.
    #[serde(default)]
    pub disc_set: Option<DiscSetInfo>,
    /// Bytes decoded from the plan head budget.
    #[serde(default)]
    pub decoded: u64,
    /// The external tool (and the version it reported) that listed the
    /// archive; extraction uses the same one.
    #[serde(default)]
    pub tool: Option<(Tool, String)>,
    /// Every file of the multi-volume set this file belongs to (absolute,
    /// in volume order): the planner turns them into one item.
    #[serde(default)]
    pub volume_parts: Vec<PathBuf>,
}

impl Inspection {
    pub(crate) fn new(container: Container) -> Inspection {
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
            disc_set: None,
            decoded: 0,
            tool: None,
            volume_parts: Vec::new(),
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
        self.disc_set = None;
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
/// runs no tool. Errors only when the file cannot be
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
                    out.volume_parts = parts;
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
pub(crate) fn names_an_archive(ext: &str) -> bool {
    by_extension(ext).is_some_and(|m| Container::of_magic(m).is_archive())
}

// The end-record precheck lives with the zip backend now.
pub use crate::archive::zip::{central_directory_claim, CentralDirectoryClaim};

/// macOS/Windows metadata stored in an archive (`__MACOSX/…`, `._x`,
/// `.DS_Store`, `Thumbs.db`): listed, never a payload.
pub fn is_metadata_entry(name: &str) -> bool {
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

/// ROM patch formats: an archive holding only these is a patch for a ROM
/// the user must supply, not a game.
const PATCH_EXTS: [&str; 7] = ["ips", "bps", "ups", "xdelta", "vcdiff", "ppf", "aps"];

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
        ArchiveError::Damaged { .. } | ArchiveError::UnsafeEntry { .. } => out.damaged(),
        ArchiveError::ToolFailed { .. } => {
            out.damaged();
            out.note(NOTE_TOOL_FAILED);
        }
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
            out.volume_parts = parts;
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
    out.tool = reader.tool().map(|t| (t.tool, t.version.clone()));
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
    let (mut substantive, mut nested, mut patches) = (0usize, 0usize, 0usize);
    for entry in &entries {
        let info = ZipEntryInfo {
            name: entry_name(entry),
            crc32: entry.crc32.map(|c| format!("{c:08X}")),
            size: entry.size,
            index: Some(entry.index),
            unsafe_why: entry.unsafe_name,
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
            if PATCH_EXTS.contains(&entry_ext.as_str()) {
                patches += 1;
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
    let mut budget = Budget::plan_head(limits);
    let is_disc = inspect_discs(reader.as_mut(), &entries, &mut budget, out);
    out.decoded = budget.spent();
    if is_disc? {
        if reader.is_poisoned() {
            out.damaged();
        }
        return Ok(());
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
    // `Pokemon Jupiter (Ruby Hack).zip` holds only `Version 6.ips`.
    if roms.is_empty() && substantive > 0 && patches == substantive {
        out.issue = Some(ArchiveIssue::NotARom {
            what: NotARom::Patch,
        });
        return Ok(());
    }
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
        out.decoded = budget.spent();
    }
    if reader.is_poisoned() {
        out.damaged();
    }
    Ok(())
}

// --- Disc archives (archives spec §6.2) ---

/// Extensions that make an archive a disc archive. An `.m3u` alone does
/// not: floppy systems (Amiga, C64, Atari ST) ship playlists too.
const DISC_EXTS: [&str; 9] = [
    "cue", "gdi", "ccd", "iso", "img", "chd", "cdi", "pbp", "cso",
];
/// Disc images that are a disc on their own, without a sheet.
const IMAGE_EXTS: [&str; 6] = ["iso", "img", "chd", "cdi", "pbp", "cso"];
/// Images whose bytes are sectors, read as their own primary track.
const SECTOR_IMAGE_EXTS: [&str; 2] = ["iso", "img"];
/// Files that ride with an image of the same stem (CloneCD).
const IMAGE_COMPANION_EXTS: [&str; 2] = ["ccd", "sub"];
/// Most discs (sheets, playlists and sheetless images) one disc archive
/// may hold.
/// Pairing discs for twins and shared tracks grows with the square of
/// their count, and no single game ships more than a handful of discs:
/// beyond this the archive is a collection, judged from its listing
/// without reading any sheet.
pub const MAX_DISC_SHEETS: usize = 64;

/// One file entry of a disc archive.
struct Listed {
    /// Position in the inspected entries.
    at: usize,
    name: String,
    key: String,
    ext: String,
    /// `name_key` of the directory and of the file stem, folded once.
    dir: String,
    stem: String,
}

/// The listed files of a disc archive by the keys a disc looks them up
/// by, built once: a disc never folds every listed name again (a full
/// listing times every disc it may hold).
struct Lookup {
    /// By `name_key` of the whole name (the first of equal keys).
    by_key: HashMap<String, usize>,
    /// By directory key, in listing order.
    by_dir: HashMap<String, Vec<usize>>,
    /// By directory and stem key, in listing order.
    by_stem: HashMap<(String, String), Vec<usize>>,
}

impl Lookup {
    fn of(files: &[Listed]) -> Lookup {
        let mut l = Lookup {
            by_key: HashMap::new(),
            by_dir: HashMap::new(),
            by_stem: HashMap::new(),
        };
        for (i, f) in files.iter().enumerate() {
            l.by_key.entry(f.key.clone()).or_insert(i);
            l.by_dir.entry(f.dir.clone()).or_default().push(i);
            l.by_stem
                .entry((f.dir.clone(), f.stem.clone()))
                .or_default()
                .push(i);
        }
        l
    }

    /// The files in the directory of `f`, `f` among them.
    fn dir_of(&self, f: &Listed) -> &[usize] {
        self.by_dir.get(&f.dir).map_or(&[], Vec::as_slice)
    }

    /// The files of `f`'s directory and stem, `f` among them.
    fn stem_of(&self, f: &Listed) -> &[usize] {
        self.by_stem
            .get(&(f.dir.clone(), f.stem.clone()))
            .map_or(&[], Vec::as_slice)
    }
}

/// One disc of a disc archive, by positions into the listed files.
struct Disc {
    /// The disc's sheets (a `.cue` and its `.gdi` twin).
    sheets: Vec<usize>,
    /// Tracks (or the image and its companions), in sheet order.
    tracks: Vec<usize>,
    primary: Option<usize>,
    check: SheetCheck,
    /// The members are known (sheet read, or the Redump pattern).
    known: bool,
    /// A `.cue` whose set was inferred: its track 01 of a GD-ROM is only
    /// the low-density area.
    deferred_cue: bool,
    /// Title key without disc tags (`final game (usa)`).
    title: String,
    /// `N` of a `(Disc N)` tag.
    number: Option<u32>,
}

/// What reading one small entry gave.
enum Small {
    Bytes(Vec<u8>),
    /// Beyond the budget, encrypted or with an unsupported method: not
    /// read, never damage.
    NotRead,
}

/// Decoded bytes it costs to reach the start of entry `at`: the earlier
/// entries of its solid block (a 7z folder, a RAR solid group, a tar
/// stream inside a compressor); nothing for an independent entry.
fn decode_cost(entries: &[Entry], at: usize) -> u64 {
    let Some(block) = entries[at].solid_block else {
        return 0;
    };
    entries[..at]
        .iter()
        .filter(|e| e.solid_block == Some(block))
        .map(|e| e.size)
        .fold(0u64, u64::saturating_add)
}

/// Whether `want` decoded bytes of entry `at` fit what `budget` has left.
fn reachable(entries: &[Entry], at: usize, want: u64, budget: &Budget) -> bool {
    let need = decode_cost(entries, at).saturating_add(want);
    budget.remaining().is_none_or(|left| need <= left)
}

/// Reads at most `cap` bytes of entry `index` (to its end when shorter,
/// so the stream checks size and CRC).
fn read_entry(
    reader: &mut dyn ArchiveReader,
    index: usize,
    cap: u64,
    budget: &mut Budget,
) -> Result<Vec<u8>, ArchiveError> {
    let stream = reader.open_entry(index, budget)?;
    let mut bytes = Vec::new();
    stream
        .take(cap)
        .read_to_end(&mut bytes)
        .map_err(ArchiveError::from)?;
    Ok(bytes)
}

/// [`read_entry`] with the outcomes that are not damage folded into
/// [`Small::NotRead`] (noted on `out`); `Err` for the rest.
fn read_small(
    reader: &mut dyn ArchiveReader,
    index: usize,
    cap: u64,
    budget: &mut Budget,
    out: &mut Inspection,
) -> Result<Small, ArchiveError> {
    match read_entry(reader, index, cap, budget) {
        Ok(bytes) => Ok(Small::Bytes(bytes)),
        Err(e) => not_read(e, out).map(|()| Small::NotRead),
    }
}

/// `Ok` (noted) for an error that only means "not read".
fn not_read(e: ArchiveError, out: &mut Inspection) -> Result<(), ArchiveError> {
    match e {
        ArchiveError::BudgetSpent { .. } => {
            out.spent();
            Ok(())
        }
        ArchiveError::Encrypted => {
            out.note(NOTE_ENCRYPTED);
            Ok(())
        }
        ArchiveError::Unsupported { .. } => {
            out.note(NOTE_UNSUPPORTED_COMPRESSION);
            Ok(())
        }
        other => Err(other),
    }
}

/// `(directory, file name)` of a `/`-separated entry name.
fn split_dir(name: &str) -> (&str, &str) {
    name.rsplit_once('/').unwrap_or(("", name))
}

/// The file stem of an entry name.
fn entry_stem(name: &str) -> &str {
    let file = split_dir(name).1;
    file.rsplit_once('.').map_or(file, |(stem, _)| stem)
}

/// A sheet reference made an entry name: relative to the sheet's
/// directory, `\` read as `/`, `.` components dropped. `None` when it is
/// absolute (`/x`, `C:\x`, `\\server\x`) or climbs with `..`.
fn reference_name(sheet: &str, reference: &str) -> Option<String> {
    let r = reference.replace('\\', "/");
    let drive = r.as_bytes().get(1) == Some(&b':');
    if r.is_empty() || r.starts_with('/') || drive {
        return None;
    }
    let mut parts: Vec<&str> = match split_dir(sheet).0 {
        "" => Vec::new(),
        dir => dir.split('/').collect(),
    };
    for part in r.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

/// Why a sheet's references do not resolve.
enum Unresolved {
    Outside,
    Missing(Vec<String>),
}

/// The listed files `refs` of `sheet` name (case-insensitive and NFC, as
/// on disk), in order.
fn resolve(
    sheet: &str,
    refs: &[String],
    by_key: &HashMap<String, usize>,
) -> Result<Vec<usize>, Unresolved> {
    let mut found = Vec::with_capacity(refs.len());
    let mut missing = Vec::new();
    for r in refs {
        let name = reference_name(sheet, r).ok_or(Unresolved::Outside)?;
        match by_key.get(&name_key(&name)) {
            Some(&i) => found.push(i),
            None => missing.push(r.clone()),
        }
    }
    if missing.is_empty() {
        Ok(found)
    } else {
        Err(Unresolved::Missing(missing))
    }
}

/// `N` of a `(Disc N)`, `(Disc N of M)`, `(Disk N)` or `(CD N)` group.
fn disc_tag(group: &str) -> Option<u32> {
    let mut words = group.split_whitespace();
    let word = words.next()?;
    if !["disc", "disk", "cd"]
        .iter()
        .any(|w| word.eq_ignore_ascii_case(w))
    {
        return None;
    }
    let n = words.next()?.parse::<u32>().ok()?;
    match (words.next(), words.next(), words.next()) {
        (None, None, None) => Some(n),
        (Some(of), Some(m), None) if of.eq_ignore_ascii_case("of") && m.parse::<u32>().is_ok() => {
            Some(n)
        }
        _ => None,
    }
}

/// The title key of a disc's file stem (disc tags removed, `name_key`
/// folded) and its disc number.
fn disc_title(stem: &str) -> (String, Option<u32>) {
    let (title, number) = disc_title_text(stem);
    (name_key(&title), number)
}

/// `stem` without its disc tag, spaces collapsed (`Final Game (USA)` of
/// `Final Game (USA) (Disc 2)`), and the tag's number.
pub fn disc_title_text(stem: &str) -> (String, Option<u32>) {
    let mut title = String::with_capacity(stem.len());
    let mut number = None;
    let mut rest = stem;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let group = &rest[open + 1..open + close];
        title.push_str(&rest[..open]);
        match disc_tag(group) {
            Some(n) => number = number.or(Some(n)),
            None => title.push_str(&rest[open..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    title.push_str(rest);
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    (title, number)
}

/// The files of `sheet`'s directory a guessed set holds: every file there
/// but other discs' sheets, the first track-like one as primary.
fn guessed_set(files: &[Listed], lookup: &Lookup, sheet: usize) -> (Vec<usize>, Option<usize>) {
    let tracks: Vec<usize> = lookup
        .dir_of(&files[sheet])
        .iter()
        .copied()
        .filter(|&i| i != sheet && SheetKind::from_extension(&files[i].ext).is_none())
        .collect();
    let primary = tracks
        .iter()
        .copied()
        .find(|&i| matches!(files[i].ext.as_str(), "bin" | "iso" | "img"));
    (tracks, primary)
}

/// The disc of a cue/gdi sheet at `si`: parsed when reachable, inferred
/// from the listing when not. `Err` stops the archive with an issue.
fn sheet_disc(
    (reader, entries, files, lookup): (&mut dyn ArchiveReader, &[Entry], &[Listed], &Lookup),
    si: usize,
    kind: SheetKind,
    budget: &mut Budget,
    out: &mut Inspection,
) -> Result<Result<Disc, ArchiveIssue>, ArchiveError> {
    let f = &files[si];
    let entry = &entries[f.at];
    let (title, number) = disc_title(entry_stem(&f.name));
    let mut disc = Disc {
        sheets: vec![si],
        tracks: Vec::new(),
        primary: None,
        check: SheetCheck::Read,
        known: true,
        deferred_cue: false,
        title,
        number,
    };
    let fits = entry.size <= MAX_SHEET_BYTES as u64;
    let text = if fits && reachable(entries, f.at, entry.size, budget) {
        match read_small(reader, entry.index, MAX_SHEET_BYTES as u64 + 1, budget, out)? {
            Small::Bytes(b) => Some(b),
            Small::NotRead => None,
        }
    } else {
        None
    };
    let parsed: Option<Result<Sheet, _>> = text.map(|t| parse_sheet(kind, &t));
    match parsed {
        Some(Ok(sheet)) => {
            let found = match resolve(&f.name, &sheet.files, &lookup.by_key) {
                Ok(found) => found,
                Err(Unresolved::Outside) => return Ok(Err(ArchiveIssue::SetOutsideFolder)),
                Err(Unresolved::Missing(missing)) => {
                    return Ok(Err(ArchiveIssue::SetIncomplete { missing }))
                }
            };
            disc.primary =
                primary_track(&sheet).and_then(|t| found.get(sheet.tracks[t].file).copied());
            let mut tracks = found;
            let mut seen = HashSet::new();
            tracks.retain(|i| seen.insert(*i));
            disc.tracks = tracks;
        }
        // A sheet that is no sheet (too large, not text, malformed): its
        // directory is a guess.
        Some(Err(_)) => {
            disc.known = false;
            (disc.tracks, disc.primary) = guessed_set(files, lookup, si);
        }
        None => {
            disc.check = SheetCheck::Deferred;
            // Only the sheet's directory can hold its set.
            let listing: Vec<&str> = lookup
                .dir_of(f)
                .iter()
                .map(|&i| files[i].name.as_str())
                .collect();
            match infer_redump_set(&listing, &f.name) {
                Some(names) => {
                    let tracks: Vec<usize> = names
                        .iter()
                        .filter_map(|n| lookup.by_key.get(&name_key(n)).copied())
                        .collect();
                    // Track 03 is a GD-ROM's data track: when a `.gdi`
                    // names the set, or for a `.gdi` itself.
                    let gdi_twin = kind == SheetKind::Gdi
                        || lookup.stem_of(f).iter().any(|&i| files[i].ext == "gdi");
                    disc.primary = if gdi_twin && tracks.len() >= 3 {
                        Some(tracks[2])
                    } else {
                        tracks.first().copied()
                    };
                    disc.deferred_cue = kind == SheetKind::Cue && !gdi_twin;
                    disc.tracks = tracks;
                }
                None => {
                    disc.known = false;
                    out.spent();
                    (disc.tracks, disc.primary) = guessed_set(files, lookup, si);
                }
            }
        }
    }
    Ok(Ok(disc))
}

/// Judges an archive as a disc set when it holds disc games (archives
/// spec §6.2). `Ok(false)`: no disc here, the cartridge rules apply.
fn inspect_discs(
    reader: &mut dyn ArchiveReader,
    entries: &[Entry],
    budget: &mut Budget,
    out: &mut Inspection,
) -> io::Result<bool> {
    let files: Vec<Listed> = entries
        .iter()
        .enumerate()
        .map(|(at, e)| (at, entry_name(e)))
        .filter(|(_, name)| !is_metadata_entry(name))
        .map(|(at, name)| Listed {
            at,
            key: name_key(&name),
            ext: extension(Path::new(&name)),
            dir: name_key(split_dir(&name).0),
            stem: name_key(entry_stem(&name)),
            name,
        })
        .collect();
    if !files.iter().any(|f| DISC_EXTS.contains(&f.ext.as_str())) {
        return Ok(false);
    }
    let discs = files
        .iter()
        .filter(|f| f.ext == "m3u" || (f.ext != "ccd" && DISC_EXTS.contains(&f.ext.as_str())))
        .count();
    if discs > MAX_DISC_SHEETS {
        out.payloads.clear();
        out.issue = Some(ArchiveIssue::SeveralGames { count: discs });
        return Ok(true);
    }
    let lookup = Lookup::of(&files);
    match disc_set(reader, entries, &files, &lookup, budget, out) {
        Ok(Ok(Some(set))) => out.disc_set = Some(set),
        Ok(Ok(None)) => return Ok(false),
        Ok(Err(issue)) => {
            out.payloads.clear();
            out.issue = Some(issue);
        }
        // Past the wall time: the set is not judged, the item not fully
        // read (the listing itself was whole).
        Err(ArchiveError::BudgetSpent { .. }) => {
            out.payloads.clear();
            out.spent();
        }
        Err(e) => record_error(e, out)?,
    }
    Ok(true)
}

/// The disc set of a disc archive; `Ok(Err(issue))` when the set is
/// unusable, `Ok(Ok(None))` when no disc was found after all. The
/// budget's wall time and cancel flag are checked per disc, playlist and
/// pairing, also where nothing is decoded.
#[allow(clippy::type_complexity)]
fn disc_set(
    reader: &mut dyn ArchiveReader,
    entries: &[Entry],
    files: &[Listed],
    lookup: &Lookup,
    budget: &mut Budget,
    out: &mut Inspection,
) -> Result<Result<Option<DiscSetInfo>, ArchiveIssue>, ArchiveError> {
    let mut discs: Vec<Disc> = Vec::new();
    // Each m3u and the files it lists.
    let mut playlists: Vec<(usize, HashSet<usize>)> = Vec::new();
    for (si, f) in files.iter().enumerate() {
        let Some(kind) = SheetKind::from_extension(&f.ext) else {
            continue;
        };
        budget.check()?;
        if kind == SheetKind::M3u {
            let entry = &entries[f.at];
            let mut listed = HashSet::new();
            if entry.size <= MAX_SHEET_BYTES as u64 && reachable(entries, f.at, entry.size, budget)
            {
                if let Small::Bytes(text) =
                    read_small(reader, entry.index, MAX_SHEET_BYTES as u64 + 1, budget, out)?
                {
                    if let Ok(sheet) = parse_sheet(kind, &text) {
                        listed = match resolve(&f.name, &sheet.files, &lookup.by_key) {
                            Ok(found) => found.into_iter().collect(),
                            Err(Unresolved::Outside) => {
                                return Ok(Err(ArchiveIssue::SetOutsideFolder))
                            }
                            Err(Unresolved::Missing(missing)) => {
                                return Ok(Err(ArchiveIssue::SetIncomplete { missing }))
                            }
                        };
                    }
                }
            }
            playlists.push((si, listed));
            continue;
        }
        match sheet_disc((reader, entries, files, lookup), si, kind, budget, out)? {
            Ok(disc) => discs.push(disc),
            Err(issue) => return Ok(Err(issue)),
        }
    }
    // A `.cue` and a `.gdi` over the same tracks are one disc.
    let mut twins: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < discs.len() {
        budget.check()?;
        let mut j = i + 1;
        while j < discs.len() {
            let (a, b) = (&discs[i], &discs[j]);
            let kinds = |d: &Disc| {
                d.sheets
                    .iter()
                    .map(|&s| files[s].ext.clone())
                    .collect::<Vec<_>>()
            };
            let same_tracks = || {
                let mut ta = a.tracks.clone();
                let mut tb = b.tracks.clone();
                ta.sort_unstable();
                tb.sort_unstable();
                !ta.is_empty() && ta == tb
            };
            let twin = a.known && b.known && a.tracks.len() == b.tracks.len() && same_tracks() && {
                let (ka, kb) = (kinds(a), kinds(b));
                ka.iter().all(|k| !kb.contains(k))
            };
            if twin {
                let b = discs.remove(j);
                let a = &mut discs[i];
                // A gdi names a GD-ROM's data track: its primary wins.
                if files[b.sheets[0]].ext == "gdi" {
                    a.primary = b.primary.or(a.primary);
                    a.deferred_cue = false;
                }
                if b.check == SheetCheck::Deferred {
                    a.check = SheetCheck::Deferred;
                }
                a.sheets.extend(b.sheets);
                twins.extend(a.sheets.iter().copied());
            } else {
                j += 1;
            }
        }
        i += 1;
    }
    // A track of two discs that are not twins.
    let mut uses: HashMap<usize, usize> = HashMap::new();
    for d in &discs {
        for &t in &d.tracks {
            *uses.entry(t).or_default() += 1;
        }
    }
    if uses.values().any(|&n| n > 1) {
        return Ok(Err(ArchiveIssue::SharedTrack));
    }
    // Images no sheet lists are discs of their own.
    let in_sets: HashSet<usize> = discs
        .iter()
        .flat_map(|d| d.tracks.iter().chain(&d.sheets).copied())
        .collect();
    for (ii, f) in files.iter().enumerate() {
        if !IMAGE_EXTS.contains(&f.ext.as_str()) || in_sets.contains(&ii) {
            continue;
        }
        budget.check()?;
        let mut tracks = vec![ii];
        tracks.extend(
            lookup
                .stem_of(f)
                .iter()
                .copied()
                .filter(|&c| IMAGE_COMPANION_EXTS.contains(&files[c].ext.as_str())),
        );
        let (title, number) = disc_title(entry_stem(&f.name));
        discs.push(Disc {
            sheets: Vec::new(),
            primary: SECTOR_IMAGE_EXTS.contains(&f.ext.as_str()).then_some(ii),
            tracks,
            check: SheetCheck::Read,
            known: true,
            deferred_cue: false,
            title,
            number,
        });
    }
    if discs.is_empty() {
        return Ok(Ok(None));
    }
    // One game: the discs one playlist lists, or discs of one title.
    let mut group: Vec<usize> = (0..discs.len()).collect();
    fn root(group: &mut [usize], mut i: usize) -> usize {
        while group[i] != i {
            group[i] = group[group[i]];
            i = group[i];
        }
        i
    }
    let join = |group: &mut Vec<usize>, a: usize, b: usize| {
        let (ra, rb) = (root(group, a), root(group, b));
        group[ra.max(rb)] = ra.min(rb);
    };
    for a in 0..discs.len() {
        budget.check()?;
        for b in a + 1..discs.len() {
            if discs[a].title == discs[b].title {
                join(&mut group, a, b);
            }
        }
    }
    for (_, listed) in &playlists {
        budget.check()?;
        let hit: Vec<usize> = (0..discs.len())
            .filter(|&d| {
                discs[d]
                    .sheets
                    .iter()
                    .chain(&discs[d].tracks)
                    .any(|m| listed.contains(m))
            })
            .collect();
        for w in hit.windows(2) {
            join(&mut group, w[0], w[1]);
        }
    }
    let mut roots: Vec<usize> = (0..discs.len()).map(|d| root(&mut group, d)).collect();
    roots.sort_unstable();
    roots.dedup();
    if roots.len() > 1 {
        return Ok(Err(ArchiveIssue::SeveralGames { count: roots.len() }));
    }
    discs.sort_by_key(|d| {
        (
            d.number.unwrap_or(u32::MAX),
            d.sheets.first().or(d.tracks.first()).copied(),
        )
    });
    // Members, and a needed entry under an unsafe name.
    let mut members: Vec<usize> = playlists.iter().map(|(s, _)| *s).collect();
    for d in &discs {
        members.extend(d.sheets.iter().chain(&d.tracks).copied());
    }
    members.sort_unstable();
    members.dedup();
    if let Some(&m) = members
        .iter()
        .find(|&&m| entries[files[m].at].unsafe_name.is_some())
    {
        let e = &entries[files[m].at];
        return Ok(Err(ArchiveIssue::UnsafeEntry {
            name: files[m].name.clone(),
            why: e.unsafe_name.expect("checked"),
        }));
    }
    let first = &discs[0];
    let mut primary = first.primary;
    let mut identity = None;
    if let Some(p) = primary {
        identity = identify_track(reader, entries, &files[p], budget, out)?;
        // An inferred cue's track 01 of a GD-ROM is the low-density area:
        // the fingerprint lives in track 03.
        let dreamcast = identity.as_ref().is_some_and(|f: &DiscFound| {
            f.system
                == FoundSystem::Known {
                    id: "dreamcast".into(),
                }
        });
        if first.deferred_cue && dreamcast && first.tracks.len() >= 3 {
            primary = Some(first.tracks[2]);
            out.payloads.clear();
            identity = identify_track(reader, entries, &files[first.tracks[2]], budget, out)?;
        }
    }
    let names = |v: &[usize]| v.iter().map(|&i| files[i].name.clone()).collect::<Vec<_>>();
    // What a playlist lists per disc: the gdi of a pair, else its sheet,
    // else its image.
    let per_disc: Vec<usize> = discs
        .iter()
        .filter_map(|d| {
            d.sheets
                .iter()
                .copied()
                .find(|&s| files[s].ext == "gdi")
                .or_else(|| d.sheets.first().copied())
                .or_else(|| d.primary.or_else(|| d.tracks.first().copied()))
        })
        .collect();
    let title = per_disc
        .first()
        .map(|&d| disc_title_text(entry_stem(&files[d].name)).0)
        .unwrap_or_default();
    twins.sort_unstable();
    twins.dedup();
    let sheets: Vec<usize> = members
        .iter()
        .copied()
        .filter(|&m| SheetKind::from_extension(&files[m].ext).is_some())
        .collect();
    Ok(Ok(Some(DiscSetInfo {
        sheets: names(&sheets),
        members: names(&members),
        primary: primary.map(|p| files[p].name.clone()),
        sheet_check: if discs.iter().any(|d| d.check == SheetCheck::Deferred) {
            SheetCheck::Deferred
        } else {
            SheetCheck::Read
        },
        identity,
        multi_disc: discs.len() > 1,
        twins: names(&twins),
        extractable: discs.iter().all(|d| d.known),
        discs: names(&per_disc),
        title,
    })))
}

/// Reads the head of the primary track `track` (at most
/// [`MAX_WINDOW`] bytes, when the budget reaches it), sniffs it into the
/// inspection's payload and asks [`DiscReader`] who the disc is; a
/// `SYSTEM.CNF` stored past the head is fetched with a skip charged to
/// the same budget.
fn identify_track(
    reader: &mut dyn ArchiveReader,
    entries: &[Entry],
    track: &Listed,
    budget: &mut Budget,
    out: &mut Inspection,
) -> Result<Option<DiscFound>, ArchiveError> {
    let entry = &entries[track.at];
    let window = (MAX_WINDOW as u64).min(entry.size);
    let mut payload = Payload {
        name: track.name.clone(),
        ext: track.ext.clone(),
        size: entry.size,
        crc32: entry.crc32.map(|c| format!("{c:08X}")),
        sniff: Sniff::default(),
    };
    if entry.encrypted {
        out.note(NOTE_ENCRYPTED);
        out.payloads.push(payload);
        return Ok(None);
    }
    if !reachable(entries, track.at, window, budget) {
        out.spent();
        out.payloads.push(payload);
        return Ok(None);
    }
    let bytes = match read_small(reader, entry.index, MAX_WINDOW as u64, budget, out)? {
        Small::Bytes(b) => b,
        Small::NotRead => {
            out.payloads.push(payload);
            return Ok(None);
        }
    };
    payload.sniff = sniff(&mut SliceAt(&bytes), entry.size, &track.ext);
    out.payloads.push(payload);
    let index = entry.index;
    let mut failed: Option<ArchiveError> = None;
    let mut fetch = |range: Range<u64>| -> io::Result<Option<Vec<u8>>> {
        let len = range.end.saturating_sub(range.start);
        let got = (|| {
            let mut stream = reader.open_entry(index, budget)?;
            io::copy(&mut (&mut stream).take(range.start), &mut io::sink())
                .map_err(ArchiveError::from)?;
            let mut bytes = Vec::new();
            stream
                .take(len)
                .read_to_end(&mut bytes)
                .map_err(ArchiveError::from)?;
            Ok::<_, ArchiveError>(bytes)
        })();
        match got {
            Ok(bytes) => Ok(Some(bytes)),
            Err(ArchiveError::BudgetSpent { .. }) => Ok(None),
            Err(e) => {
                let kind = io::ErrorKind::Other;
                failed = Some(e);
                Err(kind.into())
            }
        }
    };
    let mut src = WindowSource::new(bytes);
    let found = DiscReader::identify_with(&mut src, Some(entry.size), Some(&mut fetch));
    if let Some(e) = failed {
        not_read(e, out)?;
        return Ok(None);
    }
    Ok(found.ok().flatten().map(DiscFound::of))
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

    #[test]
    fn a_failed_tool_is_damaged_and_noted() {
        for status in [None, Some(3)] {
            let mut out = Inspection::new(Container::Rar);
            record_error(
                ArchiveError::ToolFailed {
                    tool: Tool::Unrar,
                    status,
                },
                &mut out,
            )
            .unwrap();
            assert_eq!(out.issue, Some(ArchiveIssue::Damaged), "{status:?}");
            assert!(
                out.notes.iter().any(|n| n == NOTE_TOOL_FAILED),
                "{status:?}"
            );
        }
        let mut out = Inspection::new(Container::Zip);
        record_error(ArchiveError::Damaged { detail: "x".into() }, &mut out).unwrap();
        assert!(out.notes.is_empty());
    }
}
