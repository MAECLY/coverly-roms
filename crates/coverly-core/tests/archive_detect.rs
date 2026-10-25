//! Container detection by magic (spec `2026-10-23-organize-archives-and-discs.md`
//! §3.1): one positive and one near-miss per row of the magic table. Every
//! fixture is crafted here.

use coverly_core::archive::detect::{
    by_extension, detect_container, lzma_alone_plausible, multi_volume_part, tar_header_ok,
    VolumePart,
};
use coverly_core::archive::DECODER_MEM_CAP;
use coverly_core::identify::sniff::ContainerMagic as M;

fn detect(head: &[u8]) -> Option<M> {
    detect_container(head, head.len() as u64, None)
}

/// `head` padded with zeros to `len` bytes.
fn padded(head: &[u8], len: usize) -> Vec<u8> {
    let mut v = head.to_vec();
    v.resize(len, 0);
    v
}

// ---------------------------------------------------------------- zip

/// End-of-central-directory record.
fn eocd(
    disk: u16,
    cd_disk: u16,
    entries: u16,
    cd_size: u32,
    cd_offset: u32,
    comment: &[u8],
) -> Vec<u8> {
    let mut v = b"PK\x05\x06".to_vec();
    v.extend_from_slice(&disk.to_le_bytes());
    v.extend_from_slice(&cd_disk.to_le_bytes());
    v.extend_from_slice(&entries.to_le_bytes());
    v.extend_from_slice(&entries.to_le_bytes());
    v.extend_from_slice(&cd_size.to_le_bytes());
    v.extend_from_slice(&cd_offset.to_le_bytes());
    v.extend_from_slice(&u16::try_from(comment.len()).unwrap().to_le_bytes());
    v.extend_from_slice(comment);
    v
}

#[test]
fn zip_local_header() {
    assert_eq!(detect(&padded(b"PK\x03\x04\x14\x00", 64)), Some(M::Zip));
    // Near miss: `PK` followed by something else.
    assert_eq!(detect(&padded(b"PK\x03\x05\x14\x00", 64)), None);
}

#[test]
fn zip_empty_eocd() {
    let empty = eocd(0, 0, 0, 0, 0, b"");
    assert_eq!(detect(&empty), Some(M::Zip));
    assert_eq!(detect(b"PK\x05"), None, "truncated signature");
}

#[test]
fn zip_spanned_marker_is_multi_volume() {
    let got = detect(&padded(b"PK\x07\x08PK\x03\x04", 64));
    assert_eq!(got, Some(M::ZipSpanned));
    assert!(got.unwrap().is_multi_volume());
    assert!(!M::Zip.is_multi_volume());
    // An EOCD that says "disk 2 of a set" is a later or last part too.
    let mut file = vec![0xAAu8; 300];
    file.extend(eocd(2, 0, 1, 50, 100, b""));
    assert_eq!(
        detect_container(&file[..16], file.len() as u64, Some(&file)),
        Some(M::ZipSpanned)
    );
}

#[test]
fn zip_with_sfx_prefix_found_by_eocd() {
    // A 4 KiB "stub", a 100-byte central directory, then the EOCD with a
    // comment. Offsets are relative to the stub-less zip, as SFX tools write.
    let mut file = vec![0x4Du8; 4096];
    file[0] = b'M';
    file[1] = b'Z';
    let cd_offset = 2000u32;
    file.extend(vec![0u8; 100]);
    file.extend(eocd(0, 0, 3, 100, cd_offset, b"made by test"));
    let len = file.len() as u64;
    // The head alone (no PK magic at 0) is not a zip.
    assert_eq!(detect_container(&file[..512], len, None), None);
    // The tail is the last 64 KiB + 22 bytes, here the whole file.
    assert_eq!(
        detect_container(&file[..512], len, Some(&file)),
        Some(M::ZipEocdOnly)
    );
    // A tail shorter than the file, still ending at EOF.
    let tail = &file[file.len() - 200..];
    assert_eq!(
        detect_container(&file[..512], len, Some(tail)),
        Some(M::ZipEocdOnly)
    );
    // Near miss: the comment does not end at EOF (trailing junk).
    let mut junk = file.clone();
    junk.extend_from_slice(b"trailing");
    assert_eq!(
        detect_container(&junk[..512], junk.len() as u64, Some(&junk)),
        None
    );
}

#[test]
fn eocd_offsets_outside_file_rejected() {
    // Central directory claims 1 MiB in a 400-byte file.
    let mut file = vec![0x11u8; 400];
    file.extend(eocd(0, 0, 1, 1 << 20, 0, b""));
    assert_eq!(
        detect_container(&file[..16], file.len() as u64, Some(&file)),
        None
    );
    // Offset past the EOCD.
    let mut file = vec![0x11u8; 400];
    file.extend(eocd(0, 0, 1, 10, 5000, b""));
    assert_eq!(
        detect_container(&file[..16], file.len() as u64, Some(&file)),
        None
    );
    // A tail longer than the file is a caller error, never a zip.
    let file = eocd(0, 0, 0, 0, 0, b"");
    assert_eq!(detect_container(b"xx", 4, Some(&file)), None);
}

#[test]
fn zip64_eocd_needs_its_locator() {
    let mut file = vec![0x22u8; 300];
    let mut locator = b"PK\x06\x07".to_vec();
    locator.extend_from_slice(&[0u8; 16]);
    file.extend(&locator);
    file.extend(eocd(0, 0, 0xFFFF, u32::MAX, u32::MAX, b""));
    assert_eq!(
        detect_container(&file[..16], file.len() as u64, Some(&file)),
        Some(M::ZipEocdOnly)
    );
    let mut file = vec![0x22u8; 320];
    file.extend(eocd(0, 0, 0xFFFF, u32::MAX, u32::MAX, b""));
    assert_eq!(
        detect_container(&file[..16], file.len() as u64, Some(&file)),
        None,
        "zip64 sentinels without a locator"
    );
}

// ---------------------------------------------------------------- 7z

const SEVENZ: [u8; 6] = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];

#[test]
fn sevenz_magic() {
    let mut h = SEVENZ.to_vec();
    h.extend_from_slice(&[0, 4]);
    assert_eq!(detect(&padded(&h, 32)), Some(M::SevenZip));
}

#[test]
fn sevenz_major_version_nonzero_rejected() {
    let mut h = SEVENZ.to_vec();
    h.extend_from_slice(&[1, 4]);
    assert_eq!(detect(&padded(&h, 32)), None);
    assert_eq!(detect(&SEVENZ), None, "no version bytes");
}

// ---------------------------------------------------------------- rar

#[test]
fn rar4_magic() {
    assert_eq!(detect(&padded(b"Rar!\x1a\x07\x00", 20)), Some(M::Rar4));
    assert_eq!(detect(&padded(b"Rar!\x1a\x08\x00", 20)), None);
}

#[test]
fn rar5_magic() {
    assert_eq!(detect(&padded(b"Rar!\x1a\x07\x01\x00", 20)), Some(M::Rar5));
    assert_eq!(
        detect(&padded(b"Rar!\x1a\x07\x02\x00", 20)),
        None,
        "unknown RAR version byte"
    );
}

// ---------------------------------------------------------------- gzip

#[test]
fn gzip_magic() {
    assert_eq!(
        detect(&padded(&[0x1F, 0x8B, 0x08, 0x08, 0, 0, 0, 0, 0, 3], 32)),
        Some(M::Gzip)
    );
    assert_eq!(
        detect(&padded(&[0x1F, 0x8B, 0x07, 0x00], 32)),
        None,
        "method other than deflate"
    );
}

#[test]
fn gzip_reserved_flags_rejected() {
    for bit in 5..8 {
        let flg = 1u8 << bit;
        assert_eq!(
            detect(&padded(&[0x1F, 0x8B, 0x08, flg], 32)),
            None,
            "reserved FLG bit {bit}"
        );
    }
}

// ---------------------------------------------------------------- bzip2

#[test]
fn bzip2_magic_with_block_magic() {
    for level in b'1'..=b'9' {
        let mut h = vec![b'B', b'Z', b'h', level];
        h.extend_from_slice(&[0x31, 0x41, 0x59, 0x26, 0x53, 0x59]);
        assert_eq!(detect(&padded(&h, 32)), Some(M::Bzip2));
    }
    let mut h = b"BZh0".to_vec();
    h.extend_from_slice(&[0x31, 0x41, 0x59, 0x26, 0x53, 0x59]);
    assert_eq!(detect(&padded(&h, 32)), None, "level 0 does not exist");
}

#[test]
fn bzip2_text_bzh9_rejected() {
    assert_eq!(detect(b"BZh9 is how this note starts"), None);
}

#[test]
fn bzip2_empty_stream() {
    let mut h = b"BZh9".to_vec();
    h.extend_from_slice(&[0x17, 0x72, 0x45, 0x38, 0x50, 0x90, 0, 0, 0, 0]);
    assert_eq!(detect(&h), Some(M::Bzip2));
}

// ---------------------------------------------------------------- xz

fn xz_header(flags: [u8; 2]) -> Vec<u8> {
    let mut h = vec![0xFD, b'7', b'z', b'X', b'Z', 0x00];
    h.extend_from_slice(&flags);
    h.extend_from_slice(&crc32fast::hash(&flags).to_le_bytes());
    h
}

#[test]
fn xz_magic_crc_checked() {
    for check in [0x00, 0x01, 0x04, 0x0A] {
        assert_eq!(detect(&padded(&xz_header([0, check]), 64)), Some(M::Xz));
    }
}

#[test]
fn xz_bad_header_crc_rejected() {
    let mut h = xz_header([0, 0x04]);
    h[9] ^= 0xFF;
    assert_eq!(detect(&padded(&h, 64)), None);
    // A valid CRC over reserved flag bits is still not xz.
    assert_eq!(detect(&padded(&xz_header([1, 0x04]), 64)), None);
    assert_eq!(detect(&padded(&xz_header([0, 0x14]), 64)), None);
    assert_eq!(detect(&xz_header([0, 4])[..10]), None, "truncated");
}

// ---------------------------------------------------------------- zstd

#[test]
fn zstd_magic() {
    let frame = ruzstd::encoding::compress_to_vec(
        &b"zstd frame"[..],
        ruzstd::encoding::CompressionLevel::Fastest,
    );
    assert_eq!(detect(&frame), Some(M::Zstd));
}

#[test]
fn zstd_skippable_frame() {
    for low in 0x50..=0x5F_u8 {
        let mut h = vec![low, 0x2A, 0x4D, 0x18];
        h.extend_from_slice(&4u32.to_le_bytes());
        h.extend_from_slice(b"skip");
        assert_eq!(detect(&h), Some(M::Zstd), "skippable {low:#x}");
    }
    assert_eq!(detect(&[0x60, 0x2A, 0x4D, 0x18, 0, 0, 0, 0]), None);
}

#[test]
fn zstd_reserved_bit_rejected() {
    assert_eq!(detect(&padded(&[0x28, 0xB5, 0x2F, 0xFD, 0x08], 16)), None);
    assert_eq!(detect(&[0x28, 0xB5, 0x2F, 0xFD]), None, "no descriptor");
}

// ---------------------------------------------------------------- lz4

#[test]
fn lz4_frame() {
    let frame = {
        use std::io::Write;
        let mut enc = lz4_flex::frame::FrameEncoder::new(Vec::new());
        enc.write_all(b"lz4 frame content").unwrap();
        enc.finish().unwrap()
    };
    assert_eq!(detect(&frame), Some(M::Lz4));
}

#[test]
fn lz4_legacy() {
    assert_eq!(
        detect(&padded(&[0x02, 0x21, 0x4C, 0x18, 0x10, 0, 0, 0], 32)),
        Some(M::Lz4)
    );
}

#[test]
fn lz4_bad_version_bits_rejected() {
    for flg in [0x00u8, 0x80, 0xC0] {
        assert_eq!(
            detect(&padded(&[0x04, 0x22, 0x4D, 0x18, flg, 0x40], 32)),
            None,
            "FLG {flg:#x}"
        );
    }
    assert_eq!(
        detect(&padded(&[0x04, 0x22, 0x4D, 0x18, 0x64, 0x40], 32)),
        Some(M::Lz4)
    );
}

// ---------------------------------------------------------------- tar

/// One 512-byte tar header for `name`, with `magic` at 257.
fn tar_header(name: &str, magic: &[u8]) -> Vec<u8> {
    let mut h = vec![0u8; 512];
    h[..name.len()].copy_from_slice(name.as_bytes());
    h[100..108].copy_from_slice(b"0000644\0");
    h[124..136].copy_from_slice(b"00000000004\0");
    h[156] = b'0';
    h[257..257 + magic.len()].copy_from_slice(magic);
    h[148..156].fill(b' ');
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    let field = format!("{sum:06o}\0 ");
    h[148..156].copy_from_slice(field.as_bytes());
    h
}

#[test]
fn tar_ustar_checksum_ok() {
    let posix = tar_header("game.gba", b"ustar\x0000");
    assert_eq!(detect(&padded(&posix, 2048)), Some(M::Tar));
    let gnu = tar_header("game.gba", b"ustar  \0");
    assert_eq!(detect(&padded(&gnu, 2048)), Some(M::Tar));
}

#[test]
fn tar_ustar_bad_checksum_rejected() {
    let mut h = tar_header("game.gba", b"ustar\x0000");
    h[0] = b'G';
    assert_eq!(detect(&padded(&h, 2048)), None);
    assert!(!tar_header_ok(&h));
    assert_eq!(
        detect(&tar_header("x", b"ustar\x0000")[..300]),
        None,
        "truncated"
    );
}

#[test]
fn v7_tar_by_extension_and_checksum() {
    let v7 = tar_header("old.bin", b"");
    // No magic: detection by bytes alone says nothing.
    assert_eq!(detect(&padded(&v7, 2048)), None);
    // The extension proposes tar and the checksum confirms it.
    assert_eq!(by_extension("tar"), Some(M::Tar));
    assert!(tar_header_ok(&v7));
    let mut bad = v7.clone();
    bad[1] ^= 0x01;
    assert!(!tar_header_ok(&bad));
    assert!(
        !tar_header_ok(&[0u8; 512]),
        "an all-zero block is no header"
    );
}

// ---------------------------------------------------------------- lzma alone

fn lzma_alone(props: u8, dict: u32, size: u64) -> Vec<u8> {
    let mut h = vec![props];
    h.extend_from_slice(&dict.to_le_bytes());
    h.extend_from_slice(&size.to_le_bytes());
    h.extend_from_slice(&[0x00, 0x21, 0x10, 0x42]);
    h
}

#[test]
fn lzma_alone_only_by_extension_with_plausible_props() {
    let good = lzma_alone(0x5D, 1 << 24, 4096);
    assert_eq!(detect(&good), None, "no magic: never by bytes alone");
    assert_eq!(by_extension("lzma"), Some(M::LzmaAlone));
    assert_eq!(by_extension("LZMA"), Some(M::LzmaAlone));
    let len = good.len() as u64;
    assert!(lzma_alone_plausible(&good, len, DECODER_MEM_CAP));
    assert!(
        lzma_alone_plausible(&lzma_alone(0x5D, 1 << 24, u64::MAX), len, DECODER_MEM_CAP),
        "unknown size"
    );
    assert!(!lzma_alone_plausible(
        &lzma_alone(225, 1 << 24, 4096),
        len,
        DECODER_MEM_CAP
    ));
    assert!(
        !lzma_alone_plausible(&lzma_alone(0x5D, u32::MAX, 4096), len, DECODER_MEM_CAP),
        "dictionary above the cap"
    );
    assert!(
        !lzma_alone_plausible(&lzma_alone(0x5D, 1 << 24, 1 << 50), len, DECODER_MEM_CAP),
        "declared size above any entry"
    );
    assert!(
        !lzma_alone_plausible(&good[..12], 12, DECODER_MEM_CAP),
        "truncated"
    );
}

// ---------------------------------------------------------------- others

#[test]
fn chd_magic() {
    assert_eq!(
        detect(&padded(b"MComprHD\0\0\0\x7c\0\0\0\x05", 128)),
        Some(M::Chd)
    );
    assert_eq!(detect(&padded(b"MComprHX", 128)), None);
}

#[test]
fn cab_arj_lzh_compress_ecm_recognised() {
    assert_eq!(detect(&padded(b"MSCF\0\0\0\0", 64)), Some(M::Cab));
    assert_eq!(
        detect(&padded(b"MSCF\x01\0\0\0", 64)),
        None,
        "cab reserved1"
    );
    assert_eq!(detect(&padded(&arj_main_header(30), 64)), Some(M::Arj));
    assert_eq!(
        detect(&padded(&[0x60, 0xEA, 0xFF, 0xFF], 64)),
        None,
        "arj header size above 2600"
    );
    assert_eq!(detect(&padded(b"\x22\x1a-lh5-", 64)), Some(M::Lzh));
    assert_eq!(detect(&padded(b"\x22\x1a-lhd-", 64)), Some(M::Lzh));
    assert_eq!(detect(&padded(b"\x22\x1a-lz5-", 64)), Some(M::Lzh));
    assert_eq!(detect(&padded(b"\x22\x1a-lh5_", 64)), None);
    assert_eq!(detect(&padded(&[0x1F, 0x9D, 0x90], 64)), Some(M::Compress));
    assert_eq!(
        detect(&padded(&[0x1F, 0x9D, 0x05], 64)),
        None,
        "compress max bits below 9"
    );
    assert_eq!(detect(&padded(b"ECM\0\x01", 64)), Some(M::Ecm));
    assert_eq!(detect(&padded(b"ECMX", 64)), None);
    for (ext, want) in [
        ("cab", M::Cab),
        ("arj", M::Arj),
        ("lzh", M::Lzh),
        ("lha", M::Lzh),
        ("Z", M::Compress),
        ("ecm", M::Ecm),
    ] {
        assert_eq!(by_extension(ext), Some(want), "{ext}");
    }
}

#[test]
fn extension_fallback_table() {
    for (ext, want) in [
        ("zip", M::Zip),
        ("ZIPX", M::Zip),
        ("7z", M::SevenZip),
        ("rar", M::Rar4),
        ("r00", M::Rar4),
        ("cbr", M::Rar4),
        ("gz", M::Gzip),
        ("tgz", M::Gzip),
        ("gzip", M::Gzip),
        ("bz2", M::Bzip2),
        ("tbz", M::Bzip2),
        ("tbz2", M::Bzip2),
        ("xz", M::Xz),
        ("txz", M::Xz),
        ("zst", M::Zstd),
        ("tzst", M::Zstd),
        ("lz4", M::Lz4),
        ("chd", M::Chd),
    ] {
        assert_eq!(by_extension(ext), Some(want), "{ext}");
    }
    for ext in ["gba", "bin", "cue", "iso", "", "zipp"] {
        assert_eq!(by_extension(ext), None, "{ext}");
    }
}

#[test]
fn multi_volume_name_patterns() {
    let part = |first: bool, stem: &str| {
        Some(VolumePart {
            first,
            set_stem: stem.to_string(),
        })
    };
    let cases: &[(&str, Option<VolumePart>)] = &[
        ("Game.7z.001", part(true, "Game")),
        ("Game.7z.002", part(false, "Game")),
        ("Game.7Z.0010", part(false, "Game")),
        ("Game.part1.rar", part(true, "Game")),
        ("Game.part01.rar", part(true, "Game")),
        ("Game.part001.RAR", part(true, "Game")),
        ("Game.part2.rar", part(false, "Game")),
        ("Game.part10.rar", part(false, "Game")),
        ("Game.r00", part(false, "Game")),
        ("Game.R99", part(false, "Game")),
        ("Game.z01", part(false, "Game")),
        ("Game.Z12", part(false, "Game")),
        ("Next Tetris, The.7z.001", part(true, "Next Tetris, The")),
        // Not volumes.
        ("Game.rar", None),
        ("Game.zip", None),
        ("Game.7z", None),
        ("Game.part.rar", None),
        ("Game.parta.rar", None),
        ("Game.7z.01", None),
        ("Game.7z.000", None),
        ("Game.r0", None),
        ("Game.z00", None),
        ("Game.r001", None),
        (".7z.001", None),
        ("Game.001", None),
    ];
    for (name, want) in cases {
        assert_eq!(&multi_volume_part(name), want, "{name}");
    }
}

#[test]
fn html_named_zip_is_no_container() {
    // The `3-D Tetris (USA).zip` shape: an HTML error page saved as .zip.
    let html =
        b"<!DOCTYPE html>\n<html><head><title>404</title></head><body>Not found</body></html>\n";
    assert_eq!(detect(html), None);
    assert_eq!(detect_container(html, html.len() as u64, Some(html)), None);
    // The extension still proposes zip; the reader decides (spec §3.2 rule 3).
    assert_eq!(by_extension("zip"), Some(M::Zip));
}

#[test]
fn magic_ignores_tiny_and_empty_inputs() {
    assert_eq!(detect(b""), None);
    assert_eq!(detect(b"P"), None);
    assert_eq!(detect_container(b"", 0, Some(b"")), None);
}

// ------------------------------------------------- review fixes (Task 3)

/// An ARJ main header: id, basic header size, `size` header bytes (first
/// header size 30, file type 2 = main header) and the CRC32 of those bytes.
fn arj_main_header(size: u16) -> Vec<u8> {
    let mut basic = vec![0u8; usize::from(size)];
    basic[0] = 30;
    basic[6] = 2;
    let mut v = vec![0x60, 0xEA];
    v.extend_from_slice(&size.to_le_bytes());
    v.extend_from_slice(&basic);
    v.extend_from_slice(&crc32fast::hash(&basic).to_le_bytes());
    v
}

#[test]
fn arj_needs_its_basic_header_crc() {
    assert_eq!(detect(&padded(&arj_main_header(40), 128)), Some(M::Arj));
    // Two bytes of magic and a small number are what raw 6502/68000 code
    // can start with (`60 EA` = RTS NOP, BRA.S): without the header CRC it
    // is not ARJ, and a ROM must keep its header identification.
    let mut bad = arj_main_header(40);
    let last = bad.len() - 1;
    bad[last] ^= 0xFF;
    assert_eq!(detect(&padded(&bad, 128)), None, "header CRC mismatch");
    assert_eq!(
        detect(&padded(&[0x60, 0xEA, 0x28, 0x00, 0x1E], 4096)),
        None,
        "code that starts like ARJ"
    );
    // The CRC must lie inside the bytes the caller has.
    let whole = arj_main_header(40);
    assert_eq!(detect(&whole[..whole.len() - 2]), None, "header cut short");
}

/// A zip64 end-of-central-directory locator: disk of the zip64 record,
/// its offset and the total number of disks.
fn zip64_locator(record_disk: u32, record_offset: u64, total_disks: u32) -> Vec<u8> {
    let mut v = b"PK\x06\x07".to_vec();
    v.extend_from_slice(&record_disk.to_le_bytes());
    v.extend_from_slice(&record_offset.to_le_bytes());
    v.extend_from_slice(&total_disks.to_le_bytes());
    v
}

#[test]
fn zip64_eocd_with_ffff_disk_numbers_is_one_archive() {
    // APPNOTE 4.4.1.4 lets a writer set any EOCD field it moves to the
    // zip64 record to -1, disk numbers included: the locator decides.
    let mut file = vec![0x22u8; 300];
    file.extend(zip64_locator(0, 200, 1));
    file.extend(eocd(0xFFFF, 0xFFFF, 0xFFFF, u32::MAX, u32::MAX, b""));
    assert_eq!(
        detect_container(&file[..16], file.len() as u64, Some(&file)),
        Some(M::ZipEocdOnly)
    );
    // The same record on a set of three disks is one part of a set.
    let mut file = vec![0x22u8; 300];
    file.extend(zip64_locator(2, 200, 3));
    file.extend(eocd(0xFFFF, 0xFFFF, 0xFFFF, u32::MAX, u32::MAX, b""));
    assert_eq!(
        detect_container(&file[..16], file.len() as u64, Some(&file)),
        Some(M::ZipSpanned)
    );
}

#[test]
fn zip64_locator_offset_outside_file_rejected() {
    // The zip64 record must start before its locator.
    let mut file = vec![0x22u8; 300];
    file.extend(zip64_locator(0, 1 << 40, 1));
    file.extend(eocd(0, 0, 0xFFFF, u32::MAX, u32::MAX, b""));
    assert_eq!(
        detect_container(&file[..16], file.len() as u64, Some(&file)),
        None
    );
}

#[test]
fn lz4_after_skippable_frame() {
    // Skippable frames precede lz4 frames as well as zstd ones: the frame
    // after them decides.
    let frame = {
        use std::io::Write;
        let mut enc = lz4_flex::frame::FrameEncoder::new(Vec::new());
        enc.write_all(b"lz4 frame content").unwrap();
        enc.finish().unwrap()
    };
    let mut file = vec![0x50, 0x2A, 0x4D, 0x18, 4, 0, 0, 0, 1, 2, 3, 4];
    file.extend(&frame);
    assert_eq!(detect(&file), Some(M::Lz4));
    let mut zstd = vec![0x5F, 0x2A, 0x4D, 0x18, 0, 0, 0, 0];
    zstd.extend(zstd_frame_header());
    assert_eq!(detect(&zstd), Some(M::Zstd));
    // A skippable frame whose end the head does not reach stays zstd.
    assert_eq!(
        detect(&padded(&[0x50, 0x2A, 0x4D, 0x18, 0, 0, 1, 0], 64)),
        Some(M::Zstd)
    );
}

/// A minimal zstd frame header (magic + descriptor, single segment).
fn zstd_frame_header() -> Vec<u8> {
    vec![0x28, 0xB5, 0x2F, 0xFD, 0x20, 0x05, 0x01, 0x00, 0x00]
}

// ------------------------------------------- open_archive dispatch (Task 16)

mod dispatch {
    use std::path::{Path, PathBuf};

    use coverly_core::archive::testing::{
        gzip_with_zeros, sevenz, tar, zip_crafted, zip_with, SevenZShape, ZipMethod, ZipSpec,
    };
    use coverly_core::archive::{
        open_archive, open_archive_with, ArchiveError, Compressor, Format, Limits, Tool,
    };

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn names(r: &dyn coverly_core::archive::ArchiveReader) -> Vec<String> {
        r.entries()
            .iter()
            .map(|e| e.name.as_str().to_string())
            .collect()
    }

    /// The root cause of the real case: a 7z named `.zip` was reported as
    /// a damaged zip. Magic decides.
    #[test]
    fn sevenz_named_zip_opens_as_sevenz() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = sevenz(
            &[
                ("Next Tetris, The (Track 01).bin", b"PLAYSTATION data"),
                ("Next Tetris, The.cue", b"FILE \"x\" BINARY\n"),
            ],
            SevenZShape::non_solid_lzma(),
        );
        let path = write(dir.path(), "Next Tetris, The.zip", &bytes);
        let reader = open_archive(&path, &Limits::default())
            .expect("opens")
            .expect("a container");
        assert_eq!(reader.format(), Format::SevenZip);
        assert_eq!(
            names(reader.as_ref()),
            ["Next Tetris, The (Track 01).bin", "Next Tetris, The.cue"]
        );
    }

    #[test]
    fn zip_named_rar_opens_as_zip() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = zip_with(&[("Tetris (World).gb", b"GB ROM", ZipMethod::Deflate)]);
        let path = write(dir.path(), "Tetris.rar", &bytes);
        let reader = open_archive(&path, &Limits::default()).unwrap().unwrap();
        assert_eq!(reader.format(), Format::Zip);
        assert_eq!(names(reader.as_ref()), ["Tetris (World).gb"]);
    }

    #[test]
    fn html_named_zip_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let html = b"<!DOCTYPE html>\n<html><head><title>Download</title></head>\
                     <body>Not found</body></html>\n";
        let path = write(dir.path(), "3-D Tetris (USA).zip", html);
        assert!(open_archive(&path, &Limits::default()).unwrap().is_none());
    }

    #[test]
    fn garbage_named_zip_is_damaged() {
        // No magic, an archive extension, not text: the zip reader decides.
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "Broken.zip", &[0xA5u8; 4096]);
        let err = open_archive(&path, &Limits::default())
            .err()
            .expect("an error");
        assert!(matches!(err, ArchiveError::Damaged { .. }), "{err:?}");
    }

    #[test]
    fn no_magic_no_archive_extension_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "Tetris.gba", &[0xA5u8; 4096]);
        assert!(open_archive(&path, &Limits::default()).unwrap().is_none());
    }

    #[test]
    fn single_compressed_rom_opens_as_single() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "Tetris.gba.gz", &gzip_with_zeros(b"GBA ROM", 0));
        let reader = open_archive(&path, &Limits::default()).unwrap().unwrap();
        assert_eq!(reader.format(), Format::Single(Compressor::Gzip));
        assert_eq!(names(reader.as_ref()), ["Tetris.gba"]);
    }

    #[test]
    fn gz_holding_tar_opens_as_tar_by_content() {
        // A `.gz` (not `.tgz`) whose stream is a tar: the content decides.
        let dir = tempfile::tempdir().unwrap();
        let bytes = gzip_with_zeros(&tar(&[("Tetris.gb", b"GB ROM")]), 0);
        let path = write(dir.path(), "roms.gz", &bytes);
        let reader = open_archive(&path, &Limits::default()).unwrap().unwrap();
        assert_eq!(
            reader.format(),
            Format::Tar {
                outer: Some(Compressor::Gzip)
            }
        );
        assert_eq!(names(reader.as_ref()), ["Tetris.gb"]);
    }

    #[test]
    fn plain_tar_by_magic() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "roms.bin", &tar(&[("Tetris.gb", b"GB ROM")]));
        let reader = open_archive(&path, &Limits::default()).unwrap().unwrap();
        assert_eq!(reader.format(), Format::Tar { outer: None });
    }

    #[test]
    fn multi_volume_7z_001_002_one_error_with_parts() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = sevenz(
            &[("Game (Track 1).bin", &[7u8; 3000][..])],
            SevenZShape::non_solid_lzma(),
        );
        let (a, b) = bytes.split_at(bytes.len() / 2);
        let first = write(dir.path(), "Game.7z.001", a);
        let second = write(dir.path(), "Game.7z.002", b);
        write(dir.path(), "Other.7z.001", a);
        let want = vec![first.clone(), second.clone()];
        for path in [&first, &second] {
            match open_archive(path, &Limits::default()) {
                Err(ArchiveError::MultiVolume { parts }) => assert_eq!(parts, want, "{path:?}"),
                other => panic!("{path:?}: {:?}", other.map(|r| r.map(|r| r.format()))),
            }
        }
    }

    #[test]
    fn lone_complete_7z_001_opens() {
        // A `.7z.001` that holds the whole archive (no further parts, the
        // header inside the file) is just a 7z.
        let dir = tempfile::tempdir().unwrap();
        let bytes = sevenz(&[("Game.bin", b"data")], SevenZShape::non_solid_lzma());
        let path = write(dir.path(), "Game.7z.001", &bytes);
        let reader = open_archive(&path, &Limits::default()).unwrap().unwrap();
        assert_eq!(reader.format(), Format::SevenZip);
    }

    #[test]
    fn lone_truncated_7z_001_is_multi_volume() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = sevenz(
            &[("Game.bin", &[1u8; 2000][..])],
            SevenZShape::non_solid_lzma(),
        );
        let path = write(dir.path(), "Game.7z.001", &bytes[..bytes.len() / 2]);
        match open_archive(&path, &Limits::default()) {
            Err(ArchiveError::MultiVolume { parts }) => assert_eq!(parts, vec![path]),
            other => panic!("{:?}", other.map(|r| r.map(|r| r.format()))),
        }
    }

    #[test]
    fn spanned_zip_and_old_rar_volumes_collect_parts() {
        let dir = tempfile::tempdir().unwrap();
        // Split zip: `.z01` parts then the `.zip` with the end record.
        let z01 = write(dir.path(), "Game.z01", b"PK\x07\x08 first part");
        let zip = write(dir.path(), "Game.zip", b"PK\x07\x08 last part");
        match open_archive(&z01, &Limits::default()) {
            Err(ArchiveError::MultiVolume { parts }) => {
                assert_eq!(parts, vec![z01.clone(), zip.clone()]);
            }
            other => panic!("{:?}", other.map(|r| r.map(|r| r.format()))),
        }
        match open_archive(&zip, &Limits::default()) {
            Err(ArchiveError::MultiVolume { parts }) => assert_eq!(parts, vec![z01, zip]),
            other => panic!("{:?}", other.map(|r| r.map(|r| r.format()))),
        }
        // Old-style RAR 4 volume: main header flag MHD_VOLUME, `.r00` next.
        let mut rar4 = b"Rar!\x1a\x07\x00".to_vec();
        rar4.extend_from_slice(&[0x00, 0x00, 0x73, 0x01, 0x00, 0x0D, 0x00]);
        rar4.extend_from_slice(&[0u8; 6]);
        let rar = write(dir.path(), "Disc.rar", &rar4);
        let r00 = write(dir.path(), "Disc.r00", b"continuation");
        for path in [&rar, &r00] {
            match open_archive_with(path, &Limits::default(), Some(&[])) {
                Err(ArchiveError::MultiVolume { parts }) => {
                    assert_eq!(parts, vec![rar.clone(), r00.clone()], "{path:?}");
                }
                other => panic!("{path:?}: {:?}", other.map(|r| r.map(|r| r.format()))),
            }
        }
    }

    #[test]
    fn rar5_volume_flag_is_multi_volume() {
        let dir = tempfile::tempdir().unwrap();
        // Signature, header CRC32, size 3, type 1 (main), flags 0,
        // archive flags 0x0001 (volume).
        let mut rar5 = b"Rar!\x1a\x07\x01\x00".to_vec();
        rar5.extend_from_slice(&[0, 0, 0, 0, 0x03, 0x01, 0x00, 0x01]);
        let path = write(dir.path(), "Disc.rar", &rar5);
        match open_archive_with(&path, &Limits::default(), Some(&[])) {
            Err(ArchiveError::MultiVolume { parts }) => assert_eq!(parts, vec![path]),
            other => panic!("{:?}", other.map(|r| r.map(|r| r.format()))),
        }
    }

    #[test]
    fn rar_without_tool_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let mut rar5 = b"Rar!\x1a\x07\x01\x00".to_vec();
        rar5.extend_from_slice(&[0, 0, 0, 0, 0x03, 0x01, 0x00, 0x00]);
        rar5.extend_from_slice(&[0u8; 64]);
        let path = write(dir.path(), "Tetris.rar", &rar5);
        let want = |e: &ArchiveError| {
            matches!(e, ArchiveError::Unsupported { format, need: Some(Tool::Unrar) }
                if format == "RAR")
        };
        // The setting off: no tool runs even when one is installed.
        let off = Limits {
            external_tools: false,
            ..Limits::default()
        };
        let err = open_archive(&path, &off).err().expect("unsupported");
        assert!(want(&err), "{err:?}");
        // The setting on, nothing installed.
        let err = open_archive_with(&path, &Limits::default(), Some(&[]))
            .err()
            .expect("unsupported");
        assert!(want(&err), "{err:?}");
    }

    #[test]
    fn recognised_formats_never_opened_are_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let mut cab = b"MSCF".to_vec();
        cab.extend_from_slice(&[0u8; 60]);
        let path = write(dir.path(), "Game.cab", &cab);
        let err = open_archive(&path, &Limits::default()).err().expect("err");
        assert!(
            matches!(&err, ArchiveError::Unsupported { format, need: None } if format == "CAB"),
            "{err:?}"
        );
        // A CHD is a disc image, not an archive.
        let mut chd = b"MComprHD".to_vec();
        chd.extend_from_slice(&[0u8; 120]);
        let path = write(dir.path(), "Game.chd", &chd);
        assert!(open_archive(&path, &Limits::default()).unwrap().is_none());
    }

    #[test]
    fn declared_sizes_sum_overflow() {
        // Two zip64 entries whose declared sizes overflow u64 when summed:
        // too large, never a wrapped total and never damaged.
        let dir = tempfile::tempdir().unwrap();
        let half = u64::MAX / 2 + 1;
        let bytes = zip_crafted(&[
            ZipSpec::new("a.bin", b"a", ZipMethod::Stored)
                .declared_size(half)
                .zip64(),
            ZipSpec::new("b.bin", b"b", ZipMethod::Stored)
                .declared_size(half)
                .zip64(),
        ]);
        let path = write(dir.path(), "huge.zip", &bytes);
        let limits = Limits {
            max_entry_size: u64::MAX,
            max_total_size: u64::MAX,
            ..Limits::default()
        };
        let err = open_archive(&path, &limits).err().expect("too large");
        assert!(matches!(err, ArchiveError::TooLarge { .. }), "{err:?}");
    }

    /// `.z64` (N64) and other `.zNN`/`.rNN` names are volume patterns only
    /// when the set is real: an N64 ROM is no archive, even next to a
    /// zipped copy of itself.
    #[test]
    fn n64_z64_rom_is_not_a_volume() {
        let dir = tempfile::tempdir().unwrap();
        let mut n64 = vec![0x80, 0x37, 0x12, 0x40];
        n64.resize(4096, 0x11);
        let rom = write(dir.path(), "Super Mario 64 (USA).z64", &n64);
        assert!(open_archive(&rom, &Limits::default()).unwrap().is_none());
        // A normal zip of the same game beside it changes nothing.
        write(
            dir.path(),
            "Super Mario 64 (USA).zip",
            &zip_with(&[("Super Mario 64 (USA).z64", &n64, ZipMethod::Deflate)]),
        );
        assert!(open_archive(&rom, &Limits::default()).unwrap().is_none());
        // A stray `.r16` without a RAR signature or a `.rar` beside it.
        let stray = write(dir.path(), "Game.r16", &[0xA5u8; 512]);
        assert!(open_archive_with(&stray, &Limits::default(), Some(&[]))
            .unwrap()
            .is_none());
    }

    #[test]
    fn later_split_zip_part_without_magic_is_a_volume() {
        // `.z02` has no magic of its own: the spanned `.z01` (or a spanned
        // `.zip`) beside it makes it a part.
        let dir = tempfile::tempdir().unwrap();
        let z01 = write(dir.path(), "Game.z01", b"PK\x07\x08PK\x03\x04 first");
        let z02 = write(dir.path(), "Game.z02", &[0x33u8; 256]);
        match open_archive(&z02, &Limits::default()) {
            Err(ArchiveError::MultiVolume { parts }) => assert_eq!(parts, vec![z01, z02]),
            other => panic!("{:?}", other.map(|r| r.map(|r| r.format()))),
        }
    }

    /// The real pile (spec §1): three 7z named `.zip`, an HTML page, real
    /// zips. Read-only; needs the owner's volume (plan AP-1).
    #[test]
    #[ignore = "needs the real download pile (COVERLY_SAMPLE_DIR)"]
    fn real_sample_dispatch() {
        let dir = std::env::var("COVERLY_SAMPLE_DIR")
            .unwrap_or_else(|_| "/Volumes/TinyExt/ROMs-Downloaded".into());
        let dir = Path::new(&dir);
        if !dir.is_dir() {
            eprintln!("skipped: {} is not mounted", dir.display());
            return;
        }
        let l = Limits::default();
        for name in [
            "Next Tetris, The.zip",
            "Next Tetris, The_ On-Line Edition.zip",
            "Tetris Plus.zip",
        ] {
            let r = open_archive(&dir.join(name), &l).expect(name).expect(name);
            assert_eq!(r.format(), Format::SevenZip, "{name}");
        }
        assert!(open_archive(&dir.join("3-D Tetris (USA).zip"), &l)
            .unwrap()
            .is_none());
        let r = open_archive(&dir.join("FIFA Soccer 07.zip"), &l)
            .unwrap()
            .unwrap();
        assert_eq!(r.format(), Format::Zip);
    }
}
