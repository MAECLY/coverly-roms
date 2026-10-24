//! Synthetic archive fixtures for tests (feature `test-util`). Every byte
//! is crafted here; no copyrighted content.

/// Name of the single entry written by [`zip_raw_entry`].
pub const RAW_ENTRY_NAME: &str = "entry.bin";

/// A zip with one entry named [`RAW_ENTRY_NAME`] whose compression method
/// is `method` and whose stored bytes are exactly `raw_payload`.
///
/// Nothing is compressed: the payload is written as the entry's compressed
/// data, so tests can hand crafted LZMA/XZ/PPMd/zstd streams (or garbage)
/// to the zip backend. Sizes and CRC are those of `raw_payload`.
pub fn zip_raw_entry(method: u16, raw_payload: &[u8]) -> Vec<u8> {
    let name = RAW_ENTRY_NAME.as_bytes();
    let crc = crc32fast::hash(raw_payload);
    let size = u32::try_from(raw_payload.len()).expect("fixture payload under 4 GiB");
    let name_len = u16::try_from(name.len()).expect("short name");
    // Version needed: 6.3 covers LZMA, XZ, PPMd and zstd.
    let version: u16 = 63;
    let dos_time: u16 = 0;
    let dos_date: u16 = (1 << 5) | 1; // 1980-01-01

    let mut out = Vec::with_capacity(raw_payload.len() + 128);
    // Local file header.
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&method.to_le_bytes());
    out.extend_from_slice(&dos_time.to_le_bytes());
    out.extend_from_slice(&dos_date.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes()); // compressed
    out.extend_from_slice(&size.to_le_bytes()); // uncompressed
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra length
    out.extend_from_slice(name);
    out.extend_from_slice(raw_payload);

    // Central directory.
    let cd_offset = u32::try_from(out.len()).expect("fixture under 4 GiB");
    out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes()); // made by (MS-DOS)
    out.extend_from_slice(&version.to_le_bytes()); // needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&method.to_le_bytes());
    out.extend_from_slice(&dos_time.to_le_bytes());
    out.extend_from_slice(&dos_date.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra length
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number start
    out.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
    out.extend_from_slice(&0u32.to_le_bytes()); // external attributes
    out.extend_from_slice(&0u32.to_le_bytes()); // local header offset
    out.extend_from_slice(name);
    let cd_size = u32::try_from(out.len()).expect("fixture under 4 GiB") - cd_offset;

    // End of central directory.
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // this disk
    out.extend_from_slice(&0u16.to_le_bytes()); // disk with the CD
    out.extend_from_slice(&1u16.to_le_bytes()); // entries on this disk
    out.extend_from_slice(&1u16.to_le_bytes()); // entries in total
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    out
}

/// The 5 LZMA property bytes: `(pb * 5 + lp) * 9 + lc`, then the
/// little-endian dictionary size.
pub fn lzma_props(lc: u8, lp: u8, pb: u8, dict: u32) -> [u8; 5] {
    let d = dict.to_le_bytes();
    [(pb * 5 + lp) * 9 + lc, d[0], d[1], d[2], d[3]]
}

/// A raw LZMA2 stream that stores `data` in uncompressed chunks (the
/// first resets the dictionary), then the end marker. No encoder needed.
pub fn lzma2_stored(data: &[u8]) -> Vec<u8> {
    const CHUNK: usize = 64 * 1024;
    let mut out = Vec::with_capacity(data.len() + data.len() / CHUNK * 3 + 4);
    for (i, chunk) in data.chunks(CHUNK).enumerate() {
        out.push(if i == 0 { 0x01 } else { 0x02 });
        let size = u16::try_from(chunk.len() - 1).expect("chunk ≤ 64 KiB");
        out.extend_from_slice(&size.to_be_bytes());
        out.extend_from_slice(chunk);
    }
    out.push(0x00);
    out
}

/// One block of [`xz_stream`]: its data (stored as LZMA2 uncompressed
/// chunks) and the LZMA2 dictionary property its header declares.
#[derive(Clone, Debug)]
pub struct XzBlock {
    pub data: Vec<u8>,
    pub dict_prop: u8,
}

fn xz_vli(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// A complete single-stream `.xz` file with a CRC32 check: stream header,
/// one block per [`XzBlock`] (header with both sizes and its declared
/// dictionary; no pre-filters), index and footer. The declared dictionary
/// is written as given, so tests can declare one the walker must refuse
/// without encoding anything with it.
pub fn xz_stream(blocks: &[XzBlock]) -> Vec<u8> {
    const MAGIC: [u8; 6] = [0xFD, b'7', b'z', b'X', b'Z', 0x00];
    let flags = [0x00u8, 0x01]; // CRC32
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&flags);
    out.extend_from_slice(&crc32fast::hash(&flags).to_le_bytes());

    let mut records = Vec::new();
    for block in blocks {
        let payload = lzma2_stored(&block.data);
        let mut header = vec![0u8, 0xC0]; // size byte later; 1 filter, both sizes
        xz_vli(&mut header, payload.len() as u64);
        xz_vli(&mut header, block.data.len() as u64);
        header.extend_from_slice(&[0x21, 0x01, block.dict_prop]);
        while !(header.len() + 4).is_multiple_of(4) {
            header.push(0);
        }
        let size = header.len() + 4;
        header[0] = u8::try_from(size / 4 - 1).expect("short header");
        let crc = crc32fast::hash(&header);
        header.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(&payload);
        out.resize(out.len() + (4 - payload.len() % 4) % 4, 0);
        out.extend_from_slice(&crc32fast::hash(&block.data).to_le_bytes());
        records.push(((size + payload.len() + 4) as u64, block.data.len() as u64));
    }

    let mut index = vec![0u8];
    xz_vli(&mut index, records.len() as u64);
    for (unpadded, uncompressed) in records {
        xz_vli(&mut index, unpadded);
        xz_vli(&mut index, uncompressed);
    }
    while !index.len().is_multiple_of(4) {
        index.push(0);
    }
    let crc = crc32fast::hash(&index);
    index.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&index);

    let backward = u32::try_from(index.len() / 4 - 1).expect("small index");
    let mut tail = Vec::with_capacity(6);
    tail.extend_from_slice(&backward.to_le_bytes());
    tail.extend_from_slice(&flags);
    out.extend_from_slice(&crc32fast::hash(&tail).to_le_bytes());
    out.extend_from_slice(&tail);
    out.extend_from_slice(b"YZ");
    out
}
