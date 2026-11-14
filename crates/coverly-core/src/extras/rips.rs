//! Music rip tag readers (extras spec D15).

use std::path::Path;

use super::RipTags;

/// The tags a music rip carries in its own header, for the add dialog's
/// suggestions (D13, D15). The readers land with the rip formats; until
/// then nothing is read and no file is opened.
pub fn read_rip(_path: &Path) -> Option<RipTags> {
    None
}
