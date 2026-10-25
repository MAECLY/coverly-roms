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

// ---------------------------------------------------------------------------
// 7z: a crafted header writer for the bounded walker (S4, S5). Every field
// is written as given, so tests can declare what a hostile archive would.
// ---------------------------------------------------------------------------

/// 7z coder ids.
pub mod sevenz_ids {
    pub const COPY: &[u8] = &[0x00];
    pub const DELTA: &[u8] = &[0x03];
    pub const BCJ_X86: &[u8] = &[0x03, 0x03, 0x01, 0x03];
    pub const LZMA: &[u8] = &[0x03, 0x01, 0x01];
    pub const LZMA2: &[u8] = &[0x21];
    pub const PPMD: &[u8] = &[0x03, 0x04, 0x01];
    pub const BZIP2: &[u8] = &[0x04, 0x02, 0x02];
    pub const AES: &[u8] = &[0x06, 0xF1, 0x07, 0x01];
}

/// A 7z number: the count of leading one bits of the first byte is the
/// number of extra little-endian bytes.
pub fn sz_number(out: &mut Vec<u8>, value: u64) {
    let mut first = 0u64;
    let mut mask = 0x80u64;
    let mut extra = 0usize;
    while extra < 8 {
        if value < (1u64 << (7 * (extra + 1))) {
            first |= value >> (8 * extra);
            break;
        }
        first |= mask;
        mask >>= 1;
        extra += 1;
    }
    out.push(first as u8);
    let mut v = value;
    for _ in 0..extra {
        out.push(v as u8);
        v >>= 8;
    }
}

/// A 7z bit vector, most significant bit first.
pub fn sz_bits(out: &mut Vec<u8>, bits: &[bool]) {
    for chunk in bits.chunks(8) {
        let mut b = 0u8;
        for (i, &set) in chunk.iter().enumerate() {
            if set {
                b |= 0x80 >> i;
            }
        }
        out.push(b);
    }
}

/// One coder of a [`SzFolder`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SzCoder {
    pub id: Vec<u8>,
    pub props: Vec<u8>,
    /// `Some((in, out))` writes a complex coder with these stream counts.
    pub streams: Option<(u64, u64)>,
}

impl SzCoder {
    pub fn new(id: &[u8], props: &[u8]) -> Self {
        Self {
            id: id.to_vec(),
            props: props.to_vec(),
            streams: None,
        }
    }
    pub fn copy() -> Self {
        Self::new(sevenz_ids::COPY, &[])
    }
    /// LZMA with `lc=3, lp=0, pb=2` and the given dictionary.
    pub fn lzma(dict: u32) -> Self {
        Self::new(sevenz_ids::LZMA, &lzma_props(3, 0, 2, dict))
    }
    /// LZMA2 with the given dictionary property.
    pub fn lzma2(dict_prop: u8) -> Self {
        Self::new(sevenz_ids::LZMA2, &[dict_prop])
    }
    /// PPMd7 with its 5-byte properties: order, then memory (u32 LE).
    pub fn ppmd(order: u8, mem: u32) -> Self {
        let m = mem.to_le_bytes();
        Self::new(sevenz_ids::PPMD, &[order, m[0], m[1], m[2], m[3]])
    }
    pub fn bcj_x86() -> Self {
        Self::new(sevenz_ids::BCJ_X86, &[])
    }
    /// Delta with `distance` (stored as `distance - 1`).
    pub fn delta(distance: u8) -> Self {
        Self::new(sevenz_ids::DELTA, &[distance - 1])
    }
    pub fn aes() -> Self {
        // First byte: cycles power 19, no salt, no IV.
        Self::new(sevenz_ids::AES, &[0x13])
    }
}

/// One folder (solid block) of a 7z.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SzFolder {
    /// In [`SzFolder::chain`] order: the first coder reads the packed
    /// stream, each later one reads the output of the one before it, and
    /// the last one's output is the folder's (the layout `sevenz-rust2`
    /// writes: bind pair `(i + 1, i)`).
    pub coders: Vec<SzCoder>,
    /// `(in_index, out_index)` pairs as written.
    pub bind_pairs: Vec<(u64, u64)>,
    /// Packed-stream in-indices, written only when there are two or more.
    pub packed: Vec<u64>,
    /// One size per coder out stream.
    pub unpack_sizes: Vec<u64>,
    /// Written in the unpack info's `kCRC` when any folder has one.
    pub crc: Option<u32>,
}

impl SzFolder {
    /// A simple chain of one-in/one-out coders whose out streams all have
    /// `unpack_size` bytes.
    pub fn chain(coders: Vec<SzCoder>, unpack_size: u64) -> Self {
        let n = coders.len() as u64;
        Self {
            bind_pairs: (0..n.saturating_sub(1)).map(|i| (i + 1, i)).collect(),
            packed: Vec::new(),
            unpack_sizes: vec![unpack_size; coders.len()],
            coders,
            crc: None,
        }
    }
    pub fn single(coder: SzCoder, unpack_size: u64) -> Self {
        Self::chain(vec![coder], unpack_size)
    }
}

/// The `kSubStreamsInfo` of a [`SzStreams`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SzSubstreams {
    /// `kNumUnPackStream`, one count per folder.
    pub num_unpack: Option<Vec<u64>>,
    /// `kSize`: the sizes as written (N − 1 per folder).
    pub sizes: Option<Vec<u64>>,
    /// `kCRC` with every digest defined.
    pub crcs: Option<Vec<u32>>,
}

/// A `StreamsInfo`: pack info, folders, substreams.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SzStreams {
    /// Offset of the first packed stream after the 32-byte start header.
    pub pack_pos: u64,
    pub pack_sizes: Vec<u64>,
    pub folders: Vec<SzFolder>,
    pub substreams: Option<SzSubstreams>,
}

/// One `FilesInfo` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SzFile {
    /// UTF-16 units of the name, without the terminating NUL.
    pub name: Vec<u16>,
    pub has_stream: bool,
    /// For an entry without a stream: directory (`true`) or empty file.
    pub dir: bool,
    pub attrib: Option<u32>,
    /// Windows FILETIME (100 ns since 1601).
    pub mtime: Option<u64>,
}

impl SzFile {
    pub fn file(name: &str) -> Self {
        Self {
            name: name.encode_utf16().collect(),
            has_stream: true,
            dir: false,
            attrib: None,
            mtime: None,
        }
    }
    pub fn empty(name: &str) -> Self {
        Self {
            has_stream: false,
            ..Self::file(name)
        }
    }
    pub fn dir(name: &str) -> Self {
        Self {
            has_stream: false,
            dir: true,
            attrib: Some(0x10),
            ..Self::file(name)
        }
    }
    pub fn attrib(mut self, attrib: u32) -> Self {
        self.attrib = Some(attrib);
        self
    }
    pub fn mtime(mut self, filetime: u64) -> Self {
        self.mtime = Some(filetime);
        self
    }
    pub fn raw_name(mut self, units: &[u16]) -> Self {
        self.name = units.to_vec();
        self
    }
}

/// A plain `kHeader`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SzHeader {
    pub streams: Option<SzStreams>,
    pub files: Vec<SzFile>,
    /// Declared file count; `None` writes `files.len()`.
    pub num_files: Option<u64>,
    /// Extra `FilesInfo` properties `(id, data)`, written before its `kEnd`.
    pub extra_props: Vec<(u64, Vec<u8>)>,
    /// Bytes written after the header's final `kEnd`.
    pub trailing: Vec<u8>,
}

fn sz_digests(out: &mut Vec<u8>, crcs: &[u32]) {
    out.push(1); // all defined
    for crc in crcs {
        out.extend_from_slice(&crc.to_le_bytes());
    }
}

/// Writes a `StreamsInfo` (without a leading property id), ending in `kEnd`.
pub fn sz_streams_info(out: &mut Vec<u8>, s: &SzStreams) {
    if !s.pack_sizes.is_empty() || !s.folders.is_empty() {
        out.push(0x06); // kPackInfo
        sz_number(out, s.pack_pos);
        sz_number(out, s.pack_sizes.len() as u64);
        out.push(0x09); // kSize
        for &size in &s.pack_sizes {
            sz_number(out, size);
        }
        out.push(0x00);
        out.push(0x07); // kUnPackInfo
        out.push(0x0B); // kFolder
        sz_number(out, s.folders.len() as u64);
        out.push(0); // not external
        for f in &s.folders {
            sz_number(out, f.coders.len() as u64);
            for c in &f.coders {
                let mut flags = u8::try_from(c.id.len()).expect("short id");
                if c.streams.is_some() {
                    flags |= 0x10;
                }
                if !c.props.is_empty() {
                    flags |= 0x20;
                }
                out.push(flags);
                out.extend_from_slice(&c.id);
                if let Some((ins, outs)) = c.streams {
                    sz_number(out, ins);
                    sz_number(out, outs);
                }
                if !c.props.is_empty() {
                    sz_number(out, c.props.len() as u64);
                    out.extend_from_slice(&c.props);
                }
            }
            for &(i, o) in &f.bind_pairs {
                sz_number(out, i);
                sz_number(out, o);
            }
            if f.packed.len() > 1 {
                for &p in &f.packed {
                    sz_number(out, p);
                }
            }
        }
        out.push(0x0C); // kCodersUnPackSize
        for f in &s.folders {
            for &size in &f.unpack_sizes {
                sz_number(out, size);
            }
        }
        if s.folders.iter().any(|f| f.crc.is_some()) {
            out.push(0x0A); // kCRC
            let defined: Vec<bool> = s.folders.iter().map(|f| f.crc.is_some()).collect();
            if defined.iter().all(|&d| d) {
                out.push(1);
            } else {
                out.push(0);
                sz_bits(out, &defined);
            }
            for crc in s.folders.iter().filter_map(|f| f.crc) {
                out.extend_from_slice(&crc.to_le_bytes());
            }
        }
        out.push(0x00);
    }
    if let Some(sub) = &s.substreams {
        out.push(0x08); // kSubStreamsInfo
        if let Some(nums) = &sub.num_unpack {
            out.push(0x0D);
            for &n in nums {
                sz_number(out, n);
            }
        }
        if let Some(sizes) = &sub.sizes {
            out.push(0x09);
            for &size in sizes {
                sz_number(out, size);
            }
        }
        if let Some(crcs) = &sub.crcs {
            out.push(0x0A);
            sz_digests(out, crcs);
        }
        out.push(0x00);
    }
    out.push(0x00);
}

fn sz_files_info(out: &mut Vec<u8>, h: &SzHeader) {
    out.push(0x05); // kFilesInfo
    let files = &h.files;
    sz_number(out, h.num_files.unwrap_or(files.len() as u64));
    let empty: Vec<bool> = files.iter().map(|f| !f.has_stream).collect();
    if empty.iter().any(|&e| e) {
        let mut data = Vec::new();
        sz_bits(&mut data, &empty);
        out.push(0x0E);
        sz_number(out, data.len() as u64);
        out.extend_from_slice(&data);
        let empty_file: Vec<bool> = files
            .iter()
            .filter(|f| !f.has_stream)
            .map(|f| !f.dir)
            .collect();
        if empty_file.iter().any(|&e| e) {
            let mut data = Vec::new();
            sz_bits(&mut data, &empty_file);
            out.push(0x0F);
            sz_number(out, data.len() as u64);
            out.extend_from_slice(&data);
        }
    }
    let mut names = vec![0u8]; // not external
    for f in files {
        for u in &f.name {
            names.extend_from_slice(&u.to_le_bytes());
        }
        names.extend_from_slice(&[0, 0]);
    }
    out.push(0x11);
    sz_number(out, names.len() as u64);
    out.extend_from_slice(&names);
    let vector = |out: &mut Vec<u8>, id: u8, values: Vec<Option<Vec<u8>>>| {
        if values.iter().all(Option::is_none) {
            return;
        }
        let mut data = Vec::new();
        let defined: Vec<bool> = values.iter().map(Option::is_some).collect();
        if defined.iter().all(|&d| d) {
            data.push(1);
        } else {
            data.push(0);
            sz_bits(&mut data, &defined);
        }
        data.push(0); // not external
        for v in values.into_iter().flatten() {
            data.extend_from_slice(&v);
        }
        out.push(id);
        sz_number(out, data.len() as u64);
        out.extend_from_slice(&data);
    };
    vector(
        out,
        0x14,
        files
            .iter()
            .map(|f| f.mtime.map(|t| t.to_le_bytes().to_vec()))
            .collect(),
    );
    vector(
        out,
        0x15,
        files
            .iter()
            .map(|f| f.attrib.map(|a| a.to_le_bytes().to_vec()))
            .collect(),
    );
    for (id, data) in &h.extra_props {
        sz_number(out, *id);
        sz_number(out, data.len() as u64);
        out.extend_from_slice(data);
    }
    out.push(0x00);
}

/// The bytes of a plain `kHeader` (first byte `0x01`).
pub fn sevenz_header_bytes(h: &SzHeader) -> Vec<u8> {
    let mut out = vec![0x01];
    if let Some(s) = &h.streams {
        out.push(0x04); // kMainStreamsInfo
        sz_streams_info(&mut out, s);
    }
    if !h.files.is_empty() || h.num_files.is_some() || !h.extra_props.is_empty() {
        sz_files_info(&mut out, h);
    }
    out.push(0x00);
    out.extend_from_slice(&h.trailing);
    out
}

/// The bytes of a `kEncodedHeader` (first byte `0x17`) over `streams`.
pub fn sevenz_encoded_header_bytes(streams: &SzStreams) -> Vec<u8> {
    let mut out = vec![0x17];
    sz_streams_info(&mut out, streams);
    out
}

/// A whole 7z file: the 32-byte start header, `packed`, then `header` as
/// the next header (offsets and CRCs computed).
pub fn sevenz_file(packed: &[u8], header: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + packed.len() + header.len());
    out.extend_from_slice(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C, 0, 4]);
    let mut start = Vec::with_capacity(20);
    start.extend_from_slice(&(packed.len() as u64).to_le_bytes());
    start.extend_from_slice(&(header.len() as u64).to_le_bytes());
    start.extend_from_slice(&crc32fast::hash(header).to_le_bytes());
    out.extend_from_slice(&crc32fast::hash(&start).to_le_bytes());
    out.extend_from_slice(&start);
    out.extend_from_slice(packed);
    out.extend_from_slice(header);
    out
}

/// A non-solid 7z with the Copy coder: one folder per file with data,
/// substream CRCs, empty files for empty data. Decodable by 7-Zip.
pub fn sevenz_copy(files: &[(&str, &[u8])]) -> Vec<u8> {
    let (packed, header) = sevenz_copy_parts(files);
    sevenz_file(&packed, &sevenz_header_bytes(&header))
}

/// The packed bytes and the header of [`sevenz_copy`], for tests that
/// change the header before writing the file.
pub fn sevenz_copy_parts(files: &[(&str, &[u8])]) -> (Vec<u8>, SzHeader) {
    let mut packed = Vec::new();
    let mut streams = SzStreams::default();
    let mut crcs = Vec::new();
    let mut entries = Vec::new();
    for (name, data) in files {
        if data.is_empty() {
            entries.push(SzFile::empty(name));
            continue;
        }
        packed.extend_from_slice(data);
        streams.pack_sizes.push(data.len() as u64);
        streams
            .folders
            .push(SzFolder::single(SzCoder::copy(), data.len() as u64));
        crcs.push(crc32fast::hash(data));
        entries.push(SzFile::file(name));
    }
    streams.substreams = Some(SzSubstreams {
        crcs: (!crcs.is_empty()).then_some(crcs),
        ..SzSubstreams::default()
    });
    let header = SzHeader {
        streams: (!streams.folders.is_empty()).then_some(streams),
        files: entries,
        ..SzHeader::default()
    };
    (packed, header)
}

/// The encoded-header `StreamsInfo` for `raw_header` stored as LZMA2
/// (uncompressed chunks) at `pack_pos`: one folder, its CRC in the
/// substreams (as 7-Zip writes it). Tests change it before writing.
pub fn sevenz_encoded_streams(pack_pos: u64, raw_header: &[u8]) -> (Vec<u8>, SzStreams) {
    let packed = lzma2_stored(raw_header);
    let streams = SzStreams {
        pack_pos,
        pack_sizes: vec![packed.len() as u64],
        folders: vec![SzFolder::single(
            SzCoder::lzma2(16),
            raw_header.len() as u64,
        )],
        substreams: Some(SzSubstreams {
            crcs: Some(vec![crc32fast::hash(raw_header)]),
            ..SzSubstreams::default()
        }),
    };
    (packed, streams)
}

/// A 7z whose header `raw_header` (plain `kHeader` bytes, or anything a
/// test wants decoded) is stored behind a `kEncodedHeader`; `main_packed`
/// comes first, as the streams of `raw_header` expect.
pub fn sevenz_with_encoded_header(main_packed: &[u8], raw_header: &[u8]) -> Vec<u8> {
    sevenz_with_encoded(main_packed, raw_header, |_| {})
}

/// [`sevenz_with_encoded_header`] with a hook that changes the encoded
/// header's `StreamsInfo` before it is written.
pub fn sevenz_with_encoded(
    main_packed: &[u8],
    raw_header: &[u8],
    edit: impl FnOnce(&mut SzStreams),
) -> Vec<u8> {
    let (enc_packed, mut streams) = sevenz_encoded_streams(main_packed.len() as u64, raw_header);
    edit(&mut streams);
    let mut packed = main_packed.to_vec();
    packed.extend_from_slice(&enc_packed);
    sevenz_file(&packed, &sevenz_encoded_header_bytes(&streams))
}

// ---------------------------------------------------------------------------
// 7z: real archives through the `sevenz-rust2` writer (the shapes of spec
// §1, synthetic content).
// ---------------------------------------------------------------------------

/// The coders of a [`SevenZShape`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SevenZCoder {
    Copy,
    Lzma {
        dict: u32,
    },
    Lzma2 {
        dict: u32,
    },
    /// BCJ x86 in front of LZMA2.
    BcjLzma2 {
        dict: u32,
    },
    Ppmd {
        order: u32,
        mem: u32,
    },
}

/// How [`sevenz`] lays out its files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SevenZShape {
    /// One folder for every file with data (`false`, like the PS1 archive
    /// of §1: LZMA:26, 12 blocks) or one folder for all of them (`true`,
    /// like the Dreamcast and Saturn archives: LZMA2:24, solid).
    pub solid: bool,
    pub coder: SevenZCoder,
    /// Store `.cue`/`.gdi`/`.m3u` after every other file, as the solid
    /// archives of §1 do.
    pub sheets_last: bool,
}

impl SevenZShape {
    /// The PS1 shape of spec §1 with a small dictionary.
    pub fn non_solid_lzma() -> Self {
        Self {
            solid: false,
            coder: SevenZCoder::Lzma { dict: 1 << 20 },
            sheets_last: false,
        }
    }

    /// The Dreamcast/Saturn shape of spec §1 with a small dictionary.
    pub fn solid_lzma2_sheets_last() -> Self {
        Self {
            solid: true,
            coder: SevenZCoder::Lzma2 { dict: 1 << 20 },
            sheets_last: true,
        }
    }
}

fn is_sheet(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".cue", ".gdi", ".m3u"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

fn sevenz_methods(coder: SevenZCoder) -> Vec<sevenz_rust2::EncoderConfiguration> {
    use sevenz_rust2::encoder_options::{Lzma2Options, LzmaOptions, PpmdOptions};
    use sevenz_rust2::EncoderMethod;
    match coder {
        SevenZCoder::Copy => vec![EncoderMethod::COPY.into()],
        SevenZCoder::Lzma { dict } => {
            let mut o = LzmaOptions::from_level(5);
            o.set_dictionary_size(dict);
            vec![o.into()]
        }
        SevenZCoder::Lzma2 { dict } => {
            let mut o = Lzma2Options::from_level(5);
            o.set_dictionary_size(dict);
            vec![o.into()]
        }
        SevenZCoder::BcjLzma2 { dict } => {
            let mut o = Lzma2Options::from_level(5);
            o.set_dictionary_size(dict);
            vec![EncoderMethod::BCJ_X86_FILTER.into(), o.into()]
        }
        SevenZCoder::Ppmd { order, mem } => {
            vec![PpmdOptions::from_order_memory_size(order, mem).into()]
        }
    }
}

/// A real 7z written by `sevenz-rust2`. A name ending in `/` is a
/// directory (written first, as 7-Zip does); an empty file has no stream.
/// Files keep their order, except that sheets go last with `sheets_last`.
pub fn sevenz(files: &[(&str, &[u8])], shape: SevenZShape) -> Vec<u8> {
    use sevenz_rust2::{ArchiveEntry, ArchiveWriter, SourceReader};
    use std::io::Cursor;

    let mut w = ArchiveWriter::new(Cursor::new(Vec::new())).expect("7z writer");
    w.set_content_methods(sevenz_methods(shape.coder));
    let mut data: Vec<(&str, &[u8])> = files
        .iter()
        .copied()
        .filter(|(n, d)| !n.ends_with('/') && !d.is_empty())
        .collect();
    if shape.sheets_last {
        data.sort_by_key(|(n, _)| is_sheet(n));
    }
    for (name, _) in files.iter().filter(|(n, _)| n.ends_with('/')) {
        w.push_archive_entry::<&[u8]>(
            ArchiveEntry::new_directory(name.trim_end_matches('/')),
            None,
        )
        .expect("push dir");
    }
    if shape.solid && !data.is_empty() {
        let entries = data
            .iter()
            .map(|(n, _)| ArchiveEntry::new_file(n))
            .collect();
        let readers = data
            .iter()
            .map(|(_, d)| SourceReader::new(Cursor::new(d.to_vec())))
            .collect();
        w.push_archive_entries(entries, readers)
            .expect("push solid");
    } else {
        for (name, bytes) in &data {
            w.push_archive_entry(ArchiveEntry::new_file(name), Some(*bytes))
                .expect("push file");
        }
    }
    for (name, _) in files
        .iter()
        .filter(|(n, d)| !n.ends_with('/') && d.is_empty())
    {
        w.push_archive_entry::<&[u8]>(ArchiveEntry::new_file(name), None)
            .expect("push empty");
    }
    w.finish().expect("7z finish").into_inner()
}

// ---------------------------------------------------------------------------
// tar (plan Task 12): crafted headers, pax and GNU records, compressed tar.
// ---------------------------------------------------------------------------

/// One record of [`tar_crafted`]: a header (typeflag, name, link) and the
/// bytes written after it. `declared_size` writes another size into the
/// header than the data written, so tests can declare sizes the reader must
/// refuse without writing them.
#[derive(Clone, Debug)]
pub struct TarSpec {
    pub typeflag: u8,
    pub name: Vec<u8>,
    pub link: Vec<u8>,
    pub data: Vec<u8>,
    pub declared_size: Option<u64>,
    /// GNU magic (`ustar  \0`) instead of POSIX ustar (`ustar\0` `00`).
    pub gnu: bool,
    pub bad_checksum: bool,
    pub mtime: u64,
    /// Raw bytes written into the header at an offset (GNU sparse flags).
    pub patch: Vec<(usize, u8)>,
}

impl TarSpec {
    /// A record of `typeflag` with `name` and `data`.
    pub fn raw(typeflag: u8, name: &[u8], data: &[u8]) -> Self {
        Self {
            typeflag,
            name: name.to_vec(),
            link: Vec::new(),
            data: data.to_vec(),
            declared_size: None,
            gnu: false,
            bad_checksum: false,
            mtime: 1_700_000_000,
            patch: Vec::new(),
        }
    }

    pub fn file(name: &str, data: &[u8]) -> Self {
        Self::raw(b'0', name.as_bytes(), data)
    }

    /// A directory; the name gets its trailing `/`.
    pub fn dir(name: &str) -> Self {
        Self::raw(
            b'5',
            format!("{}/", name.trim_end_matches('/')).as_bytes(),
            b"",
        )
    }

    pub fn symlink(name: &str, target: &str) -> Self {
        Self::raw(b'2', name.as_bytes(), b"").link(target.as_bytes())
    }

    pub fn hardlink(name: &str, target: &str) -> Self {
        Self::raw(b'1', name.as_bytes(), b"").link(target.as_bytes())
    }

    /// A pax extended header (`x`) for the next entry.
    pub fn pax(records: &[(&str, &[u8])]) -> Self {
        Self::raw(b'x', b"PaxHeaders/entry", &pax_records(records))
    }

    /// A pax global header (`g`).
    pub fn pax_global(records: &[(&str, &[u8])]) -> Self {
        Self::raw(b'g', b"pax_global_header", &pax_records(records))
    }

    /// A GNU long name record (`L`): `name` plus the NUL GNU tar writes.
    pub fn gnu_longname(name: &[u8]) -> Self {
        let mut data = name.to_vec();
        data.push(0);
        Self::raw(b'L', b"././@LongLink", &data).gnu()
    }

    /// A GNU long link record (`K`).
    pub fn gnu_longlink(target: &[u8]) -> Self {
        let mut data = target.to_vec();
        data.push(0);
        Self::raw(b'K', b"././@LongLink", &data).gnu()
    }

    pub fn link(mut self, target: &[u8]) -> Self {
        self.link = target.to_vec();
        self
    }

    pub fn declared_size(mut self, size: u64) -> Self {
        self.declared_size = Some(size);
        self
    }

    pub fn gnu(mut self) -> Self {
        self.gnu = true;
        self
    }

    pub fn bad_checksum(mut self) -> Self {
        self.bad_checksum = true;
        self
    }

    pub fn patch(mut self, offset: usize, byte: u8) -> Self {
        self.patch.push((offset, byte));
        self
    }
}

/// `length key=value\n` records; each length counts itself.
pub fn pax_records(records: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (key, value) in records {
        let body = 1 + key.len() + 1 + value.len() + 1; // " key=value\n"
        let mut digits = body.to_string().len();
        let len = loop {
            let len = body + digits;
            if len.to_string().len() == digits {
                break len;
            }
            digits += 1;
        };
        out.extend_from_slice(format!("{len} {key}=").as_bytes());
        out.extend_from_slice(value);
        out.push(b'\n');
    }
    out
}

fn tar_octal(field: &mut [u8], value: u64) {
    let digits = field.len() - 1;
    let text = format!("{value:0digits$o}");
    assert!(text.len() <= digits, "{value} does not fit the field");
    field[..digits].copy_from_slice(text.as_bytes());
    field[digits] = 0;
}

/// The 512-byte header of `spec`, with a valid checksum unless
/// `bad_checksum`.
pub fn tar_header(spec: &TarSpec) -> [u8; 512] {
    let mut h = [0u8; 512];
    assert!(
        spec.name.len() <= 100,
        "use a pax or GNU record for long names"
    );
    assert!(
        spec.link.len() <= 100,
        "use a pax or GNU record for long links"
    );
    h[..spec.name.len()].copy_from_slice(&spec.name);
    tar_octal(&mut h[100..108], 0o644);
    tar_octal(&mut h[108..116], 1000);
    tar_octal(&mut h[116..124], 1000);
    let size = spec.declared_size.unwrap_or(spec.data.len() as u64);
    if size < 1 << 33 {
        tar_octal(&mut h[124..136], size);
    } else {
        // GNU base-256.
        h[124] = 0x80;
        h[128..136].copy_from_slice(&size.to_be_bytes());
    }
    tar_octal(&mut h[136..148], spec.mtime);
    h[156] = spec.typeflag;
    h[157..157 + spec.link.len()].copy_from_slice(&spec.link);
    if spec.gnu {
        h[257..265].copy_from_slice(b"ustar  \0");
    } else {
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
    }
    h[265..269].copy_from_slice(b"user");
    h[297..302].copy_from_slice(b"group");
    for &(offset, byte) in &spec.patch {
        h[offset] = byte;
    }
    h[148..156].fill(b' ');
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    let sum = if spec.bad_checksum { sum + 1 } else { sum };
    h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
    h[155] = b' ';
    h
}

/// Every record of `specs` (header, data, zero padding to 512), then the
/// end-of-archive marker (two zero blocks).
pub fn tar_crafted(specs: &[TarSpec]) -> Vec<u8> {
    let mut out = tar_records(specs);
    out.resize(out.len() + 1024, 0);
    out
}

/// The records of `specs` without the end marker.
pub fn tar_records(specs: &[TarSpec]) -> Vec<u8> {
    let mut out = Vec::new();
    for spec in specs {
        out.extend_from_slice(&tar_header(spec));
        out.extend_from_slice(&spec.data);
        out.resize(out.len().next_multiple_of(512), 0);
    }
    out
}

/// A ustar of regular files (a name ending in `/` is a directory).
pub fn tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let specs: Vec<TarSpec> = entries
        .iter()
        .map(|(name, data)| {
            if name.ends_with('/') {
                TarSpec::dir(name)
            } else {
                TarSpec::file(name, data)
            }
        })
        .collect();
    tar_crafted(&specs)
}

/// [`tar`] of `entries` in one gzip member, followed inside that member by
/// `trailer_zeros` zero bytes after the end marker. Cheap for gigabytes
/// (see [`gzip_with_zeros`]).
pub fn tar_gz(entries: &[(&str, &[u8])], trailer_zeros: u64) -> Vec<u8> {
    gzip_with_zeros(&tar(entries), trailer_zeros)
}

/// Deflates `input` with `flush` and returns the bytes it produced.
fn deflate_chunk(
    c: &mut flate2::Compress,
    mut input: &[u8],
    flush: flate2::FlushCompress,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() / 4 + 4096);
    loop {
        if out.capacity() - out.len() < 4096 {
            out.reserve(64 * 1024);
        }
        let before = c.total_in();
        let status = c.compress_vec(input, &mut out, flush).expect("deflate");
        let used = usize::try_from(c.total_in() - before).expect("chunk");
        input = &input[used..];
        let room_left = out.len() < out.capacity();
        if input.is_empty() && room_left {
            let finished = !matches!(flush, flate2::FlushCompress::Finish)
                || status == flate2::Status::StreamEnd;
            if finished {
                return out;
            }
        }
    }
}

/// One gzip member of `prefix` followed by `zeros` zero bytes, valid (CRC32
/// and ISIZE) and built in time independent of `zeros`: after a full flush
/// the deflate state is reset, so every 1 MiB chunk of zeros encodes to
/// the same bytes, which are repeated.
pub fn gzip_with_zeros(prefix: &[u8], zeros: u64) -> Vec<u8> {
    use flate2::{Compress, Compression, FlushCompress};
    const CHUNK: u64 = 1 << 20;
    let zero_chunk = vec![0u8; CHUNK as usize];
    let mut c = Compress::new(Compression::fast(), false);
    let mut out = vec![0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xFF];
    out.extend(deflate_chunk(&mut c, prefix, FlushCompress::Full));
    let chunks = zeros / CHUNK;
    if chunks <= 3 {
        for _ in 0..chunks {
            out.extend(deflate_chunk(&mut c, &zero_chunk, FlushCompress::Full));
        }
    } else {
        out.extend(deflate_chunk(&mut c, &zero_chunk, FlushCompress::Full));
        let steady = deflate_chunk(&mut c, &zero_chunk, FlushCompress::Full);
        let again = deflate_chunk(&mut c, &zero_chunk, FlushCompress::Full);
        assert_eq!(
            steady, again,
            "deflate of zeros after a full flush is not steady"
        );
        out.extend_from_slice(&steady);
        out.extend_from_slice(&again);
        for _ in 3..chunks {
            out.extend_from_slice(&steady);
        }
    }
    let rest = usize::try_from(zeros % CHUNK).expect("rest");
    out.extend(deflate_chunk(
        &mut c,
        &zero_chunk[..rest],
        FlushCompress::Full,
    ));
    out.extend(deflate_chunk(&mut c, &[], FlushCompress::Finish));

    let mut crc = crc32fast::Hasher::new();
    crc.update(prefix);
    let mut chunk_crc = crc32fast::Hasher::new();
    chunk_crc.update(&zero_chunk);
    let mut n = chunks;
    while n > 0 {
        if n & 1 == 1 {
            crc.combine(&chunk_crc);
        }
        let twice = chunk_crc.clone();
        chunk_crc.combine(&twice);
        n >>= 1;
    }
    crc.update(&zero_chunk[..rest]);
    out.extend_from_slice(&crc.finalize().to_le_bytes());
    let total = (prefix.len() as u64).wrapping_add(zeros);
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out
}

// ---------------------------------------------------------------------------
// Fake external tools (plan Task 13, AP-5)
// ---------------------------------------------------------------------------

/// What one run of `coverly-fake-tool` does when its arguments start with
/// [`FakeCase::args_prefix`].
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FakeCase {
    /// Matches when the tool's arguments start with these (empty: always).
    pub args_prefix: Vec<String>,
    /// Printed to stdout first, as is.
    pub stdout: String,
    /// Then the bytes of this file (binary output: tar, entry data).
    pub stdout_file: Option<std::path::PathBuf>,
    /// Then this many `o` bytes on stdout.
    pub stdout_fill: u64,
    pub stderr: String,
    /// This many `e` bytes on stderr, written before stdout.
    pub stderr_fill: u64,
    /// Prints one `key=value` line per fact the hygiene tests check: the
    /// arguments (JSON), the working directory and its entry count, and
    /// each of [`FAKE_REPORTED_ENV`] (`<unset>` when absent).
    pub report: bool,
    /// Spawns a copy of itself that sleeps this long and then creates
    /// `grandchild_marker` (the timeout test checks it never appears).
    pub grandchild_sleep_ms: u64,
    pub grandchild_marker: Option<std::path::PathBuf>,
    /// The grandchild leaves the tool's process group (Unix `setsid`) and
    /// keeps the output pipes open: a timeout must still return on time.
    pub grandchild_detach: bool,
    /// The grandchild holds only stderr (its stdout is closed): the tool's
    /// output ends while a helper still holds the error pipe.
    pub grandchild_stderr_only: bool,
    /// Sleeps this long after writing, before exiting.
    pub sleep_ms: u64,
    pub exit: i32,
    /// Appended to the file named by the last argument (the archive) after
    /// writing: the archive changes between two listing runs.
    pub append_to_archive: String,
}

/// The script of one fake tool: the first case whose prefix matches runs;
/// with none, the tool exits 0 silently.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FakeScript {
    pub cases: Vec<FakeCase>,
}

impl FakeScript {
    /// A tool that prints `banner` to every call.
    pub fn banner(banner: &str) -> Self {
        Self {
            cases: vec![FakeCase {
                stdout: format!("{banner}\n"),
                ..FakeCase::default()
            }],
        }
    }

    /// Adds `case` before the existing ones (it wins over them).
    pub fn with_first(mut self, case: FakeCase) -> Self {
        self.cases.insert(0, case);
        self
    }

    /// The case for `args`, if any.
    pub fn case_for(&self, args: &[String]) -> Option<&FakeCase> {
        self.cases.iter().find(|c| args.starts_with(&c.args_prefix))
    }
}

/// Environment variables the fake tool reports when [`FakeCase::report`].
pub const FAKE_REPORTED_ENV: &[&str] = &[
    "RAR",
    "TAR_READER_OPTIONS",
    "HOME",
    "USERPROFILE",
    "PATH",
    "LC_ALL",
    "LANG",
];

/// First argument with which the fake tool runs as its own grandchild.
pub const FAKE_GRANDCHILD_ARG: &str = "__coverly_fake_grandchild";

/// Where the fake tool at `exe` reads its script: next to it, so a cleared
/// environment (spec §5.2) still reaches it.
pub fn fake_script_path(exe: &std::path::Path) -> std::path::PathBuf {
    exe.with_extension("fake.json")
}

/// The built `coverly-fake-tool` binary: `COVERLY_FAKE_TOOL_BIN` when set,
/// else next to the running test's `deps` directory (where Cargo puts the
/// package's binaries).
pub fn fake_tool_binary() -> std::path::PathBuf {
    if let Some(bin) = std::env::var_os("COVERLY_FAKE_TOOL_BIN") {
        return bin.into();
    }
    let exe = std::env::current_exe().expect("current exe");
    let mut dir = exe.parent().expect("exe dir").to_path_buf();
    if dir.ends_with("deps") {
        dir.pop();
    }
    dir.join(format!("coverly-fake-tool{}", std::env::consts::EXE_SUFFIX))
}

/// Copies the fake tool into `dir` as `name` (`7zz`, `unrar`, `bsdtar`…;
/// `.exe` is appended on Windows) with `script` beside it.
pub fn fake_tool(dir: &std::path::Path, name: &str, script: &FakeScript) -> std::path::PathBuf {
    let exe = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(fake_tool_binary(), &exe)
        .expect("copy coverly-fake-tool (build it with --features test-util)");
    let json = serde_json::to_vec_pretty(script).expect("script json");
    std::fs::write(fake_script_path(&exe), json).expect("write fake script");
    exe
}

/// Runs a tool through the crate's `run_tool` (stdout returned, no sink),
/// for the hygiene tests of `tests/archive_external.rs`.
pub fn run_tool(
    t: &super::external::FoundTool,
    args: &[&std::ffi::OsStr],
    archive: &std::path::Path,
    out_cap: u64,
    timeout: std::time::Duration,
) -> Result<super::external::ToolRun, super::ArchiveError> {
    super::external::run_tool_for_tests(t, args, archive, out_cap, timeout)
}

// ---------------------------------------------------------------------------
// Crafted external-tool listings (plan Task 14, S7)
// ---------------------------------------------------------------------------

/// One entry of a crafted 7-Zip or unrar listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolEntry {
    /// The name exactly as the tool would print it (a test may put a
    /// newline in it to forge records).
    pub name: String,
    pub size: u64,
    pub packed: Option<u64>,
    pub crc32: Option<u32>,
    pub dir: bool,
    /// 7z folder (`Block = n`).
    pub block: Option<u32>,
}

impl ToolEntry {
    pub fn file(name: &str, size: u64, crc32: u32) -> Self {
        Self {
            name: name.into(),
            size,
            packed: Some(size),
            crc32: Some(crc32),
            dir: false,
            block: Some(0),
        }
    }

    pub fn dir(name: &str) -> Self {
        Self {
            name: name.into(),
            size: 0,
            packed: Some(0),
            crc32: None,
            dir: true,
            block: None,
        }
    }
}

/// The 7-Zip banner the listing builders print.
pub const SEVEN_ZIP_BANNER: &str =
    "7-Zip (z) 24.08 (arm64) : Copyright (c) 1999-2024 Igor Pavlov : 2024-08-11";
/// The unrar banner the listing builders print.
pub const UNRAR_BANNER: &str = "UNRAR 7.01 freeware      Copyright (c) 1993-2024 Alexander Roshal";

const SEVEN_ZIP_SEP: &str =
    "------------------- ----- ------------ ------------  ------------------------";
const UNRAR_SEP: &str = "----------- ----------  ---------- -----  ----";

/// One `7zz l -slt -ba` record of a 7z archive, as 7-Zip 24 prints it.
pub fn seven_zip_record(e: &ToolEntry) -> String {
    let opt = |v: Option<u64>| v.map(|n| n.to_string()).unwrap_or_default();
    format!(
        "Path = {}\nSize = {}\nPacked Size = {}\nModified = 2018-11-10 11:36:34.1850625\n\
         Attributes = {}\nCRC = {}\nEncrypted = -\nMethod = {}\nBlock = {}\n",
        e.name,
        e.size,
        opt(e.packed),
        if e.dir { "D" } else { "A" },
        e.crc32.map(|c| format!("{c:08X}")).unwrap_or_default(),
        if e.dir { "" } else { "LZMA:26" },
        e.block.map(|b| b.to_string()).unwrap_or_default(),
    )
}

/// The whole `7zz l -slt -ba -sccUTF-8 -p- -- <a>` output: records
/// separated by blank lines.
pub fn seven_zip_slt(entries: &[ToolEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str(&seven_zip_record(e));
        out.push('\n');
    }
    out
}

/// The `N files, M folders` summary line 7-Zip prints for `entries`.
pub fn seven_zip_summary_line(files: u64, folders: u64, size: u64, packed: u64) -> String {
    let counts = if folders > 0 {
        format!("{files} files, {folders} folders")
    } else {
        format!("{files} files")
    };
    format!("2018-11-10 15:45:22       {size:>12} {packed:>12}  {counts}")
}

/// The whole non-`-ba` `7zz l -sccUTF-8 -p- -- <a>` output: banner,
/// archive block, table and the summary computed from `entries`.
pub fn seven_zip_l(entries: &[ToolEntry]) -> String {
    let files = entries.iter().filter(|e| !e.dir).count() as u64;
    let folders = entries.iter().filter(|e| e.dir).count() as u64;
    let size: u64 = entries.iter().map(|e| e.size).sum();
    let packed: u64 = entries.iter().filter_map(|e| e.packed).sum();
    seven_zip_l_with(
        entries,
        &seven_zip_summary_line(files, folders, size, packed),
    )
}

/// [`seven_zip_l`] with an explicit summary line (tests forge it).
pub fn seven_zip_l_with(entries: &[ToolEntry], summary: &str) -> String {
    let mut out = format!(
        "\n{SEVEN_ZIP_BANNER}\n\nScanning the drive for archives:\n1 file, 1000 bytes (1 KiB)\n\n\
         Listing archive: game.7z\n\n--\nPath = game.7z\nType = 7z\nPhysical Size = 1000\n\
         Headers Size = 396\nMethod = LZMA:26\nSolid = -\nBlocks = 1\n\n\
         \x20  Date      Time    Attr         Size   Compressed  Name\n{SEVEN_ZIP_SEP}\n"
    );
    for e in entries {
        let attr = if e.dir { "D...." } else { "....A" };
        let packed = e.packed.map(|p| p.to_string()).unwrap_or_default();
        out.push_str(&format!(
            "2018-11-10 11:36:34 {attr} {:>12} {packed:>12}  {}\n",
            e.size, e.name
        ));
    }
    out.push_str(&format!("{SEVEN_ZIP_SEP}\n{summary}\n"));
    out
}

/// One `unrar lt` record, as unrar 7 prints it.
pub fn unrar_record(e: &ToolEntry) -> String {
    let mut out = format!("        Name: {}\n", e.name);
    if e.dir {
        out.push_str("        Type: Directory\n");
    } else {
        out.push_str("        Type: File\n");
        out.push_str(&format!("        Size: {}\n", e.size));
        out.push_str(&format!(" Packed size: {}\n", e.packed.unwrap_or(e.size)));
        out.push_str("       Ratio: 100%\n");
    }
    out.push_str("       mtime: 2024-01-01 10:00:00,000000000\n");
    out.push_str(if e.dir {
        "  Attributes: drwxr-xr-x\n"
    } else {
        "  Attributes: -rw-r--r--\n"
    });
    if let Some(crc) = e.crc32 {
        out.push_str(&format!("       CRC32: {crc:08X}\n"));
    }
    out.push_str("     Host OS: Unix\n");
    out.push_str(" Compression: RAR 5.0(v50) -m3 -md=128K\n");
    out
}

fn unrar_preamble() -> String {
    format!("\n{UNRAR_BANNER}\n\nArchive: game.rar\nDetails: RAR 5\n\n")
}

/// The whole `unrar lt -cfg- -p- -scur -c- -- <a>` output.
pub fn unrar_lt(entries: &[ToolEntry]) -> String {
    let mut out = unrar_preamble();
    for e in entries {
        out.push_str(&unrar_record(e));
        out.push('\n');
    }
    out
}

/// The whole `unrar l -cfg- -p- -scur -c- -- <a>` output, totals computed
/// from `entries` (unrar counts every file header, folders too).
pub fn unrar_l(entries: &[ToolEntry]) -> String {
    let size: u64 = entries.iter().map(|e| e.size).sum();
    unrar_l_with(entries, size, entries.len() as u64)
}

/// [`unrar_l`] with explicit totals (tests forge them).
pub fn unrar_l_with(entries: &[ToolEntry], size: u64, count: u64) -> String {
    let mut out = unrar_preamble();
    out.push_str(&format!(
        " Attributes      Size     Date    Time   Name\n{UNRAR_SEP}\n"
    ));
    for e in entries {
        let attr = if e.dir { "drwxr-xr-x" } else { "-rw-r--r--" };
        out.push_str(&format!(
            " {attr} {:>9}  2024-01-01 10:00  {}\n",
            e.size, e.name
        ));
    }
    out.push_str(&format!("{UNRAR_SEP}\n{size:>21} {:18} {count}\n\n", ""));
    out
}

/// The exact arguments of the 7-Zip record run up to the archive: the
/// fake only answers the pinned switches.
pub const SEVEN_ZIP_RECORD_ARGS: &[&str] = &["l", "-slt", "-ba", "-sccUTF-8", "-p-", "--"];
/// The exact arguments of the 7-Zip summary run up to the archive.
pub const SEVEN_ZIP_SUMMARY_ARGS: &[&str] = &["l", "-sccUTF-8", "-p-", "--"];
/// The exact arguments of the unrar record run up to the archive.
pub const UNRAR_RECORD_ARGS: &[&str] = &["lt", "-cfg-", "-p-", "-scur", "-c-", "--"];
/// The exact arguments of the unrar totals run up to the archive.
pub const UNRAR_SUMMARY_ARGS: &[&str] = &["l", "-cfg-", "-p-", "-scur", "-c-", "--"];

/// A fake `7zz` that prints `slt` to the record run, `l` to the summary
/// run and the banner otherwise.
pub fn seven_zip_listing_script(slt: &str, l: &str) -> FakeScript {
    FakeScript {
        cases: vec![
            FakeCase {
                args_prefix: SEVEN_ZIP_RECORD_ARGS.iter().map(|a| (*a).into()).collect(),
                stdout: slt.into(),
                ..FakeCase::default()
            },
            FakeCase {
                args_prefix: SEVEN_ZIP_SUMMARY_ARGS.iter().map(|a| (*a).into()).collect(),
                stdout: l.into(),
                ..FakeCase::default()
            },
            FakeCase {
                stdout: format!("\n{SEVEN_ZIP_BANNER}\n"),
                ..FakeCase::default()
            },
        ],
    }
}

/// A fake `unrar` that prints `lt` to the record run, `l` to the totals
/// run and the banner otherwise.
pub fn unrar_listing_script(lt: &str, l: &str) -> FakeScript {
    FakeScript {
        cases: vec![
            FakeCase {
                args_prefix: UNRAR_RECORD_ARGS.iter().map(|a| (*a).into()).collect(),
                stdout: lt.into(),
                ..FakeCase::default()
            },
            FakeCase {
                args_prefix: UNRAR_SUMMARY_ARGS.iter().map(|a| (*a).into()).collect(),
                stdout: l.into(),
                ..FakeCase::default()
            },
            FakeCase {
                stdout: format!("\n{UNRAR_BANNER}\n"),
                ..FakeCase::default()
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// External reads by name, by stream and through bsdtar (plan Task 15, S8)
// ---------------------------------------------------------------------------

/// The switches of a 7-Zip read of one named entry, before `--`.
pub const SEVEN_ZIP_NAME_ARGS: &[&str] = &["e", "-so", "-spd", "-r-", "-p-"];
/// The switches of a 7-Zip read of every file in archive order.
pub const SEVEN_ZIP_STREAM_ARGS: &[&str] = &["e", "-so", "-spd", "-p-"];
/// The switches of an unrar read, by name or (with no name) every file.
pub const UNRAR_READ_ARGS: &[&str] = &["p", "-inul", "-cfg-", "-p-"];
/// The switches of bsdtar's pax transcode, before `--`.
pub const BSDTAR_TRANSCODE_ARGS: &[&str] = &["-c", "--format=pax", "-f", "-"];

/// The full arguments of one read: `switches`, `--`, the archive, then
/// `name` when the read is by name. A fake case with them as its prefix
/// answers exactly that read.
pub fn read_args(switches: &[&str], archive: &std::path::Path, name: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = switches.iter().map(|a| (*a).to_string()).collect();
    out.push("--".into());
    out.push(archive.to_string_lossy().into_owned());
    out.extend(name.map(str::to_string));
    out
}

/// The full arguments of bsdtar's pax transcode of `archive`.
pub fn transcode_args(archive: &std::path::Path) -> Vec<String> {
    let mut out: Vec<String> = BSDTAR_TRANSCODE_ARGS
        .iter()
        .map(|a| (*a).to_string())
        .collect();
    out.push("--".into());
    out.push(format!("@{}", archive.to_string_lossy()));
    out
}

/// A case that answers `args` with `stdout` and `exit`.
pub fn answer(args: Vec<String>, stdout: &[u8], exit: i32) -> FakeCase {
    FakeCase {
        args_prefix: args,
        stdout: String::from_utf8(stdout.to_vec()).expect("text output; use stdout_file"),
        exit,
        ..FakeCase::default()
    }
}

/// The listing shape of the real PS1 case of spec §1 (`Next Tetris,
/// The.zip`, a 7z archive): one folder, eleven tracks in their own 7z
/// blocks, and the sheet last. Names, sizes and CRCs as 7-Zip lists them.
pub fn next_tetris_ps1_listing() -> Vec<ToolEntry> {
    const DIR: &str = "Next Tetris, The (USA)";
    const TRACKS: [(u64, u64, u32); 11] = [
        (34_508_544, 17_811_251, 0x8B6F_8183),
        (31_592_064, 28_953_335, 0x7A0A_30B7),
        (22_391_040, 17_850_413, 0x3D51_CA5D),
        (20_227_200, 17_448_776, 0x216A_95DD),
        (26_561_136, 20_147_572, 0xA5F7_EB7E),
        (42_455_952, 38_707_993, 0x19A5_3BD7),
        (42_220_752, 37_996_260, 0xCDA9_E912),
        (58_868_208, 56_582_490, 0x8E37_50D6),
        (42_437_136, 40_475_740, 0x3D25_596B),
        (39_560_640, 36_366_328, 0x06D6_3F64),
        (44_925_552, 42_940_838, 0x495C_5A02),
    ];
    let mut out = vec![ToolEntry::dir(&format!("{DIR}/"))];
    for (i, (size, packed, crc)) in TRACKS.into_iter().enumerate() {
        let mut e = ToolEntry::file(&format!("{DIR}/{DIR} (Track {:02}).bin", i + 1), size, crc);
        e.packed = Some(packed);
        e.block = u32::try_from(i).ok();
        out.push(e);
    }
    let mut cue = ToolEntry::file(&format!("{DIR}/{DIR}.cue"), 1269, 0xC164_24E9);
    cue.packed = Some(159);
    cue.block = Some(11);
    out.push(cue);
    out
}

// ---------------------------------------------------------------------------
// Mutation engine (plan Task 17, S1 across every format).
// ---------------------------------------------------------------------------

/// How a length field found by [`length_fields`] is encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldWidth {
    Le16,
    Le32,
    Le64,
    /// A tar size field: 11 octal digits and a NUL.
    Octal12,
}

impl FieldWidth {
    /// The field's size in bytes.
    pub fn size(self) -> usize {
        match self {
            FieldWidth::Le16 => 2,
            FieldWidth::Le32 => 4,
            FieldWidth::Le64 => 8,
            FieldWidth::Octal12 => 12,
        }
    }
}

const SEVENZ_MAGIC: [u8; 6] = [b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C];

/// Offsets of the length, count and offset fields a hostile archive would
/// rewrite: zip local and central headers and the end record, the 7z
/// start header, every ustar header's size, the gzip ISIZE trailer and
/// the xz stream footer's backward size. Every field lies inside `bytes`.
pub fn length_fields(bytes: &[u8]) -> Vec<(usize, FieldWidth)> {
    use FieldWidth::{Le16, Le32, Le64, Octal12};
    let mut out = Vec::new();
    let push = |at: usize, w: FieldWidth, out: &mut Vec<(usize, FieldWidth)>| {
        if at
            .checked_add(w.size())
            .is_some_and(|end| end <= bytes.len())
        {
            out.push((at, w));
        }
    };
    if bytes.starts_with(&SEVENZ_MAGIC) {
        push(12, Le64, &mut out);
        push(20, Le64, &mut out);
        return out;
    }
    if bytes.starts_with(&[0x1F, 0x8B]) && bytes.len() >= 18 {
        push(bytes.len() - 4, Le32, &mut out);
        return out;
    }
    if bytes.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0]) && bytes.len() >= 24 {
        push(bytes.len() - 8, Le32, &mut out);
        return out;
    }
    if bytes.get(257..262) == Some(b"ustar") {
        let mut at = 0;
        while at + 512 <= bytes.len() && bytes.get(at + 257..at + 262) == Some(b"ustar") {
            push(at + 124, Octal12, &mut out);
            let size = std::str::from_utf8(&bytes[at + 124..at + 135])
                .ok()
                .and_then(|s| u64::from_str_radix(s.trim_matches(['\0', ' ']), 8).ok())
                .unwrap_or(0);
            let data = usize::try_from(size.div_ceil(512) * 512).unwrap_or(usize::MAX);
            at = at.saturating_add(512).saturating_add(data);
        }
        return out;
    }
    let mut i = 0;
    while i + 4 <= bytes.len() {
        match &bytes[i..i + 4] {
            b"PK\x03\x04" => {
                for (off, w) in [(18, Le32), (22, Le32), (26, Le16), (28, Le16)] {
                    push(i + off, w, &mut out);
                }
            }
            b"PK\x01\x02" => {
                for (off, w) in [
                    (20, Le32),
                    (24, Le32),
                    (28, Le16),
                    (30, Le16),
                    (32, Le16),
                    (42, Le32),
                ] {
                    push(i + off, w, &mut out);
                }
            }
            b"PK\x05\x06" => {
                for (off, w) in [(8, Le16), (10, Le16), (12, Le32), (16, Le32), (20, Le16)] {
                    push(i + off, w, &mut out);
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// A xorshift64* generator; deterministic per seed, never stuck at zero.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // splitmix64 of the seed, so nearby seeds diverge at once.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Rng((z ^ (z >> 31)) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

/// A value a hostile archive writes into a length field: zero, one, the
/// maximum, off by one from the original, the input length, or random.
fn hostile_value(rng: &mut Rng, original: u64, max: u64, input_len: usize) -> u64 {
    let v = match rng.below(8) {
        0 => 0,
        1 => 1,
        2 => max,
        3 => max - 1,
        4 => original.wrapping_add(1),
        5 => original.wrapping_sub(1),
        6 => input_len as u64,
        _ => rng.next(),
    };
    v & max
}

fn rewrite_field(rng: &mut Rng, bytes: &mut [u8], at: usize, w: FieldWidth) {
    let input_len = bytes.len();
    let field = &mut bytes[at..at + w.size()];
    match w {
        FieldWidth::Octal12 => {
            let original = std::str::from_utf8(&field[..11])
                .ok()
                .and_then(|s| u64::from_str_radix(s.trim_matches(['\0', ' ']), 8).ok())
                .unwrap_or(0);
            let v = hostile_value(rng, original, 0o777_7777_7777, input_len);
            let text = format!("{v:011o}\0");
            field.copy_from_slice(&text.as_bytes()[..12]);
        }
        _ => {
            let n = w.size();
            let mut le = [0u8; 8];
            le[..n].copy_from_slice(field);
            let original = u64::from_le_bytes(le);
            let max = if n == 8 {
                u64::MAX
            } else {
                (1u64 << (8 * n)) - 1
            };
            let v = hostile_value(rng, original, max, input_len);
            field.copy_from_slice(&v.to_le_bytes()[..n]);
        }
    }
}

/// Re-seals a 7z start header (its CRC, and the next header's CRC when the
/// next header lies inside the file), so a mutation reaches the header
/// walker instead of stopping at the start header's CRC.
fn reseal_sevenz(bytes: &mut [u8]) {
    if bytes.len() < 32 || !bytes.starts_with(&SEVENZ_MAGIC) {
        return;
    }
    let field = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().expect("8 bytes"));
    let (off, size) = (field(12), field(20));
    let span = 32u64
        .checked_add(off)
        .and_then(|start| start.checked_add(size).map(|end| (start, end)));
    if let Some((start, end)) = span {
        if let (Ok(start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) {
            if end <= bytes.len() {
                let crc = crc32fast::hash(&bytes[start..end]);
                bytes[28..32].copy_from_slice(&crc.to_le_bytes());
            }
        }
    }
    let crc = crc32fast::hash(&bytes[12..32]);
    bytes[8..12].copy_from_slice(&crc.to_le_bytes());
}

/// `n` seeded mutations of `bytes` (plan Task 17), each different from
/// `bytes`: bit flips, truncations, rewrites of the length fields
/// [`length_fields`] finds (a random 4-byte window when it finds none),
/// and runs of random bytes. 7z mutations that keep the start header
/// re-seal its CRCs half of the time. The same seed gives the same
/// mutations; empty input gives none.
pub fn mutate(seed: u64, bytes: &[u8], n: usize) -> impl Iterator<Item = Vec<u8>> {
    let base = bytes.to_vec();
    let fields = length_fields(&base);
    let mut rng = Rng::new(seed);
    let count = if base.is_empty() { 0 } else { n };
    (0..count).map(move |round| loop {
        let mut m = base.clone();
        match round % 4 {
            0 => {
                for _ in 0..1 + rng.below(4) {
                    let at = rng.below(m.len());
                    m[at] ^= 1 << rng.below(8);
                }
            }
            1 => {
                m.truncate(rng.below(m.len()));
            }
            2 => {
                if fields.is_empty() {
                    let at = rng.below(m.len().saturating_sub(4).max(1));
                    let end = (at + 4).min(m.len());
                    let v = rng.next().to_le_bytes();
                    m[at..end].copy_from_slice(&v[..end - at]);
                } else {
                    for _ in 0..1 + rng.below(2) {
                        let (at, w) = fields[rng.below(fields.len())];
                        rewrite_field(&mut rng, &mut m, at, w);
                    }
                }
            }
            _ => {
                let at = rng.below(m.len());
                let run = (1 + rng.below(16)).min(m.len() - at);
                for b in &mut m[at..at + run] {
                    *b = rng.next() as u8;
                }
            }
        }
        if rng.next() & 1 == 0 {
            reseal_sevenz(&mut m);
        }
        if m != base {
            return m;
        }
    })
}
