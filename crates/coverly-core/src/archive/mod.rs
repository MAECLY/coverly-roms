//! Archive reader: one trait over every container and compressor Coverly
//! opens (spec `2026-10-23-organize-archives-and-discs.md` §4).
//!
//! The public model is format-neutral: a backend lists its entries once
//! (the integrity listing of §4.3), validates their names, and then serves
//! decoded bytes through [`ArchiveReader::open_entry`] or, for one decode
//! pass over solid blocks, [`ArchiveReader::stream_entries`]. Errors are
//! [`ArchiveError`] values that the planner maps to `LeftReason`s (§4.2)
//! and that dedupe phase B sees as `io::Error` (§10).

use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub mod budget;
pub mod detect;
pub mod guard;
pub mod lzma;
pub mod names;
pub mod sevenz;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;
pub mod verify;
pub mod zip;
pub mod zstd;

pub use budget::{Budget, ChargedRead, GateWait, MemoryGate, Permit};
pub use guard::{guarded, GuardedArchive, GuardedRead, Poison};
pub use lzma::{open_decoder, Lzma2Params, LzmaKind, LzmaParams, XzWalker};
pub use names::{RelPath, UnsafeWhy};
pub use verify::VerifyingReader;
pub use zip::ZipReader;

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const GIB: u64 = 1024 * MIB;

/// Most entries one archive may list (spec §4.4, organise's zip rule).
pub const MAX_ENTRIES: u64 = 10_000;
/// Largest header we parse: 7z header (encoded and decoded), zip central
/// directory, external tool listing output (spec §4.4).
pub const MAX_HEADER: u64 = 16 * MIB;
/// Largest declared size of one entry (spec §4.4; above any DVD-9).
pub const MAX_ENTRY_SIZE: u64 = 16 * GIB;
/// Largest sum of declared entry sizes of one archive (spec §4.4).
pub const MAX_TOTAL_SIZE: u64 = 64 * GIB;
/// Memory one decoder may need (spec §4.4, S2, S3).
pub const DECODER_MEM_CAP: u64 = 256 * MIB;
/// Decoded bytes per archive at plan time to read heads (spec §4.4).
pub const PLAN_HEAD_BUDGET: u64 = 64 * MIB;
/// Decoded bytes per archive at plan time to list a tar inside a
/// compressor (spec §4.4).
pub const PLAN_LISTING_BUDGET: u64 = 256 * MIB;
/// Bytes drained after a tar end-of-archive marker (spec §4.4, S6).
pub const TAR_TRAILER_ALLOWANCE: u64 = MIB;
/// Wall time per archive at plan time (spec §4.4).
pub const PLAN_WALL_TIME: Duration = Duration::from_secs(20);

/// A single-file compressor, alone or around a tar (spec §3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Compressor {
    Gzip,
    Bzip2,
    Xz,
    Zstd,
    Lz4,
    /// The legacy `.lzma` ("LZMA alone") format.
    LzmaAlone,
}

/// The container a reader opened, as detected by magic (spec §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Zip,
    SevenZip,
    Rar {
        v5: bool,
    },
    Tar {
        outer: Option<Compressor>,
    },
    /// One compressed file (`game.gba.gz`): one virtual entry.
    Single(Compressor),
}

/// What an archive entry is. Only [`EntryKind::File`] entries are ever
/// read or extracted (spec §13 extras: links are never followed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Hardlink,
    Other,
}

/// A branch-converter filter (7z coders, xz filters).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BcjKind {
    X86,
    Bcj2,
    PowerPc,
    Ia64,
    Arm,
    ArmThumb,
    Arm64,
    Sparc,
    RiscV,
}

/// How an entry is compressed, with the parameters that decide its decoder
/// memory (spec §4.2; S2 reads them from the entry's own property header).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Method {
    Stored,
    Deflate,
    Deflate64,
    Bzip2,
    /// Raw dictionary size from the properties, never lowered by a
    /// declared unpacked size (S3).
    Lzma {
        dict: u32,
    },
    Lzma2 {
        dict: u32,
    },
    /// An xz stream; `dict` is the LZMA2 dictionary of its first block
    /// (0 when it has none). Every later block is checked as it is reached.
    Xz {
        dict: u32,
    },
    Ppmd {
        order: u8,
        mem: u32,
    },
    Zstd {
        window: u64,
    },
    Lz4,
    Bcj(BcjKind),
    Delta,
    /// A method we name but do not decode.
    Other(String),
}

/// One entry of an archive's validated listing (spec §4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Position in the archive's own order.
    pub index: usize,
    /// Validated name (spec §13 extras).
    pub name: RelPath,
    /// Name bytes as stored, for diagnostics only.
    pub raw_name: Vec<u8>,
    pub kind: EntryKind,
    /// Declared uncompressed size.
    pub size: u64,
    pub packed: Option<u64>,
    /// Stored CRC32, when the format has one.
    pub crc32: Option<u32>,
    pub encrypted: bool,
    pub method: Method,
    /// 7z folder or RAR solid group; `None` when independent.
    pub solid_block: Option<u32>,
    pub mtime: Option<SystemTime>,
    /// Set when the stored name failed validation; such an entry is never
    /// extracted, and is an error only when it is needed (spec §4.2).
    pub unsafe_name: Option<UnsafeWhy>,
}

/// An external program that opens what no pure-Rust crate does (spec §5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Unrar,
    SevenZip,
    Bsdtar,
}

/// Which budget ran out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BudgetKind {
    DecodedBytes,
    Time,
}

/// Why an archive, or one entry of it, cannot be used (spec §4.2).
#[derive(Debug)]
pub enum ArchiveError {
    /// Integrity listing failed, a decoder panicked, CRC or size mismatch.
    Damaged {
        detail: String,
    },
    /// RAR without a tool, LZH, an unknown coder.
    Unsupported {
        format: String,
        need: Option<Tool>,
    },
    /// Header or every payload encrypted.
    Encrypted,
    /// A decoder would need more memory than the cap (S2, S3).
    NeedsTooMuchMemory {
        needed: u64,
        cap: u64,
    },
    /// Decode bytes or wall time spent: not read, never damaged.
    BudgetSpent {
        what: BudgetKind,
    },
    /// The budget's cancel flag was set (apply, spec §9.1 "Cancellation").
    Cancelled,
    TooManyEntries {
        count: u64,
    },
    TooLarge {
        declared: u64,
    },
    /// Zip-slip, device name, link… (spec §13 extras).
    UnsafeEntry {
        name: String,
        why: UnsafeWhy,
    },
    /// One part of a multi-volume set (spec §3.2 rule 6).
    MultiVolume {
        parts: Vec<PathBuf>,
    },
    /// An external tool exited non-zero while streaming.
    ToolFailed {
        tool: Tool,
        status: Option<i32>,
    },
    Io(io::Error),
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Damaged { detail } => write!(f, "damaged archive: {detail}"),
            Self::Unsupported { format, need } => match need {
                Some(tool) => write!(f, "unsupported archive {format} (needs {tool:?})"),
                None => write!(f, "unsupported archive {format}"),
            },
            Self::Encrypted => f.write_str("encrypted archive"),
            Self::NeedsTooMuchMemory { needed, cap } => {
                write!(f, "decoder needs {needed} bytes, cap is {cap}")
            }
            Self::BudgetSpent { what } => write!(f, "budget spent: {what:?}"),
            Self::Cancelled => f.write_str("cancelled"),
            Self::TooManyEntries { count } => write!(f, "too many entries: {count}"),
            Self::TooLarge { declared } => write!(f, "too large: {declared} bytes declared"),
            Self::UnsafeEntry { name, why } => write!(f, "unsafe entry {name:?}: {why:?}"),
            Self::MultiVolume { parts } => {
                write!(f, "multi-volume archive ({} parts)", parts.len())
            }
            Self::ToolFailed { tool, status } => match status {
                Some(code) => write!(f, "{tool:?} failed with status {code}"),
                None => write!(f, "{tool:?} was terminated"),
            },
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ArchiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// An `io::Error` that carries an `ArchiveError` (one raised inside a
/// `Read`: a CRC mismatch, a spent budget, a caught decoder panic) turns
/// back into that error, so `?` keeps it `Damaged` instead of `Io`.
impl From<io::Error> for ArchiveError {
    fn from(e: io::Error) -> Self {
        if !e.get_ref().is_some_and(|inner| inner.is::<ArchiveError>()) {
            return Self::Io(e);
        }
        let kind = e.kind();
        match e.into_inner().map(|inner| inner.downcast::<ArchiveError>()) {
            Some(Ok(archive)) => *archive,
            // Not reached: the payload was checked above.
            Some(Err(other)) => Self::Io(io::Error::new(kind, other)),
            None => Self::Io(kind.into()),
        }
    }
}

/// The seam for dedupe phase B (spec §10): its "unreadable unit" path
/// needs no new types. The `ArchiveError` stays inside as the source.
///
/// `BudgetSpent` maps to `QuotaExceeded` (decoded bytes) or `TimedOut`
/// (wall time), never to `Interrupted`: `std` read loops and Coverly's own
/// (`identify::hash`, `organize::inspect`) retry `Interrupted`, so a spent
/// budget would spin forever. Dedupe recognises it with
/// [`is_budget_spent`] and turns it into `Ok(None)` (not read).
impl From<ArchiveError> for io::Error {
    fn from(e: ArchiveError) -> Self {
        let kind = match e {
            ArchiveError::Io(inner) => return inner,
            ArchiveError::Damaged { .. }
            | ArchiveError::UnsafeEntry { .. }
            | ArchiveError::ToolFailed { .. } => io::ErrorKind::InvalidData,
            ArchiveError::Unsupported { .. }
            | ArchiveError::Encrypted
            | ArchiveError::MultiVolume { .. } => io::ErrorKind::Unsupported,
            ArchiveError::NeedsTooMuchMemory { .. } => io::ErrorKind::OutOfMemory,
            ArchiveError::TooManyEntries { .. } | ArchiveError::TooLarge { .. } => {
                io::ErrorKind::FileTooLarge
            }
            ArchiveError::BudgetSpent {
                what: BudgetKind::DecodedBytes,
            } => io::ErrorKind::QuotaExceeded,
            ArchiveError::BudgetSpent {
                what: BudgetKind::Time,
            } => io::ErrorKind::TimedOut,
            // Never `Interrupted`: read loops retry it (AP-8).
            ArchiveError::Cancelled => io::ErrorKind::Other,
        };
        io::Error::new(kind, e)
    }
}

/// Whether `e` came from a spent decode budget ([`ArchiveError::BudgetSpent`]),
/// which callers treat as *not read*, never as damaged (spec §4.4, §10).
/// A plain `QuotaExceeded` or `TimedOut` from the file system is not one.
pub fn is_budget_spent(e: &io::Error) -> bool {
    e.get_ref()
        .and_then(|inner| inner.downcast_ref::<ArchiveError>())
        .is_some_and(|inner| matches!(inner, ArchiveError::BudgetSpent { .. }))
}

/// Bounds for opening one archive (spec §4.4). `Default` is the spec's
/// values; tests override fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_entries: u64,
    pub max_header: u64,
    pub max_entry_size: u64,
    pub max_total_size: u64,
    pub decoder_mem_cap: u64,
    pub plan_head_budget: u64,
    pub plan_listing_budget: u64,
    pub tar_trailer_allowance: u64,
    pub plan_wall_time: Duration,
    /// Whether external tools (unrar, 7zz, bsdtar) may run (setting
    /// `organize.external_tools`, default `auto`).
    pub external_tools: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: MAX_ENTRIES,
            max_header: MAX_HEADER,
            max_entry_size: MAX_ENTRY_SIZE,
            max_total_size: MAX_TOTAL_SIZE,
            decoder_mem_cap: DECODER_MEM_CAP,
            plan_head_budget: PLAN_HEAD_BUDGET,
            plan_listing_budget: PLAN_LISTING_BUDGET,
            tar_trailer_allowance: TAR_TRAILER_ALLOWANCE,
            plan_wall_time: PLAN_WALL_TIME,
            external_tools: true,
        }
    }
}

/// One reader over every format (spec §4.2).
pub trait ArchiveReader: Send {
    fn format(&self) -> Format;

    /// The validated listing (spec §4.3). Never empty for an `Ok` reader.
    fn entries(&self) -> &[Entry];

    /// A reader of one entry's decoded bytes, verified against the
    /// listing's size and CRC at EOF. Bytes decoded to reach it (solid
    /// prefix, tar skip) are charged to `budget`.
    fn open_entry<'a>(
        &'a mut self,
        index: usize,
        budget: &'a mut Budget,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError>;

    /// Streams the `wanted` entries in storage order with one decode pass
    /// per solid block or stream. The extractor uses this; `open_entry` is
    /// for heads and hashes.
    fn stream_entries(
        &mut self,
        wanted: &[usize],
        budget: &mut Budget,
        sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
    ) -> Result<(), ArchiveError>;
}

/// Opens `path` as an archive. `Ok(None)` when it is no container by magic
/// (and has no archive extension); `Err` when it is one that cannot be used.
///
/// Stub until the dispatch lands (plan Task 16): nothing is a container yet.
pub fn open_archive(
    path: &Path,
    limits: &Limits,
) -> Result<Option<Box<dyn ArchiveReader>>, ArchiveError> {
    let _ = (path, limits);
    Ok(None)
}

/// File entries only, in archive order (directories and links removed).
/// This is dedupe's `entry: usize` index space (spec §10): the n-th item
/// here is entry `n` of `EntryListing::list`.
pub fn file_entries(r: &dyn ArchiveReader) -> impl Iterator<Item = &Entry> {
    r.entries().iter().filter(|e| e.kind == EntryKind::File)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_default_matches_spec() {
        let l = Limits::default();
        assert_eq!(l.max_entries, 10_000);
        assert_eq!(l.max_header, 16 * 1024 * 1024);
        assert_eq!(l.max_entry_size, 16 * 1024 * 1024 * 1024);
        assert_eq!(l.max_total_size, 64 * 1024 * 1024 * 1024);
        assert_eq!(l.decoder_mem_cap, 256 * 1024 * 1024);
        assert_eq!(l.plan_head_budget, 64 * 1024 * 1024);
        assert_eq!(l.plan_listing_budget, 256 * 1024 * 1024);
        assert_eq!(l.tar_trailer_allowance, 1024 * 1024);
        assert_eq!(l.plan_wall_time, Duration::from_secs(20));
        assert!(l.external_tools, "organize.external_tools defaults to auto");
    }

    #[test]
    fn archive_error_to_io_kind() {
        use io::ErrorKind as K;
        let cases: Vec<(ArchiveError, K)> = vec![
            (
                ArchiveError::Damaged {
                    detail: "crc".into(),
                },
                K::InvalidData,
            ),
            (
                ArchiveError::Unsupported {
                    format: "rar".into(),
                    need: Some(Tool::Unrar),
                },
                K::Unsupported,
            ),
            (ArchiveError::Encrypted, K::Unsupported),
            (
                ArchiveError::NeedsTooMuchMemory {
                    needed: 1 << 32,
                    cap: DECODER_MEM_CAP,
                },
                K::OutOfMemory,
            ),
            (
                ArchiveError::BudgetSpent {
                    what: BudgetKind::DecodedBytes,
                },
                K::QuotaExceeded,
            ),
            (
                ArchiveError::BudgetSpent {
                    what: BudgetKind::Time,
                },
                K::TimedOut,
            ),
            (ArchiveError::Cancelled, K::Other),
            (
                ArchiveError::TooManyEntries { count: 10_001 },
                K::FileTooLarge,
            ),
            (
                ArchiveError::TooLarge { declared: 1 << 40 },
                K::FileTooLarge,
            ),
            (
                ArchiveError::UnsafeEntry {
                    name: "../x".into(),
                    why: UnsafeWhy::DotDot,
                },
                K::InvalidData,
            ),
            (
                ArchiveError::MultiVolume {
                    parts: vec![PathBuf::from("a.7z.001")],
                },
                K::Unsupported,
            ),
            (
                ArchiveError::ToolFailed {
                    tool: Tool::SevenZip,
                    status: Some(2),
                },
                K::InvalidData,
            ),
            (
                ArchiveError::Io(io::Error::new(K::PermissionDenied, "denied")),
                K::PermissionDenied,
            ),
        ];
        for (err, kind) in cases {
            let label = format!("{err:?}");
            let io_err: io::Error = err.into();
            assert_eq!(io_err.kind(), kind, "{label}");
        }
    }

    #[test]
    fn archive_error_survives_as_io_source() {
        let io_err: io::Error = ArchiveError::NeedsTooMuchMemory { needed: 2, cap: 1 }.into();
        let inner = io_err
            .get_ref()
            .and_then(|e| e.downcast_ref::<ArchiveError>())
            .expect("ArchiveError kept inside io::Error");
        assert!(matches!(
            inner,
            ArchiveError::NeedsTooMuchMemory { needed: 2, cap: 1 }
        ));
    }

    /// A stream whose budget is spent: every read fails the same way.
    struct SpentStream;

    impl Read for SpentStream {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(ArchiveError::BudgetSpent {
                what: BudgetKind::DecodedBytes,
            }
            .into())
        }
    }

    /// `std` read loops (and Coverly's own, e.g. `identify::hash`) retry
    /// `ErrorKind::Interrupted`; a spent budget mapped to it would spin
    /// forever instead of reaching dedupe's `Ok(None)` path.
    #[test]
    fn budget_spent_stream_ends_read_loops() {
        let mut out = Vec::new();
        let err = SpentStream
            .read_to_end(&mut out)
            .expect_err("spent budget is an error");
        assert!(is_budget_spent(&err), "{err:?}");
        let err = crate::identify::hash::hash_stream(
            SpentStream,
            1024,
            crate::identify::hash::HeaderRule::Plain,
        )
        .expect_err("hash of a spent stream fails");
        assert!(is_budget_spent(&err), "{err:?}");
    }

    #[test]
    fn archive_errors_survive_an_io_round_trip() {
        // A `Damaged` raised inside a `Read` (CRC mismatch, caught panic)
        // must stay `Damaged` after `?` turns the io::Error back.
        let back = ArchiveError::from(io::Error::from(ArchiveError::Damaged {
            detail: "crc".into(),
        }));
        assert!(
            matches!(&back, ArchiveError::Damaged { detail } if detail == "crc"),
            "{back:?}"
        );
        let back = ArchiveError::from(io::Error::from(ArchiveError::BudgetSpent {
            what: BudgetKind::Time,
        }));
        assert!(
            matches!(
                back,
                ArchiveError::BudgetSpent {
                    what: BudgetKind::Time
                }
            ),
            "{back:?}"
        );
        // A plain io::Error stays `Io`.
        let back = ArchiveError::from(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        assert!(
            matches!(&back, ArchiveError::Io(e) if e.kind() == io::ErrorKind::PermissionDenied)
        );
    }

    #[test]
    fn is_budget_spent_only_for_budget_errors() {
        for what in [BudgetKind::DecodedBytes, BudgetKind::Time] {
            assert!(is_budget_spent(&ArchiveError::BudgetSpent { what }.into()));
        }
        assert!(!is_budget_spent(
            &ArchiveError::Damaged { detail: "x".into() }.into()
        ));
        assert!(!is_budget_spent(&io::Error::new(
            io::ErrorKind::QuotaExceeded,
            "disk quota"
        )));
        assert!(!is_budget_spent(&io::Error::from(io::ErrorKind::TimedOut)));
    }

    struct FakeReader(Vec<Entry>);

    impl ArchiveReader for FakeReader {
        fn format(&self) -> Format {
            Format::Zip
        }
        fn entries(&self) -> &[Entry] {
            &self.0
        }
        fn open_entry<'a>(
            &'a mut self,
            _index: usize,
            _budget: &'a mut Budget,
        ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
            Ok(Box::new(io::empty()))
        }
        fn stream_entries(
            &mut self,
            _wanted: &[usize],
            _budget: &mut Budget,
            _sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
        ) -> Result<(), ArchiveError> {
            Ok(())
        }
    }

    fn entry(index: usize, kind: EntryKind) -> Entry {
        Entry {
            index,
            name: RelPath::parse(&format!("e{index}")).unwrap(),
            raw_name: format!("e{index}").into_bytes(),
            kind,
            size: 0,
            packed: None,
            crc32: None,
            encrypted: false,
            method: Method::Stored,
            solid_block: None,
            mtime: None,
            unsafe_name: None,
        }
    }

    #[test]
    fn file_entries_skips_dirs_and_links_keeps_order() {
        let reader = FakeReader(vec![
            entry(0, EntryKind::File),
            entry(1, EntryKind::Dir),
            entry(2, EntryKind::File),
            entry(3, EntryKind::Symlink),
            entry(4, EntryKind::File),
            entry(5, EntryKind::Hardlink),
            entry(6, EntryKind::Other),
        ]);
        let got: Vec<usize> = file_entries(&reader).map(|e| e.index).collect();
        assert_eq!(got, vec![0, 2, 4]);
    }

    #[test]
    fn open_archive_stub_reports_no_container() {
        let got = open_archive(Path::new("nothing.bin"), &Limits::default()).expect("stub");
        assert!(got.is_none());
    }
}
