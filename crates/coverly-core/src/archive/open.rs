//! [`open_archive`](super::open_archive) dispatch (spec
//! `2026-10-23-organize-archives-and-discs.md` §3.2).
//!
//! 1. Magic wins: the 128 KiB head and the file's tail (for a prefixed
//!    zip's end record) decide the container; the extension is only a
//!    fallback for bytes without magic.
//! 2. Multi-volume sets are never opened: a volume name pattern, the zip
//!    spanned marker or the RAR volume flag gives one
//!    [`ArchiveError::MultiVolume`] listing every part next to the file
//!    (rule 6). A lone first part (`.7z.001`, `.part1.rar`) whose own
//!    header shows a whole archive is opened as one.
//! 3. No magic and an archive extension: text (an HTML page saved as
//!    `.zip`) is no container; anything else goes to that format's reader,
//!    which reports it damaged (rule 3).
//! 4. A compressor holding a tar opens as a tar, decided by the extension
//!    (`.tgz`, `.tar.xz`) or by the first decoded block.
//! 5. RAR, and 7z coders `sevenz-rust2` lacks, go through an external
//!    tool when the setting allows one and the caller passed one; RAR
//!    without one is `Unsupported { "RAR", need: Some(Unrar) }`. No tools
//!    passed runs none: `PATH` is never searched here, only by a caller
//!    that knows the roots to avoid (spec §5.2).
//!
//! Every backend is opened inside [`guarded`] and returned inside a
//! [`GuardedArchive`] (S1).

use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::identify::sniff::{text_kind, ContainerMagic as M};

use super::detect::{by_extension, detect_container, multi_volume_part, tar_header_ok, EOCD_TAIL};
use super::external::{FoundTool, PaxReader, ToolReader};
use super::{
    guarded, open_compressor, ArchiveError, ArchiveReader, Budget, Compressor, Format,
    GuardedArchive, Limits, SevenZReader, SingleReader, TarReader, Tool, ZipReader,
};

/// The prefix detection reads (the same as `inspect_file`).
const HEAD_LEN: u64 = 128 * 1024;
/// A tar header block.
const TAR_BLOCK: u64 = 512;

type Opened = Result<Option<Box<dyn ArchiveReader>>, ArchiveError>;

pub(super) fn open(path: &Path, limits: &Limits, tools: Option<&[FoundTool]>) -> Opened {
    let tools = tools.unwrap_or(&[]);
    let (head, tail, len) = probe(path)?;
    if len == 0 {
        return Ok(None);
    }

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let magic = detect_container(&head, len, Some(&tail));
    if let Some(parts) = volume_set(path, &name, magic, &head, len) {
        return Err(ArchiveError::MultiVolume { parts });
    }
    match magic {
        Some(m) => by_magic(path, &name, m, limits, tools),
        None => by_fallback(path, &name, &head, limits, tools),
    }
}

/// The detection head, the tail (a zip end record's search window) and
/// the length of `path`.
fn probe(path: &Path) -> Result<(Vec<u8>, Vec<u8>, u64), ArchiveError> {
    let mut file = File::open(path).map_err(ArchiveError::Io)?;
    let len = file.metadata().map_err(ArchiveError::Io)?.len();
    if len == 0 {
        return Ok((Vec::new(), Vec::new(), 0));
    }
    let head = read_up_to(&mut file, HEAD_LEN)?;
    let tail = if len > head.len() as u64 {
        let n = len.min(EOCD_TAIL as u64);
        file.seek(SeekFrom::End(-(n as i64)))
            .map_err(ArchiveError::Io)?;
        read_up_to(&mut file, n)?
    } else {
        head.clone()
    };
    Ok((head, tail, len))
}

/// The container magic of another file of a set; unreadable is none.
fn magic_of(path: &Path) -> Option<(M, Vec<u8>)> {
    let (head, tail, len) = probe(path).ok()?;
    detect_container(&head, len, Some(&tail)).map(|m| (m, head))
}

fn read_up_to(file: &mut File, n: u64) -> Result<Vec<u8>, ArchiveError> {
    let mut buf = Vec::new();
    file.take(n)
        .read_to_end(&mut buf)
        .map_err(ArchiveError::Io)?;
    Ok(buf)
}

fn wrap<A: ArchiveReader + 'static>(reader: A) -> Opened {
    Ok(Some(Box::new(GuardedArchive::new(reader))))
}

fn unsupported(format: &str) -> Opened {
    Err(ArchiveError::Unsupported {
        format: format.into(),
        need: None,
    })
}

fn by_magic(path: &Path, name: &str, magic: M, limits: &Limits, tools: &[FoundTool]) -> Opened {
    match magic {
        M::Zip | M::ZipEocdOnly => wrap(guarded(|| ZipReader::open(path, limits))?),
        // Caught by `volume_set`; kept for a direct call.
        M::ZipSpanned => Err(ArchiveError::MultiVolume {
            parts: vec![path.to_path_buf()],
        }),
        M::SevenZip => sevenz(path, limits, tools),
        M::Rar4 => rar(path, false, limits, tools),
        M::Rar5 => rar(path, true, limits, tools),
        M::Gzip => compressed(path, name, Compressor::Gzip, limits),
        M::Bzip2 => compressed(path, name, Compressor::Bzip2, limits),
        M::Xz => compressed(path, name, Compressor::Xz, limits),
        M::Zstd => compressed(path, name, Compressor::Zstd, limits),
        M::Lz4 => compressed(path, name, Compressor::Lz4, limits),
        M::LzmaAlone => compressed(path, name, Compressor::LzmaAlone, limits),
        M::Tar => wrap(guarded(|| TarReader::open(path, None, limits))?),
        // A disc image, not an archive: unchanged (spec §3.1).
        M::Chd => Ok(None),
        M::Cab => unsupported("CAB"),
        M::Arj => unsupported("ARJ"),
        M::Lzh => unsupported("LZH"),
        M::Compress => unsupported("Unix compress"),
        M::Ecm => unsupported("ECM"),
    }
}

/// No magic: the extension proposes a format and its reader decides
/// (spec §3.2 rule 3). Text is never a container.
fn by_fallback(
    path: &Path,
    name: &str,
    head: &[u8],
    limits: &Limits,
    tools: &[FoundTool],
) -> Opened {
    let ext = Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(proposed) = by_extension(&ext) else {
        return Ok(None);
    };
    if text_kind(head).is_some() {
        return Ok(None);
    }
    match proposed {
        M::Zip
        | M::SevenZip
        | M::Tar
        | M::Gzip
        | M::Bzip2
        | M::Xz
        | M::Zstd
        | M::Lz4
        | M::LzmaAlone => by_magic(path, name, proposed, limits, tools),
        // No tool is run on bytes without a RAR signature.
        M::Rar4 | M::Rar5 => Err(ArchiveError::Damaged {
            detail: "no RAR signature".into(),
        }),
        // A CHD, cab, arj… extension without its magic: nothing we judge.
        _ => Ok(None),
    }
}

/// A compressor around a tar (by the name, or by the first decoded block)
/// or around one file.
fn compressed(path: &Path, name: &str, c: Compressor, limits: &Limits) -> Opened {
    if names_a_tar(name) || holds_tar(path, c, limits) {
        wrap(guarded(|| TarReader::open(path, Some(c), limits))?)
    } else {
        wrap(guarded(|| SingleReader::open(path, c, limits))?)
    }
}

/// `.tgz`, `.tbz`, `.tbz2`, `.txz`, `.tzst`, or a stem ending in `.tar`.
fn names_a_tar(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some((stem, ext)) = lower.rsplit_once('.') else {
        return false;
    };
    matches!(ext, "tgz" | "tbz" | "tbz2" | "txz" | "tzst") || stem.ends_with(".tar")
}

/// Whether the first decoded block of the stream is a tar header. The
/// probe decodes at most one block under the plan head budget, inside the
/// guard; any failure is "no" and the single-file reader reports it.
fn holds_tar(path: &Path, c: Compressor, limits: &Limits) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let budget = Budget::plan_head(limits);
    guarded(|| {
        let stream = open_compressor(c, BufReader::new(file), limits, &budget)?;
        let mut block = Vec::with_capacity(TAR_BLOCK as usize);
        stream
            .take(TAR_BLOCK)
            .read_to_end(&mut block)
            .map_err(ArchiveError::from)?;
        Ok(tar_header_ok(&block))
    })
    .unwrap_or(false)
}

/// A 7z by our bounded walker; a folder with a coder `sevenz-rust2` lacks
/// is listed by 7-Zip instead when one may run. Without one the walked
/// reader is kept: those folders' entries are `Unsupported`, the rest
/// still decode.
fn sevenz(path: &Path, limits: &Limits, tools: &[FoundTool]) -> Opened {
    let reader = guarded(|| SevenZReader::open(path, limits))?;
    if reader.unsupported_coder().is_some() && limits.external_tools {
        if let Ok(listed) = guarded(|| ToolReader::open(tools, path, Format::SevenZip, limits)) {
            return wrap(listed);
        }
    }
    wrap(reader)
}

/// RAR through unrar or 7-Zip (strict text listing), else bsdtar (pax
/// transcode); none → `Unsupported { "RAR", need: Some(Unrar) }`.
fn rar(path: &Path, v5: bool, limits: &Limits, tools: &[FoundTool]) -> Opened {
    let missing = || ArchiveError::Unsupported {
        format: "RAR".into(),
        need: Some(Tool::Unrar),
    };
    if !limits.external_tools {
        return Err(missing());
    }
    let format = Format::Rar { v5 };
    match guarded(|| ToolReader::open(tools, path, format, limits)) {
        Ok(listed) => return wrap(listed),
        Err(ArchiveError::Unsupported { need: Some(_), .. }) => {}
        Err(e) => return Err(e),
    }
    match guarded(|| PaxReader::open(tools, path, format, limits)) {
        Ok(listed) => wrap(listed),
        Err(ArchiveError::Unsupported { need: Some(_), .. }) => Err(missing()),
        Err(e) => Err(e),
    }
}

// ------------------------------------------------------------ multi-volume

/// How the parts of one set are named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    /// `.7z.001`, `.7z.002`, …
    SevenZ,
    /// `.part1.rar`, `.part2.rar`, …
    RarParts,
    /// `.rar` then `.r00`, `.r01`, …
    RarOld,
    /// `.z01`, `.z02`, … then `.zip`.
    Zip,
}

/// The family and lower-cased set stem `name` belongs to, if any. A plain
/// `.rar`/`.zip` counts as the opening (old RAR) or closing (split zip)
/// part of a set only when collecting the parts of one.
fn family_of(name: &str) -> Option<(Family, String)> {
    let lower = name.to_ascii_lowercase();
    if let Some(part) = multi_volume_part(name) {
        let (_, ext) = lower.rsplit_once('.')?;
        let family = if ext.bytes().all(|b| b.is_ascii_digit()) {
            Family::SevenZ
        } else if ext == "rar" {
            Family::RarParts
        } else if ext.starts_with('r') {
            Family::RarOld
        } else {
            Family::Zip
        };
        return Some((family, part.set_stem.to_lowercase()));
    }
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    let family = match ext.to_ascii_lowercase().as_str() {
        "rar" => Family::RarOld,
        "zip" => Family::Zip,
        _ => return None,
    };
    Some((family, stem.to_lowercase()))
}

/// The parts of the set `path` belongs to, if it is a volume (spec §3.2
/// rule 6): by name pattern, by the zip spanned marker, or by the RAR
/// volume flag, never by size alone.
fn volume_set(
    path: &Path,
    name: &str,
    magic: Option<M>,
    head: &[u8],
    len: u64,
) -> Option<Vec<PathBuf>> {
    if let Some(part) = multi_volume_part(name) {
        let (family, stem) = family_of(name)?;
        let parts = collect_parts(path, family, &stem);
        if !set_is_real(path, family, magic, &parts) {
            return None;
        }
        if part.first && parts.len() <= 1 {
            let whole = match family {
                Family::SevenZ => magic == Some(M::SevenZip) && !sevenz_header_past_eof(head, len),
                Family::RarParts => matches!(magic, Some(M::Rar4 | M::Rar5)) && !rar_volume(head),
                Family::RarOld | Family::Zip => false,
            };
            if whole {
                return None;
            }
        }
        return Some(parts);
    }
    let family = match magic {
        Some(M::ZipSpanned) => Family::Zip,
        Some(M::Rar4 | M::Rar5) if rar_volume(head) => Family::RarOld,
        _ => return None,
    };
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    Some(collect_parts(path, family, &stem))
}

/// `.rNN` and `.zNN` are also ROM extensions (`.z64` is N64): such a name
/// is a volume only with archive evidence. `.rNN`: its own RAR signature,
/// or a `<stem>.rar` beside it with the RAR volume flag. `.zNN`: its own
/// spanned marker, a spanned `<stem>.z01`, or a `<stem>.zip` whose end
/// record names another disk. `.7z.NNN` and `.partN.rar` are unambiguous.
fn set_is_real(path: &Path, family: Family, magic: Option<M>, parts: &[PathBuf]) -> bool {
    let others = || parts.iter().filter(move |p| p.as_path() != path);
    let ext_is = |p: &Path, want: &str| {
        p.extension()
            .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(want))
    };
    match family {
        Family::SevenZ | Family::RarParts => true,
        Family::RarOld => {
            matches!(magic, Some(M::Rar4 | M::Rar5))
                || others().filter(|p| ext_is(p, "rar")).any(|p| {
                    magic_of(p)
                        .is_some_and(|(m, h)| matches!(m, M::Rar4 | M::Rar5) && rar_volume(&h))
                })
        }
        Family::Zip => {
            magic == Some(M::ZipSpanned)
                || others()
                    .filter(|p| ext_is(p, "z01") || ext_is(p, "zip"))
                    .any(|p| magic_of(p).is_some_and(|(m, _)| m == M::ZipSpanned))
        }
    }
}

/// `path` and every file next to it of the same family and set stem, in
/// volume order. An unreadable folder gives `path` alone.
fn collect_parts(path: &Path, family: Family, stem: &str) -> Vec<PathBuf> {
    let parent = path.parent().unwrap_or(Path::new(""));
    let listed = if parent.as_os_str().is_empty() {
        fs::read_dir(".")
    } else {
        fs::read_dir(parent)
    };
    let mut parts: Vec<PathBuf> = match listed {
        Ok(dir) => dir
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| family_of(n).is_some_and(|(f, s)| f == family && s == stem))
            .map(|n| parent.join(n))
            .collect(),
        Err(_) => Vec::new(),
    };
    if !parts.iter().any(|p| p.file_name() == path.file_name()) {
        parts.push(path.to_path_buf());
    }
    parts.sort_by_cached_key(|p| {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        (volume_number(&name), name)
    });
    parts
}

/// The position of a (lower-cased) part name in its set: `.7z.NNN` and
/// `.partN.rar` by number, `.rar` before `.r00`, `.zNN` before `.zip`.
fn volume_number(name: &str) -> u64 {
    let Some((rest, ext)) = name.rsplit_once('.') else {
        return 0;
    };
    let digits = |s: &str| s.parse::<u64>().unwrap_or(u64::MAX);
    match ext {
        "rar" => rest.rsplit_once(".part").map_or(0, |(_, n)| digits(n)),
        "zip" => u64::MAX,
        _ if ext.bytes().all(|b| b.is_ascii_digit()) => digits(ext),
        _ => ext.get(1..).map_or(0, |n| digits(n).saturating_add(1)),
    }
}

/// A 7z start header whose next header lies past the end of the file: the
/// first part of a split set (checked arithmetic; an overflow is past).
fn sevenz_header_past_eof(head: &[u8], len: u64) -> bool {
    let field = |at: usize| {
        head.get(at..at + 8)
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
    };
    let (Some(offset), Some(size)) = (field(12), field(20)) else {
        return true;
    };
    32u64
        .checked_add(offset)
        .and_then(|end| end.checked_add(size))
        .is_none_or(|end| end > len)
}

/// The RAR volume flag: RAR 4 main header `MHD_VOLUME` (0x0001), RAR 5
/// main archive header flag 0x0001.
fn rar_volume(head: &[u8]) -> bool {
    if head.starts_with(b"Rar!\x1a\x07\x00") {
        // HEAD_CRC (2), HEAD_TYPE (1) = 0x73 main header, HEAD_FLAGS (2).
        return head.get(9) == Some(&0x73)
            && head
                .get(10..12)
                .is_some_and(|f| u16::from_le_bytes([f[0], f[1]]) & 0x0001 != 0);
    }
    if head.starts_with(b"Rar!\x1a\x07\x01\x00") {
        return rar5_main_flags(head).is_some_and(|flags| flags & 0x0001 != 0);
    }
    false
}

/// Archive flags of the RAR 5 main header right after the signature: CRC32,
/// header size, type 1, header flags, optional extra/data sizes, flags.
fn rar5_main_flags(head: &[u8]) -> Option<u64> {
    let mut pos = 12; // signature (8) + header CRC32 (4)
    let _size = vint(head, &mut pos)?;
    if vint(head, &mut pos)? != 1 {
        return None;
    }
    let header_flags = vint(head, &mut pos)?;
    if header_flags & 0x01 != 0 {
        vint(head, &mut pos)?; // extra area size
    }
    if header_flags & 0x02 != 0 {
        vint(head, &mut pos)?; // data size
    }
    vint(head, &mut pos)
}

/// A RAR 5 variable-length integer: 7 bits per byte, at most 10 bytes.
fn vint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *buf.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 0x7F).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn family_of_names() {
        let f = |n: &str| family_of(n);
        assert_eq!(f("Game.7z.002"), Some((Family::SevenZ, "game".into())));
        assert_eq!(f("Game.part2.rar"), Some((Family::RarParts, "game".into())));
        assert_eq!(f("Game.R01"), Some((Family::RarOld, "game".into())));
        assert_eq!(f("Game.rar"), Some((Family::RarOld, "game".into())));
        assert_eq!(f("Game.z01"), Some((Family::Zip, "game".into())));
        assert_eq!(f("Game.ZIP"), Some((Family::Zip, "game".into())));
        assert_eq!(f("Game.7z"), None);
        assert_eq!(f(".rar"), None);
    }

    #[test]
    fn volume_order() {
        let mut names = vec!["g.7z.0010", "g.7z.002", "g.7z.001"];
        names.sort_by_key(|n| volume_number(n));
        assert_eq!(names, ["g.7z.001", "g.7z.002", "g.7z.0010"]);
        let mut names = vec!["g.r01", "g.r00", "g.rar"];
        names.sort_by_key(|n| volume_number(n));
        assert_eq!(names, ["g.rar", "g.r00", "g.r01"]);
        let mut names = vec!["g.zip", "g.z02", "g.z01"];
        names.sort_by_key(|n| volume_number(n));
        assert_eq!(names, ["g.z01", "g.z02", "g.zip"]);
        let mut names = vec!["g.part10.rar", "g.part2.rar", "g.part1.rar"];
        names.sort_by_key(|n| volume_number(n));
        assert_eq!(names, ["g.part1.rar", "g.part2.rar", "g.part10.rar"]);
    }

    #[test]
    fn names_a_tar_forms() {
        for n in [
            "a.tgz",
            "a.TBZ2",
            "a.tar.gz",
            "a.tar.zst",
            "a.txz",
            "a.tzst",
        ] {
            assert!(names_a_tar(n), "{n}");
        }
        for n in ["a.gz", "a.gba.xz", "tar", "a.tar"] {
            assert!(!names_a_tar(n), "{n}");
        }
    }

    #[test]
    fn sevenz_next_header_overflow_is_past_eof() {
        let mut head = vec![0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 4, 0, 0, 0, 0];
        head.extend_from_slice(&u64::MAX.to_le_bytes());
        head.extend_from_slice(&1u64.to_le_bytes());
        assert!(sevenz_header_past_eof(&head, 1 << 20));
        assert!(sevenz_header_past_eof(&head[..20], 1 << 20));
    }

    #[test]
    fn rar5_vint_bounds() {
        // 11 continuation bytes: more than a u64 holds.
        let buf = [0xFFu8; 11];
        assert_eq!(vint(&buf, &mut 0), None);
        let mut pos = 0;
        assert_eq!(vint(&[0x81, 0x01], &mut pos), Some(0x81));
        assert_eq!(pos, 2);
        // Truncated RAR 5 headers never claim a volume.
        assert!(!rar_volume(b"Rar!\x1a\x07\x01\x00\0\0\0\0"));
        assert!(!rar_volume(b"Rar!\x1a\x07\x00\0\0\x73"));
    }

    #[test]
    fn io_error_kept_for_missing_file() {
        let err = open(
            Path::new("/nonexistent/coverly/x.zip"),
            &Limits::default(),
            Some(&[]),
        )
        .err()
        .expect("missing");
        assert!(matches!(&err, ArchiveError::Io(e) if e.kind() == io::ErrorKind::NotFound));
    }
}
