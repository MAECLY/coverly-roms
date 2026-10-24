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
        store(&user, self.raw)
    }
}

/// User sectors as stored: as they are, or as raw Mode 2 Form 1 sectors
/// (sync, mode byte 2, subheader, user data at +24).
fn store(user: &[[u8; USER_BYTES]], raw: bool) -> Vec<u8> {
    if !raw {
        return user.concat();
    }
    let mut out = Vec::with_capacity(user.len() * 2352);
    for data in user {
        let mut s = [0u8; 2352];
        s[..12].copy_from_slice(&SYNC);
        s[15] = 2;
        s[16..24].copy_from_slice(&[0, 0, 0x08, 0, 0, 0, 0x08, 0]);
        s[24..24 + USER_BYTES].copy_from_slice(data);
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
