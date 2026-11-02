//! Loose files no other reader owns (adjustments spec §8.2, D12): a
//! Switch `main.npdm`, a GameCube/Wii `.dol`, a PS3/Vita SELF (`.self`,
//! `eboot.bin`), a PSP `EBOOT.PBP`, a PSP `~PSP` module (`.prx`) and a
//! Vita content-manager backup (`.psvimg`/`.psvmd`, by name only).
//! Formats an earlier reader parses (`.tmd`, `.rpx`, `.nro`, `.nso`,
//! `.nca`, `.xbe`, `.xex`, `.3dsx`) are extended in that reader, so
//! nothing here shadows them; [`super::identify_one`] calls this module
//! after `sony` and before `orbis`/`xbox`.
//!
//! Sources (`docs/02-research/loose-executable-headers.md`): switchbrew
//! «NPDM» (`META` at 0, AciOffset at 0x70, ProgramId at ACI0 + 0x10);
//! wiibrew «DOL» and Dolphin `DolReader.cpp` (the 0x100-byte header, the
//! `offset + AlignUp(size, 32) ≤ len` check, the `mtspr HID4` word that
//! makes a DOL Wii-only); scetool `sce.h` (SCE header, `self_header_t`,
//! app info, `self_type`, NPDRM control info) and vita-toolchain
//! `vita-make-fself.c` (version 3 little-endian, homebrew authids);
//! PPSSPP `Loaders.cpp`, `PBPReader.cpp` and `PrxDecrypter.h` (PBP
//! offsets, PSAR at 0x24, `ME`/`PSISOIMG0000` PS1 classics, `NPUMDIMG`
//! PSN ISOs, `modname` at 0x0A); psvimgtools (the title id of a backup
//! lives only in its path).
//!
//! Only plaintext headers are read, within the D24 budgets. No reader
//! decrypts, derives or takes a key (D1); a `.psvimg` is never read.

use std::path::{Path, PathBuf};

use super::wiiu::find;
use super::{
    psf, sony, Budgeted, ConsoleId, DirListing, FolderMatch, IdSource, PartKind, TitleKind,
    MAX_META_FILE,
};
use crate::disc::serial::normalise_console_serial;

/// Detail of a Vita backup recognised by its name.
pub const PSVIMG_DETAIL: &str = "PSVIMG name";

fn be32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        h.get(o..o.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn le32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        h.get(o..o.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn be64(h: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        h.get(o..o.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn le64(h: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        h.get(o..o.checked_add(8)?)?.try_into().ok()?,
    ))
}

/// A loose file by its own header (§8.2); `ext` is lower-case, `name`
/// the file name. `None` when it is none of this module's formats.
pub fn identify(b: &mut Budgeted, ext: &str, name: &str) -> Option<ConsoleId> {
    match ext {
        "npdm" => npdm(b),
        "dol" => dol(b),
        "self" => sce_self(b, false),
        "bin" if name.eq_ignore_ascii_case("eboot.bin") => sce_self(b, true),
        "pbp" => pbp(b),
        "prx" => prx(b),
        _ => by_name(ext),
    }
}

/// A file told by its extension alone, never opened (§8.2): a Vita
/// content-manager backup (`.psvimg`, `.psvmd`) is encrypted with a key
/// tied to the account, so not one byte of it is read. The title id is
/// in its path; the planner takes it from there.
pub fn by_name(ext: &str) -> Option<ConsoleId> {
    if !matches!(ext, "psvimg" | "psvmd") {
        return None;
    }
    let mut id = ConsoleId::magic("psvita", PSVIMG_DETAIL);
    id.part = Some(PartKind::Backup);
    id.encrypted = Some(true);
    Some(id)
}

/// Size of the NPDM header.
const NPDM_HEADER: usize = 0x80;
/// Bytes of the ACI0 read: magic and the ProgramId at 0x10.
const ACI0_READ: usize = 0x18;

/// A Switch `main.npdm`: `META` at 0, the ACI0 at AciOffset (0x70) with
/// its `ACI0` magic and the ProgramId at +0x10. Part of a game's ExeFS.
fn npdm(b: &mut Budgeted) -> Option<ConsoleId> {
    if b.len() < NPDM_HEADER as u64 {
        return None;
    }
    let h = b.read(0, NPDM_HEADER).ok()?;
    if h.get(..4)? != b"META" {
        return None;
    }
    let mut id = ConsoleId::magic("switch", "NPDM header");
    id.part = Some(PartKind::ExeFs);
    let aci = u64::from(le32(&h, 0x70)?);
    let fits = aci
        .checked_add(ACI0_READ as u64)
        .is_some_and(|end| aci >= NPDM_HEADER as u64 && end <= b.len());
    if !fits {
        return Some(id);
    }
    let Ok(a) = b.read(aci, ACI0_READ) else {
        return Some(id);
    };
    if a.get(..4) != Some(b"ACI0") {
        return Some(id);
    }
    let program = le64(&a, 0x10)?;
    if program != 0 {
        id.confidence = 0.95;
        id.title_id = Some(format!("{program:016X}"));
        id.id_source = Some(IdSource::Npdm);
    }
    Some(id)
}

/// The DOL header: 7 text and 11 data sections.
const DOL_HEADER: usize = 0x100;
const DOL_TEXTS: usize = 7;
const DOL_SECTIONS: usize = 18;
/// GameCube and Wii main RAM as the loader maps it.
const RAM: std::ops::Range<u64> = 0x8000_0000..0x8180_0000;
/// Most text bytes searched for the Wii-only instruction (the D24
/// decoded budget, though nothing is decompressed).
const DOL_SCAN: u64 = super::MAX_DECODED;
/// `mtspr HID4, rS` with any `rS`: only the Wii's Broadway has HID4.
const HID4_MASK: u32 = 0xFC1F_FFFF;
const HID4_WORD: u32 = 0x7C13_FBA6;

/// One DOL section: file offset, load address, size.
#[derive(Clone, Copy)]
struct Section {
    offset: u64,
    addr: u64,
    size: u64,
}

/// A GameCube or Wii executable by its structure (there is no magic):
/// every non-empty section lies inside the file (`offset +
/// AlignUp(size, 32)`) and loads inside main RAM, the entry point too,
/// and at least one text section exists. Wii when a text word is
/// `mtspr HID4` within the first [`DOL_SCAN`] text bytes, else GameCube.
/// Homebrew (a retail game's DOL lives inside its disc).
fn dol(b: &mut Budgeted) -> Option<ConsoleId> {
    if b.len() < DOL_HEADER as u64 {
        return None;
    }
    let h = b.read(0, DOL_HEADER).ok()?;
    let mut texts = Vec::new();
    for i in 0..DOL_SECTIONS {
        let s = Section {
            offset: u64::from(be32(&h, i * 4)?),
            addr: u64::from(be32(&h, 0x48 + i * 4)?),
            size: u64::from(be32(&h, 0x90 + i * 4)?),
        };
        if s.size == 0 {
            continue;
        }
        let end = s.offset.checked_add(s.size.checked_next_multiple_of(32)?)?;
        let load_end = s.addr.checked_add(s.size)?;
        if s.offset < DOL_HEADER as u64
            || end > b.len()
            || !RAM.contains(&s.addr)
            || load_end > RAM.end
        {
            return None;
        }
        if i < DOL_TEXTS {
            texts.push(s);
        }
    }
    let entry = u64::from(be32(&h, 0xE0)?);
    if texts.is_empty() || !RAM.contains(&entry) {
        return None;
    }
    let system = if has_hid4(b, &texts) {
        "wii"
    } else {
        "gamecube"
    };
    let mut id = ConsoleId::proven(system, "DOL header");
    id.kind = TitleKind::Homebrew;
    Some(id)
}

/// A Homebrew Channel app (wiibrew «Homebrew Channel»): a folder inside
/// one named `apps` holding a `boot.dol` that checks out as a DOL. The
/// folder is the item (`meta.xml` and `icon.png` travel with it), moved
/// whole to the DOL's system with the homebrew note (D13). Only the
/// DOL's header and text are read, within the D24 budgets.
pub fn probe_apps(l: &DirListing) -> Option<FolderMatch> {
    if !l.parent.eq_ignore_ascii_case("apps") {
        return None;
    }
    let name = find(l.names, "boot.dol")?;
    let mut id = l.with_file(Path::new(name), dol)?;
    id.detail = "apps boot.dol".into();
    Some(FolderMatch {
        id,
        probe: PathBuf::from(name),
        leave: None,
    })
}

/// Whether a text section holds `mtspr HID4` within [`DOL_SCAN`] bytes;
/// a refused read ends the search (no Wii proof).
fn has_hid4(b: &mut Budgeted, texts: &[Section]) -> bool {
    let mut left = DOL_SCAN;
    for s in texts {
        if left == 0 {
            break;
        }
        // A section shorter than a word holds no instruction to find.
        let n = s.size.min(left) & !3;
        if n == 0 {
            continue;
        }
        let Some(bytes) = usize::try_from(n)
            .ok()
            .and_then(|n| b.read(s.offset, n).ok())
        else {
            return false;
        };
        let (words, _) = bytes.as_chunks::<4>();
        if words
            .iter()
            .any(|w| u32::from_be_bytes(*w) & HID4_MASK == HID4_WORD)
        {
            return true;
        }
        left -= n;
    }
    false
}

/// SCE header plus the SELF extended header, enough for every offset read.
const SCE_HEAD: usize = 0x70;
/// App info: authid, vendor id, self type, version.
const APP_INFO: usize = 0x18;
/// PS3 `self_type` of an application (scetool `SELF_TYPE_APP`).
const SELF_TYPE_APP: u32 = 4;
/// Vita homebrew authids (vita-make-fself: default, `-s`, `-ss`).
const VITA_HOMEBREW: [u64; 3] = [
    0x2F00_0000_0000_0001,
    0x2F00_0000_0000_0002,
    0x2F00_0000_0000_0003,
];
/// Control-info entries walked looking for the NPDRM one.
const MAX_CONTROL_INFO: usize = 8;
/// One control-info entry header plus the NPD fields read.
const NPD_READ: usize = 0x50;

/// A PS3 or Vita SELF (`SCE\0`, header type 1): version bytes
/// `00 00 00 02` (big-endian, PS3) or `03 00 00 00` (little-endian,
/// Vita); an unknown version is not identified. The app info says the
/// kind: a Vita homebrew authid or a lone PS3 `APP` is homebrew. An
/// `eboot.bin` is a game's executable (part `ExeFs`). A PS3 NPDRM SELF's
/// control info gives the content id and title id.
fn sce_self(b: &mut Budgeted, eboot: bool) -> Option<ConsoleId> {
    if b.len() < SCE_HEAD as u64 {
        return None;
    }
    let h = b.read(0, SCE_HEAD).ok()?;
    if h.get(..4)? != b"SCE\0" {
        return None;
    }
    let (system, big) = match h.get(4..8)? {
        [0, 0, 0, 2] => ("ps3", true),
        [3, 0, 0, 0] => ("psvita", false),
        _ => return None,
    };
    let u16_at = |o: usize| -> Option<u16> {
        let b: [u8; 2] = h.get(o..o + 2)?.try_into().ok()?;
        Some(if big {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    };
    let u64_at = |o: usize| if big { be64(&h, o) } else { le64(&h, o) };
    if u16_at(0xA)? != 1 {
        return None;
    }
    let mut id = ConsoleId::magic(system, "SELF header");
    if eboot {
        id.part = Some(PartKind::ExeFs);
    }
    // PS3 `self_header_t.app_info_offset` at 0x28; Vita's SCE header
    // keeps it at 0x38.
    let info_at = u64_at(if big { 0x28 } else { 0x38 })?;
    let fits = |at: u64, n: usize| {
        at >= 0x20 && at.checked_add(n as u64).is_some_and(|end| end <= b.len())
    };
    if !fits(info_at, APP_INFO) {
        return Some(id);
    }
    let Ok(info) = b.read(info_at, APP_INFO) else {
        return Some(id);
    };
    let (authid, self_type) = if big {
        (be64(&info, 0)?, be32(&info, 0xC)?)
    } else {
        (le64(&info, 0)?, le32(&info, 0xC)?)
    };
    id.confidence = 0.95;
    let homebrew = if big {
        self_type == SELF_TYPE_APP && !eboot
    } else {
        VITA_HOMEBREW.contains(&authid)
    };
    if homebrew {
        id.kind = TitleKind::Homebrew;
    }
    if big {
        let at = be64(&h, 0x58)?;
        let size = be64(&h, 0x60)?;
        if let Some(cid) = npdrm_content_id(b, at, size) {
            id.title_id = cid
                .get(7..16)
                .filter(|t| sony::retail_id(t))
                .map(str::to_owned);
            id.id_source = id.title_id.as_ref().map(|_| IdSource::SelfHeader);
            id.serial = id
                .title_id
                .as_deref()
                .and_then(|t| normalise_console_serial("ps3", t));
            id.content_id = Some(cid);
        }
    }
    Some(id)
}

/// The NPDRM content id of a PS3 SELF's control info at `at` (`size`
/// bytes): the type 3 entry's `NPD\0` header holds it at +0x20. At most
/// [`MAX_CONTROL_INFO`] entries are walked, each bounded by the table.
fn npdrm_content_id(b: &mut Budgeted, at: u64, size: u64) -> Option<String> {
    let table_end = at.checked_add(size)?;
    if at < 0x20 || table_end > b.len() {
        return None;
    }
    let mut cur = at;
    for _ in 0..MAX_CONTROL_INFO {
        let head_end = cur.checked_add(0x10)?;
        if head_end > table_end {
            return None;
        }
        let head = b.read(cur, 0x10).ok()?;
        let (kind, len) = (be32(&head, 0)?, u64::from(be32(&head, 4)?));
        if len < 0x10 || cur.checked_add(len)? > table_end {
            return None;
        }
        if kind == 3 {
            if len < NPD_READ as u64 {
                return None;
            }
            let e = b.read(cur, NPD_READ).ok()?;
            if e.get(0x10..0x14)? != b"NPD\0" {
                return None;
            }
            let raw = e.get(0x20..0x50)?;
            let text = std::str::from_utf8(raw).ok()?;
            return sony::content_id(text);
        }
        if be64(&head, 8)? == 0 {
            return None;
        }
        cur = cur.checked_add(len)?;
    }
    None
}

/// PBP header: magic, version and eight section offsets.
const PBP_HEADER: usize = 0x28;
/// The PSAR magics read at its offset.
const PSAR_READ: usize = 14;
/// A PBP's `PARAM.SFO` that would not fit the metadata budget: the file
/// is a PSP one by its magic, nothing more is read and it is asked about.
const PBP_UNREAD_WEIGHT: f32 = 0.70;

/// A PSP `EBOOT.PBP` (`\0PBP`): its `PARAM.SFO` (between the first two
/// offsets) and the magic of its PSAR (offset at 0x24). A PS1 classic
/// (`CATEGORY` `ME`, PSAR `PSISOIMG0000`/`PSTITLEIMG0000`) is `psx`; a
/// PSN ISO (`NPUMDIMG`) and any other PBP are `psp`, homebrew when the
/// category is `MG` and `DISC_ID` is no retail id. An SFO over
/// [`MAX_META_FILE`] leaves a `psp` verdict below the auto line.
fn pbp(b: &mut Budgeted) -> Option<ConsoleId> {
    if b.len() < PBP_HEADER as u64 {
        return None;
    }
    let h = b.read(0, PBP_HEADER).ok()?;
    if h.get(..4)? != crate::disc::pbp::PBP_MAGIC {
        return None;
    }
    let sfo_at = u64::from(le32(&h, 0x08)?);
    let sfo_end = u64::from(le32(&h, 0x0C)?);
    let psar_at = u64::from(le32(&h, 0x24)?);
    if sfo_at < PBP_HEADER as u64 || sfo_end < sfo_at || sfo_end > b.len() || psar_at > b.len() {
        return None;
    }
    let psar = psar_at
        .checked_add(PSAR_READ as u64)
        .filter(|&end| psar_at >= PBP_HEADER as u64 && end <= b.len())
        .and_then(|_| b.read(psar_at, PSAR_READ).ok());
    let starts = |magic: &[u8]| psar.as_deref().is_some_and(|p| p.starts_with(magic));
    let ps1_image = starts(crate::disc::pbp::PSISO) || starts(crate::disc::pbp::PSTITLE);
    let psn_iso = starts(b"NPUMDIMG");
    let sfo_len = sfo_end - sfo_at;
    let sfo = if sfo_len > MAX_META_FILE {
        None
    } else {
        let n = usize::try_from(sfo_len).ok()?;
        b.read(sfo_at, n).ok().and_then(|bytes| psf::parse(&bytes))
    };
    let Some(sfo) = sfo else {
        if ps1_image {
            return Some(pbp_id("psx", TitleKind::Base, None));
        }
        if psn_iso {
            return Some(pbp_id("psp", TitleKind::Base, None));
        }
        let mut id = ConsoleId::magic("psp", "PBP header");
        id.confidence = PBP_UNREAD_WEIGHT;
        return Some(id);
    };
    let category = sfo.text("CATEGORY");
    let disc = sfo.text("DISC_ID").and_then(sony::title_id);
    let retail = disc.as_deref().is_some_and(sony::retail_id);
    let (system, kind) = if ps1_image || category == Some("ME") {
        ("psx", TitleKind::Base)
    } else if psn_iso {
        ("psp", TitleKind::Base)
    } else if category == Some("MG") && !retail {
        ("psp", TitleKind::Homebrew)
    } else {
        ("psp", TitleKind::Base)
    };
    Some(pbp_id(system, kind, disc.filter(|_| retail)))
}

/// A PBP verdict with its `DISC_ID` as the title id and serial (the
/// PlayStation `AAAA-NNNNN` form the PS3 rule writes, as PS1 and PSP DATs
/// key on it).
fn pbp_id(system: &str, kind: TitleKind, disc: Option<String>) -> ConsoleId {
    let mut id = ConsoleId::proven(system, "PBP PARAM.SFO");
    id.kind = kind;
    if let Some(disc) = disc {
        id.serial = normalise_console_serial("ps3", &disc);
        id.title_id = Some(disc);
        id.id_source = Some(IdSource::Pbp);
    }
    id
}

/// `~PSP` header up to the end of `modname[28]` at 0x0A.
const PRX_HEAD: usize = 0x26;

/// A PSP module (`~PSP` at 0): a game's or a plugin's `.prx`, never a
/// game alone (part `Module`). A printable `modname` proves the header.
fn prx(b: &mut Budgeted) -> Option<ConsoleId> {
    if b.len() < PRX_HEAD as u64 {
        return None;
    }
    let h = b.read(0, PRX_HEAD).ok()?;
    if h.get(..4)? != b"~PSP" {
        return None;
    }
    let mut id = ConsoleId::magic("psp", "PRX header");
    id.part = Some(PartKind::Module);
    let name = h.get(0x0A..PRX_HEAD)?;
    let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    if end > 0
        && name[..end]
            .iter()
            .all(|c| c.is_ascii_graphic() || *c == b' ')
    {
        id.confidence = 0.95;
    }
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consoles::testing::{self as ct, SparseAt};

    fn run(bytes: &[u8], ext: &str, name: &str) -> Option<ConsoleId> {
        let mut f = SparseAt::new(bytes.len() as u64);
        f.put(0, bytes);
        let mut b = Budgeted::new(&mut f, bytes.len() as u64);
        identify(&mut b, ext, name)
    }

    #[test]
    fn npdm_gives_the_program_id() {
        let id = run(&ct::npdm(0x0100_0000_0001_0000), "npdm", "main.npdm").unwrap();
        assert_eq!(id.system, "switch");
        assert_eq!(id.part, Some(PartKind::ExeFs));
        assert_eq!(id.title_id.as_deref(), Some("0100000000010000"));
        assert_eq!(id.id_source, Some(IdSource::Npdm));
        let mut bad = ct::npdm(1);
        bad[..4].copy_from_slice(b"MATE");
        assert!(run(&bad, "npdm", "main.npdm").is_none());
    }

    #[test]
    fn dol_by_structure_and_hid4() {
        let gc = run(&ct::dol(2, false), "dol", "a.dol").unwrap();
        assert_eq!(
            (gc.system.as_str(), gc.kind),
            ("gamecube", TitleKind::Homebrew)
        );
        let wii = run(&ct::dol(2, true), "dol", "boot.dol").unwrap();
        assert_eq!(wii.system, "wii");
        // A text section shorter than one word does not end the search.
        let mut tiny = ct::dol(2, true);
        tiny[0x90..0x94].copy_from_slice(&2u32.to_be_bytes());
        assert_eq!(run(&tiny, "dol", "boot.dol").unwrap().system, "wii");
        // A load address outside RAM: not a DOL.
        let mut bad = ct::dol(1, false);
        bad[0x48..0x4C].copy_from_slice(&0x0000_3100u32.to_be_bytes());
        assert!(run(&bad, "dol", "a.dol").is_none());
        // A section past the end of the file.
        let mut short = ct::dol(1, false);
        short.truncate(0x180);
        assert!(run(&short, "dol", "a.dol").is_none());
        // No text section.
        let mut empty = ct::dol(1, false);
        empty[0x90..0x94].copy_from_slice(&0u32.to_be_bytes());
        assert!(run(&empty, "dol", "a.dol").is_none());
    }

    #[test]
    fn self_versions_and_kinds() {
        let ps3 = run(
            &ct::self_header([0, 0, 0, 2], 4, 0x1010_0000_0100_0003),
            "self",
            "a.self",
        )
        .unwrap();
        assert_eq!(
            (ps3.system.as_str(), ps3.kind),
            ("ps3", TitleKind::Homebrew)
        );
        let vita = run(
            &ct::self_header([3, 0, 0, 0], 8, 0x2F00_0000_0000_0001),
            "self",
            "a.self",
        )
        .unwrap();
        assert_eq!(
            (vita.system.as_str(), vita.kind),
            ("psvita", TitleKind::Homebrew)
        );
        let retail = run(
            &ct::self_header([3, 0, 0, 0], 8, 0x2100_0000_0000_0001),
            "self",
            "a.self",
        )
        .unwrap();
        assert_eq!(retail.kind, TitleKind::Unknown);
        let eboot = run(&ct::self_header([0, 0, 0, 2], 4, 1), "bin", "EBOOT.BIN").unwrap();
        assert_eq!(
            (eboot.kind, eboot.part),
            (TitleKind::Unknown, Some(PartKind::ExeFs))
        );
        assert!(run(&ct::self_header([0, 0, 0, 1], 4, 1), "self", "a.self").is_none());
        // Any other `.bin` is not read as a SELF.
        assert!(run(&ct::self_header([0, 0, 0, 2], 4, 1), "bin", "data.bin").is_none());
    }

    #[test]
    fn a_ps3_npdrm_self_gives_its_content_id() {
        let cid = "UP0001-NPUB30001_00-0000000000000001";
        let id = run(&ct::ps3_npdrm_self(cid), "self", "a.self").unwrap();
        assert_eq!(id.content_id.as_deref(), Some(cid));
        assert_eq!(id.title_id.as_deref(), Some("NPUB30001"));
        assert_eq!(id.id_source, Some(IdSource::SelfHeader));
        assert_eq!(id.kind, TitleKind::Unknown);
    }

    #[test]
    fn pbp_kinds() {
        let ps1 = run(
            &ct::pbp("ME", "SLUS00594", b"PSISOIMG0000"),
            "pbp",
            "EBOOT.PBP",
        )
        .unwrap();
        assert_eq!((ps1.system.as_str(), ps1.kind), ("psx", TitleKind::Base));
        assert_eq!(ps1.title_id.as_deref(), Some("SLUS00594"));
        let psn = run(&ct::pbp("EG", "NPUH10001", b"NPUMDIMG"), "pbp", "EBOOT.PBP").unwrap();
        assert_eq!((psn.system.as_str(), psn.kind), ("psp", TitleKind::Base));
        let game = run(&ct::pbp("MG", "ULUS10041", b""), "pbp", "EBOOT.PBP").unwrap();
        assert_eq!((game.system.as_str(), game.kind), ("psp", TitleKind::Base));
        let hb = run(&ct::pbp("MG", "000000001", b""), "pbp", "EBOOT.PBP").unwrap();
        assert_eq!((hb.system.as_str(), hb.kind), ("psp", TitleKind::Homebrew));
        let mut bad = ct::pbp("MG", "ULUS10041", b"");
        bad[1] = b'X';
        assert!(run(&bad, "pbp", "EBOOT.PBP").is_none());
    }

    #[test]
    fn a_pbp_sfo_over_the_budget_is_asked_about() {
        let mut f = ct::pbp_big_sfo();
        let len = f.len;
        let mut b = Budgeted::new(&mut f, len);
        let id = identify(&mut b, "pbp", "EBOOT.PBP").unwrap();
        assert_eq!((id.system.as_str(), id.kind), ("psp", TitleKind::Unknown));
        assert!(id.confidence < 0.85);
        assert!(b.used().read <= 0x100);
    }

    #[test]
    fn prx_is_a_module() {
        let id = run(&ct::prx("sceKernelLibrary"), "prx", "a.prx").unwrap();
        assert_eq!(
            (id.system.as_str(), id.part),
            ("psp", Some(PartKind::Module))
        );
        assert!(run(&ct::prx("x")[1..], "prx", "a.prx").is_none());
    }

    #[test]
    fn a_backup_is_never_read() {
        let mut f = SparseAt::new(0x1000);
        let mut guard = ct::PanicOn {
            inner: &mut f,
            ranges: std::iter::once(0..0x1000).collect(),
        };
        let mut b = Budgeted::new(&mut guard, 0x1000);
        let id = identify(&mut b, "psvimg", "game.psvimg").unwrap();
        assert_eq!(
            (id.system.as_str(), id.part),
            ("psvita", Some(PartKind::Backup))
        );
        assert!(identify(&mut b, "psvmd", "game.psvmd").is_some());
        assert_eq!(b.used().read, 0);
    }
}
