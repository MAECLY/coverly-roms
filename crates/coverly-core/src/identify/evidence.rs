//! Pieces of evidence about which system a file belongs to (spec §6.1).

use serde::{Deserialize, Serialize};

/// Where a piece of evidence comes from, strongest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdLevel {
    /// CRC/SHA1 found in a DAT.
    Hash,
    /// Header magic.
    Header,
    /// File extension.
    Extension,
    /// Name, tags, fuzzy title.
    Name,
    /// Online lookup (ScreenScraper by CRC), only when configured.
    Online,
    /// Chosen by the user.
    User,
}

/// One clue: `system` with `weight` in 0..=1, `detail` is data for the
/// UI (a CRC and game name, `CGB 0xC0`, an extension) — never a sentence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: IdLevel,
    pub system: String,
    pub weight: f32,
    pub detail: String,
}

impl Evidence {
    pub fn new(kind: IdLevel, system: &str, weight: f32, detail: impl Into<String>) -> Evidence {
        Evidence {
            kind,
            system: system.to_string(),
            weight,
            detail: detail.into(),
        }
    }
}
