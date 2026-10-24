//! Size and CRC32 checks of decoded entry bytes (spec
//! `2026-10-23-organize-archives-and-discs.md` §4.2, §9.1).
//!
//! Every entry reader a backend returns is wrapped in a
//! [`VerifyingReader`]. It reads at most `size + 1` bytes from the decoder
//! (so an entry that inflates past its declared size is caught after one
//! extra byte, not after gigabytes) and, at EOF, the byte count and the
//! CRC32 must match the listing; anything else is
//! [`ArchiveError::Damaged`] with detail `"size"` or `"crc"`. Errors are
//! sticky: once a mismatch is reported, every later read reports it again.

use std::io::{self, Read, Take};

use super::ArchiveError;

/// Detail of a size mismatch (short or overrun).
pub const SIZE_MISMATCH: &str = "size";
/// Detail of a CRC32 mismatch.
pub const CRC_MISMATCH: &str = "crc";

/// A reader that verifies the declared size and, when the format stores
/// one, the CRC32 of an entry at EOF.
pub struct VerifyingReader<R> {
    inner: Take<R>,
    size: u64,
    crc32: Option<u32>,
    read: u64,
    hasher: crc32fast::Hasher,
    state: State,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Reading,
    Verified,
    Failed(&'static str),
}

impl<R: Read> VerifyingReader<R> {
    /// Wraps `inner`, which must yield exactly `size` bytes whose CRC32 is
    /// `crc32` (when `Some`).
    pub fn new(inner: R, size: u64, crc32: Option<u32>) -> Self {
        Self {
            inner: inner.take(size.saturating_add(1)),
            size,
            crc32,
            read: 0,
            hasher: crc32fast::Hasher::new(),
            state: State::Reading,
        }
    }

    fn fail(&mut self, detail: &'static str) -> io::Error {
        self.state = State::Failed(detail);
        damaged(detail)
    }

    /// Whether EOF was reached with the size (and CRC) matching.
    pub fn verified(&self) -> bool {
        self.state == State::Verified
    }

    /// Bytes returned so far.
    pub fn bytes_read(&self) -> u64 {
        self.read
    }
}

impl<R: Read> Read for VerifyingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.state {
            State::Verified => return Ok(0),
            State::Failed(detail) => return Err(damaged(detail)),
            State::Reading => {}
        }
        if buf.is_empty() {
            return Ok(0);
        }
        let n = self.inner.read(buf)?;
        if n == 0 {
            if self.read != self.size {
                return Err(self.fail(SIZE_MISMATCH));
            }
            let crc = std::mem::take(&mut self.hasher).finalize();
            if self.crc32.is_some_and(|want| want != crc) {
                return Err(self.fail(CRC_MISMATCH));
            }
            self.state = State::Verified;
            return Ok(0);
        }
        self.read += n as u64;
        if self.read > self.size {
            // Overrun: the byte past the declared size is never handed out.
            return Err(self.fail(SIZE_MISMATCH));
        }
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

fn damaged(detail: &str) -> io::Error {
    ArchiveError::Damaged {
        detail: detail.to_owned(),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn damaged_detail(err: &io::Error) -> String {
        match err.get_ref().and_then(|e| e.downcast_ref::<ArchiveError>()) {
            Some(ArchiveError::Damaged { detail }) => detail.clone(),
            other => panic!("expected Damaged, got {other:?} ({err:?})"),
        }
    }

    fn payload() -> Vec<u8> {
        (0..10_000u32).map(|i| (i * 7 % 256) as u8).collect()
    }

    #[test]
    fn verifying_reader_passes_matching_entry() {
        let data = payload();
        let crc = crc32fast::hash(&data);
        let mut r = VerifyingReader::new(io::Cursor::new(&data), data.len() as u64, Some(crc));
        let mut out = Vec::new();
        r.read_to_end(&mut out).expect("size and crc match");
        assert_eq!(out, data);
        assert!(r.verified());
        assert_eq!(r.bytes_read(), data.len() as u64);
        assert_eq!(r.read(&mut [0u8; 4]).expect("EOF stays EOF"), 0);
    }

    #[test]
    fn verifying_reader_size_short_damaged() {
        let data = payload();
        let crc = crc32fast::hash(&data);
        let short = &data[..data.len() - 1];
        let mut r = VerifyingReader::new(io::Cursor::new(short), data.len() as u64, Some(crc));
        let mut out = Vec::new();
        let err = r.read_to_end(&mut out).expect_err("one byte short");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(damaged_detail(&err), SIZE_MISMATCH);
        assert!(!r.verified());
        let again = r.read(&mut [0u8; 4]).expect_err("sticky");
        assert_eq!(damaged_detail(&again), SIZE_MISMATCH);
    }

    /// The shape of `zip_entry_inflates_past_declared_size`: the decoder
    /// keeps producing bytes past the declared size.
    #[test]
    fn verifying_reader_overrun_damaged() {
        let declared = 4096u64;
        // An endless decoder: never more than declared + 1 bytes are read.
        let endless = io::repeat(0xAB);
        let mut r = VerifyingReader::new(endless, declared, None);
        let mut out = Vec::new();
        let err = r.read_to_end(&mut out).expect_err("inflates past size");
        assert_eq!(damaged_detail(&err), SIZE_MISMATCH);
        assert!(
            out.len() as u64 <= declared,
            "overrun bytes are never handed out: {}",
            out.len()
        );
        assert!(r.bytes_read() <= declared + 1);
    }

    #[test]
    fn verifying_reader_crc_mismatch_damaged() {
        let mut data = payload();
        let crc = crc32fast::hash(&data);
        data[5000] ^= 0x01;
        let mut r = VerifyingReader::new(io::Cursor::new(&data), data.len() as u64, Some(crc));
        let err = io::copy(&mut r, &mut io::sink()).expect_err("one bit flipped");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(damaged_detail(&err), CRC_MISMATCH);
        assert!(!r.verified());
    }

    #[test]
    fn verifying_reader_no_crc_checks_size_only() {
        let data = payload();
        let mut r = VerifyingReader::new(io::Cursor::new(&data), data.len() as u64, None);
        io::copy(&mut r, &mut io::sink()).expect("no crc stored: size decides");
        assert!(r.verified());

        let mut r = VerifyingReader::new(io::Cursor::new(&data), data.len() as u64 + 1, None);
        let err = io::copy(&mut r, &mut io::sink()).expect_err("size still checked");
        assert_eq!(damaged_detail(&err), SIZE_MISMATCH);
    }

    #[test]
    fn verifying_reader_empty_entry() {
        let mut r = VerifyingReader::new(io::empty(), 0, Some(0));
        assert_eq!(r.read(&mut [0u8; 8]).expect("empty entry"), 0);
        assert!(r.verified());
        let mut r = VerifyingReader::new(io::Cursor::new([1u8]), 0, None);
        let err = r.read(&mut [0u8; 8]).expect_err("one byte past 0");
        assert_eq!(damaged_detail(&err), SIZE_MISMATCH);
    }

    #[test]
    fn verifying_reader_passes_inner_errors_through() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
            }
        }
        let mut r = VerifyingReader::new(Broken, 10, None);
        let err = r.read(&mut [0u8; 8]).expect_err("inner error");
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }
}
