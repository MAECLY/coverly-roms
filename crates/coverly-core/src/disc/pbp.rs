//! PlayStation discs converted to PSP `EBOOT.PBP` (plan DD-3): the
//! `DATA.PSAR` member holds a `PSISOIMG0000` image (one disc) or a
//! `PSTITLEIMG0000` table of up to five of them. Each image is the raw
//! 2352-byte track in 16-sector blocks, indexed at `+0x4000` (32-byte
//! entries: offset, length) and stored from `+0x100000`; a block whose
//! length is a whole block is plain, any other is raw deflate.
//!
//! Only the blocks a read reaches are inflated, at most
//! [`MAX_INFLATED`] bytes for the whole file.
//! The outer `PARAM.SFO` and the converter's disc id in the image header are
//! never read: converters rewrite them, so the identity comes from the
//! disc's own `SYSTEM.CNF` (duplicates spec §5.4).

use std::io::{self, Read, Seek};

use super::blocks::{
    damaged, le_u32, read_exact_at, BlockIndex, BlockSource, Stored, MAX_INFLATED,
};
use super::source::{SectorLayout, SectorSource, USER_BYTES};

/// The `EBOOT.PBP` magic.
pub const PBP_MAGIC: &[u8; 4] = b"\0PBP";
/// A single-disc `DATA.PSAR`.
pub const PSISO: &[u8; 12] = b"PSISOIMG0000";
/// A multi-disc `DATA.PSAR`.
pub const PSTITLE: &[u8; 14] = b"PSTITLEIMG0000";
/// Decoded bytes per block: sixteen raw sectors.
pub const BLOCK: usize = 16 * 2352;
/// Where an image's block index starts, from the image header.
pub const INDEX_AT: u64 = 0x4000;
/// Where an image's blocks are stored, from the image header.
pub const DATA_AT: u64 = 0x10_0000;
/// Bytes per index entry.
pub const INDEX_ENTRY: u64 = 32;
/// Where a multi-disc table lists its images, from `DATA.PSAR`.
pub const DISC_TABLE_AT: u64 = 0x200;
/// Most discs a multi-disc `DATA.PSAR` lists.
pub const MAX_DISCS: usize = 5;

/// An opened PS1 `EBOOT.PBP`.
#[derive(Debug)]
pub struct Pbp<R> {
    reader: R,
    /// Absolute offsets of the `PSISOIMG0000` headers, in disc order.
    discs: Vec<u64>,
    /// Decoded bytes held so far, over every disc.
    spent: usize,
}

impl<R: Read + Seek> Pbp<R> {
    /// `Ok(None)` when `reader` is not a PBP holding a PlayStation disc (a
    /// PSP game's `EBOOT.PBP`, another file).
    pub fn open(mut reader: R) -> io::Result<Option<Pbp<R>>> {
        let Ok(head) = read_exact_at(&mut reader, 0, 0x28) else {
            return Ok(None);
        };
        if &head[..4] != PBP_MAGIC {
            return Ok(None);
        }
        let psar = u64::from(le_u32(&head, 0x24));
        let Ok(tag) = read_exact_at(&mut reader, psar, PSTITLE.len()) else {
            return Ok(None);
        };
        let discs = if tag.starts_with(PSISO) {
            vec![psar]
        } else if tag.starts_with(PSTITLE) {
            let table = read_exact_at(&mut reader, psar + DISC_TABLE_AT, 4 * MAX_DISCS)?;
            let mut discs = Vec::new();
            for k in 0..MAX_DISCS {
                let off = u64::from(le_u32(&table, 4 * k));
                if off == 0 {
                    break;
                }
                let at = psar + off;
                if !read_exact_at(&mut reader, at, PSISO.len())?.starts_with(PSISO) {
                    return Err(damaged(format!("disc {} of the PBP has no image", k + 1)));
                }
                discs.push(at);
            }
            discs
        } else {
            return Ok(None);
        };
        if discs.is_empty() {
            return Ok(None);
        }
        Ok(Some(Pbp {
            reader,
            discs,
            spent: 0,
        }))
    }

    /// Discs in the file.
    pub fn disc_count(&self) -> usize {
        self.discs.len()
    }

    /// Decoded bytes held so far, over every disc read.
    pub fn inflated(&self) -> usize {
        self.spent
    }

    /// The reader, back.
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// The raw track of disc `n` (0-based) as a sector source.
    pub fn disc(&mut self, n: usize) -> io::Result<PbpDisc<'_, R>> {
        let base = *self
            .discs
            .get(n)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no disc {n}")))?;
        let cap = MAX_INFLATED.saturating_sub(self.spent);
        let index = PsisoIndex {
            reader: &mut self.reader,
            base,
        };
        let src = BlockSource::new(index, Some(SectorLayout::Raw2352Mode2), cap)?;
        Ok(PbpDisc {
            src,
            spent: &mut self.spent,
        })
    }
}

/// One disc of a [`Pbp`]. What it inflates counts for the whole file.
pub struct PbpDisc<'a, R: Read + Seek> {
    src: BlockSource<PsisoIndex<'a, R>>,
    spent: &'a mut usize,
}

impl<R: Read + Seek> PbpDisc<'_, R> {
    /// Decoded bytes this disc holds.
    pub fn inflated(&self) -> usize {
        self.src.inflated()
    }
}

impl<R: Read + Seek> Drop for PbpDisc<'_, R> {
    fn drop(&mut self) {
        *self.spent += self.src.inflated();
    }
}

impl<R: Read + Seek> SectorSource for PbpDisc<'_, R> {
    fn layout(&self) -> SectorLayout {
        self.src.layout()
    }

    fn read_user(&mut self, lba: u32, out: &mut [u8; USER_BYTES]) -> io::Result<bool> {
        self.src.read_user(lba, out)
    }

    fn raw_sector(&mut self, n: u32) -> io::Result<Option<Vec<u8>>> {
        self.src.raw_sector(n)
    }
}

struct PsisoIndex<'a, R> {
    reader: &'a mut R,
    base: u64,
}

impl<R: Read + Seek> BlockIndex for PsisoIndex<'_, R> {
    fn block_size(&self) -> usize {
        BLOCK
    }

    fn locate(&mut self, n: u64) -> io::Result<Option<Stored>> {
        if n >= (DATA_AT - INDEX_AT) / INDEX_ENTRY {
            return Ok(None);
        }
        let at = self.base + INDEX_AT + n * INDEX_ENTRY;
        let Ok(entry) = read_exact_at(self.reader, at, 8) else {
            return Ok(None);
        };
        let (offset, len) = (le_u32(&entry, 0), le_u32(&entry, 4));
        if len == 0 {
            return Ok(None);
        }
        Ok(Some(Stored {
            offset: self.base + DATA_AT + u64::from(offset),
            len: u64::from(len),
            deflated: len as usize != BLOCK,
        }))
    }

    fn read_stored(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        read_exact_at(self.reader, offset, len)
    }
}
