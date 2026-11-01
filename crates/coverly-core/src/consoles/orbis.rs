//! PS4 and PS5 package and image headers (systems batch 3 D9, D10). Only
//! the CNT header, the entry table (PS4, at most [`MAX_ENTRIES`] entries)
//! and the plaintext `param.sfo` entry are read; the `icon0.png` entry is
//! only located. Encrypted entries, the PFS body, licences and the PS5
//! entry table are never read (D4).
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
        assert!(id.is_some());
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
}
