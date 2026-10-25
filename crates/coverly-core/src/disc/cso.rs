//! Compressed ISO (`.cso`, CISO v0/v1), the PSP's usual image format: a
//! 24-byte header (`CISO`, header size, total bytes, block size, version,
//! index alignment), then one `u32` per block plus one (bit 31 set: stored
//! plain; the rest: offset `<< align`), then the blocks, each plain or raw
//! deflate. Only the blocks a read reaches are inflated, at most
//! [`MAX_INFLATED`] bytes in all.

use std::io::{self, Read, Seek};

use super::blocks::{
    damaged, le_u32, read_exact_at, BlockIndex, BlockSource, Stored, MAX_INFLATED,
};
use super::source::{SectorLayout, SectorSource, USER_BYTES};

/// The CISO magic.
pub const CSO_MAGIC: &[u8; 4] = b"CISO";
/// Header bytes before the block index.
pub const HEADER: u64 = 0x18;
/// Smallest and largest block size read.
const BLOCK_SIZES: std::ops::RangeInclusive<u32> = 512..=(64 * 1024);

/// An opened `.cso` image, a sector source over its ISO.
pub struct Cso<R: Read + Seek> {
    src: BlockSource<CsoIndex<R>>,
    total: u64,
}

impl<R: Read + Seek> Cso<R> {
    /// `Ok(None)` when `reader` is not a CISO v0/v1 image.
    pub fn open(mut reader: R) -> io::Result<Option<Cso<R>>> {
        let Ok(head) = read_exact_at(&mut reader, 0, HEADER as usize) else {
            return Ok(None);
        };
        if &head[..4] != CSO_MAGIC || head[0x14] > 1 {
            return Ok(None);
        }
        let total = u64::from_le_bytes(head[8..16].try_into().unwrap_or([0; 8]));
        let block = le_u32(&head, 0x10);
        if !BLOCK_SIZES.contains(&block) || total == 0 {
            return Err(damaged(format!("CSO block size {block}")));
        }
        let index = CsoIndex {
            reader,
            block: block as usize,
            blocks: total.div_ceil(u64::from(block)),
            align: u32::from(head[0x15]).min(31),
        };
        Ok(Some(Cso {
            src: BlockSource::new(index, None, MAX_INFLATED)?,
            total,
        }))
    }

    /// The ISO's length, as the header states it.
    pub fn image_len(&self) -> u64 {
        self.total
    }

    /// Decoded bytes held so far.
    pub fn inflated(&self) -> usize {
        self.src.inflated()
    }

    /// The reader, back.
    pub fn into_inner(self) -> R {
        self.src.into_index().reader
    }
}

impl<R: Read + Seek> SectorSource for Cso<R> {
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

struct CsoIndex<R> {
    reader: R,
    block: usize,
    blocks: u64,
    align: u32,
}

impl<R: Read + Seek> BlockIndex for CsoIndex<R> {
    fn block_size(&self) -> usize {
        self.block
    }

    fn locate(&mut self, n: u64) -> io::Result<Option<Stored>> {
        if n >= self.blocks {
            return Ok(None);
        }
        let entry = read_exact_at(&mut self.reader, HEADER + 4 * n, 8)?;
        let (this, next) = (le_u32(&entry, 0), le_u32(&entry, 4));
        let pos = |v: u32| u64::from(v & 0x7FFF_FFFF) << self.align;
        let (start, end) = (pos(this), pos(next));
        let len = end
            .checked_sub(start)
            .ok_or_else(|| damaged(format!("CSO block {n} ends before it starts")))?;
        let plain = this & 0x8000_0000 != 0;
        Ok(Some(Stored {
            offset: start,
            // A plain block is the block itself; alignment may pad it.
            len: if plain {
                len.min(self.block as u64)
            } else {
                len
            },
            deflated: !plain,
        }))
    }

    fn read_stored(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        read_exact_at(&mut self.reader, offset, len)
    }
}
