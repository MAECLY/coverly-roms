//! A [`SectorSource`] over a block-compressed disc image (PBP `PSISO`,
//! CSO): the stored track is cut into blocks of a fixed decoded size, each
//! stored plain or as a raw deflate stream. Only the blocks a read reaches
//! are fetched, and at most [`MAX_INFLATED`] decoded bytes are ever held for
//! one image; past that cap a sector reads as out of range (never an
//! error), so identification answers from what it has.

use std::collections::HashMap;
use std::io::{self, Read};

use flate2::read::DeflateDecoder;

use super::source::{detect_layout, SectorLayout, SectorSource, DETECT_BYTES, USER_BYTES};

/// Most decoded bytes one image may hold (plan DD-3).
pub const MAX_INFLATED: usize = 1024 * 1024;

/// One stored block: where its bytes are and whether they are deflated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Stored {
    pub offset: u64,
    pub len: u64,
    pub deflated: bool,
}

/// The index of a block-compressed image.
pub(crate) trait BlockIndex {
    /// Decoded bytes per block.
    fn block_size(&self) -> usize;
    /// Where block `n` is stored; `None` past the last block.
    fn locate(&mut self, n: u64) -> io::Result<Option<Stored>>;
    /// `len` stored bytes at `offset`.
    fn read_stored(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>>;
}

/// The sector source over a [`BlockIndex`].
pub(crate) struct BlockSource<I> {
    index: I,
    layout: SectorLayout,
    blocks: HashMap<u64, Vec<u8>>,
    inflated: usize,
    /// Decoded bytes this source may hold.
    cap: usize,
}

impl<I: BlockIndex> BlockSource<I> {
    /// A source over `index`, its layout detected from the first stored
    /// sectors (or `layout` when given).
    /// At most `cap` decoded bytes are held (never more than
    /// [`MAX_INFLATED`]).
    pub(crate) fn new(index: I, layout: Option<SectorLayout>, cap: usize) -> io::Result<Self> {
        let mut src = BlockSource {
            index,
            layout: SectorLayout::Iso2048,
            blocks: HashMap::new(),
            inflated: 0,
            cap: cap.min(MAX_INFLATED),
        };
        src.layout = match layout {
            Some(l) => l,
            None => {
                let mut head = vec![0u8; DETECT_BYTES];
                let n = src.read_at(0, &mut head)?;
                head.truncate(n);
                detect_layout(&head)
            }
        };
        Ok(src)
    }

    /// The index, for its reader.
    pub(crate) fn into_index(self) -> I {
        self.index
    }

    /// Decoded bytes held so far.
    pub(crate) fn inflated(&self) -> usize {
        self.inflated
    }

    /// Decoded block `n`; `None` past the end or the cap.
    fn block(&mut self, n: u64) -> io::Result<Option<&[u8]>> {
        if !self.blocks.contains_key(&n) {
            let size = self.index.block_size();
            if self.inflated + size > self.cap {
                return Ok(None);
            }
            let Some(stored) = self.index.locate(n)? else {
                return Ok(None);
            };
            if stored.len > size as u64 {
                return Err(damaged(format!("block {n} stores more than it holds")));
            }
            let raw = self.index.read_stored(stored.offset, stored.len as usize)?;
            let bytes = if stored.deflated {
                let mut out = Vec::with_capacity(size);
                DeflateDecoder::new(&raw[..])
                    .take(size as u64 + 1)
                    .read_to_end(&mut out)
                    .map_err(|e| damaged(format!("block {n}: {e}")))?;
                if out.len() > size {
                    return Err(damaged(format!("block {n} inflates past its size")));
                }
                out
            } else {
                raw
            };
            self.inflated += size;
            self.blocks.insert(n, bytes);
        }
        Ok(self.blocks.get(&n).map(Vec::as_slice))
    }

    /// Fills `out` from stored byte `offset` of the track; the bytes read
    /// (fewer at the end, past the cap, or at a short block).
    fn read_at(&mut self, offset: u64, out: &mut [u8]) -> io::Result<usize> {
        let size = self.index.block_size() as u64;
        let mut done = 0;
        while done < out.len() {
            let at = offset + done as u64;
            let Some(block) = self.block(at / size)? else {
                break;
            };
            let skip = (at % size) as usize;
            let Some(avail) = block.get(skip..) else {
                break;
            };
            let n = avail.len().min(out.len() - done);
            if n == 0 {
                break;
            }
            out[done..done + n].copy_from_slice(&avail[..n]);
            done += n;
        }
        Ok(done)
    }
}

impl<I: BlockIndex> SectorSource for BlockSource<I> {
    fn layout(&self) -> SectorLayout {
        self.layout
    }

    fn read_user(&mut self, lba: u32, out: &mut [u8; USER_BYTES]) -> io::Result<bool> {
        let offset =
            u64::from(lba) * self.layout.sector_size() as u64 + self.layout.user_offset() as u64;
        Ok(self.read_at(offset, out)? == USER_BYTES)
    }

    fn raw_sector(&mut self, n: u32) -> io::Result<Option<Vec<u8>>> {
        let mut out = vec![0u8; self.layout.sector_size()];
        let offset = u64::from(n) * out.len() as u64;
        let full = self.read_at(offset, &mut out)? == out.len();
        Ok(full.then_some(out))
    }
}

pub(crate) fn damaged(detail: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, detail)
}

/// Reads exactly `len` bytes at `offset` of `r`.
pub(crate) fn read_exact_at<R: Read + io::Seek>(
    r: &mut R,
    offset: u64,
    len: usize,
) -> io::Result<Vec<u8>> {
    r.seek(io::SeekFrom::Start(offset))?;
    let mut out = vec![0u8; len];
    r.read_exact(&mut out)?;
    Ok(out)
}

pub(crate) fn le_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `count` plain blocks of `size` bytes, block `n` filled with `n`.
    struct Plain {
        size: usize,
        count: u64,
        reads: usize,
    }

    impl BlockIndex for Plain {
        fn block_size(&self) -> usize {
            self.size
        }

        fn locate(&mut self, n: u64) -> io::Result<Option<Stored>> {
            Ok((n < self.count).then_some(Stored {
                offset: n,
                len: self.size as u64,
                deflated: false,
            }))
        }

        fn read_stored(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
            self.reads += 1;
            Ok(vec![offset as u8; len])
        }
    }

    #[test]
    fn reads_stop_at_the_inflate_cap_without_an_error() {
        let index = Plain {
            size: 64 * 1024,
            count: 100,
            reads: 0,
        };
        let mut src = BlockSource::new(index, Some(SectorLayout::Iso2048), usize::MAX).unwrap();
        let per_block = (64 * 1024 / USER_BYTES) as u32;
        let mut out = [0u8; USER_BYTES];
        for n in 0..16 {
            assert!(src.read_user(n * per_block, &mut out).unwrap(), "block {n}");
            assert!(out.iter().all(|&b| b == n as u8));
        }
        // Block 16 would pass 1 MiB: out of range, not an error.
        assert!(!src.read_user(16 * per_block, &mut out).unwrap());
        assert_eq!(src.inflated(), MAX_INFLATED);
        // Blocks already held are served again without a read.
        assert!(src.read_user(3, &mut out).unwrap());
        assert_eq!(src.into_index().reads, 16);
    }

    #[test]
    fn a_block_that_stores_more_than_it_holds_is_damaged() {
        struct Big;
        impl BlockIndex for Big {
            fn block_size(&self) -> usize {
                2048
            }
            fn locate(&mut self, _n: u64) -> io::Result<Option<Stored>> {
                Ok(Some(Stored {
                    offset: 0,
                    len: 4096,
                    deflated: true,
                }))
            }
            fn read_stored(&mut self, _offset: u64, len: usize) -> io::Result<Vec<u8>> {
                Ok(vec![0; len])
            }
        }
        let mut src = BlockSource::new(Big, Some(SectorLayout::Iso2048), MAX_INFLATED).unwrap();
        let mut out = [0u8; USER_BYTES];
        let err = src.read_user(0, &mut out).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
