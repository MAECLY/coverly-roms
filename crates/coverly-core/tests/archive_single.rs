//! Single-file compressors of the archive reader (spec §5.1, §9.1). Fixtures
//! are synthetic.

use std::io::Read;

use ruzstd::decoding::{FrameDecoder, StreamingDecoder};
use ruzstd::encoding::{compress_to_vec, CompressionLevel};

/// Decode one zstd frame and return the (stored, calculated) content checksums.
fn zstd_checksums(frame: &[u8]) -> (Option<u32>, Option<u32>) {
    let mut decoder = StreamingDecoder::new(frame).expect("frame header");
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).expect("decodes");
    let frame: FrameDecoder = decoder.into_frame_decoder();
    (
        frame.get_checksum_from_data(),
        frame.get_calculated_checksum(),
    )
}

/// Spec §9.1 checks the zstd content checksum when a frame has one. `ruzstd`
/// only computes it with its `hash` feature: without it this test does not
/// compile, and the dev encoder writes frames without a checksum.
#[test]
fn ruzstd_verifies_content_checksum() {
    let data = b"Coverly synthetic payload ".repeat(64);
    let mut frame = compress_to_vec(&data[..], CompressionLevel::Fastest);
    // Frame header descriptor: bit 2 is Content_Checksum_flag.
    assert_ne!(frame[4] & 0b100, 0, "encoder wrote no content checksum");

    let (stored, calculated) = zstd_checksums(&frame);
    assert!(stored.is_some());
    assert_eq!(stored, calculated);

    let last = frame.len() - 1;
    frame[last] ^= 0xFF;
    let (stored, calculated) = zstd_checksums(&frame);
    assert_ne!(stored, calculated, "a flipped checksum byte must be seen");
}
