//! zstd decoded frame by frame, each frame's window checked against the
//! decoder memory cap before its decoder exists (spec
//! `2026-10-23-organize-archives-and-discs.md` §4, S2).
//!
//! A zstd stream is a sequence of frames, and every frame declares its own
//! window: a small first frame proves nothing about the second one. So
//! [`ZstdFrames`] parses each frame header itself ([`read_frame_start`]),
//! checks [`ZstdFrameHeader::mem_needed`] against the cap (and takes it from
//! the memory gate, when one is set), and only then builds a `ruzstd`
//! decoder for that one frame. With `Single_Segment` the window **is** the
//! declared frame content size, so a huge declared size is refused like a
//! huge dictionary, and the frame's output is bounded by it. Skippable
//! frames are drained through a small buffer, never allocated. The content
//! checksum (when the frame has one) and the declared content size are
//! verified at the end of every frame.
//!
//! Used by the zip backend (method 93) and by the single-file `.zst`
//! reader. Callers wrap the reader in `GuardedRead`, `ChargedRead` and
//! `VerifyingReader` as for every other decoder.

use std::io::{self, Chain, Cursor, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ruzstd::decoding::{FrameDecoder, StreamingDecoder};

use super::budget::{GateWait, MemoryGate, Permit};
use super::guard::guarded;
use super::lzma::{check, Tracked};
use super::{ArchiveError, KIB};

/// Magic number of a zstd frame (little-endian on disk).
pub const ZSTD_MAGIC: u32 = 0xFD2F_B528;
/// Skippable frames use magic numbers `0x184D2A50..=0x184D2A5F`.
const SKIPPABLE_MAGIC: std::ops::RangeInclusive<u32> = 0x184D_2A50..=0x184D_2A5F;
/// Decoder state besides the window: the block buffer (128 KiB), the
/// literals buffer, the Huffman/FSE tables and the sequence scratch.
/// Generous on purpose: the check must never be below what is allocated.
pub const FIXED_STATE: u64 = 512 * KIB;
/// Smallest window a non-single-segment frame may declare (RFC 8878).
const MIN_WINDOW: u64 = 1 << 10;

/// A parsed zstd frame header (RFC 8878 §3.1.1.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZstdFrameHeader {
    /// Window size in bytes; with `single_segment` it is the content size.
    pub window: u64,
    pub single_segment: bool,
    /// Declared decompressed size of the frame, when present.
    pub content_size: Option<u64>,
    /// Whether a 4-byte content checksum follows the last block.
    pub checksum: bool,
    /// Dictionary id; `None` when absent or 0.
    pub dict_id: Option<u32>,
}

impl ZstdFrameHeader {
    /// Decoder memory for this frame: the window plus the fixed state.
    pub fn mem_needed(&self) -> u64 {
        self.window.saturating_add(FIXED_STATE)
    }
}

/// What starts at the current position of a zstd stream.
#[derive(Debug)]
pub enum FrameStart {
    /// Clean end of the stream (no byte left).
    End,
    /// A skippable frame of this many bytes (after its 8-byte header).
    Skippable(u32),
    /// A zstd frame: its parsed header and the header's exact bytes.
    Frame(ZstdFrameHeader, Vec<u8>),
}

/// Parses a zstd frame header from `bytes` (magic first). `bytes` may be
/// longer than the header; the header's length is returned with it.
pub fn parse_frame_header(bytes: &[u8]) -> Result<(ZstdFrameHeader, usize), ArchiveError> {
    let magic = bytes
        .get(..4)
        .map(|m| u32::from_le_bytes([m[0], m[1], m[2], m[3]]))
        .ok_or_else(|| damaged("zstd frame header truncated"))?;
    if magic != ZSTD_MAGIC {
        return Err(damaged("zstd magic"));
    }
    let desc = *bytes
        .get(4)
        .ok_or_else(|| damaged("zstd frame header truncated"))?;
    let len = 5 + header_rest_len(desc)?;
    let header = bytes
        .get(..len)
        .ok_or_else(|| damaged("zstd frame header truncated"))?;
    Ok((decode_header(desc, &header[5..]), len))
}

/// Reads the next frame start from `src`. Errors of `src` stay I/O; a
/// short or invalid header is `Damaged`.
pub(super) fn read_frame_start<R: Read>(src: &mut Tracked<R>) -> Result<FrameStart, ArchiveError> {
    let mut magic = [0u8; 4];
    match src.read_full(&mut magic)? {
        0 => return Ok(FrameStart::End),
        4 => {}
        _ => return Err(damaged("zstd frame header truncated")),
    }
    let magic_num = u32::from_le_bytes(magic);
    if SKIPPABLE_MAGIC.contains(&magic_num) {
        let mut size = [0u8; 4];
        if src.read_full(&mut size)? != 4 {
            return Err(damaged("zstd skippable frame truncated"));
        }
        return Ok(FrameStart::Skippable(u32::from_le_bytes(size)));
    }
    if magic_num != ZSTD_MAGIC {
        return Err(damaged("zstd magic"));
    }
    let mut desc = [0u8; 1];
    if src.read_full(&mut desc)? != 1 {
        return Err(damaged("zstd frame header truncated"));
    }
    let rest_len = header_rest_len(desc[0])?;
    let mut bytes = Vec::with_capacity(5 + rest_len);
    bytes.extend_from_slice(&magic);
    bytes.push(desc[0]);
    bytes.resize(5 + rest_len, 0);
    if src.read_full(&mut bytes[5..])? != rest_len {
        return Err(damaged("zstd frame header truncated"));
    }
    let header = decode_header(desc[0], &bytes[5..]);
    Ok(FrameStart::Frame(header, bytes))
}

/// Bytes after the descriptor: window descriptor, dictionary id and frame
/// content size, as the descriptor says. A reserved bit set is `Damaged`.
fn header_rest_len(desc: u8) -> Result<usize, ArchiveError> {
    if desc & 0x08 != 0 {
        return Err(damaged("zstd reserved descriptor bit"));
    }
    let single = desc & 0x20 != 0;
    let window = usize::from(!single);
    let dict = [0, 1, 2, 4][usize::from(desc & 0x03)];
    let fcs = fcs_len(desc);
    Ok(window + dict + fcs)
}

fn fcs_len(desc: u8) -> usize {
    match desc >> 6 {
        0 => usize::from(desc & 0x20 != 0),
        1 => 2,
        2 => 4,
        _ => 8,
    }
}

/// Decodes the fields after the descriptor (`rest` has exactly
/// [`header_rest_len`] bytes).
fn decode_header(desc: u8, rest: &[u8]) -> ZstdFrameHeader {
    let single_segment = desc & 0x20 != 0;
    let mut pos = 0;
    let mut window = 0;
    if !single_segment {
        let wd = rest[0];
        pos = 1;
        let base = 1u64 << (10 + u32::from(wd >> 3));
        window = base + (base / 8) * u64::from(wd & 0x07);
    }
    let dict_len = [0, 1, 2, 4][usize::from(desc & 0x03)];
    let dict = le(&rest[pos..pos + dict_len]);
    pos += dict_len;
    let fcs_bytes = fcs_len(desc);
    let content_size = (fcs_bytes > 0).then(|| {
        let v = le(&rest[pos..pos + fcs_bytes]);
        if fcs_bytes == 2 {
            v + 256
        } else {
            v
        }
    });
    if single_segment {
        window = content_size.unwrap_or(0);
    }
    ZstdFrameHeader {
        window,
        single_segment,
        content_size,
        checksum: desc & 0x04 != 0,
        dict_id: u32::try_from(dict).ok().filter(|&d| d != 0),
    }
}

fn le(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .rev()
        .fold(0u64, |acc, &b| (acc << 8) | u64::from(b))
}

/// The window of the first zstd frame, after skipping skippable frames,
/// checked against `cap` (S2). What they skip comes out of `allowance`,
/// one for the whole listing (`max_header`): zip entries may share one
/// frame, so a per-entry limit would let 10,000 of them read 16 MiB each.
/// Used by the zip backend (method 93) and the single-file `.zst` reader.
pub(super) fn read_zstd_first_window(
    raw: &mut impl Read,
    cap: u64,
    allowance: &mut u64,
) -> Result<u64, ArchiveError> {
    let mut src = Tracked::new(raw);
    loop {
        match read_frame_start(&mut src)? {
            FrameStart::End => return Err(damaged("zstd stream without a frame")),
            FrameStart::Skippable(len) => {
                *allowance = allowance
                    .checked_sub(u64::from(len))
                    .ok_or_else(|| damaged("zstd skippable frames over the limit"))?;
                let drained = io::copy(&mut (&mut src).take(u64::from(len)), &mut io::sink())?;
                if drained != u64::from(len) {
                    return Err(damaged("zstd skippable frame truncated"));
                }
            }
            FrameStart::Frame(header, _) => {
                check(header.mem_needed(), cap)?;
                return Ok(header.window);
            }
        }
    }
}

/// The decoder of one frame and what is verified at its end. Fields drop
/// in order: the decoder before its gate permit.
struct ActiveFrame<R: Read> {
    dec: StreamingDecoder<Chain<Cursor<Vec<u8>>, Tracked<R>>, FrameDecoder>,
    header: ZstdFrameHeader,
    produced: u64,
    _permit: Option<Permit>,
}

/// A zstd decoder that walks the frames itself (S2): every frame header is
/// parsed and its window checked against the cap (and taken from the
/// memory gate, when one is set) before that frame's decoder is built.
pub struct ZstdFrames<R: Read> {
    src: Option<Tracked<R>>,
    src_failed: Arc<AtomicBool>,
    frame: Option<ActiveFrame<R>>,
    cap: u64,
    gate: Option<(&'static MemoryGate, GateWait)>,
    skippable_left: u64,
    frames: u64,
    finished: bool,
    failed: bool,
}

impl<R: Read> ZstdFrames<R> {
    /// A frame walker over `src` (positioned at the first frame) whose
    /// frame decoders may need at most `cap` bytes each.
    pub fn new(src: R, cap: u64) -> Self {
        let src = Tracked::new(src);
        Self {
            src_failed: src.flag(),
            src: Some(src),
            frame: None,
            cap,
            gate: None,
            skippable_left: u64::MAX,
            frames: 0,
            finished: false,
            failed: false,
        }
    }

    /// Takes each frame's memory from `gate` while that frame decodes; the
    /// wait ends early on `wait`'s cancel flag or deadline. The caller must
    /// not hold another permit for this same stream.
    pub fn with_gate(mut self, gate: &'static MemoryGate, wait: GateWait) -> Self {
        self.gate = Some((gate, wait));
        self
    }

    /// Caps the bytes of skippable frames drained in total (they are read
    /// through a small buffer, so this bounds time, not memory).
    pub fn with_skippable_limit(mut self, limit: u64) -> Self {
        self.skippable_left = limit;
        self
    }

    /// Frames fully decoded and verified so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    fn step(&mut self, buf: &mut [u8]) -> Result<usize, ArchiveError> {
        loop {
            if self.finished {
                return Ok(0);
            }
            if let Some(frame) = self.frame.as_mut() {
                let n = frame
                    .dec
                    .read(buf)
                    .map_err(|e| decode_error(e, &self.src_failed))?;
                if n > 0 {
                    frame.produced += n as u64;
                    if frame
                        .header
                        .content_size
                        .is_some_and(|c| frame.produced > c)
                    {
                        return Err(damaged("zstd frame longer than its content size"));
                    }
                    return Ok(n);
                }
                self.end_frame()?;
                continue;
            }
            let src = self
                .src
                .as_mut()
                .ok_or_else(|| damaged("zstd stream already failed"))?;
            match read_frame_start(src)? {
                FrameStart::End => {
                    if self.frames == 0 {
                        return Err(damaged("zstd stream without a frame"));
                    }
                    self.finished = true;
                    return Ok(0);
                }
                FrameStart::Skippable(len) => {
                    let len = u64::from(len);
                    if len > self.skippable_left {
                        return Err(damaged("zstd skippable frames over the limit"));
                    }
                    self.skippable_left -= len;
                    let drained = io::copy(&mut src.take(len), &mut io::sink())?;
                    if drained != len {
                        return Err(damaged("zstd skippable frame truncated"));
                    }
                }
                FrameStart::Frame(header, bytes) => self.begin_frame(header, bytes)?,
            }
        }
    }

    fn begin_frame(&mut self, header: ZstdFrameHeader, bytes: Vec<u8>) -> Result<(), ArchiveError> {
        if header.dict_id.is_some() {
            return Err(ArchiveError::Unsupported {
                format: "zstd dictionary".into(),
                need: None,
            });
        }
        if !header.single_segment && header.window < MIN_WINDOW {
            return Err(damaged("zstd window too small"));
        }
        let needed = header.mem_needed();
        check(needed, self.cap)?;
        let permit = match &self.gate {
            Some((gate, wait)) => Some(gate.acquire_until(needed, wait)?),
            None => None,
        };
        let src = self
            .src
            .take()
            .ok_or_else(|| damaged("zstd stream already failed"))?;
        let mut decoder = FrameDecoder::new();
        // A second line behind the check above: the crate refuses a window
        // over the cap before allocating it.
        decoder.set_max_window_size(self.cap);
        let dec = StreamingDecoder::new_with_decoder(Cursor::new(bytes).chain(src), decoder)
            .map_err(|e| {
                decode_error(
                    io::Error::new(io::ErrorKind::InvalidData, e),
                    &self.src_failed,
                )
            })?;
        self.frame = Some(ActiveFrame {
            dec,
            header,
            produced: 0,
            _permit: permit,
        });
        Ok(())
    }

    /// Verifies a frame whose decoder reported its end and gives the source
    /// back for the next frame. The frame's gate permit is released here.
    fn end_frame(&mut self) -> Result<(), ArchiveError> {
        let frame = self
            .frame
            .take()
            .ok_or_else(|| damaged("zstd stream already failed"))?;
        let decoder = &frame.dec.decoder;
        if !decoder.is_finished() {
            return Err(damaged("zstd frame truncated"));
        }
        if frame.header.checksum {
            let stored = decoder.get_checksum_from_data();
            if stored.is_none() || stored != decoder.get_calculated_checksum() {
                return Err(damaged("zstd checksum"));
            }
        }
        if frame
            .header
            .content_size
            .is_some_and(|c| c != frame.produced)
        {
            return Err(damaged("zstd content size"));
        }
        let (_, src) = frame.dec.into_inner().into_inner();
        self.src = Some(src);
        self.frames += 1;
        Ok(())
    }
}

impl<R: Read> Read for ZstdFrames<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.failed {
            return Err(damaged("zstd stream already failed").into());
        }
        if buf.is_empty() {
            return Ok(0);
        }
        // S1: the frame parser and the crate's decoder run inside the guard.
        guarded(|| self.step(buf)).map_err(|e| {
            self.failed = true;
            // A dropped frame gives its gate permit back at once.
            self.frame = None;
            e.into()
        })
    }
}

/// An error out of the frame decoder: one that carries an `ArchiveError`
/// stays itself, one after a failed source read stays I/O, anything else is
/// the decoder rejecting the data.
fn decode_error(e: io::Error, src_failed: &AtomicBool) -> ArchiveError {
    if e.get_ref().is_some_and(|inner| inner.is::<ArchiveError>()) {
        return ArchiveError::from(e);
    }
    if src_failed.load(Ordering::Relaxed) {
        return ArchiveError::Io(e);
    }
    damaged(format!("zstd: {e}"))
}

fn damaged(detail: impl Into<String>) -> ArchiveError {
    ArchiveError::Damaged {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::{DECODER_MEM_CAP, MIB};

    fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 13 % 251) as u8).collect()
    }

    fn compress(data: &[u8]) -> Vec<u8> {
        ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Fastest)
    }

    fn read_all(r: &mut dyn Read) -> (Vec<u8>, Option<ArchiveError>) {
        let mut out = Vec::new();
        let err = r.read_to_end(&mut out).err().map(ArchiveError::from);
        (out, err)
    }

    /// A frame header declaring a window of `2^log` bytes, then garbage.
    fn huge_window_frame(log: u8) -> Vec<u8> {
        let mut f = ZSTD_MAGIC.to_le_bytes().to_vec();
        f.push(0x00); // no content size, not single segment, no checksum
        f.push((log - 10) << 3);
        f.extend_from_slice(&[0xAB; 32]);
        f
    }

    #[test]
    fn zstd_round_trip_two_frames_with_skippable() {
        let a = sample(200_000);
        let b = sample(5_000);
        let mut stream = compress(&a);
        stream.extend_from_slice(&0x184D_2A53u32.to_le_bytes());
        stream.extend_from_slice(&3u32.to_le_bytes());
        stream.extend_from_slice(b"xyz");
        stream.extend_from_slice(&compress(&b));
        let gate = MemoryGate::leaked(DECODER_MEM_CAP);
        let mut r = ZstdFrames::new(stream.as_slice(), DECODER_MEM_CAP)
            .with_gate(gate, GateWait::default());
        let (out, err) = read_all(&mut r);
        assert!(err.is_none(), "{err:?}");
        assert_eq!(out.len(), a.len() + b.len());
        assert_eq!(&out[..a.len()], a.as_slice());
        assert_eq!(&out[a.len()..], b.as_slice());
        assert_eq!(r.frames(), 2);
        assert_eq!(gate.in_use(), 0, "frame permits are released");
    }

    #[test]
    fn zstd_window_over_cap_refused_before_decoding() {
        let stream = huge_window_frame(30);
        let mut r = ZstdFrames::new(stream.as_slice(), DECODER_MEM_CAP);
        let (_, err) = read_all(&mut r);
        assert!(
            matches!(err, Some(ArchiveError::NeedsTooMuchMemory { needed, cap })
                if needed == (1 << 30) + FIXED_STATE && cap == DECODER_MEM_CAP),
            "{err:?}"
        );
    }

    #[test]
    fn zstd_second_frame_huge_window_refused() {
        let mut stream = compress(&sample(1000));
        stream.extend_from_slice(&huge_window_frame(31));
        let mut r = ZstdFrames::new(stream.as_slice(), DECODER_MEM_CAP);
        let (out, err) = read_all(&mut r);
        assert_eq!(out, sample(1000), "the first frame decodes");
        assert!(
            matches!(err, Some(ArchiveError::NeedsTooMuchMemory { .. })),
            "{err:?}"
        );
    }

    #[test]
    fn zstd_single_segment_huge_content_size_refused() {
        let mut f = ZSTD_MAGIC.to_le_bytes().to_vec();
        f.push(0xE0); // 8-byte content size, single segment
        f.extend_from_slice(&(4u64 << 30).to_le_bytes());
        f.extend_from_slice(&[0; 16]);
        let (header, len) = parse_frame_header(&f).unwrap();
        assert_eq!(len, 13);
        assert!(header.single_segment);
        assert_eq!(header.window, 4 << 30);
        let mut r = ZstdFrames::new(f.as_slice(), DECODER_MEM_CAP);
        let (_, err) = read_all(&mut r);
        assert!(
            matches!(err, Some(ArchiveError::NeedsTooMuchMemory { .. })),
            "{err:?}"
        );
    }

    #[test]
    fn zstd_gate_over_its_cap_refused() {
        let stream = compress(&sample(1000));
        let gate = MemoryGate::leaked(64 * KIB);
        let mut r = ZstdFrames::new(stream.as_slice(), DECODER_MEM_CAP)
            .with_gate(gate, GateWait::default());
        let (_, err) = read_all(&mut r);
        assert!(
            matches!(err, Some(ArchiveError::NeedsTooMuchMemory { cap, .. }) if cap == 64 * KIB),
            "{err:?}"
        );
    }

    #[test]
    fn zstd_header_fields_parsed() {
        let data = sample(70_000);
        let stream = compress(&data);
        let (header, _) = parse_frame_header(&stream).unwrap();
        assert!(header.window >= MIN_WINDOW);
        assert!(header.mem_needed() <= 16 * MIB, "{header:?}");
        assert_eq!(header.dict_id, None);
    }

    #[test]
    fn zstd_reserved_bit_damaged() {
        let mut f = ZSTD_MAGIC.to_le_bytes().to_vec();
        f.push(0x08);
        f.push(0x00);
        assert!(matches!(
            parse_frame_header(&f),
            Err(ArchiveError::Damaged { .. })
        ));
    }

    #[test]
    fn zstd_dictionary_frame_unsupported() {
        let mut f = ZSTD_MAGIC.to_le_bytes().to_vec();
        f.push(0x01); // 1-byte dictionary id
        f.push(0x00); // window 1 KiB
        f.push(7);
        f.extend_from_slice(&[0; 8]);
        let mut r = ZstdFrames::new(f.as_slice(), DECODER_MEM_CAP);
        let (_, err) = read_all(&mut r);
        assert!(
            matches!(err, Some(ArchiveError::Unsupported { .. })),
            "{err:?}"
        );
    }

    #[test]
    fn zstd_corrupt_checksum_damaged() {
        let data = sample(10_000);
        let mut stream = compress(&data);
        let (header, _) = parse_frame_header(&stream).unwrap();
        if !header.checksum {
            // The encoder wrote no checksum: nothing to corrupt here.
            return;
        }
        let last = stream.len() - 1;
        stream[last] ^= 0xFF;
        let mut r = ZstdFrames::new(stream.as_slice(), DECODER_MEM_CAP);
        let (_, err) = read_all(&mut r);
        assert!(matches!(err, Some(ArchiveError::Damaged { .. })), "{err:?}");
    }

    #[test]
    fn zstd_truncated_frame_damaged() {
        let data = sample(100_000);
        let stream = compress(&data);
        let cut = &stream[..stream.len() / 2];
        let mut r = ZstdFrames::new(cut, DECODER_MEM_CAP);
        let (_, err) = read_all(&mut r);
        assert!(matches!(err, Some(ArchiveError::Damaged { .. })), "{err:?}");
    }

    #[test]
    fn zstd_empty_stream_damaged() {
        let mut r = ZstdFrames::new(&[][..], DECODER_MEM_CAP);
        let (_, err) = read_all(&mut r);
        assert!(matches!(err, Some(ArchiveError::Damaged { .. })), "{err:?}");
    }
}
