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
    pub fn put(&mut self, offset: u64, bytes: &[u8]) -> &mut Self {
        self.parts.insert(offset, bytes.to_vec());
        self
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
