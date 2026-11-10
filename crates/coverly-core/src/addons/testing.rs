//! Synthetic patch bytes for tests: only the bytes the sniffers read, no
//! real patch data.

/// beat varint (BPS/UPS): 7 bits per byte, the high bit ends the number,
/// +1 carry per byte.
pub fn varint(mut n: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let x = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(0x80 | x);
            return out;
        }
        out.push(x);
        n -= 1;
    }
}

/// The 12-byte BPS/UPS footer: source, target and patch CRC32 (LE).
fn footer(source_crc: u32) -> Vec<u8> {
    let mut b = source_crc.to_le_bytes().to_vec();
    b.extend(0x1111_1111u32.to_le_bytes());
    b.extend(0x2222_2222u32.to_le_bytes());
    b
}

/// `BPS1`, source size, target size, empty metadata, a dummy action
/// stream and the footer.
pub fn bps(source_size: u64, source_crc: u32) -> Vec<u8> {
    let mut b = b"BPS1".to_vec();
    b.extend(varint(source_size));
    b.extend(varint(source_size));
    b.extend(varint(0));
    b.extend([0u8; 8]);
    b.extend(footer(source_crc));
    b
}

/// `UPS1`, source size, target size, a dummy body and the footer.
pub fn ups(source_size: u64, source_crc: u32) -> Vec<u8> {
    let mut b = b"UPS1".to_vec();
    b.extend(varint(source_size));
    b.extend(varint(source_size));
    b.extend([0u8; 8]);
    b.extend(footer(source_crc));
    b
}

/// One IPS record (offset 0x000010, one byte) and `EOF`.
pub fn ips() -> Vec<u8> {
    b"PATCH\x00\x00\x10\x00\x01\xAAEOF".to_vec()
}

/// VCDIFF integer: big-endian base-128, continuation bit on every byte
/// but the last.
pub fn vcdiff_int(mut n: u64) -> Vec<u8> {
    let mut out = vec![(n & 0x7f) as u8];
    n >>= 7;
    while n != 0 {
        out.push(0x80 | (n & 0x7f) as u8);
        n >>= 7;
    }
    out.reverse();
    out
}

/// An xdelta3 (VCDIFF) header, with the app header `app` when given.
pub fn xdelta(app: Option<&str>) -> Vec<u8> {
    let mut b = vec![0xD6, 0xC3, 0xC4, 0x00];
    match app {
        None => b.push(0),
        Some(a) => {
            b.push(0x04);
            b.extend(vcdiff_int(a.len() as u64));
            b.extend(a.as_bytes());
        }
    }
    b.extend([0u8; 16]);
    b
}

/// `PPF10`, `PPF20` or `PPF30` and a blank header.
pub fn ppf(version: u8) -> Vec<u8> {
    let mut b = format!("PPF{version}0").into_bytes();
    b.extend([0u8; 60]);
    b
}

/// An N64 APS header: `APS10`, type 1 (N64), encoding 0, a 50-byte
/// description, the image format, the cart id (3 bytes) at offset 58,
/// CRC 8, pad 5 and size 4.
pub fn aps_n64(cart_id: &str) -> Vec<u8> {
    let mut b = b"APS10".to_vec();
    b.push(1);
    b.push(0);
    b.extend([b' '; 50]);
    b.push(0);
    b.extend(cart_id.as_bytes());
    b.extend([0u8; 8 + 5 + 4]);
    b
}
