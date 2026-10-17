//! Opening a candidate (organise spec §7.2): a raw file, the entries of a
//! zip (central-directory CRCs plus a bounded inflate of ROM-like
//! entries) or the magic of a 7z/CHD/RAR container. Never extracts and
//! never inflates more than [`STREAM_CAP`] bytes of an entry.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};
use zip::result::ZipError;

use crate::identify::sniff::{
    sniff, ContainerMagic, FileAt, NotARom, SliceAt, Sniff, StreamAt, PREFIX, STREAM_CAP,
};
use crate::scan::classify::extension;
use crate::systems::{ExtClass, SystemDb};

/// Zips with more file entries than this are treated as damaged (a
/// central directory that large is not a game).
pub const MAX_ZIP_ENTRIES: usize = 10_000;
/// Zips with more ROM-like entries than this are collections, not games.
pub const MAX_ROM_ENTRIES: usize = 8;

/// Note: an entry is encrypted (its CRC is known, its bytes are not).
pub const NOTE_ENCRYPTED: &str = "encrypted";
/// Note: an entry uses a compression method this build cannot inflate.
pub const NOTE_UNSUPPORTED_COMPRESSION: &str = "unsupported-compression";

/// How the candidate is packaged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Container {
    Raw,
    Zip,
    SevenZip,
    Chd,
    Rar,
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
    /// CRC32 as 8 upper-case hex digits (zip entries only: raw files are
    /// hashed later, only when a DAT needs it).
    pub crc32: Option<String>,
    pub sniff: Sniff,
}

/// One file entry of a zip, as its central directory states it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ZipEntryInfo {
    pub name: String,
    pub crc32: String,
    pub size: u64,
}

/// Why a candidate cannot be used as it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ArchiveIssue {
    /// Truncated, corrupt, or the magic does not match the extension.
    Damaged,
    /// A container Coverly recognises but cannot open (RAR).
    Unsupported,
    /// The file is a web page or text, whatever its name says.
    NotARom { what: NotARom },
    /// A zip with more than [`MAX_ROM_ENTRIES`] ROM-like entries.
    TooManyRoms { count: usize },
}

/// What [`inspect_file`] found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Inspection {
    pub container: Container,
    /// ROM-like contents, sniffed (at most [`MAX_ROM_ENTRIES`]).
    pub payloads: Vec<Payload>,
    /// Every file entry of a zip (arcade sets are confirmed by them).
    pub entries: Vec<ZipEntryInfo>,
    pub issue: Option<ArchiveIssue>,
    /// Machine-readable notes ([`NOTE_ENCRYPTED`], …), not prose.
    pub notes: Vec<String>,
}

impl Inspection {
    fn new(container: Container) -> Inspection {
        Inspection {
            container,
            payloads: Vec::new(),
            entries: Vec::new(),
            issue: None,
            notes: Vec::new(),
        }
    }

    fn note(&mut self, note: &str) {
        if !self.notes.iter().any(|n| n == note) {
            self.notes.push(note.to_string());
        }
    }

    fn damaged(&mut self) {
        self.payloads.clear();
        self.entries.clear();
        self.issue = Some(ArchiveIssue::Damaged);
    }
}

/// Opens the candidate at `path` (`len` bytes) and reports its container,
/// its sniffed ROM-like contents and any issue. Errors only when the file
/// cannot be opened or its first bytes cannot be read.
pub fn inspect_file(path: &Path, len: u64, db: &SystemDb) -> io::Result<Inspection> {
    let ext = extension(path);
    let mut file = File::open(path)?;
    let mut head = vec![0u8; PREFIX.min(usize::try_from(len).unwrap_or(PREFIX))];
    let n = read_prefix(&mut file, &mut head)?;
    head.truncate(n);
    let first = sniff(&mut SliceAt(&head), head.len() as u64, &ext);
    let mut out = Inspection::new(container_for(&ext, first.container));
    match out.container {
        Container::Zip => inspect_zip(&mut file, &first, db, &mut out),
        Container::SevenZip => {
            if first.container != Some(ContainerMagic::SevenZip) {
                out.issue = Some(ArchiveIssue::Damaged);
            }
        }
        Container::Chd => {
            if first.container != Some(ContainerMagic::Chd) {
                out.issue = Some(ArchiveIssue::Damaged);
            }
        }
        Container::Rar => out.issue = Some(ArchiveIssue::Unsupported),
        Container::Raw => {
            let sniffed = if len <= head.len() as u64 {
                first
            } else {
                sniff(&mut FileAt(file), len, &ext)
            };
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

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// By extension first (a `.zip` that is HTML is still judged as a zip
/// that failed), else by magic (a ROM named `.bin` that is really a zip).
fn container_for(ext: &str, magic: Option<ContainerMagic>) -> Container {
    match (ext, magic) {
        ("zip", _) | (_, Some(ContainerMagic::Zip)) => Container::Zip,
        ("7z", _) | (_, Some(ContainerMagic::SevenZip)) => Container::SevenZip,
        ("chd", _) | (_, Some(ContainerMagic::Chd)) => Container::Chd,
        ("rar", _) | (_, Some(ContainerMagic::Rar)) => Container::Rar,
        _ => Container::Raw,
    }
}

/// A zip entry that is read and sniffed.
struct RomEntry {
    index: usize,
    info: ZipEntryInfo,
    encrypted: bool,
}

fn inspect_zip(file: &mut File, first: &Sniff, db: &SystemDb, out: &mut Inspection) {
    if first.container != Some(ContainerMagic::Zip) {
        match first.not_a_rom {
            Some(what) => out.issue = Some(ArchiveIssue::NotARom { what }),
            None => out.damaged(),
        }
        return;
    }
    let Ok(mut archive) = zip::ZipArchive::new(&mut *file) else {
        out.damaged();
        return;
    };
    if archive.len() > MAX_ZIP_ENTRIES {
        out.damaged();
        return;
    }
    let mut candidates: Vec<(RomEntry, ExtClass)> = Vec::new();
    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index_raw(index) else {
            out.damaged();
            return;
        };
        if entry.is_dir() {
            continue;
        }
        let info = ZipEntryInfo {
            name: match entry.name() {
                Ok(name) => name.into_owned(),
                Err(_) => String::from_utf8_lossy(entry.name_raw()).into_owned(),
            },
            crc32: format!("{:08X}", entry.crc32()),
            size: entry.size(),
        };
        let class = db.extension_class(&extension(Path::new(&info.name)));
        if matches!(
            class,
            ExtClass::Unique | ExtClass::Shared | ExtClass::Generic
        ) {
            candidates.push((
                RomEntry {
                    index,
                    info: info.clone(),
                    encrypted: entry.encrypted(),
                },
                class,
            ));
        }
        out.entries.push(info);
    }
    // A generic extension (`bin`, `iso`…) is a payload only when it is the
    // zip's only file: next to other files it is arcade data or a track.
    let lone = out.entries.len() == 1;
    let roms: Vec<RomEntry> = candidates
        .into_iter()
        .filter(|(_, class)| lone || *class != ExtClass::Generic)
        .map(|(rom, _)| rom)
        .collect();
    if roms.len() > MAX_ROM_ENTRIES {
        out.issue = Some(ArchiveIssue::TooManyRoms { count: roms.len() });
        return;
    }
    for rom in roms {
        let ext = extension(Path::new(&rom.info.name));
        let sniffed = if rom.encrypted {
            out.note(NOTE_ENCRYPTED);
            Sniff::default()
        } else {
            match archive.by_index(rom.index) {
                Ok(entry) => sniff(
                    &mut StreamAt::new(entry.take(STREAM_CAP), STREAM_CAP),
                    rom.info.size,
                    &ext,
                ),
                Err(ZipError::UnsupportedArchive(msg)) if msg == ZipError::PASSWORD_REQUIRED => {
                    out.note(NOTE_ENCRYPTED);
                    Sniff::default()
                }
                Err(
                    ZipError::UnsupportedArchive(_) | ZipError::CompressionMethodNotSupported(_),
                ) => {
                    out.note(NOTE_UNSUPPORTED_COMPRESSION);
                    Sniff::default()
                }
                Err(_) => {
                    out.damaged();
                    return;
                }
            }
        };
        out.payloads.push(Payload {
            name: rom.info.name,
            ext,
            size: rom.info.size,
            crc32: Some(rom.info.crc32),
            sniff: sniffed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_decides_before_magic() {
        assert_eq!(container_for("zip", None), Container::Zip);
        assert_eq!(
            container_for("bin", Some(ContainerMagic::Zip)),
            Container::Zip
        );
        assert_eq!(container_for("7z", None), Container::SevenZip);
        assert_eq!(container_for("chd", None), Container::Chd);
        assert_eq!(container_for("rar", None), Container::Rar);
        assert_eq!(container_for("gba", None), Container::Raw);
    }
}
