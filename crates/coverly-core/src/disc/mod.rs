//! Disc games: reading sectors from disc images and archive windows, the
//! sheets (`.cue`, `.gdi`, `.m3u`) that tie tracks together, ISO 9660 and
//! the system rules over it, and serials in one normal form.
//!
//! Every read here is bounded: a [`source::WindowSource`] holds at most
//! [`source::MAX_WINDOW`] decoded bytes, a [`source::FileSource`] reads one
//! sector at a time, a sheet is parsed only when it is at most
//! [`sheets::MAX_SHEET_BYTES`] long, and a root directory is walked for at
//! most [`iso9660::MAX_ROOT_RECORDS`] records. Nothing recurses.

pub mod iso9660;
pub mod serial;
pub mod sheets;
pub mod source;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;

pub use iso9660::{
    find_root_file, identify_playstation, read_pvd, DirRecord, PlayStationDisc, Pvd,
};
pub use serial::normalise_serial;
pub use sheets::{
    infer_redump_set, parse_sheet, primary_track, Sheet, SheetError, SheetKind, TrackInfo,
    TrackMode,
};
pub use source::{detect_layout, FileSource, SectorLayout, SectorSource, WindowSource};

/// The system a disc belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiscSystem {
    /// A system of `systems.toml`, by id (`psx`, `saturn`, `dreamcast`…).
    Known(&'static str),
    /// A system Coverly recognises but has no folder for yet, by display
    /// name (`PlayStation 2`, `3DO`): the item is left with
    /// `UnsupportedSystem`.
    Unsupported(&'static str),
}
