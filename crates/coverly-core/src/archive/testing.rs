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
