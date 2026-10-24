//! Archive reader: one trait over every container and compressor Coverly
//! opens (spec `2026-10-23-organize-archives-and-discs.md` §4).

#[cfg(any(test, feature = "test-util"))]
pub mod testing;
