//! ZArchive, Cemu's `.wua` and Xenia's `.zar` (spec §8.2, §8.5).
//!
//! Layout read from Exzap/ZArchive (MIT-0) at master, 2026-10-31:
//! `include/zarchive/zarchivecommon.h` (structures) and
//! `src/zarchivewriter.cpp` (how they are written); no code copied. All
//! integers are big-endian.
//!
//! - The file ends in a 0x90-byte footer: six `{u64 offset, u64 size}`
//!   sections (compressed data, offset records, names, file tree, meta
//!   directory, meta data), a 32-byte SHA-256, `u64 total_size` (the file
//!   length), `u32 version` `0x61BF3A01` and `u32 magic` `0x169F52D6`.
//! - Only file data is compressed: the files are concatenated into one
//!   stream cut into 64 KiB blocks, each zstd-compressed on its own (a
//!   block whose stored size is exactly 64 KiB is stored as is). An
//!   offset record (`u64 base` + 16 × `u16 size − 1`) locates 16 blocks.
//! - Names: a length byte (bit 7 set: a second byte holds bits 7–14 of
//!   the length, as the writer stores it), then the bytes.
//! - File tree: 16-byte entries, breadth first, entry 0 the unnamed root.
//!   `u32 name offset` (bit 31 set = file), then for a file `u32 offset
//!   low`, `u32 size low`, `u32` with the size's (high 16) and offset's
//!   (low 16) bits 32–47; for a directory `u32 first child`, `u32 count`.
//!   Children are sorted by name, case-insensitively.
//!
//! Only the footer, the names, the file tree, one offset record and the
//! blocks of one small file are read (D24); nothing else is decompressed.

use std::io::Read;

use super::{Budgeted, MAX_DECODED};

/// The footer at the end of the file.
pub const FOOTER_LEN: u64 = 0x90;
const MAGIC: u32 = 0x169F_52D6;
const VERSION1: u32 = 0x61BF_3A01;
/// Uncompressed size of one block.
pub const BLOCK: u64 = 64 << 10;
/// One offset record: base offset and 16 sizes.
const RECORD_LEN: u64 = 8 + 2 * 16;
const PER_RECORD: u64 = 16;
/// Largest names or file-tree section read (D24; a Wii U title with its
/// update and DLC has about 20 KiB of each).
pub const MAX_SECTION: u64 = 256 << 10;
/// Most bytes decoded for one file read through [`Archive::read_file`].
pub const MAX_FILE_DECODED: u64 = 256 << 10;
const ENTRY_LEN: usize = 16;
const IS_FILE: u32 = 0x8000_0000;
const NO_NAME: u32 = 0x7FFF_FFFF;
/// Largest zstd window accepted (the writer compresses 64 KiB blocks).
const MAX_WINDOW: u64 = 1 << 20;

/// One `{offset, size}` section of the footer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    pub offset: u64,
    pub size: u64,
}

impl Section {
    fn end(self) -> Option<u64> {
        self.offset.checked_add(self.size)
    }
}

/// The parts of the footer this reader uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Footer {
    pub data: Section,
    pub records: Section,
    pub names: Section,
    pub tree: Section,
}

fn be16(h: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes(h.get(o..o + 2)?.try_into().ok()?))
}

fn be32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(h.get(o..o + 4)?.try_into().ok()?))
}

fn be64(h: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_be_bytes(h.get(o..o + 8)?.try_into().ok()?))
}

/// Whether `tail` (the last bytes of a file) ends with a ZArchive
/// footer's version and magic.
pub fn ends_with_footer(tail: &[u8]) -> bool {
    let n = tail.len();
    n as u64 >= FOOTER_LEN
        && be32(tail, n - 8) == Some(VERSION1)
        && be32(tail, n - 4) == Some(MAGIC)
}

/// What the end of the file says: `None` without the magic and version,
/// `Some(None)` when they are there but the footer does not check out
/// (total size, a section past the end), `Some(Some(footer))` otherwise.
pub fn footer(b: &mut Budgeted) -> Option<Option<Footer>> {
    let len = b.len();
    if len <= FOOTER_LEN {
        return None;
    }
    let f = b.read(len - FOOTER_LEN, FOOTER_LEN as usize).ok()?;
    if be32(&f, 0x8C)? != MAGIC || be32(&f, 0x88)? != VERSION1 {
        return None;
    }
    let section = |i: usize| -> Option<Section> {
        Some(Section {
            offset: be64(&f, i * 16)?,
            size: be64(&f, i * 16 + 8)?,
        })
    };
    let parsed = (|| {
        if be64(&f, 0x80)? != len {
            return None;
        }
        let all: Vec<Section> = (0..6).map(section).collect::<Option<_>>()?;
        if all.iter().any(|s| s.end().is_none_or(|e| e > len)) {
            return None;
        }
        Some(Footer {
            data: all[0],
            records: all[1],
            names: all[2],
            tree: all[3],
        })
    })();
    Some(parsed)
}

/// One file-tree entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Node {
    Dir { start: u32, count: u32 },
    File { offset: u64, size: u64 },
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    name: u32,
    node: Node,
}

/// The names and the file tree of an archive (no file data).
pub struct Archive {
    footer: Footer,
    names: Vec<u8>,
    tree: Vec<Entry>,
}

/// Reads the names and the file tree; `None` when either is over
/// [`MAX_SECTION`] or the root entry is not an unnamed directory.
pub fn open(b: &mut Budgeted, footer: Footer) -> Option<Archive> {
    if footer.names.size > MAX_SECTION || footer.tree.size > MAX_SECTION {
        return None;
    }
    let names = b
        .read(
            footer.names.offset,
            usize::try_from(footer.names.size).ok()?,
        )
        .ok()?;
    let raw = b
        .read(footer.tree.offset, usize::try_from(footer.tree.size).ok()?)
        .ok()?;
    let tree: Vec<Entry> = raw
        .as_chunks::<ENTRY_LEN>()
        .0
        .iter()
        .map(|e| {
            let word = be32(e, 0)?;
            let (a, b2, c) = (be32(e, 4)?, be32(e, 8)?, be32(e, 12)?);
            let node = if word & IS_FILE != 0 {
                Node::File {
                    offset: u64::from(a) | (u64::from(c & 0xFFFF) << 32),
                    size: u64::from(b2) | (u64::from(c >> 16) << 32),
                }
            } else {
                Node::Dir {
                    start: a,
                    count: b2,
                }
            };
            Some(Entry {
                name: word & !IS_FILE,
                node,
            })
        })
        .collect::<Option<_>>()?;
    let root = tree.first()?;
    if !matches!(root.node, Node::Dir { .. }) || root.name != NO_NAME {
        return None;
    }
    Some(Archive {
        footer,
        names,
        tree,
    })
}

impl Archive {
    /// The root directory's index.
    pub const ROOT: usize = 0;

    /// The name of entry `i` (`None` for the root or a bad offset).
    pub fn name(&self, i: usize) -> Option<&str> {
        let off = usize::try_from(self.tree.get(i)?.name).ok()?;
        let first = *self.names.get(off)?;
        let (len, start) = if first & 0x80 != 0 {
            let second = *self.names.get(off + 1)?;
            (
                usize::from(first & 0x7F) | (usize::from(second) << 7),
                off + 2,
            )
        } else {
            (usize::from(first), off + 1)
        };
        std::str::from_utf8(self.names.get(start..start.checked_add(len)?)?).ok()
    }

    pub fn node(&self, i: usize) -> Option<Node> {
        Some(self.tree.get(i)?.node)
    }

    /// The children of directory `i`, empty for a file or a range past
    /// the tree (never an entry at or before `i`: the tree is breadth
    /// first, so a child always comes after its parent).
    pub fn children(&self, i: usize) -> std::ops::Range<usize> {
        let Some(Node::Dir { start, count }) = self.node(i) else {
            return 0..0;
        };
        let (Ok(start), Ok(count)) = (usize::try_from(start), usize::try_from(count)) else {
            return 0..0;
        };
        match start.checked_add(count) {
            Some(end) if start > i && end <= self.tree.len() => start..end,
            _ => 0..0,
        }
    }

    /// The child of `dir` named `name`, ignoring ASCII case.
    pub fn child(&self, dir: usize, name: &str) -> Option<usize> {
        self.children(dir)
            .find(|&c| self.name(c).is_some_and(|n| n.eq_ignore_ascii_case(name)))
    }

    /// The entry at `path` below `dir`.
    pub fn lookup(&self, dir: usize, path: &[&str]) -> Option<usize> {
        path.iter().try_fold(dir, |at, part| self.child(at, part))
    }

    /// The bytes of file `i` when it is at most `max` bytes, decoding only
    /// its blocks (at most [`MAX_FILE_DECODED`] bytes for this file, and
    /// within the item's [`MAX_DECODED`]).
    pub fn read_file(&self, b: &mut Budgeted, i: usize, max: u64) -> Option<Vec<u8>> {
        let Node::File { offset, size } = self.node(i)? else {
            return None;
        };
        if size == 0 || size > max {
            return None;
        }
        self.read_range(b, offset, size)
    }

    /// The first `n` bytes of file `i` (all of it when shorter), decoding
    /// only the blocks they cover, with the same caps as [`read_file`]
    /// (an Xbox 360 `default.xex` header, spec §8.5).
    ///
    /// [`read_file`]: Archive::read_file
    pub fn read_head(&self, b: &mut Budgeted, i: usize, n: u64) -> Option<Vec<u8>> {
        let Node::File { offset, size } = self.node(i)? else {
            return None;
        };
        if size == 0 || n == 0 {
            return None;
        }
        self.read_range(b, offset, size.min(n))
    }

    /// `size` bytes of the data stream at `offset`.
    fn read_range(&self, b: &mut Budgeted, offset: u64, size: u64) -> Option<Vec<u8>> {
        let end = offset.checked_add(size)?;
        let first = offset / BLOCK;
        let last = (end - 1) / BLOCK;
        let blocks = last - first + 1;
        if blocks.checked_mul(BLOCK)? > MAX_FILE_DECODED.min(MAX_DECODED) {
            return None;
        }
        let mut data = Vec::new();
        for block in first..=last {
            data.extend(self.block(b, block)?);
        }
        let from = usize::try_from(offset - first * BLOCK).ok()?;
        let to = from.checked_add(usize::try_from(size).ok()?)?;
        data.get(from..to).map(<[u8]>::to_vec)
    }

    /// Block `index`, decoded (64 KiB).
    fn block(&self, b: &mut Budgeted, index: u64) -> Option<Vec<u8>> {
        let record = index / PER_RECORD;
        let sub = usize::try_from(index % PER_RECORD).ok()?;
        let at = record.checked_mul(RECORD_LEN)?;
        if at.checked_add(RECORD_LEN)? > self.footer.records.size {
            return None;
        }
        let rec = b
            .read(
                self.footer.records.offset.checked_add(at)?,
                RECORD_LEN as usize,
            )
            .ok()?;
        let mut offset = be64(&rec, 0)?;
        for k in 0..sub {
            offset = offset.checked_add(u64::from(be16(&rec, 8 + 2 * k)?) + 1)?;
        }
        let stored = u64::from(be16(&rec, 8 + 2 * sub)?) + 1;
        if offset.checked_add(stored)? > self.footer.data.size {
            return None;
        }
        let raw = b
            .read(
                self.footer.data.offset.checked_add(offset)?,
                usize::try_from(stored).ok()?,
            )
            .ok()?;
        b.decoded(BLOCK).ok()?;
        if stored == BLOCK {
            return Some(raw);
        }
        use ruzstd::decoding::{FrameDecoder, StreamingDecoder};
        let mut frame = FrameDecoder::new();
        frame.set_max_window_size(MAX_WINDOW);
        let dec = StreamingDecoder::new_with_decoder(&raw[..], frame).ok()?;
        let mut out = Vec::with_capacity(BLOCK as usize);
        dec.take(BLOCK + 1).read_to_end(&mut out).ok()?;
        (out.len() as u64 == BLOCK).then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{self as t, SparseAt};
    use super::*;

    fn budget(s: &mut SparseAt) -> Budgeted<'_> {
        let len = s.len;
        Budgeted::new(s, len)
    }

    #[test]
    fn footer_names_tree_and_one_file() {
        let big: Vec<u8> = (0..70_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let mut s = t::zarchive(&[
            ("a/one.txt", b"hello".as_slice()),
            ("a/big.bin", &big),
            ("B/two.txt", b"world"),
        ]);
        let mut b = budget(&mut s);
        let f = footer(&mut b).unwrap().unwrap();
        let a = open(&mut b, f).unwrap();
        let root: Vec<&str> = a
            .children(Archive::ROOT)
            .filter_map(|i| a.name(i))
            .collect();
        assert_eq!(root, ["a", "B"]);
        let one = a.lookup(Archive::ROOT, &["A", "ONE.TXT"]).unwrap();
        assert_eq!(a.read_file(&mut b, one, 1024).unwrap(), b"hello");
        let two = a.lookup(Archive::ROOT, &["b", "two.txt"]).unwrap();
        assert_eq!(a.read_file(&mut b, two, 1024).unwrap(), b"world");
        // A file across two blocks.
        let i = a.lookup(Archive::ROOT, &["a", "big.bin"]).unwrap();
        assert_eq!(a.read_file(&mut b, i, 1 << 20).unwrap(), big);
        assert_eq!(a.read_file(&mut b, i, 1024), None, "over the caller's max");
        assert!(b.used().decoded <= MAX_DECODED);
    }

    #[test]
    fn not_a_zarchive() {
        let z = t::zarchive(&[("x.bin", b"x".as_slice())]).bytes();
        assert!(ends_with_footer(&z));
        assert!(!ends_with_footer(&z[..z.len() - 1]));
        let mut s = SparseAt::new(0x1000);
        assert_eq!(footer(&mut budget(&mut s)), None);
        let mut tiny = SparseAt::new(0x10);
        assert_eq!(footer(&mut budget(&mut tiny)), None);
    }

    #[test]
    fn sections_past_the_end_are_magic_only() {
        let mut s = t::zarchive(&[("x.bin", b"x".as_slice())]);
        let tail = s.len - FOOTER_LEN;
        // Names section offset far past the end of the file.
        s.put(tail + 0x20, &u64::MAX.to_be_bytes());
        assert_eq!(footer(&mut budget(&mut s)), Some(None));
        let mut s = t::zarchive(&[("x.bin", b"x".as_slice())]);
        // Total size disagrees with the file length.
        s.put(tail + 0x80, &1u64.to_be_bytes());
        assert_eq!(footer(&mut budget(&mut s)), Some(None));
    }

    #[test]
    fn oversized_sections_and_bad_trees_are_refused() {
        let mut s = t::zarchive(&[("x.bin", b"x".as_slice())]);
        let mut b = budget(&mut s);
        let mut f = footer(&mut b).unwrap().unwrap();
        f.tree.size = MAX_SECTION + 16;
        assert!(open(&mut b, f).is_none());
        // A directory whose children point back at itself.
        let mut s = t::zarchive(&[("x.bin", b"x".as_slice())]);
        let f = footer(&mut budget(&mut s)).unwrap().unwrap();
        s.put(f.tree.offset + 4, &0u32.to_be_bytes());
        let mut b = budget(&mut s);
        let a = open(&mut b, f).unwrap();
        assert_eq!(a.children(Archive::ROOT), 0..0);
    }
}
