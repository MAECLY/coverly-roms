//! Synthetic disc tracks for tests: only the bytes the readers look at,
//! everything else zero. No byte comes from a real disc.
//!
//! [`IsoBuilder`] writes an ISO 9660 track (system area, primary volume
//! descriptor at LBA 16, terminator, root directory at LBA 20, files after
//! it) as 2048-byte sectors or raw 2352-byte Mode 2 Form 1 sectors. Its
//! directory records come from the `identify::sniff` fixtures.

use crate::identify::sniff::fixtures::dir_record;

use super::source::USER_BYTES;

/// The licence text of a PlayStation disc's raw sector 4, with the uneven
/// spacing real discs have.
pub const PSX_LICENCE: &str =
    "          Licensed  by          Sony Computer Entertainment Amer  ica ";

/// Where the root directory starts.
const ROOT_LBA: u32 = 20;

/// Fewest sectors a built track has.
const MIN_SECTORS: u32 = 24;

const SYNC: [u8; 12] = [
    0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00,
];

enum Entry {
    File(String, Vec<u8>),
    Dir(String),
}

/// Builds a synthetic ISO 9660 data track.
pub struct IsoBuilder {
    system_id: String,
    raw: bool,
    mode1: bool,
    system_area: Vec<u8>,
    space_size: Option<u32>,
    created: [u8; 17],
    modified: [u8; 17],
    licence: Option<String>,
    fillers: usize,
    entries: Vec<Entry>,
}

impl IsoBuilder {
    /// A track whose PVD has system id `system_id`, 2048-byte sectors and
    /// an empty root directory.
    pub fn new(system_id: &str) -> IsoBuilder {
        IsoBuilder {
            system_id: system_id.to_owned(),
            raw: false,
            mode1: false,
            system_area: Vec::new(),
            space_size: None,
            created: *b"1998010100000000\0",
            modified: *b"1998010100000000\0",
            licence: None,
            fillers: 0,
            entries: Vec::new(),
        }
    }

    /// Raw 2352-byte Mode 2 Form 1 sectors instead of 2048-byte ones.
    pub fn raw(mut self, raw: bool) -> IsoBuilder {
        self.raw = raw;
        self
    }

    /// Raw 2352-byte Mode 1 sectors (Sega, NEC and 3DO data tracks).
    pub fn raw_mode1(mut self) -> IsoBuilder {
        self.raw = true;
        self.mode1 = true;
        self
    }

    /// Bytes written over the user data of the system area from sector 0
    /// (a Sega `IP.BIN`, an Opera header), at most 16 sectors.
    pub fn system_area(mut self, bytes: &[u8]) -> IsoBuilder {
        self.system_area = bytes[..bytes.len().min(16 * USER_BYTES)].to_vec();
        self
    }

    /// The PVD's volume space size (default: the sectors built).
    pub fn space_size(mut self, blocks: u32) -> IsoBuilder {
        self.space_size = Some(blocks);
        self
    }

    /// The PVD's creation and modification dates.
    pub fn dates(mut self, created: [u8; 17], modified: [u8; 17]) -> IsoBuilder {
        self.created = created;
        self.modified = modified;
        self
    }

    /// Text in the user data of sector 4 (the licence screen data).
    pub fn licence(mut self, text: &str) -> IsoBuilder {
        self.licence = Some(text.to_owned());
        self
    }

    /// `n` empty files (`F0000.TXT;1`…) in the root before the entries.
    pub fn filler(mut self, n: usize) -> IsoBuilder {
        self.fillers = n;
        self
    }

    /// A file in the root directory (`name` as stored, `SYSTEM.CNF;1`).
    pub fn file(mut self, name: &str, bytes: &[u8]) -> IsoBuilder {
        self.entries
            .push(Entry::File(name.to_owned(), bytes.to_vec()));
        self
    }

    /// An empty subdirectory in the root directory.
    pub fn dir(mut self, name: &str) -> IsoBuilder {
        self.entries.push(Entry::Dir(name.to_owned()));
        self
    }

    /// The track's stored bytes.
    pub fn build(self) -> Vec<u8> {
        // (name, data or None for a directory)
        let mut names: Vec<(Vec<u8>, Option<&[u8]>)> = Vec::new();
        for i in 0..self.fillers {
            names.push((format!("F{i:04}.TXT;1").into_bytes(), Some(&[][..])));
        }
        for entry in &self.entries {
            match entry {
                Entry::File(name, bytes) => names.push((name.as_bytes().to_vec(), Some(bytes))),
                Entry::Dir(name) => names.push((name.as_bytes().to_vec(), None)),
            }
        }
        let rec_len = |name: &[u8]| (33 + name.len()).next_multiple_of(2);
        let mut all: Vec<Vec<u8>> = vec![vec![0], vec![1]];
        all.extend(names.iter().map(|(n, _)| n.clone()));

        // Pack records into sectors: one never crosses a sector boundary.
        let mut placed = Vec::with_capacity(all.len()); // (sector, offset)
        let (mut sector, mut offset) = (0u32, 0usize);
        for name in &all {
            if offset + rec_len(name) > USER_BYTES {
                sector += 1;
                offset = 0;
            }
            placed.push((sector, offset));
            offset += rec_len(name);
        }
        let dir_sectors = sector + 1;

        // Extents after the directory: one sector per subdirectory, as many
        // as each file needs (at least one).
        let mut next = ROOT_LBA + dir_sectors;
        let mut extents = Vec::with_capacity(names.len());
        for (_, data) in &names {
            let blocks = data.map_or(1, |d| d.len().div_ceil(USER_BYTES).max(1)) as u32;
            extents.push(next);
            next += blocks;
        }
        let total = next.max(MIN_SECTORS);
        let mut user = vec![[0u8; USER_BYTES]; total as usize];

        for (k, chunk) in self.system_area.chunks(USER_BYTES).enumerate() {
            user[k][..chunk.len()].copy_from_slice(chunk);
        }
        if let Some(text) = &self.licence {
            user[4][..text.len()].copy_from_slice(text.as_bytes());
        }
        let pvd = &mut user[16];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[6] = 1;
        pvd[8..72].fill(b' ');
        pvd[8..8 + self.system_id.len()].copy_from_slice(self.system_id.as_bytes());
        pvd[40..49].copy_from_slice(b"SYNTHDISC");
        let space = self.space_size.unwrap_or(total);
        pvd[80..84].copy_from_slice(&space.to_le_bytes());
        pvd[84..88].copy_from_slice(&space.to_be_bytes());
        dir_record(
            &mut pvd[156..190],
            ROOT_LBA,
            dir_sectors * USER_BYTES as u32,
            b"\0",
        );
        pvd[156 + 25] = 0x02;
        pvd[813..830].copy_from_slice(&self.created);
        pvd[830..847].copy_from_slice(&self.modified);
        let end = &mut user[17];
        end[0] = 255;
        end[1..6].copy_from_slice(b"CD001");
        end[6] = 1;

        for (i, name) in all.iter().enumerate() {
            let (s, o) = placed[i];
            let (lba, size, is_dir) = match i {
                0 | 1 => (ROOT_LBA, dir_sectors * USER_BYTES as u32, true),
                _ => match names[i - 2].1 {
                    Some(data) => (extents[i - 2], data.len() as u32, false),
                    None => (extents[i - 2], USER_BYTES as u32, true),
                },
            };
            let rec = &mut user[(ROOT_LBA + s) as usize][o..];
            dir_record(rec, lba, size, name);
            if is_dir {
                rec[25] = 0x02;
            }
        }
        for (i, (_, data)) in names.iter().enumerate() {
            if let Some(data) = data {
                for (k, chunk) in data.chunks(USER_BYTES).enumerate() {
                    user[extents[i] as usize + k][..chunk.len()].copy_from_slice(chunk);
                }
            }
        }
        store(&user, self.raw, self.mode1)
    }
}

/// User sectors as stored: as they are, as raw Mode 2 Form 1 sectors
/// (sync, mode byte 2, subheader, user data at +24), or as raw Mode 1
/// sectors (sync, mode byte 1, user data at +16).
fn store(user: &[[u8; USER_BYTES]], raw: bool, mode1: bool) -> Vec<u8> {
    if !raw {
        return user.concat();
    }
    let mut out = Vec::with_capacity(user.len() * 2352);
    for data in user {
        let mut s = [0u8; 2352];
        s[..12].copy_from_slice(&SYNC);
        if mode1 {
            s[15] = 1;
            s[16..16 + USER_BYTES].copy_from_slice(data);
        } else {
            s[15] = 2;
            s[16..24].copy_from_slice(&[0, 0, 0x08, 0, 0, 0, 0x08, 0]);
            s[24..24 + USER_BYTES].copy_from_slice(data);
        }
        out.extend_from_slice(&s);
    }
    out
}

/// `SYSTEM.CNF` text that boots `boot` (`cdrom:\SLUS_008.62;1`).
fn psx_cnf(boot: &str) -> Vec<u8> {
    format!("BOOT = {boot}\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFFF0\r\n").into_bytes()
}

/// A PlayStation data track (track 01): `PLAYSTATION` PVD, the licence
/// text in sector 4 and a root `SYSTEM.CNF` whose `BOOT` is `boot`.
pub fn psx_track(raw: bool, boot: &str) -> Vec<u8> {
    psx_track_with(raw, boot, Some(PSX_LICENCE))
}

/// [`psx_track`] with the sector 4 text of choice (`None`: none).
pub fn psx_track_with(raw: bool, boot: &str, licence: Option<&str>) -> Vec<u8> {
    let mut b = IsoBuilder::new("PLAYSTATION")
        .raw(raw)
        .file("SYSTEM.CNF;1", &psx_cnf(boot));
    if let Some(text) = licence {
        b = b.licence(text);
    }
    b.build()
}

/// A PlayStation 2 DVD image head: `PLAYSTATION` PVD and a `SYSTEM.CNF`
/// that boots `SLUS_200.62` through `BOOT2`.
pub fn ps2_iso() -> Vec<u8> {
    IsoBuilder::new("PLAYSTATION")
        .file(
            "SYSTEM.CNF;1",
            b"BOOT2 = cdrom0:\\SLUS_200.62;1\r\nVER = 1.00\r\nVMODE = NTSC\r\n",
        )
        .build()
}

/// A Sega boot header of [`sega::IP_BIN_HEAD`](super::sega::IP_BIN_HEAD)
/// bytes: `magic` at 0, the rest spaces, then `fields` (offset, bytes)
/// written over it.
pub fn ip_bin(magic: &[u8], fields: &[(usize, &[u8])]) -> Vec<u8> {
    let mut h = vec![b' '; super::sega::IP_BIN_HEAD];
    h[..magic.len()].copy_from_slice(magic);
    for (at, bytes) in fields {
        h[*at..*at + bytes.len()].copy_from_slice(bytes);
    }
    h
}

/// A Saturn track 01 (raw Mode 1): `IP.BIN` with `product` at 0x20 and
/// `CD-1/1` at 0x38, over an ISO 9660 volume.
pub fn saturn_track(product: &str) -> Vec<u8> {
    let ip = ip_bin(
        b"SEGA SEGASATURN ",
        &[
            (0x10, b"SEGA TP T-57    "),
            (0x20, product.as_bytes()),
            (0x2A, b"V1.000"),
            (0x30, b"19961122"),
            (0x38, b"CD-1/1  "),
        ],
    );
    IsoBuilder::new("SEGA SEGASATURN")
        .raw_mode1()
        .system_area(&ip)
        .build()
}

/// A Dreamcast track 03 (the high-density data track, raw Mode 1):
/// `IP.BIN` with `GD-ROM<disc>` in the device field and `product` at 0x40.
pub fn dreamcast_track03(product: &str, disc: &str) -> Vec<u8> {
    let device = format!("8B4E GD-ROM{disc}");
    let ip = ip_bin(
        b"SEGA SEGAKATANA ",
        &[
            (0x10, b"SEGA ENTERPRISES"),
            (0x20, device.as_bytes()),
            (0x30, b"JUE     "),
            (0x40, product.as_bytes()),
            (0x4A, b"V1.000"),
        ],
    );
    IsoBuilder::new("SEGA SEGAKATANA")
        .raw_mode1()
        .system_area(&ip)
        .build()
}

/// A Dreamcast track 01 (the low-density area): a plain ISO 9660 track
/// with a readme and no `IP.BIN`.
pub fn dreamcast_track01() -> Vec<u8> {
    IsoBuilder::new("SEGA SEGAKATANA")
        .raw_mode1()
        .file("README.TXT;1", b"This is a Dreamcast GD-ROM.\r\n")
        .build()
}

/// A Sega CD track 01 (raw Mode 1): `SEGADISCSYSTEM`, system type
/// `SEGA MEGA DRIVE` at 0x100 and serial field `GM MK-4407 -00` at 0x180.
pub fn segacd_track() -> Vec<u8> {
    let ip = ip_bin(
        b"SEGADISCSYSTEM  ",
        &[
            (0x10, b"SEGAOS     "),
            (0x100, b"SEGA MEGA DRIVE "),
            (0x110, b"(C)SEGA 1993.JUL"),
            (0x180, b"GM MK-4407 -00"),
        ],
    );
    IsoBuilder::new("SEGA SEGASYSTEM")
        .raw_mode1()
        .system_area(&ip)
        .build()
}

/// A PC Engine CD data track (track 02, raw Mode 1): the boot text at 0x20
/// of sector 1, no ISO 9660 volume.
pub fn pce_cd_track() -> Vec<u8> {
    let mut sector1 = vec![0u8; USER_BYTES];
    sector1[0x20..0x20 + 23].copy_from_slice(b"PC Engine CD-ROM SYSTEM");
    sector1[0x38..0x38 + 16].copy_from_slice(b"Copyright HUDSON");
    let mut area = vec![0u8; USER_BYTES];
    area.extend(sector1);
    let user: Vec<[u8; USER_BYTES]> = (0..MIN_SECTORS as usize)
        .map(|n| {
            let mut s = [0u8; USER_BYTES];
            if let Some(chunk) = area.chunks(USER_BYTES).nth(n) {
                s.copy_from_slice(chunk);
            }
            s
        })
        .collect();
    store(&user, true, true)
}

/// A Neo Geo CD track 01 (2048-byte sectors): ISO 9660 with `IPL.TXT` in
/// the root and no `SYSTEM.CNF`.
pub fn neocd_iso() -> Vec<u8> {
    IsoBuilder::new("")
        .file("ABS.TXT;1", b"")
        .file("IPL.TXT;1", b"PROG.PRG,0,00000000\r\n")
        .file("PROG.PRG;1", &[0x4E, 0x75])
        .build()
}

/// A 3DO track 01: the Opera volume header (`01 5A×5 01`) at sector 0,
/// as 2048-byte sectors or raw Mode 1.
pub fn opera_3do(raw: bool) -> Vec<u8> {
    let mut header = vec![0x01, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x01, 0x00];
    header.resize(0x28, 0);
    header.extend_from_slice(b"CD-ROM");
    let b = IsoBuilder::new("").system_area(&header);
    if raw {
        b.raw_mode1().build()
    } else {
        b.build()
    }
}

/// A raw Mode 1 track (no ISO 9660 volume) whose user sectors are zero
/// but for `sectors` (sector number, offset, bytes).
fn raw_mode1_track(sectors: &[(usize, usize, &[u8])]) -> Vec<u8> {
    let mut user = vec![[0u8; USER_BYTES]; MIN_SECTORS as usize];
    for &(n, at, bytes) in sectors {
        user[n][at..at + bytes.len()].copy_from_slice(bytes);
    }
    store(&user, true, true)
}

/// A PC-FX data track (raw Mode 1): `PC-FX:Hu_CD-ROM` at byte 0 of sector
/// 0, a title in sector 1, and, when `pce_text`, the PC Engine CD boot
/// text at 0x20 of sector 1 as well (some PC-FX discs carry both).
pub fn pcfx_track(pce_text: bool) -> Vec<u8> {
    let mut sectors: Vec<(usize, usize, &[u8])> =
        vec![(0, 0, b"PC-FX:Hu_CD-ROM "), (1, 0, b"SYNTHETIC PC-FX GAME")];
    if pce_text {
        sectors.push((1, 0x20, b"PC Engine CD-ROM SYSTEM"));
    }
    raw_mode1_track(&sectors)
}

/// A CD-i track 1 (raw Mode 2 Form 1): the Green Book disc label at
/// sector 16 (record type `record_type`, `CD-I `, version 1, system id
/// `CD-RTOS`), no ISO 9660 volume.
pub fn cdi_track(record_type: u8) -> Vec<u8> {
    let mut user = vec![[0u8; USER_BYTES]; MIN_SECTORS as usize];
    let label = &mut user[16];
    label[0] = record_type;
    label[1..6].copy_from_slice(b"CD-I ");
    label[6] = 1;
    label[8..40].fill(b' ');
    label[8..15].copy_from_slice(b"CD-RTOS");
    label[40..72].fill(b' ');
    label[40..49].copy_from_slice(b"SYNTHDISC");
    let end = &mut user[17];
    end[0] = 255;
    end[1..6].copy_from_slice(b"CD-I ");
    end[6] = 1;
    store(&user, true, false)
}

/// The Amiga CD trademark an [`amiga_cd`] image carries.
#[derive(Clone, Copy, Debug)]
pub enum AmigaTm {
    /// A `CD32.TM;1` file right after the root (LBA 21) whose block has
    /// the CRC-32 WinUAE checks; its bytes are synthetic.
    Cd32Block,
    /// A `CDTV.TM;1` file in the root directory.
    CdtvFile,
    /// No trademark.
    None,
}

/// An Amiga CD track 01 (2048-byte sectors): an ISO 9660 volume with
/// system id `system_id` and the trademark `tm`.
pub fn amiga_cd(system_id: &str, tm: AmigaTm) -> Vec<u8> {
    let b = IsoBuilder::new(system_id);
    let b = match tm {
        AmigaTm::Cd32Block => {
            let mut block = vec![0u8; USER_BYTES];
            block[..24].copy_from_slice(b"SYNTHETIC TRADEMARK DATA");
            forge_crc32(&mut block, super::signatures::CD32_TM_CRC);
            b.file("CD32.TM;1", &block)
        }
        AmigaTm::CdtvFile => b.file("CDTV.TM;1", b"SYNTHETIC TRADEMARK DATA"),
        AmigaTm::None => b,
    };
    b.file("GAME.EXE;1", &[0x00, 0x00, 0x03, 0xF3]).build()
}

/// Sets the last four bytes of `bytes` so its CRC-32 is `target`.
fn forge_crc32(bytes: &mut [u8], target: u32) {
    let table: Vec<u32> = (0..256u32)
        .map(|n| {
            (0..8).fold(n, |c, _| {
                if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                }
            })
        })
        .collect();
    let n = bytes.len() - 4;
    // The register after the prefix, then walked back from the target.
    let state = !crc32fast::hash(&bytes[..n]);
    let mut reg = !target;
    for _ in 0..4 {
        let i = (0..256)
            .find(|&i| table[i] >> 24 == reg >> 24)
            .expect("a CRC table row");
        reg = ((reg ^ table[i]) << 8) | i as u32;
    }
    bytes[n..].copy_from_slice(&(reg ^ state).to_le_bytes());
    debug_assert_eq!(crc32fast::hash(bytes), target);
}

/// The first raw sector of a Jaguar CD's session-2 boot track (an audio
/// track): two bytes, 64 bytes of `ATRI`, the 32-byte header, a load
/// address and a size, big-endian; every 16-bit word swapped when
/// `byteswapped` (as many rips store it).
pub fn jaguar_cd_session2(byteswapped: bool) -> Vec<u8> {
    let mut s = vec![0u8; 2352];
    for k in 0..16 {
        s[2 + 4 * k..6 + 4 * k].copy_from_slice(b"ATRI");
    }
    s[66..98].copy_from_slice(b"ATARI APPROVED DATA HEADER ATRI ");
    s[98..102].copy_from_slice(&0x0000_4000u32.to_be_bytes());
    s[102..106].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    if byteswapped {
        for pair in s.as_chunks_mut::<2>().0 {
            pair.swap(0, 1);
        }
    }
    s
}

/// Raw deflate of `bytes` (what PBP and CSO blocks store).
fn deflate_raw(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(bytes).expect("deflate in memory");
    enc.finish().expect("deflate in memory")
}

/// One `PSISOIMG0000` image of a raw track: a converter disc id in the
/// header (never read), the block index, then the blocks, deflated when
/// that is smaller.
fn psiso_image(track: &[u8], converter_id: &str) -> Vec<u8> {
    use super::pbp::{BLOCK, DATA_AT, INDEX_AT, INDEX_ENTRY, PSISO};
    let mut img = vec![0u8; DATA_AT as usize];
    img[..PSISO.len()].copy_from_slice(PSISO);
    img[0x400..0x400 + converter_id.len()].copy_from_slice(converter_id.as_bytes());
    let mut data = Vec::new();
    for (k, chunk) in track.chunks(BLOCK).enumerate() {
        let mut block = chunk.to_vec();
        block.resize(BLOCK, 0);
        let packed = deflate_raw(&block);
        let stored = if packed.len() < BLOCK { packed } else { block };
        let at = (INDEX_AT + k as u64 * INDEX_ENTRY) as usize;
        img[at..at + 4].copy_from_slice(&(data.len() as u32).to_le_bytes());
        img[at + 4..at + 8].copy_from_slice(&(stored.len() as u32).to_le_bytes());
        data.extend_from_slice(&stored);
    }
    img.extend_from_slice(&data);
    img
}

/// A PS1 `EBOOT.PBP` over the raw tracks `discs` (one: `PSISOIMG0000`;
/// more: a `PSTITLEIMG0000` table). Its outer `PARAM.SFO` says `DISC_ID`
/// `SLUS99999` and each image header `_SLUS_99999`: a converter's ids, which
/// the identity must never come from.
pub fn pbp_with(discs: &[&[u8]]) -> Vec<u8> {
    use super::pbp::{DISC_TABLE_AT, PBP_MAGIC, PSTITLE};
    let sfo = b"\0PSF\x01\x01\0\0DISC_ID\0SLUS99999\0TITLE\0Converted Game\0".to_vec();
    let sfo_at = 0x28u32;
    let psar_at = (sfo_at as usize + sfo.len()).next_multiple_of(16) as u32;
    let psar = match discs {
        [one] => psiso_image(one, "_SLUS_99999"),
        many => {
            let mut psar = vec![0u8; 0x8000];
            psar[..PSTITLE.len()].copy_from_slice(PSTITLE);
            for (k, disc) in many.iter().enumerate() {
                let off = psar.len() as u32;
                let at = DISC_TABLE_AT as usize + 4 * k;
                psar[at..at + 4].copy_from_slice(&off.to_le_bytes());
                psar.extend(psiso_image(disc, "_SLUS_99999"));
                psar.resize(psar.len().next_multiple_of(0x8000), 0);
            }
            psar
        }
    };
    let mut out = Vec::new();
    out.extend_from_slice(PBP_MAGIC);
    out.extend_from_slice(&0x0001_0000u32.to_le_bytes());
    // PARAM.SFO, ICON0, ICON1, PIC0, PIC1, SND0, DATA.PSP, DATA.PSAR.
    out.extend_from_slice(&sfo_at.to_le_bytes());
    for _ in 0..6 {
        out.extend_from_slice(&psar_at.to_le_bytes());
    }
    out.extend_from_slice(&psar_at.to_le_bytes());
    out.extend_from_slice(&sfo);
    out.resize(psar_at as usize, 0);
    out.extend_from_slice(&psar);
    out
}

/// [`pbp_with`] over one raw track.
pub fn pbp_of(track: &[u8]) -> Vec<u8> {
    pbp_with(&[track])
}

/// A CISO v1 image of `iso` (2048-byte blocks, no alignment), each block
/// deflated when that is smaller, else stored plain.
pub fn cso_of(iso: &[u8]) -> Vec<u8> {
    use super::cso::{CSO_MAGIC, HEADER};
    let block = 2048usize;
    let blocks = iso.len().div_ceil(block);
    let mut out = Vec::new();
    out.extend_from_slice(CSO_MAGIC);
    out.extend_from_slice(&(HEADER as u32).to_le_bytes());
    out.extend_from_slice(&(iso.len() as u64).to_le_bytes());
    out.extend_from_slice(&(block as u32).to_le_bytes());
    out.push(1);
    out.push(0);
    out.extend_from_slice(&[0, 0]);
    let index_at = out.len();
    out.resize(index_at + 4 * (blocks + 1), 0);
    let mut entries = Vec::with_capacity(blocks + 1);
    for chunk in iso.chunks(block) {
        let packed = deflate_raw(chunk);
        let pos = out.len() as u32;
        if packed.len() < chunk.len() {
            entries.push(pos);
            out.extend_from_slice(&packed);
        } else {
            entries.push(pos | 0x8000_0000);
            out.extend_from_slice(chunk);
        }
    }
    entries.push(out.len() as u32);
    for (k, e) in entries.iter().enumerate() {
        out[index_at + 4 * k..index_at + 4 * k + 4].copy_from_slice(&e.to_le_bytes());
    }
    out
}
