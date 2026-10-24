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

/// How [`zip_with`] / [`ZipSpec`] store an entry's data.
#[derive(Clone, Debug)]
pub enum ZipMethod {
    /// Method 0.
    Stored,
    /// Method 8, compressed with `flate2`.
    Deflate,
    /// Method 9. Written as stored deflate blocks, which read the same in
    /// Deflate64 (there is no Deflate64 encoder).
    Deflate64,
    /// Method 12, compressed with `bzip2`.
    Bzip2,
    /// Any method with crafted compressed bytes (LZMA, XZ, PPMd and zstd
    /// streams encoded by the test, or garbage).
    Raw { method: u16, payload: Vec<u8> },
}

/// One entry of [`zip_crafted`]: name, data and every field a hostile zip
/// may set. Sizes and CRC default to those of `data`.
#[derive(Clone, Debug)]
pub struct ZipSpec {
    pub name: Vec<u8>,
    /// Uncompressed data; `method` turns it into the stored bytes.
    pub data: Vec<u8>,
    pub method: ZipMethod,
    /// General purpose flags (bit 0 encrypted, bit 11 UTF-8).
    pub flags: u16,
    /// "Version made by" (host in the high byte; 3 = Unix).
    pub made_by: u16,
    pub external_attr: u32,
    /// Extra fields written in both the local and the central header.
    pub extra: Vec<u8>,
    /// Declared uncompressed size instead of `data.len()`.
    pub declared_size: Option<u64>,
    /// Declared CRC32 instead of the CRC of `data`.
    pub declared_crc: Option<u32>,
    /// Write sizes as 0xFFFFFFFF with a zip64 extra field.
    pub zip64: bool,
}

impl ZipSpec {
    pub fn new(name: &str, data: &[u8], method: ZipMethod) -> Self {
        Self {
            name: name.as_bytes().to_vec(),
            data: data.to_vec(),
            method,
            flags: 0,
            made_by: 63,
            external_attr: 0,
            extra: Vec::new(),
            declared_size: None,
            declared_crc: None,
            zip64: false,
        }
    }

    /// Raw name bytes (CP437 or invalid UTF-8 names).
    pub fn raw_name(mut self, name: &[u8]) -> Self {
        self.name = name.to_vec();
        self
    }

    /// Sets the UTF-8 flag (general purpose bit 11).
    pub fn utf8(mut self) -> Self {
        self.flags |= 0x0800;
        self
    }

    /// A Unix symlink entry (`S_IFLNK | 0777`); `data` is the target.
    pub fn symlink(mut self) -> Self {
        self.made_by = (3 << 8) | 63;
        self.external_attr = 0o120_777 << 16;
        self
    }

    /// Unix mode bits in the external attributes.
    pub fn unix_mode(mut self, mode: u32) -> Self {
        self.made_by = (3 << 8) | 63;
        self.external_attr = mode << 16;
        self
    }

    /// A WinZip AES entry (AE-1, so the CRC is kept): method 99, the
    /// encrypted flag and the 0x9901 extra naming `inner_method`. The
    /// stored bytes are not really encrypted (never decrypted).
    pub fn aes(mut self, inner_method: u16) -> Self {
        self.flags |= 0x0001;
        let mut extra = Vec::with_capacity(11);
        extra.extend_from_slice(&0x9901u16.to_le_bytes());
        extra.extend_from_slice(&7u16.to_le_bytes());
        extra.extend_from_slice(&1u16.to_le_bytes()); // AE-1
        extra.extend_from_slice(b"AE");
        extra.push(3); // AES-256
        extra.extend_from_slice(&inner_method.to_le_bytes());
        self.extra.extend_from_slice(&extra);
        let payload = self.stored_bytes();
        self.method = ZipMethod::Raw {
            method: 99,
            payload,
        };
        self
    }

    /// Declares `size` (and keeps the CRC of `data`).
    pub fn declared_size(mut self, size: u64) -> Self {
        self.declared_size = Some(size);
        self
    }

    pub fn declared_crc(mut self, crc: u32) -> Self {
        self.declared_crc = Some(crc);
        self
    }

    /// Writes the sizes in a zip64 extra field.
    pub fn zip64(mut self) -> Self {
        self.zip64 = true;
        self
    }

    fn method_id(&self) -> u16 {
        match &self.method {
            ZipMethod::Stored => 0,
            ZipMethod::Deflate => 8,
            ZipMethod::Deflate64 => 9,
            ZipMethod::Bzip2 => 12,
            ZipMethod::Raw { method, .. } => *method,
        }
    }

    fn stored_bytes(&self) -> Vec<u8> {
        use std::io::Write;
        match &self.method {
            ZipMethod::Stored => self.data.clone(),
            ZipMethod::Deflate | ZipMethod::Deflate64 => {
                let level = if matches!(self.method, ZipMethod::Deflate) {
                    flate2::Compression::default()
                } else {
                    flate2::Compression::none()
                };
                let mut w = flate2::write::DeflateEncoder::new(Vec::new(), level);
                w.write_all(&self.data).expect("deflate in memory");
                w.finish().expect("deflate in memory")
            }
            ZipMethod::Bzip2 => {
                let mut w = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
                w.write_all(&self.data).expect("bzip2 in memory");
                w.finish().expect("bzip2 in memory")
            }
            ZipMethod::Raw { payload, .. } => payload.clone(),
        }
    }
}

/// A zip of `entries`, each compressed as its [`ZipMethod`] says.
pub fn zip_with(entries: &[(&str, &[u8], ZipMethod)]) -> Vec<u8> {
    let specs: Vec<ZipSpec> = entries
        .iter()
        .map(|(name, data, method)| ZipSpec::new(name, data, method.clone()))
        .collect();
    zip_crafted(&specs)
}

/// What the end records of [`zip_crafted_with`] claim.
#[derive(Clone, Copy, Debug, Default)]
pub struct ZipEnd {
    /// Entry count to claim instead of the real one; above 0xFFFF it is
    /// written in a zip64 end record.
    pub claimed_entries: Option<u64>,
    /// Always write the zip64 end record and locator.
    pub zip64: bool,
}

/// A zip with crafted entries (see [`ZipSpec`]).
pub fn zip_crafted(entries: &[ZipSpec]) -> Vec<u8> {
    zip_crafted_with(entries, ZipEnd::default())
}

/// [`zip_crafted`] with crafted end records.
pub fn zip_crafted_with(entries: &[ZipSpec], end: ZipEnd) -> Vec<u8> {
    let dos_time: u16 = (12 << 11) | (30 << 5); // 12:30:00
    let dos_date: u16 = (20 << 9) | (5 << 5) | 17; // 2000-05-17
    let mut out = Vec::new();
    let mut central = Vec::new();
    for spec in entries {
        let stored = spec.stored_bytes();
        let crc = spec
            .declared_crc
            .unwrap_or_else(|| crc32fast::hash(&spec.data));
        let size = spec.declared_size.unwrap_or(spec.data.len() as u64);
        let packed = stored.len() as u64;
        let offset = out.len() as u64;
        let mut extra = spec.extra.clone();
        let (size32, packed32) = if spec.zip64 {
            extra.extend_from_slice(&0x0001u16.to_le_bytes());
            extra.extend_from_slice(&16u16.to_le_bytes());
            extra.extend_from_slice(&size.to_le_bytes());
            extra.extend_from_slice(&packed.to_le_bytes());
            (u32::MAX, u32::MAX)
        } else {
            (
                u32::try_from(size).expect("declared size under 4 GiB without zip64"),
                u32::try_from(packed).expect("fixture under 4 GiB"),
            )
        };
        let name_len = u16::try_from(spec.name.len()).expect("short name");
        let extra_len = u16::try_from(extra.len()).expect("short extra");
        let version: u16 = if spec.zip64 { 45 } else { 63 };

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(&spec.flags.to_le_bytes());
        out.extend_from_slice(&spec.method_id().to_le_bytes());
        out.extend_from_slice(&dos_time.to_le_bytes());
        out.extend_from_slice(&dos_date.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&packed32.to_le_bytes());
        out.extend_from_slice(&size32.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&extra_len.to_le_bytes());
        out.extend_from_slice(&spec.name);
        out.extend_from_slice(&extra);
        out.extend_from_slice(&stored);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&spec.made_by.to_le_bytes());
        central.extend_from_slice(&version.to_le_bytes());
        central.extend_from_slice(&spec.flags.to_le_bytes());
        central.extend_from_slice(&spec.method_id().to_le_bytes());
        central.extend_from_slice(&dos_time.to_le_bytes());
        central.extend_from_slice(&dos_date.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&packed32.to_le_bytes());
        central.extend_from_slice(&size32.to_le_bytes());
        central.extend_from_slice(&name_len.to_le_bytes());
        central.extend_from_slice(&extra_len.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        central.extend_from_slice(&spec.external_attr.to_le_bytes());
        central.extend_from_slice(
            &u32::try_from(offset)
                .expect("fixture under 4 GiB")
                .to_le_bytes(),
        );
        central.extend_from_slice(&spec.name);
        central.extend_from_slice(&extra);
    }
    let cd_offset = out.len() as u64;
    let cd_size = central.len() as u64;
    out.extend_from_slice(&central);

    let count = end.claimed_entries.unwrap_or(entries.len() as u64);
    let zip64 = end.zip64 || count > 0xFFFE;
    if zip64 {
        let record_at = out.len() as u64;
        out.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
        out.extend_from_slice(&44u64.to_le_bytes()); // record size after this field
        out.extend_from_slice(&45u16.to_le_bytes()); // made by
        out.extend_from_slice(&45u16.to_le_bytes()); // needed
        out.extend_from_slice(&0u32.to_le_bytes()); // this disk
        out.extend_from_slice(&0u32.to_le_bytes()); // disk with the CD
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&record_at.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
    }
    let count16 = if zip64 {
        u16::MAX
    } else {
        u16::try_from(count).expect("checked above")
    };
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count16.to_le_bytes());
    out.extend_from_slice(&count16.to_le_bytes());
    out.extend_from_slice(
        &u32::try_from(cd_size)
            .expect("fixture under 4 GiB")
            .to_le_bytes(),
    );
    out.extend_from_slice(
        &(if zip64 {
            u32::MAX
        } else {
            u32::try_from(cd_offset).expect("fixture under 4 GiB")
        })
        .to_le_bytes(),
    );
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// The zip method 14 payload: version (2 bytes), properties size (5) and
/// the 5 LZMA property bytes, then the raw LZMA stream.
pub fn zip_lzma_payload(props: [u8; 5], raw_lzma: &[u8]) -> Vec<u8> {
    let mut out = vec![9, 20, 5, 0];
    out.extend_from_slice(&props);
    out.extend_from_slice(raw_lzma);
    out
}

/// The zip method 98 payload header: order − 1 (4 bits), memory in MiB − 1
/// (8 bits) and the restore method (4 bits), little-endian.
pub fn zip_ppmd_header(order: u8, mem_mib: u16, restore: u8) -> [u8; 2] {
    let v = u16::from(order - 1) | ((mem_mib - 1) << 4) | (u16::from(restore) << 12);
    v.to_le_bytes()
}
