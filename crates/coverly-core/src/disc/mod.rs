//! Disc games: reading sectors from disc images and archive windows, and
//! the sheets (`.cue`, `.gdi`, `.m3u`) that tie tracks together.
//!
//! Every read here is bounded: a [`source::WindowSource`] holds at most
//! [`source::MAX_WINDOW`] decoded bytes, a [`source::FileSource`] reads one
//! sector at a time, and a sheet is parsed only when it is at most
//! [`sheets::MAX_SHEET_BYTES`] long. Nothing recurses.

pub mod sheets;
pub mod source;

pub use sheets::{
    infer_redump_set, parse_sheet, primary_track, Sheet, SheetError, SheetKind, TrackInfo,
    TrackMode,
};
pub use source::{detect_layout, FileSource, SectorLayout, SectorSource, WindowSource};
