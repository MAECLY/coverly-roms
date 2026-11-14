//! Music rip tag readers (extras spec D15): SPC ID666, NSF and NSFe,
//! GBS, VGM and VGZ GD3, and the PSF family `[TAG]` block.
//!
//! Every read is bounded (fixed offsets for SPC, NSF and GBS; at most 64
//! NSFe chunks and 64 KiB; a GD3 block of at most 64 KiB; a VGZ gunzipped
//! to at most [`VGZ_MAX_OUTPUT`]; a PSF tag block of at most
//! [`PSF_TAG_MAX`] bytes), and only files with a rip extension are ever
//! opened, through the logged opener that refuses key material by name.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::{ExtraFormat, RipTags};
use crate::identify::sniff::{is_music_ext, FileAt, ReadAt, SliceAt};

/// The most a `.vgz` is gunzipped to; past it the tags are skipped.
pub const VGZ_MAX_OUTPUT: u64 = 16 * 1024 * 1024;
/// The most bytes of a PSF `[TAG]` block read after the marker.
pub const PSF_TAG_MAX: usize = 50_000;
/// The most bytes of a VGM GD3 block read.
const GD3_MAX: usize = 64 * 1024;
/// The most chunks and bytes of an NSFe walked looking for `auth`.
const NSFE_MAX_CHUNKS: usize = 64;
const NSFE_MAX_BYTES: u64 = 64 * 1024;

const SPC_MAGIC: &[u8] = b"SNES-SPC700 Sound File Data";
const NSF_MAGIC: &[u8] = b"NESM\x1A";
const NSFE_MAGIC: &[u8] = b"NSFE";
const GBS_MAGIC: &[u8] = b"GBS";
const VGM_MAGIC: &[u8] = b"Vgm ";
const GD3_MAGIC: &[u8] = b"Gd3 ";
const PSF_MAGIC: &[u8] = b"PSF";
const TAG_MARKER: &[u8] = b"[TAG]";
const GZIP_MAGIC: &[u8] = &[0x1F, 0x8B];

/// The tags of the music rip at `path`, with its format (D13, D15), or
/// `None` when it is no rip, carries no tags or cannot be read. A file
/// without a rip extension, a symlink or a special file is never opened;
/// key material is refused by name before any open.
pub fn read_rip(path: &Path) -> Option<(ExtraFormat, RipTags)> {
    let name = path.file_name()?.to_str()?;
    let ext = ext_of(name)?;
    if crate::consoles::is_key_material(name) {
        return None;
    }
    let file = crate::addons::open_regular(path).ok()?;
    if ext == "vgm" || ext == "vgz" {
        let mut head = [0u8; 2];
        let mut src = FileAt(file);
        if src.read_at(0, &mut head).ok()? == 2 && head == GZIP_MAGIC {
            let FileAt(mut file) = src;
            file.seek(SeekFrom::Start(0)).ok()?;
            return vgz(std::io::BufReader::new(file));
        }
        return read_from(&mut src, &ext);
    }
    read_from(&mut FileAt(file), &ext)
}

/// [`read_rip`] over bytes already in memory, `name` giving the
/// extension (tests and archive entries).
pub fn read_rip_bytes(name: &str, bytes: &[u8]) -> Option<(ExtraFormat, RipTags)> {
    let ext = ext_of(name)?;
    if (ext == "vgm" || ext == "vgz") && bytes.starts_with(GZIP_MAGIC) {
        return vgz(bytes);
    }
    read_from(&mut SliceAt(bytes), &ext)
}

/// The lower-case extension of `name` when it is a rip extension.
fn ext_of(name: &str) -> Option<String> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    is_music_ext(&ext).then_some(ext)
}

fn read_from(src: &mut dyn ReadAt, ext: &str) -> Option<(ExtraFormat, RipTags)> {
    let tags = match ext {
        "spc" => (ExtraFormat::Spc, spc(src)?),
        "nsf" | "nsfe" => (ExtraFormat::Nsf, nsf_or_nsfe(src)?),
        "gbs" => (ExtraFormat::Gbs, gbs(src)?),
        "vgm" | "vgz" => (ExtraFormat::Vgm, vgm(src)?),
        _ => (ExtraFormat::Psf, psf(src, ext)?),
    };
    has_any(&tags.1).then_some(tags)
}

fn has_any(t: &RipTags) -> bool {
    t.game.is_some() || t.title.is_some() || t.artist.is_some()
}

/// `n` bytes at `offset`, or `None` when the source is shorter.
fn read_exact(src: &mut dyn ReadAt, offset: u64, n: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; n];
    (src.read_at(offset, &mut buf).ok()? == n).then_some(buf)
}

/// Up to `n` bytes at `offset` (fewer at the end of the source).
fn read_up_to(src: &mut dyn ReadAt, offset: u64, n: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; n];
    let got = src.read_at(offset, &mut buf).ok()?;
    buf.truncate(got);
    Some(buf)
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Trimmed text, `None` when empty.
fn non_empty(s: &str) -> Option<String> {
    let t = s
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .to_string();
    (!t.is_empty()).then_some(t)
}

/// A NUL-padded fixed field: UTF-8 when valid, else Latin-1.
fn fixed_text(b: &[u8]) -> Option<String> {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let b = &b[..end];
    match std::str::from_utf8(b) {
        Ok(s) => non_empty(s),
        Err(_) => non_empty(&b.iter().map(|&c| char::from(c)).collect::<String>()),
    }
}

fn field(head: &[u8], at: usize, len: usize) -> Option<String> {
    fixed_text(head.get(at..at + len)?)
}

/// SPC ID666 text tags: only when byte 0x23 says tags are present (26).
fn spc(src: &mut dyn ReadAt) -> Option<RipTags> {
    let head = read_exact(src, 0, 0xD1)?;
    if !head.starts_with(SPC_MAGIC) || head[0x23] != 26 {
        return None;
    }
    Some(RipTags {
        title: field(&head, 0x2E, 32),
        game: field(&head, 0x4E, 32),
        artist: field(&head, 0xB1, 32),
        systems: &["snes"],
        ..RipTags::default()
    })
}

fn nsf_or_nsfe(src: &mut dyn ReadAt) -> Option<RipTags> {
    let magic = read_exact(src, 0, 5)?;
    if magic.starts_with(NSFE_MAGIC) {
        return nsfe(src);
    }
    if magic != NSF_MAGIC {
        return None;
    }
    let head = read_exact(src, 0, 0x4E)?;
    Some(RipTags {
        game: field(&head, 0x0E, 32),
        artist: field(&head, 0x2E, 32),
        systems: &["nes", "famicom"],
        ..RipTags::default()
    })
}

/// NSFe: the `auth` chunk (game, artist, copyright, ripper as
/// NUL-terminated strings), within the first 64 chunks and 64 KiB.
fn nsfe(src: &mut dyn ReadAt) -> Option<RipTags> {
    let mut at = 4u64;
    for _ in 0..NSFE_MAX_CHUNKS {
        let head = read_exact(src, at, 8)?;
        let len = u64::from(u32_at(&head, 0)?);
        let id = &head[4..8];
        let body = at + 8;
        let end = body.checked_add(len)?;
        if id == b"NEND" || end > NSFE_MAX_BYTES {
            return None;
        }
        if id == b"auth" {
            let data = read_exact(src, body, usize::try_from(len).ok()?)?;
            let mut parts = data.split(|&c| c == 0).map(fixed_text);
            return Some(RipTags {
                game: parts.next().flatten(),
                artist: parts.next().flatten(),
                systems: &["nes", "famicom"],
                ..RipTags::default()
            });
        }
        at = end;
    }
    None
}

fn gbs(src: &mut dyn ReadAt) -> Option<RipTags> {
    let head = read_exact(src, 0, 0x50)?;
    if !head.starts_with(GBS_MAGIC) {
        return None;
    }
    Some(RipTags {
        game: field(&head, 0x10, 32),
        artist: field(&head, 0x30, 32),
        systems: &["gb", "gbc"],
        ..RipTags::default()
    })
}

/// VGM GD3 (offset relative to 0x14): UTF-16LE track, game, system,
/// author (English, else Japanese), the system text naming the link.
fn vgm(src: &mut dyn ReadAt) -> Option<RipTags> {
    let head = read_exact(src, 0, 0x18)?;
    if !head.starts_with(VGM_MAGIC) {
        return None;
    }
    let rel = u32_at(&head, 0x14)?;
    if rel == 0 {
        return None;
    }
    let at = 0x14 + u64::from(rel);
    let gd3 = read_exact(src, at, 12)?;
    if !gd3.starts_with(GD3_MAGIC) {
        return None;
    }
    let len = usize::try_from(u32_at(&gd3, 8)?).ok()?.min(GD3_MAX);
    let body = read_up_to(src, at + 12, len)?;
    let units: Vec<u16> = body
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u16::from_le_bytes(*p))
        .collect();
    let mut strings = units
        .split(|&u| u == 0)
        .map(|s| non_empty(&String::from_utf16_lossy(s)));
    let mut pair = || {
        let en = strings.next().flatten();
        let jp = strings.next().flatten();
        en.or(jp)
    };
    let title = pair();
    let game = pair();
    let system = pair();
    let artist = pair();
    Some(RipTags {
        title,
        game,
        artist,
        systems: system.as_deref().map_or(&[], vgm_systems),
        ..RipTags::default()
    })
}

/// A gzipped VGM, gunzipped to at most [`VGZ_MAX_OUTPUT`]; a larger one
/// is skipped whole.
fn vgz(input: impl Read) -> Option<(ExtraFormat, RipTags)> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(input)
        .take(VGZ_MAX_OUTPUT + 1)
        .read_to_end(&mut out)
        .ok()?;
    if out.len() as u64 > VGZ_MAX_OUTPUT {
        return None;
    }
    let tags = vgm(&mut SliceAt(&out))?;
    has_any(&tags).then_some((ExtraFormat::Vgm, tags))
}

/// The systems a GD3 `system` text names (D15), most specific first; no
/// link for arcade boards or anything unknown.
fn vgm_systems(text: &str) -> &'static [&'static str] {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|w| !w.is_empty())
        .collect();
    let has = |needle: &str| lower.contains(needle);
    let word = |w: &str| words.contains(&w);
    if has("mega drive") || has("megadrive") || has("genesis") {
        &["megadrive", "genesis"]
    } else if has("32x") {
        &["sega32x"]
    } else if has("mega-cd") || has("mega cd") || has("sega cd") {
        &["segacd"]
    } else if has("master system") || has("mark iii") {
        &["mastersystem"]
    } else if has("game gear") {
        &["gamegear"]
    } else if has("sg-1000") || has("sg1000") {
        &["sg1000"]
    } else if has("pc engine") || has("pc-engine") || has("turbografx") {
        &["pcengine"]
    } else if has("neo geo pocket") || has("neo-geo pocket") {
        &["ngp"]
    } else if has("neo geo") || has("neo-geo") {
        &["neogeo"]
    } else if has("game boy advance") {
        &["gba"]
    } else if has("game boy") || has("gameboy") {
        &["gb", "gbc"]
    } else if (has("entertainment system") && !has("super")) || word("nes") || has("famicom") {
        &["nes", "famicom"]
    } else if has("colecovision") {
        &["colecovision"]
    } else if word("msx") || word("msx2") {
        &["msx"]
    } else if has("x68000") {
        &["x68000"]
    } else if has("pc-98") || has("pc98") {
        &["pc98"]
    } else if has("pc-88") || has("pc88") {
        &["pc88"]
    } else if has("wonderswan") {
        &["wonderswan"]
    } else {
        &[]
    }
}

/// The PSF family: the `[TAG]` block after the reserved area and the
/// program, `key=value` lines (`game`, `title`, `artist`), UTF-8 when
/// `utf8=1` or valid, else Shift-JIS.
fn psf(src: &mut dyn ReadAt, ext: &str) -> Option<RipTags> {
    let head = read_exact(src, 0, 16)?;
    if !head.starts_with(PSF_MAGIC) {
        return None;
    }
    let version = head[3];
    let reserved = u64::from(u32_at(&head, 4)?);
    let program = u64::from(u32_at(&head, 8)?);
    let at = 16u64.checked_add(reserved)?.checked_add(program)?;
    let marker = read_exact(src, at, TAG_MARKER.len())?;
    if marker != TAG_MARKER {
        return None;
    }
    let block = read_up_to(src, at + TAG_MARKER.len() as u64, PSF_TAG_MAX)?;
    let text = psf_text(&block);
    let mut tags = RipTags {
        systems: psf_systems(ext, version),
        ..RipTags::default()
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let slot = match key.trim().to_ascii_lowercase().as_str() {
            "game" => &mut tags.game,
            "title" => &mut tags.title,
            "artist" => &mut tags.artist,
            _ => continue,
        };
        if slot.is_none() {
            *slot = non_empty(value);
        }
    }
    Some(tags)
}

/// A PSF tag block as text: UTF-8 when it says `utf8=1` or is valid,
/// else Shift-JIS (D15).
fn psf_text(block: &[u8]) -> String {
    let flagged = block
        .split(|&c| c == b'\n')
        .any(|l| l.trim_ascii().eq_ignore_ascii_case(b"utf8=1"));
    match std::str::from_utf8(block) {
        Ok(s) => s.to_string(),
        Err(_) if flagged => String::from_utf8_lossy(block).into_owned(),
        Err(_) => encoding_rs::SHIFT_JIS
            .decode_without_bom_handling(block)
            .0
            .into_owned(),
    }
}

/// The systems a PSF-family extension implies (D15); QSF (arcade) none.
fn psf_systems(ext: &str, version: u8) -> &'static [&'static str] {
    match ext {
        "psf" | "minipsf" if version == 0x02 => &["ps2"],
        "psf" | "minipsf" => &["psx"],
        "psf2" => &["ps2"],
        "gsf" | "minigsf" => &["gba"],
        "usf" | "miniusf" => &["n64"],
        "2sf" | "mini2sf" => &["nds"],
        "ssf" => &["saturn"],
        "dsf" => &["dreamcast"],
        _ => &[],
    }
}
