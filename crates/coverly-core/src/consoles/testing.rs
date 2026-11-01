//! In-memory sources for the console reader tests: a sparse file with a
//! declared length (far offsets without a real 25 GB file, also on NTFS
//! where `set_len` files are not sparse) and a recorder of every range
//! read (D4 tests: prove a reader never touches a ticket or key range).
//! No byte comes from a real game.

use std::collections::BTreeMap;
use std::io;

use crate::identify::sniff::ReadAt;

/// A sparse file in memory: declared length, bytes at given offsets,
/// zeros elsewhere.
#[derive(Clone, Debug, Default)]
pub struct SparseAt {
    pub len: u64,
    pub parts: BTreeMap<u64, Vec<u8>>,
}

impl SparseAt {
    pub fn new(len: u64) -> Self {
        SparseAt {
            len,
            parts: BTreeMap::new(),
        }
    }

    /// Puts `bytes` at `offset` (a later part wins where two overlap).
    ///
    /// Reads lay parts out by offset, not by insertion, so the bytes an
    /// older part shares with this one are overwritten here too.
    pub fn put(&mut self, offset: u64, bytes: &[u8]) -> &mut Self {
        let end = offset.saturating_add(bytes.len() as u64);
        for (&start, old) in self.parts.iter_mut() {
            let old_end = start.saturating_add(old.len() as u64);
            if start >= end || old_end <= offset {
                continue;
            }
            let from = start.max(offset);
            let to = old_end.min(end);
            old[(from - start) as usize..(to - start) as usize]
                .copy_from_slice(&bytes[(from - offset) as usize..(to - offset) as usize]);
        }
        self.parts.insert(offset, bytes.to_vec());
        self
    }

    /// Every byte, zeros included, to write a small fixture to disk.
    /// Panics above 64 MiB: far offsets stay in memory.
    pub fn bytes(&self) -> Vec<u8> {
        let len = usize::try_from(self.len)
            .ok()
            .filter(|&l| l <= 64 << 20)
            .expect("a sparse fixture this large stays in memory");
        let mut out = vec![0u8; len];
        let mut copy = self.clone();
        let n = copy.read_at(0, &mut out).expect("in memory");
        assert_eq!(n, len);
        out
    }
}

impl ReadAt for SparseAt {
    /// Zeros between parts; `Ok(0)` at or past `len`.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        if offset >= self.len {
            return Ok(0);
        }
        let left = self.len - offset;
        let n = usize::try_from(left).map_or(buf.len(), |l| l.min(buf.len()));
        let out = &mut buf[..n];
        out.fill(0);
        let end = offset + n as u64;
        for (&start, bytes) in &self.parts {
            let part_end = start.saturating_add(bytes.len() as u64);
            if start >= end || part_end <= offset {
                continue;
            }
            let from = start.max(offset);
            let to = part_end.min(end);
            let src = &bytes[(from - start) as usize..(to - start) as usize];
            out[(from - offset) as usize..(to - offset) as usize].copy_from_slice(src);
        }
        Ok(n)
    }
}

/// Records every range read through it (D4 tests).
pub struct RangeLog<'a> {
    pub inner: &'a mut dyn ReadAt,
    /// `(offset, requested length)` of every read, in order.
    pub ranges: Vec<(u64, usize)>,
}

impl<'a> RangeLog<'a> {
    pub fn new(inner: &'a mut dyn ReadAt) -> Self {
        RangeLog {
            inner,
            ranges: Vec::new(),
        }
    }

    /// Whether any read touched `[start, start + len)`.
    pub fn touched(&self, start: u64, len: u64) -> bool {
        let end = start.saturating_add(len);
        self.ranges.iter().any(|&(o, n)| {
            let o_end = o.saturating_add(n as u64);
            o < end && start < o_end
        })
    }
}

impl ReadAt for RangeLog<'_> {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.ranges.push((offset, buf.len()));
        self.inner.read_at(offset, buf)
    }
}

// ---------------------------------------------------------------------
// GameCube and Wii (spec §8.1). Synthetic headers only.
// ---------------------------------------------------------------------

/// A GameCube (or Wii, with `wii`) image is about this large.
const GC_DISC_LEN: u64 = 1_459_978_240;
const WII_DISC_LEN: u64 = 4_699_979_776;

/// The 0x80-byte disc header: `id6`, disc byte 0, version 0, the Wii or
/// the GameCube magic and the title `Test Game`.
pub fn gc_header(id6: &str, wii: bool) -> Vec<u8> {
    let mut h = vec![0u8; 0x80];
    let id = id6.as_bytes();
    h[..id.len().min(6)].copy_from_slice(&id[..id.len().min(6)]);
    if wii {
        h[0x18..0x1C].copy_from_slice(&[0x5D, 0x1C, 0x9E, 0xA3]);
    } else {
        h[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
    }
    h[0x20..0x29].copy_from_slice(b"Test Game");
    h
}

/// A plain ISO/GCM image whose disc header names `id6`.
pub fn gc_disc(id6: &str, wii: bool) -> SparseAt {
    let mut s = SparseAt::new(if wii { WII_DISC_LEN } else { GC_DISC_LEN });
    s.put(0, &gc_header(id6, wii));
    s
}

fn le32(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// A Nintendo CISO (Dolphin `CISOBlob`): `CISO`, block size 2 MiB, map
/// with block 0 present, the disc header at 0x8000.
pub fn ciso(disc: &[u8]) -> SparseAt {
    let mut s = SparseAt::new(0x8000 + 0x20_0000);
    let mut head = Vec::from(*b"CISO");
    head.extend(le32(0x20_0000));
    head.push(1);
    s.put(0, &head).put(0x8000, disc);
    s
}

/// A PSP CSO: same magic, header size 0x18 at 4, image size at 8.
pub fn psp_cso() -> SparseAt {
    let mut head = Vec::from(*b"CISO");
    head.extend(le32(0x18));
    head.extend(0x1000_0000u64.to_le_bytes());
    head.extend(le32(0x800));
    head.extend([1, 0, 0, 0]);
    let mut s = SparseAt::new(0x10_0000);
    s.put(0, &head);
    s
}

/// A GCZ image (Dolphin `CompressedBlob`) of two 0x8000-byte blocks whose
/// first holds `disc`; block 0 stored or zlib-compressed. The sub-type
/// follows the disc magic.
pub fn gcz(disc: &[u8], stored: bool) -> SparseAt {
    let wii = disc.get(0x18..0x1C) == Some(&[0x5D, 0x1C, 0x9E, 0xA3][..]);
    gcz_with(disc, stored, u32::from(wii), 2)
}

/// [`gcz`] with an explicit sub-type and block count (the table is laid
/// out for at most two blocks; a larger count is an adversarial header).
pub fn gcz_with(disc: &[u8], stored: bool, sub_type: u32, blocks: u32) -> SparseAt {
    use std::io::Write;
    const BLOCK: usize = 0x8000;
    let mut block = vec![0u8; BLOCK];
    block[..disc.len()].copy_from_slice(disc);
    let data = if stored {
        block
    } else {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&block).unwrap();
        z.finish().unwrap()
    };
    let flag = if stored { 1u64 << 63 } else { 0 };
    let mut h = Vec::new();
    h.extend(le32(0xB10B_C001));
    h.extend(le32(sub_type));
    h.extend((data.len() as u64 * 2).to_le_bytes());
    h.extend((BLOCK as u64 * 2).to_le_bytes());
    h.extend(le32(BLOCK as u32));
    h.extend(le32(blocks));
    h.extend(flag.to_le_bytes());
    h.extend((data.len() as u64 | flag).to_le_bytes());
    h.extend([0u8; 8]); // two hashes
    let start = h.len() as u64;
    let mut s = SparseAt::new(start + data.len() as u64 * 2);
    s.put(0, &h)
        .put(start, &data)
        .put(start + data.len() as u64, &data);
    s
}

/// A WBFS image (libwbfs `wbfs_head_t`): 512-byte sectors, one disc, its
/// header copy at 0x200.
pub fn wbfs(disc: &[u8]) -> SparseAt {
    wbfs_shift(disc, 9)
}

/// [`wbfs`] with another `hd_sec_sz_s`.
pub fn wbfs_shift(disc: &[u8], shift: u8) -> SparseAt {
    let mut head = Vec::from(*b"WBFS");
    head.extend(be32(0x0010_0000));
    head.extend([shift, 21, 0, 0, 1]);
    let mut s = SparseAt::new(0x1_0000_0000);
    s.put(0, &head);
    if shift < 32 {
        s.put(1u64 << shift, disc);
    }
    s
}

/// A WIA or RVZ image (Dolphin `docs/WiaAndRvz.md`): file head, then
/// `wia_disc_t` at 0x48 with `disc_type` and `dhead` (the disc header) at
/// 0x58.
pub fn wia(disc: &[u8], rvz: bool, disc_type: u32) -> SparseAt {
    let mut s = SparseAt::new(0x4000_0000);
    s.put(0, if rvz { b"RVZ\x01" } else { b"WIA\x01" })
        .put(4, &be32(0x0100_0000))
        .put(0x48, &be32(disc_type))
        .put(0x4C, &be32(5))
        .put(0x58, disc);
    s
}

/// A TGC (Dolphin `TGCBlob`): magic, `tgc_header_size` BE at 8 (0x8000),
/// the disc header right after it.
pub fn tgc(disc: &[u8]) -> SparseAt {
    let mut s = SparseAt::new(0x10_0000);
    s.put(0, &[0xAE, 0x0F, 0x38, 0xA2])
        .put(8, &be32(0x8000))
        .put(0x8000, disc);
    s
}

/// Where the sections of [`wii_wad`] are: `(ticket offset, ticket size,
/// TMD offset)`.
pub const WAD_TICKET: (u64, u64) = (0xA40, 0x2A4);
pub const WAD_TMD: u64 = 0xD00;

/// An installable Wii WAD (wiibrew «WAD files»): header `0x20`/`Is`,
/// certificate chain 0xA00, ticket 0x2A4, TMD 0x208 with title id `tid`,
/// sections 64-byte aligned from 0x40. The ticket is filled with `0xEE`
/// so a test can see it was never read.
pub fn wii_wad(tid: u64) -> SparseAt {
    let mut head = Vec::new();
    head.extend(be32(0x20));
    head.extend(*b"Is\0\0");
    head.extend(be32(0xA00)); // cert
    head.extend(be32(0)); // reserved
    head.extend(be32(WAD_TICKET.1 as u32));
    head.extend(be32(0x208)); // TMD
    head.extend(be32(0x10_0000)); // data
    head.extend(be32(0)); // footer
    let mut tmd = vec![0u8; 0x208];
    tmd[..4].copy_from_slice(&be32(0x0001_0001));
    tmd[0x140..0x14A].copy_from_slice(b"Root-CA000");
    tmd[0x18C..0x194].copy_from_slice(&tid.to_be_bytes());
    let mut s = SparseAt::new(0xF40 + 0x10_0000);
    s.put(0, &head)
        .put(WAD_TICKET.0, &vec![0xEE; WAD_TICKET.1 as usize])
        .put(WAD_TMD, &tmd);
    s
}

// ---------------------------------------------------------------------
// Nintendo 3DS (spec §8.2, 3dbrew). Synthetic headers only.
// ---------------------------------------------------------------------

/// A 3DS cartridge image is about this large.
const CTR_CART_LEN: u64 = 0x8000_0000;
/// Where an NCSD image's partition 0 starts (media unit 0x20).
pub const NCSD_P0: u64 = 0x4000;
/// ExeFS of a [`ncch_parts`] NoCrypto NCCH: offset in the NCCH and the
/// `icon` file's offset inside the ExeFS data.
const EXEFS_AT: u64 = 0x1400;
const EXEFS_ICON: u64 = 0x200;

fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// An SMDH (3dbrew «SMDH»): magic, the English short and long titles set
/// to `title`, zero icons.
pub fn smdh(title: &str) -> Vec<u8> {
    let mut s = vec![0u8; crate::consoles::ctr::SMDH_LEN];
    s[..4].copy_from_slice(b"SMDH");
    let t = utf16le(title);
    let short = 0x8 + 0x200;
    s[short..short + t.len().min(0x80)].copy_from_slice(&t[..t.len().min(0x80)]);
    let long = short + 0x80;
    s[long..long + t.len().min(0x100)].copy_from_slice(&t[..t.len().min(0x100)]);
    s
}

/// The parts of an NCCH, relative to its start: the 0x200-byte header
/// (`NCCH`, format version 2, program id `tid`, product code `code`,
/// `NoCrypto` when `no_crypto`) and, when `no_crypto`, an ExeFS whose
/// `icon` is [`smdh`]`("Test Game")`.
pub fn ncch_parts(code: &str, tid: u64, no_crypto: bool) -> Vec<(u64, Vec<u8>)> {
    let mut h = vec![0u8; 0x200];
    h[0x100..0x104].copy_from_slice(b"NCCH");
    h[0x104..0x108].copy_from_slice(&le32(0x10_0000));
    h[0x108..0x110].copy_from_slice(&tid.to_le_bytes());
    h[0x110..0x112].copy_from_slice(b"01");
    h[0x112..0x114].copy_from_slice(&2u16.to_le_bytes()); // format version
    h[0x118..0x120].copy_from_slice(&tid.to_le_bytes());
    let c = code.as_bytes();
    h[0x150..0x150 + c.len().min(16)].copy_from_slice(&c[..c.len().min(16)]);
    let mut parts = Vec::new();
    if no_crypto {
        h[0x188 + 7] = 0x04;
        h[0x1A0..0x1A4].copy_from_slice(&le32((EXEFS_AT / 0x200) as u32));
        h[0x1A4..0x1A8].copy_from_slice(&le32(0x30));
        let mut table = vec![0u8; 0x200];
        table[..5].copy_from_slice(b".code");
        table[12..16].copy_from_slice(&le32(0x200));
        table[0x10..0x14].copy_from_slice(b"icon");
        table[0x18..0x1C].copy_from_slice(&le32(EXEFS_ICON as u32));
        table[0x1C..0x20].copy_from_slice(&le32(0x36C0));
        parts.push((EXEFS_AT, table));
        parts.push((EXEFS_AT + 0x200 + EXEFS_ICON, smdh("Test Game")));
    }
    parts.insert(0, (0, h));
    parts
}

/// A bare NCCH (`.cxi`).
pub fn ncch(code: &str, tid: u64, no_crypto: bool) -> SparseAt {
    let mut s = SparseAt::new(0x10_0000);
    for (o, p) in ncch_parts(code, tid, no_crypto) {
        s.put(o, &p);
    }
    s
}

/// An NCSD cartridge image (`.3ds`, `.cci`): `NCSD` at 0x100, media id
/// `tid`, partition 0 at media unit 0x20 holding [`ncch_parts`].
pub fn ncsd(code: &str, tid: u64, no_crypto: bool) -> SparseAt {
    let mut head = vec![0u8; 0x200];
    head[0x100..0x104].copy_from_slice(b"NCSD");
    head[0x104..0x108].copy_from_slice(&le32((CTR_CART_LEN / 0x200) as u32));
    head[0x108..0x110].copy_from_slice(&tid.to_le_bytes());
    head[0x120..0x124].copy_from_slice(&le32((NCSD_P0 / 0x200) as u32));
    head[0x124..0x128].copy_from_slice(&le32(0x8000));
    let mut s = SparseAt::new(CTR_CART_LEN);
    s.put(0, &head);
    for (o, p) in ncch_parts(code, tid, no_crypto) {
        s.put(NCSD_P0 + o, &p);
    }
    s
}

/// Where the sections of [`cia`] are: the ticket `(offset, size)`, the
/// TMD and content 0.
pub const CIA_TICKET: (u64, u64) = (0x2A40, 0x350);
pub const CIA_TMD: u64 = 0x2DC0;
pub const CIA_CONTENT: u64 = 0x3900;

/// A CIA (3dbrew «CIA»): header 0x2020, certificate chain 0xA00, ticket
/// 0x350 (filled with `0xEE`), TMD with title id `tid`, version 1040 and
/// one content whose type is encrypted when `encrypted`, content 0 of
/// 1 MiB (an NCCH with product code `CTR-P-EKJA` when clear, `0xCD`
/// bytes when encrypted), and a meta section with [`smdh`]`("Test Game")`
/// when `with_meta`.
pub fn cia(tid: u64, encrypted: bool, with_meta: bool) -> SparseAt {
    const CONTENT: u64 = 0x10_0000;
    let meta_len: u32 = if with_meta { 0x400 + 0x36C0 } else { 0 };
    let mut head = Vec::new();
    head.extend(le32(0x2020));
    head.extend([0u8; 4]); // type, version
    head.extend(le32(0xA00));
    head.extend(le32(CIA_TICKET.1 as u32));
    head.extend(le32(0xB34));
    head.extend(le32(meta_len));
    head.extend(CONTENT.to_le_bytes());
    let mut tmd = vec![0u8; 0xB34];
    tmd[..4].copy_from_slice(&be32(0x0001_0004));
    tmd[0x140..0x14A].copy_from_slice(b"Root-CA000");
    tmd[0x18C..0x194].copy_from_slice(&tid.to_be_bytes());
    tmd[0x1DC..0x1DE].copy_from_slice(&1040u16.to_be_bytes());
    tmd[0x1DE..0x1E0].copy_from_slice(&1u16.to_be_bytes());
    tmd[0xB04 + 6..0xB04 + 8].copy_from_slice(&u16::from(encrypted).to_be_bytes());
    let meta_off = CIA_CONTENT + CONTENT;
    let mut s = SparseAt::new(meta_off + u64::from(meta_len));
    s.put(0, &head)
        .put(CIA_TICKET.0, &vec![0xEE; CIA_TICKET.1 as usize])
        .put(CIA_TMD, &tmd);
    if encrypted {
        s.put(CIA_CONTENT, &[0xCD; 0x200]);
    } else {
        for (o, p) in ncch_parts("CTR-P-EKJA", tid, false) {
            s.put(CIA_CONTENT + o, &p);
        }
    }
    if with_meta {
        s.put(meta_off + 0x400, &smdh("Test Game"));
    }
    s
}

/// A 3DSX (3dbrew «3DSX Format») with an extended header pointing at
/// [`smdh`]`(title)` at 0x100.
pub fn dsx_with_smdh(title: &str) -> SparseAt {
    let mut head = Vec::from(*b"3DSX");
    head.extend(0x2Cu16.to_le_bytes()); // header size
    head.extend(0x0Cu16.to_le_bytes()); // relocation header size
    head.extend([0u8; 0x18]); // version, flags, segment sizes
    head.extend(le32(0x100)); // SMDH offset
    head.extend(le32(0x36C0)); // SMDH size
    head.extend(le32(0)); // RomFS offset
    let mut s = SparseAt::new(0x100 + 0x36C0 + 0x1000);
    s.put(0, &head).put(0x100, &smdh(title));
    s
}

/// An Azahar `Z3DS` file (`file_derived.h`) around the first 128 KiB of
/// `inner`: the 0x20-byte header with `underlying`, 0x10 bytes of
/// metadata, then one zstd frame.
pub fn z3ds(underlying: &[u8; 4], inner: &SparseAt) -> SparseAt {
    let n = inner.len.min(0x2_0000) as usize;
    let mut plain = vec![0u8; n];
    let mut copy = inner.clone();
    copy.read_at(0, &mut plain).expect("in memory");
    let frame =
        ruzstd::encoding::compress_to_vec(&plain[..], ruzstd::encoding::CompressionLevel::Fastest);
    let mut h = Vec::from(*b"Z3DS");
    h.extend(underlying);
    h.extend([1, 0]);
    h.extend(0x20u16.to_le_bytes());
    h.extend(le32(0x10));
    h.extend((frame.len() as u64).to_le_bytes());
    h.extend(inner.len.to_le_bytes());
    let mut s = SparseAt::new(0x30 + frame.len() as u64);
    s.put(0, &h).put(0x30, &frame);
    s
}
