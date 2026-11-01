//! PS4 and PS5 package and image headers and the PS5 `param.json`
//! (systems batch 3 D6, D9, D10). Only the CNT header, the entry table
//! (PS4, at most [`MAX_ENTRIES`] entries) and the plaintext `param.sfo`
//! entry are read; the `icon0.png` entry is only located. A PS5 store
//! package gives its FIH header and the CNT header it points to, an exFAT
//! image 11 bytes, an `.ffpkg` the 4 bytes of its UFS2 magic. Encrypted
//! entries, the PFS body, licences and the PS5 entry table are never read
//! (D4).
//!
//! Layout checked against LibOrbisPkg at `643477263b2644e0803e0f58b8726ea4e3f3b7d4`:
//! `LibOrbisPkg/PKG/PkgReader.cs` (`ReadHeader`: big-endian, `entry_count`
//! u32 at 0x10, `entry_table_offset` u32 at 0x18, `content_id` at 0x40,
//! `content_type` u32 at 0x74, `content_flags` u32 at 0x78),
//! `LibOrbisPkg/PKG/Entry.cs` (`MetaEntry`: id, name table offset, flags1,
//! flags2, data offset, data size as big-endian u32, 8 padding bytes;
//! `Encrypted => (Flags1 & 0x80000000) != 0`) and `LibOrbisPkg/PKG/Enums.cs`
//! (`ContentType` GD 0x1A, AC 0x1B, AL 0x1C, DP 0x1E; `ContentFlags`
//! FIRST_PATCH 0x00100000, SUBSEQUENT_PATCH 0x40000000, DELTA_PATCH
//! 0x41000000, CUMULATIVE_PATCH 0x60000000; `EntryId` PARAM_SFO 0x1000,
//! ICON0_PNG 0x1200).

use super::psf;
use super::sony;
use super::{ArtFormat, ArtLocator, Budgeted, ConsoleId, IdSource, TitleKind};
use super::{MAX_DECODED, MAX_META_FILE};

/// `7F 43 4E 54` («\x7FCNT»), the PS4 package magic.
pub const CNT_MAGIC: [u8; 4] = [0x7F, b'C', b'N', b'T'];
/// The `detail` of a CNT package verdict; the plan notes it `ExtractFirst`.
pub const CNT_DETAIL: &str = "PS4 PKG header";
const CNT_HEADER: usize = 0x80;
const CONTENT_ID_AT: usize = 0x40;
const CONTENT_ID_LEN: usize = 36;
const ENTRY_LEN: usize = 32;
/// Most entries of the table read, whatever `entry_count` says (D24).
pub const MAX_ENTRIES: u32 = 64;
const ENTRY_PARAM_SFO: u32 = 0x1000;
const ENTRY_ICON0: u32 = 0x1200;
const ENCRYPTED: u32 = 0x8000_0000;
const CT_GD: u32 = 0x1A;
const CT_AC: u32 = 0x1B;
const CT_AL: u32 = 0x1C;
const CT_DP: u32 = 0x1E;
/// FIRST, SUBSEQUENT, DELTA and CUMULATIVE patch content flags.
const PATCH_FLAGS: [u32; 4] = [0x0010_0000, 0x4000_0000, 0x4100_0000, 0x6000_0000];

fn be32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        h.get(o..o.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// A CNT package (`7F 43 4E 54`): content id, title id
/// (`content_id[7..16]`), kind by content type and flags, and, when not
/// flagged encrypted and at most 64 KiB, the `param.sfo` entry through
/// [`psf::parse`] (its `CATEGORY` wins, its title id is
/// `IdSource::ParamSfo`). System: the PSF says PS4 or the title id is
/// `CUSA…` → `ps4`; `PPSA…` → `ps5`; neither → `ps4` at 0.80 (review).
/// The PFS body is never read.
pub fn cnt_pkg(b: &mut Budgeted) -> Option<ConsoleId> {
    let h = b.read(0, CNT_HEADER).ok()?;
    if h[..4] != CNT_MAGIC {
        return None;
    }
    let raw_cid = h.get(CONTENT_ID_AT..CONTENT_ID_AT + CONTENT_ID_LEN)?;
    let content_id = std::str::from_utf8(raw_cid).ok().and_then(sony::content_id);
    let tid = std::str::from_utf8(&raw_cid[7..16])
        .ok()
        .filter(|t| sony::retail_id(t))
        .map(str::to_ascii_uppercase);
    let ctype = be32(&h, 0x74)?;
    let flags = be32(&h, 0x78)?;
    let header_kind = match ctype {
        CT_GD if PATCH_FLAGS.iter().any(|f| flags & f == *f) => TitleKind::Update,
        CT_GD => TitleKind::Base,
        CT_DP => TitleKind::Update,
        CT_AC | CT_AL => TitleKind::Dlc,
        _ => TitleKind::Unknown,
    };
    let entries = entry_table(b, &h).unwrap_or_default();
    let psf = entries
        .iter()
        .find(|e| e.id == ENTRY_PARAM_SFO)
        .and_then(|e| plain_entry(b, e, MAX_META_FILE))
        .and_then(|bytes| psf::parse(&bytes));
    let psf_ps4 = psf
        .as_ref()
        .is_some_and(|p| sony::psf_systems(p).contains(&"ps4"));
    let cusa = tid.as_deref().is_some_and(|t| t.starts_with("CUSA"));
    let ppsa = tid.as_deref().is_some_and(|t| t.starts_with("PPSA"));
    let system = if psf_ps4 || cusa || !ppsa {
        "ps4"
    } else {
        "ps5"
    };
    let mut id = match &psf {
        Some(p) if system == "ps4" => sony::from_psf(p, "ps4", CNT_DETAIL),
        _ => ConsoleId::proven(system, CNT_DETAIL),
    };
    if !(psf_ps4 || cusa || ppsa) {
        id.confidence = 0.80;
    }
    if id.title_id.is_none() {
        id.title_id = tid;
        id.id_source = id.title_id.as_ref().map(|_| IdSource::Header);
    }
    // The PSF `CATEGORY` wins when it gave a kind (D9).
    if id.kind == TitleKind::Unknown {
        id.kind = header_kind;
    }
    if let Some(cid) = content_id {
        id.content_id = Some(cid);
    }
    id.region = id
        .content_id
        .as_deref()
        .and_then(super::region_from_content_id);
    let len = b.len();
    id.art.extend(
        entries
            .iter()
            .find(|e| e.id == ENTRY_ICON0)
            .filter(|e| e.flags1 & ENCRYPTED == 0)
            .filter(|e| e.size > 0 && u64::from(e.size) <= MAX_DECODED)
            .filter(|e| {
                u64::from(e.offset)
                    .checked_add(u64::from(e.size))
                    .is_some_and(|end| end <= len)
            })
            .map(|e| ArtLocator::Range {
                offset: e.offset.into(),
                len: e.size.into(),
                format: ArtFormat::Png,
            }),
    );
    Some(id)
}

/// One 32-byte entry of the table (LibOrbisPkg `MetaEntry`).
struct Entry {
    id: u32,
    flags1: u32,
    offset: u32,
    size: u32,
}

/// At most [`MAX_ENTRIES`] entries, whatever `entry_count` says; the
/// table range is checked against the file length first.
fn entry_table(b: &mut Budgeted, h: &[u8]) -> Option<Vec<Entry>> {
    let count = usize::try_from(be32(h, 0x10)?.min(MAX_ENTRIES)).ok()?;
    let at = u64::from(be32(h, 0x18)?);
    let len = count.checked_mul(ENTRY_LEN)?;
    if count == 0 || at.checked_add(u64::try_from(len).ok()?)? > b.len() {
        return None;
    }
    let t = b.read(at, len).ok()?;
    Some(
        t.as_chunks::<ENTRY_LEN>()
            .0
            .iter()
            .filter_map(|e| {
                Some(Entry {
                    id: be32(e, 0)?,
                    flags1: be32(e, 8)?,
                    offset: be32(e, 16)?,
                    size: be32(e, 20)?,
                })
            })
            .collect(),
    )
}

/// The bytes of an entry that is not flagged encrypted, at most `max`
/// bytes and inside the file; anything else is never read.
fn plain_entry(b: &mut Budgeted, e: &Entry, max: u64) -> Option<Vec<u8>> {
    if e.flags1 & ENCRYPTED != 0 || e.size == 0 || u64::from(e.size) > max {
        return None;
    }
    if u64::from(e.offset).checked_add(e.size.into())? > b.len() {
        return None;
    }
    b.read(e.offset.into(), usize::try_from(e.size).ok()?).ok()
}

// ---------------------------------------------------------------------
// PS5 (systems batch 3 D6, D10)
// ---------------------------------------------------------------------

/// The `detail` of a PS5 `param.json` verdict.
pub const PARAM_JSON_DETAIL: &str = "sce_sys/param.json";
/// The `detail` of a PS5 `param.json` verdict whose `attribute3` says
/// the title is enhanced for the PS5 Pro; the plan notes it `ProEnhanced`
/// (the flag rides on the fixed `detail` vocabulary, so [`ConsoleId`]
/// gains no field for it).
pub const PARAM_JSON_PRO_DETAIL: &str = "sce_sys/param.json, PS5 Pro";
/// The `detail` of a PS5 `.zar` verdict (D12): its `sce_sys/param.json`.
pub const ZAR_PARAM_JSON_DETAIL: &str = "ZAR sce_sys/param.json";
/// [`ZAR_PARAM_JSON_DETAIL`] for a title enhanced for the PS5 Pro.
pub const ZAR_PARAM_JSON_PRO_DETAIL: &str = "ZAR sce_sys/param.json, PS5 Pro";
/// `attribute3` bit 22 (KytyPS5 `title_supports_trinity`).
const TRINITY: i64 = 1 << 22;

/// A PS5 `param.json` (at most [`MAX_META_FILE`] bytes, UTF-8, BOM
/// stripped; the size is checked before parsing). Fields, as KytyPS5
/// `src/loader/systemContent.cpp` at `893b0355` reads them: `titleId`,
/// `contentId`, `contentVersion` else `appVersion`, `attribute3` (an
/// integer only), `localizedParameters.defaultLanguage` and
/// `<lang>.titleName` (the default language, else `en-US`, else the first
/// language object). Kind: `originContentVersion` or
/// `targetContentVersion` → Update; else Base with `eboot.bin` at the
/// folder root and Dlc without (D6). `applicationCategoryType` is not
/// used (sources disagree).
pub fn param_json(bytes: &[u8], has_eboot: bool) -> Option<ConsoleId> {
    use serde_json::Value;
    if bytes.len() as u64 > MAX_META_FILE {
        return None;
    }
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches('\u{feff}');
    let v: Value = serde_json::from_str(text).ok()?;
    let tid = v.get("titleId")?.as_str().and_then(sony::title_id)?;
    let s = |k: &str| v.get(k).and_then(Value::as_str);
    let pro = v
        .get("attribute3")
        .and_then(Value::as_i64)
        .is_some_and(|a| a & TRINITY != 0);
    let detail = if pro {
        PARAM_JSON_PRO_DETAIL
    } else {
        PARAM_JSON_DETAIL
    };
    let mut id = ConsoleId::proven("ps5", detail);
    id.id_source = Some(IdSource::ParamJson);
    id.content_id = s("contentId").and_then(sony::content_id);
    id.region = id
        .content_id
        .as_deref()
        .and_then(super::region_from_content_id);
    id.version = s("contentVersion")
        .or_else(|| s("appVersion"))
        .and_then(super::clean_title);
    let langs = v.get("localizedParameters").and_then(Value::as_object);
    id.title = langs
        .and_then(|l| {
            let name = |lang: &str| l.get(lang)?.get("titleName")?.as_str();
            l.get("defaultLanguage")
                .and_then(Value::as_str)
                .and_then(name)
                .or_else(|| name("en-US"))
                .or_else(|| l.values().find_map(|o| o.get("titleName")?.as_str()))
        })
        .and_then(super::clean_title);
    id.kind = if v.get("originContentVersion").is_some() || v.get("targetContentVersion").is_some()
    {
        TitleKind::Update
    } else if has_eboot {
        TitleKind::Base
    } else {
        TitleKind::Dlc
    };
    id.title_id = Some(tid);
    Some(id)
}

/// `7F 46 49 48` («\x7FFIH»), the PS5 store package magic.
pub const FIH_MAGIC: [u8; 4] = [0x7F, b'F', b'I', b'H'];
/// The `detail` of a PS5 store package verdict; the plan notes it
/// `NoEmulator`.
pub const FIH_DETAIL: &str = "PS5 PKG header";
/// The FIH header bytes read: the magic and the CNT offset at 0x58.
const FIH_HEADER: usize = 0x60;
const FIH_CNT_OFFSET: usize = 0x58;

fn le64(h: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        h.get(o..o.checked_add(8)?)?.try_into().ok()?,
    ))
}

/// A PS5 store package (D10): the FIH header (0x60 bytes), then the CNT
/// header (0x80 bytes) at the little-endian u64 at FIH+0x58, which must
/// start with [`CNT_MAGIC`]. Only the content id (CNT+0x40, its title id
/// and region) and the content type (CNT+0x74, kinds as for the PS4) are
/// read: the entry table and the `param.json` entry are never read (their
/// ids are not verified, and they may be encrypted, D4).
pub fn fih_pkg(b: &mut Budgeted) -> Option<ConsoleId> {
    let h = b.read(0, FIH_HEADER).ok()?;
    if h[..4] != FIH_MAGIC {
        return None;
    }
    let at = le64(&h, FIH_CNT_OFFSET)?;
    if at < FIH_HEADER as u64 || at.checked_add(CNT_HEADER as u64)? > b.len() {
        return None;
    }
    let cnt = b.read(at, CNT_HEADER).ok()?;
    if cnt[..4] != CNT_MAGIC {
        return None;
    }
    let raw_cid = cnt.get(CONTENT_ID_AT..CONTENT_ID_AT + CONTENT_ID_LEN)?;
    let mut id = ConsoleId::proven("ps5", FIH_DETAIL);
    id.content_id = std::str::from_utf8(raw_cid).ok().and_then(sony::content_id);
    id.title_id = std::str::from_utf8(&raw_cid[7..16])
        .ok()
        .filter(|t| sony::retail_id(t))
        .map(str::to_ascii_uppercase);
    id.id_source = id.title_id.as_ref().map(|_| IdSource::Header);
    id.region = id
        .content_id
        .as_deref()
        .and_then(super::region_from_content_id);
    id.kind = match be32(&cnt, 0x74)? {
        CT_GD => TitleKind::Base,
        CT_DP => TitleKind::Update,
        CT_AC | CT_AL => TitleKind::Dlc,
        _ => TitleKind::Unknown,
    };
    Some(id)
}

/// The `detail` of a PS5 exFAT image verdict; the plan notes it
/// `NoEmulator`.
pub const EXFAT_DETAIL: &str = "exFAT image";
/// The exFAT boot sector's file system name, at byte 3.
const EXFAT_OEM: &[u8; 8] = b"EXFAT   ";

/// A `PPSA` + five digits token in a file name (`PPSA09519.exfat`,
/// `Game [PPSA09519].exfat`), not inside a longer word or number;
/// upper-cased.
fn ppsa_token(name: &str) -> Option<String> {
    let b = name.as_bytes();
    (0..b.len().saturating_sub(8)).find_map(|i| {
        let t = b.get(i..i + 9)?;
        let before = i.checked_sub(1).and_then(|j| b.get(j));
        let after = b.get(i + 9);
        (t[..4].eq_ignore_ascii_case(b"PPSA")
            && t[4..].iter().all(u8::is_ascii_digit)
            && before.is_none_or(|c| !c.is_ascii_alphanumeric())
            && after.is_none_or(|c| !c.is_ascii_digit()))
        .then(|| String::from_utf8_lossy(t).to_ascii_uppercase())
    })
}

/// A PS5 exFAT image (D10; ps5upload `client/src/api/fpkg.ts` at
/// `21f40fd1`): `.exfat`, the file system name `EXFAT   ` at byte 3 **and**
/// a `PPSA` token in the file name, which gives the title id. Without the
/// token nothing is said (the `ps5` folder still accepts the weak
/// extension).
pub fn exfat_ps5(b: &mut Budgeted, ext: &str, name: &str) -> Option<ConsoleId> {
    if ext != "exfat" {
        return None;
    }
    let tid = ppsa_token(name)?;
    let h = b.read(0, 3 + EXFAT_OEM.len()).ok()?;
    if &h[3..] != EXFAT_OEM {
        return None;
    }
    let mut id = ConsoleId::magic("ps5", EXFAT_DETAIL);
    id.title_id = Some(tid);
    id.id_source = Some(IdSource::FileName);
    Some(id)
}

/// The `detail` of a `.ffpkg` verdict whose UFS2 superblock checked out.
pub const UFS2_DETAIL: &str = "UFS2 superblock";
/// `SBLOCK_UFS2` (65536) + the offset of `fs_magic` in `struct fs`
/// (0x55C), FreeBSD `sys/ufs/ffs/fs.h` at `661ca921e8cd`; the offset was
/// computed with `offsetof` over that header's `struct fs` (1376 bytes).
const UFS2_MAGIC_AT: u64 = 0x1_0000 + 0x55C;
/// `FS_UFS2_MAGIC`, stored in the host's (little-endian) byte order.
const UFS2_MAGIC: u32 = 0x1954_0119;

/// Whether the file holds a UFS2 superblock (4 bytes at 0x1055C).
pub fn ufs2(b: &mut Budgeted) -> bool {
    b.read(UFS2_MAGIC_AT, 4)
        .ok()
        .and_then(|m| m.try_into().ok())
        .is_some_and(|m: [u8; 4]| u32::from_le_bytes(m) == UFS2_MAGIC)
}

/// A PS5 file by its own header (D10): a store package (FIH) under any
/// extension, an `.exfat` image with a `PPSA` name token, an `.ffpkg`
/// whose UFS2 superblock checks out. `.ffpfs`/`.ffpfsc` are never read
/// here (extension evidence only).
pub fn identify(b: &mut Budgeted, ext: &str, name: &str) -> Option<ConsoleId> {
    if b.len() < 8 {
        return None;
    }
    if b.read(0, 4).ok()? == FIH_MAGIC {
        return fih_pkg(b);
    }
    match ext {
        "exfat" => exfat_ps5(b, ext, name),
        "ffpkg" if ufs2(b) => Some(ConsoleId::proven("ps5", UFS2_DETAIL)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{self, RangeLog, SparseAt, CNT_ENTRY_TABLE};
    use super::super::{Used, MAX_READ, MAX_SEEKS};
    use super::*;
    use crate::identify::names::Region;

    const BLOODBORNE: &str = "UP9000-CUSA00900_00-BLOODBORNE000000";
    const EU: &str = "EP0002-CUSA04447_00-SAMPLEGAME000000";

    fn run(f: &mut SparseAt) -> (Option<ConsoleId>, Used) {
        let len = f.len;
        let mut b = Budgeted::new(f, len);
        let id = cnt_pkg(&mut b);
        let used = b.used();
        assert!(used.read <= MAX_READ && used.seeks <= MAX_SEEKS, "{used:?}");
        (id, used)
    }

    fn sfo(tid: &str, category: &str, cid: &str) -> Vec<u8> {
        testing::ps4_sfo(tid, category, cid, 1)
    }

    #[test]
    fn header_gives_content_id_title_id_and_region() {
        let mut f = testing::cnt_pkg(EU, CT_GD, 0, &[]);
        let id = run(&mut f).0.unwrap();
        assert_eq!(id.system, "ps4");
        assert_eq!(id.confidence, 0.95);
        assert_eq!(id.detail, CNT_DETAIL);
        assert_eq!(id.content_id.as_deref(), Some(EU));
        assert_eq!(id.title_id.as_deref(), Some("CUSA04447"));
        assert_eq!(id.id_source, Some(IdSource::Header));
        assert_eq!(id.region, Some(Region::Europe));
        assert_eq!(id.kind, TitleKind::Base);
    }

    #[test]
    fn kinds_by_content_type_and_flags() {
        for (ctype, flags, kind) in [
            (CT_GD, 0, TitleKind::Base),
            (CT_GD, 0x0010_0000, TitleKind::Update),
            (CT_GD, 0x4000_0000, TitleKind::Update),
            (CT_GD, 0x4100_0000, TitleKind::Update),
            (CT_GD, 0x6000_0000 | 0x0000_0002, TitleKind::Update),
            (CT_DP, 0, TitleKind::Update),
            (CT_AC, 0, TitleKind::Dlc),
            (CT_AL, 0, TitleKind::Dlc),
            (0x99, 0, TitleKind::Unknown),
        ] {
            let mut f = testing::cnt_pkg(BLOODBORNE, ctype, flags, &[]);
            assert_eq!(run(&mut f).0.unwrap().kind, kind, "{ctype:#x} {flags:#x}");
        }
    }

    #[test]
    fn a_plain_param_sfo_entry_is_read_and_its_category_wins() {
        let mut f = testing::cnt_pkg(
            BLOODBORNE,
            CT_GD,
            0,
            &[(ENTRY_PARAM_SFO, 0, sfo("CUSA00900", "gp", BLOODBORNE))],
        );
        let id = run(&mut f).0.unwrap();
        assert_eq!(id.kind, TitleKind::Update);
        assert_eq!(id.title.as_deref(), Some("Test Game"));
        assert_eq!(id.version.as_deref(), Some("01.09"));
        assert_eq!(id.title_id.as_deref(), Some("CUSA00900"));
        assert_eq!(id.id_source, Some(IdSource::ParamSfo));
        assert_eq!(id.detail, CNT_DETAIL);
        assert_eq!(id.region, Some(Region::Usa));
    }

    #[test]
    fn a_ppsa_content_id_is_ps5_and_neither_is_ps4_for_review() {
        let mut f = testing::cnt_pkg("UP9000-PPSA01234_00-SAMPLEGAME000000", CT_GD, 0, &[]);
        let id = run(&mut f).0.unwrap();
        assert_eq!((id.system.as_str(), id.confidence), ("ps5", 0.95));
        assert_eq!(id.title_id.as_deref(), Some("PPSA01234"));
        let mut f = testing::cnt_pkg("UP0000-ABCD12345_00-SAMPLEGAME000000", CT_GD, 0, &[]);
        let id = run(&mut f).0.unwrap();
        assert_eq!((id.system.as_str(), id.confidence), ("ps4", 0.80));
        let mut f = testing::cnt_pkg("garbage", CT_GD, 0, &[]);
        let id = run(&mut f).0.unwrap();
        assert_eq!((id.system.as_str(), id.confidence), ("ps4", 0.80));
        assert_eq!((id.title_id, id.content_id, id.region), (None, None, None));
        // A PSF that names the PS4 makes an odd id a PS4 package.
        let mut f = testing::cnt_pkg(
            "UP0000-ABCD12345_00-SAMPLEGAME000000",
            CT_GD,
            0,
            &[(ENTRY_PARAM_SFO, 0, sfo("ABCD12345", "gd", BLOODBORNE))],
        );
        let id = run(&mut f).0.unwrap();
        assert_eq!((id.system.as_str(), id.confidence), ("ps4", 0.95));
    }

    #[test]
    fn encrypted_param_sfo_entry_is_never_read() {
        let mut f = testing::cnt_pkg(
            BLOODBORNE,
            CT_GD,
            0,
            &[(
                ENTRY_PARAM_SFO,
                ENCRYPTED,
                sfo("CUSA00900", "gp", BLOODBORNE),
            )],
        );
        let range = f.entry_range(0);
        let len = f.len;
        let mut log = RangeLog::new(&mut f);
        let id = {
            let mut b = Budgeted::new(&mut log, len);
            cnt_pkg(&mut b).unwrap()
        };
        assert!(
            !log.touched(range.start, range.end - range.start),
            "{:?}",
            log.ranges
        );
        // Header-only verdict: the header's kind and title id.
        assert_eq!(id.kind, TitleKind::Base);
        assert_eq!(id.id_source, Some(IdSource::Header));
        assert_eq!(id.title, None);
    }

    #[test]
    fn a_non_psf_param_entry_leaves_the_header_verdict() {
        let mut f = testing::cnt_pkg(
            BLOODBORNE,
            CT_AC,
            0,
            &[(ENTRY_PARAM_SFO, 0, vec![0xAB; 512])],
        );
        let id = run(&mut f).0.unwrap();
        assert_eq!(
            (id.kind, id.id_source, id.title),
            (TitleKind::Dlc, Some(IdSource::Header), None)
        );
    }

    #[test]
    fn an_oversized_param_entry_is_not_read() {
        let mut big = sfo("CUSA00900", "gp", BLOODBORNE);
        big.resize(MAX_META_FILE as usize + 1, 0);
        let mut f = testing::cnt_pkg(BLOODBORNE, CT_GD, 0, &[(ENTRY_PARAM_SFO, 0, big)]);
        let (id, used) = run(&mut f);
        assert_eq!(id.unwrap().kind, TitleKind::Base);
        assert!(used.read < MAX_META_FILE, "{used:?}");
    }

    #[test]
    fn an_adversarial_entry_count_stops_at_64_entries() {
        let mut f = testing::cnt_pkg(
            BLOODBORNE,
            CT_GD,
            0,
            &[(ENTRY_PARAM_SFO, 0, sfo("CUSA00900", "gd", BLOODBORNE))],
        );
        f.put(0x10, &0x8000_0000u32.to_be_bytes());
        // The table is longer than the file would need: still in bounds.
        f.len = f.len.max(CNT_ENTRY_TABLE + 64 * 32 + 0x1000);
        let (id, used) = run(&mut f);
        // The clamped table is read, so the `param.sfo` entry still is;
        // an unclamped count would overrun the file and drop the table.
        assert_eq!(id.unwrap().id_source, Some(IdSource::ParamSfo));
        assert!(
            used.read <= (CNT_HEADER + 64 * ENTRY_LEN) as u64 + MAX_META_FILE,
            "{used:?}"
        );
        // A table past the end of the file is not read at all.
        let mut f = testing::cnt_pkg(BLOODBORNE, CT_GD, 0, &[]);
        f.put(0x10, &1u32.to_be_bytes())
            .put(0x18, &0xFFFF_FFF0u32.to_be_bytes());
        let (id, used) = run(&mut f);
        assert_eq!(id.unwrap().title_id.as_deref(), Some("CUSA00900"));
        assert_eq!(used.read, CNT_HEADER as u64);
    }

    #[test]
    fn a_plain_icon0_entry_is_a_range_and_an_encrypted_one_is_not() {
        let png = testing::png_8x8();
        let mut f = testing::cnt_pkg(BLOODBORNE, CT_GD, 0, &[(ENTRY_ICON0, 0, png.clone())]);
        let range = f.entry_range(0);
        let id = run(&mut f).0.unwrap();
        assert_eq!(
            id.art,
            [ArtLocator::Range {
                offset: range.start,
                len: png.len() as u64,
                format: ArtFormat::Png
            }]
        );
        let mut f = testing::cnt_pkg(BLOODBORNE, CT_GD, 0, &[(ENTRY_ICON0, ENCRYPTED, png)]);
        assert!(run(&mut f).0.unwrap().art.is_empty());
    }

    #[test]
    fn not_a_cnt_package() {
        let mut f = testing::pkg(1, "UP0001-NPUB30024_00-0000000000000001");
        assert_eq!(run(&mut f).0, None);
        let mut tiny = SparseAt::new(0x20);
        tiny.put(0, &CNT_MAGIC);
        assert_eq!(run(&mut tiny).0, None);
    }

    #[test]
    fn budgets_on_the_largest_fixture() {
        let entries: Vec<(u32, u32, Vec<u8>)> = (0..63)
            .map(|i| (0x2000 + i, 0, vec![0u8; 16]))
            .chain([(ENTRY_PARAM_SFO, 0, sfo("CUSA00900", "gd", BLOODBORNE))])
            .collect();
        let mut f = testing::cnt_pkg(BLOODBORNE, CT_GD, 0, &entries);
        let (id, used) = run(&mut f);
        assert_eq!(id.unwrap().id_source, Some(IdSource::ParamSfo));
        assert!(used.seeks <= 3, "{used:?}");
    }

    // --- PS5 (D6, D10) ---

    fn json(v: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&v).unwrap()
    }

    #[test]
    fn param_json_fields() {
        let id = param_json(
            &testing::ps5_param_json("PPSA01284", "Returnal", false),
            true,
        )
        .unwrap();
        assert_eq!((id.system.as_str(), id.confidence), ("ps5", 0.95));
        assert_eq!(id.detail, PARAM_JSON_DETAIL);
        assert_eq!(id.title_id.as_deref(), Some("PPSA01284"));
        assert_eq!(id.id_source, Some(IdSource::ParamJson));
        assert_eq!(
            id.content_id.as_deref(),
            Some("UP9000-PPSA01284_00-SAMPLEGAME000000")
        );
        assert_eq!(id.region, Some(Region::Usa));
        assert_eq!(id.version.as_deref(), Some("01.000.000"));
        assert_eq!(id.title.as_deref(), Some("Returnal"));
        assert_eq!(id.kind, TitleKind::Base);
        // `appVersion` when there is no `contentVersion`; a bad content
        // id is dropped.
        let id = param_json(
            &json(serde_json::json!({
                "titleId": "ppsa01284", "appVersion": "01.002", "contentId": "short"
            })),
            true,
        )
        .unwrap();
        assert_eq!(id.title_id.as_deref(), Some("PPSA01284"));
        assert_eq!(id.version.as_deref(), Some("01.002"));
        assert_eq!((id.content_id, id.region, id.title), (None, None, None));
        // No title id: not a PS5 verdict.
        assert_eq!(
            param_json(&json(serde_json::json!({"contentVersion": "1"})), true),
            None
        );
    }

    #[test]
    fn param_json_title_by_language() {
        let names = |l: serde_json::Value| {
            param_json(
                &json(serde_json::json!({"titleId": "PPSA01284", "localizedParameters": l})),
                true,
            )
            .unwrap()
            .title
        };
        let default = names(serde_json::json!({
            "defaultLanguage": "ja-JP",
            "en-US": {"titleName": "English"},
            "ja-JP": {"titleName": "Japanese"},
        }));
        assert_eq!(default.as_deref(), Some("Japanese"));
        let en = names(serde_json::json!({
            "defaultLanguage": "fr-FR",
            "de-DE": {"titleName": "German"},
            "en-US": {"titleName": "English"},
        }));
        assert_eq!(en.as_deref(), Some("English"));
        let first = names(serde_json::json!({"de-DE": {"titleName": "German"}}));
        assert_eq!(first.as_deref(), Some("German"));
    }

    #[test]
    fn param_json_bom_invalid_and_oversize() {
        let mut bom = "\u{feff}".as_bytes().to_vec();
        bom.extend(testing::ps5_param_json("PPSA01284", "Returnal", false));
        assert!(param_json(&bom, true).is_some());
        assert_eq!(param_json(b"{\"titleId\": \"PPSA01284\"", true), None);
        assert_eq!(param_json(&[0xFF, 0xFE, b'{', 0], true), None);
        let mut big = testing::ps5_param_json("PPSA01284", "Returnal", false);
        big.truncate(big.len() - 1);
        big.resize(MAX_META_FILE as usize, b' ');
        big.push(b'}');
        assert_eq!(big.len() as u64, MAX_META_FILE + 1);
        assert_eq!(param_json(&big, true), None);
        big.retain(|&c| c != b' ');
        assert!(param_json(&big, true).is_some());
    }

    #[test]
    fn param_json_pro_kinds() {
        let pro = param_json(&testing::ps5_param_json("PPSA01284", "R", true), true).unwrap();
        assert_eq!(pro.detail, PARAM_JSON_PRO_DETAIL);
        let text = param_json(
            &json(serde_json::json!({"titleId": "PPSA01284", "attribute3": "4194304"})),
            true,
        )
        .unwrap();
        assert_eq!(text.detail, PARAM_JSON_DETAIL);
        let patch = param_json(
            &json(serde_json::json!({
                "titleId": "PPSA01284", "targetContentVersion": "01.001.000"
            })),
            true,
        )
        .unwrap();
        assert_eq!(patch.kind, TitleKind::Update);
        let origin = param_json(
            &json(serde_json::json!({
                "titleId": "PPSA01284", "originContentVersion": "01.000.000"
            })),
            false,
        )
        .unwrap();
        assert_eq!(origin.kind, TitleKind::Update);
        let dlc = param_json(&testing::ps5_param_json("PPSA01284", "R", false), false).unwrap();
        assert_eq!(dlc.kind, TitleKind::Dlc);
    }

    fn run_fih(f: &mut SparseAt) -> (Option<ConsoleId>, Used) {
        let len = f.len;
        let mut b = Budgeted::new(f, len);
        let id = fih_pkg(&mut b);
        (id, b.used())
    }

    #[test]
    fn fih_pkg_reads_the_cnt_content_id_and_type() {
        let mut f = testing::fih_pkg("EP9000-PPSA01284_00-SAMPLEGAME000000", CT_GD);
        let (id, used) = run_fih(&mut f);
        let id = id.unwrap();
        assert_eq!((id.system.as_str(), id.confidence), ("ps5", 0.95));
        assert_eq!(id.detail, FIH_DETAIL);
        assert_eq!(id.title_id.as_deref(), Some("PPSA01284"));
        assert_eq!(id.id_source, Some(IdSource::Header));
        assert_eq!(id.region, Some(Region::Europe));
        assert_eq!(id.kind, TitleKind::Base);
        assert_eq!(used.read, (FIH_HEADER + CNT_HEADER) as u64);
        let mut f = testing::fih_pkg("UP9000-PPSA01284_00-SAMPLEGAME000000", CT_AC);
        assert_eq!(run_fih(&mut f).0.unwrap().kind, TitleKind::Dlc);
    }

    #[test]
    fn fih_entry_table_is_never_read() {
        let mut f = testing::fih_pkg("UP9000-PPSA01284_00-SAMPLEGAME000000", CT_GD);
        let table = f.entry_range(0);
        let len = f.len;
        let mut log = RangeLog::new(&mut f);
        {
            let mut b = Budgeted::new(&mut log, len);
            assert!(identify(&mut b, "pkg", "x.pkg").is_some());
        }
        assert!(!log.touched(table.start, table.end - table.start));
        let cnt = testing::FIH_CNT;
        for &(o, n) in &log.ranges {
            let end = o + n as u64;
            assert!(
                end <= FIH_HEADER as u64 || (o >= cnt && end <= cnt + CNT_HEADER as u64),
                "{:?}",
                log.ranges
            );
        }
    }

    #[test]
    fn a_cnt_offset_past_the_end_is_no_verdict() {
        for at in [0xFFFF_FFFF_FFFF_FFF0u64, 0x10_0000 - 0x40, 0x10] {
            let mut f = testing::fih_pkg("UP9000-PPSA01284_00-SAMPLEGAME000000", CT_GD);
            f.put(0x58, &at.to_le_bytes());
            assert_eq!(run_fih(&mut f).0, None, "{at:#x}");
        }
        // The CNT magic must be there.
        let mut f = testing::fih_pkg("UP9000-PPSA01284_00-SAMPLEGAME000000", CT_GD);
        f.put(testing::FIH_CNT, b"XXXX");
        assert_eq!(run_fih(&mut f).0, None);
    }

    #[test]
    fn ufs2_superblock_magic() {
        for (magic, want) in [(true, true), (false, false)] {
            let mut f = testing::ufs2_image(magic);
            let len = f.len;
            let mut b = Budgeted::new(&mut f, len);
            assert_eq!(ufs2(&mut b), want);
            assert_eq!(b.used().read, 4);
        }
        let mut f = testing::ufs2_image(true);
        let len = f.len;
        let mut b = Budgeted::new(&mut f, len);
        let id = identify(&mut b, "ffpkg", "game.ffpkg").unwrap();
        assert_eq!(
            (id.system.as_str(), id.detail.as_str()),
            ("ps5", UFS2_DETAIL)
        );
        // A short file has no superblock.
        let mut tiny = SparseAt::new(0x1000);
        let mut b = Budgeted::new(&mut tiny, 0x1000);
        assert!(!ufs2(&mut b));
    }

    #[test]
    fn exfat_needs_the_oem_name_and_a_ppsa_token() {
        let check = |name: &str| {
            let mut f = testing::exfat_image();
            let len = f.len;
            let mut b = Budgeted::new(&mut f, len);
            identify(&mut b, "exfat", name)
        };
        let id = check("PPSA09519.exfat").unwrap();
        assert_eq!((id.system.as_str(), id.confidence), ("ps5", 0.90));
        assert_eq!(id.detail, EXFAT_DETAIL);
        assert_eq!(id.title_id.as_deref(), Some("PPSA09519"));
        assert_eq!(id.id_source, Some(IdSource::FileName));
        assert_eq!(
            check("Game [ppsa09519] v1.exfat")
                .unwrap()
                .title_id
                .as_deref(),
            Some("PPSA09519")
        );
        for name in [
            "game.exfat",
            "XPPSA09519.exfat",
            "PPSA095190.exfat",
            "PPSA0951.exfat",
        ] {
            assert_eq!(check(name), None, "{name}");
        }
        // Without the file system name, even a token says nothing.
        let mut f = SparseAt::new(0x1_0000);
        f.put(3, b"NTFS    ");
        let mut b = Budgeted::new(&mut f, 0x1_0000);
        assert_eq!(exfat_ps5(&mut b, "exfat", "PPSA09519.exfat"), None);
    }
}
