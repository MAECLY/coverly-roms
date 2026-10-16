//! Art sources (spec §6 `sources/`, ADR-0005): a small trait each source
//! implements, the query it answers and the candidates it returns. M2 ships
//! libretro-thumbnails; ScreenScraper, SteamGridDB and others arrive in M6
//! behind the same trait.

pub mod libretro;

use serde::{Deserialize, Serialize};

use crate::cache::CacheError;
use crate::identify::names::{ParsedName, Region};
use crate::net::NetError;
use crate::profiles::Media;
use crate::systems::System;

/// How a candidate matched the ROM, weakest first (the derived order is
/// the ranking order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchKind {
    /// Similar title by fuzzy score.
    Fuzzy,
    /// Same normalised title, any region or revision.
    TitleExact,
    /// The ROM file name itself, after the source's sanitising.
    NameExact,
    /// The canonical name from a DAT or the arcade map.
    Canonical,
}

/// One image a source offers for a ROM and media.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// Source id, e.g. `libretro`.
    pub source: String,
    pub media: Media,
    /// File name as the source lists it, without `.png`.
    pub name: String,
    pub url: String,
    /// Mirror tried when `url` fails.
    pub fallback_url: Option<String>,
    /// Regions parsed from `name`.
    pub regions: Vec<Region>,
    pub kind: MatchKind,
    /// Match score, 0–100.
    pub score: u8,
    /// Community votes, when the source has them (SteamGridDB, M6).
    pub votes: Option<u32>,
    /// How many listings of the source carry the same variant (D9 part b).
    pub consensus: u8,
}

/// What a source is asked for: one ROM, one media.
#[derive(Clone, Debug)]
pub struct ArtQuery<'a> {
    pub system: &'a System,
    pub media: Media,
    /// DAT / arcade name, when the ROM was identified.
    pub canonical: Option<&'a str>,
    /// Parsed ROM stem (numbering already stripped).
    pub rom: &'a ParsedName,
    /// Raw stem, for the exact-file-name try.
    pub stem: &'a str,
    /// `false` for `MatchMethod::HashOnly`: never match by name.
    pub by_name: bool,
    /// Whether fuzzy matching may run when nothing exact was found.
    pub fuzzy: bool,
    /// Lowest fuzzy score kept.
    pub min_score: u8,
    /// Most fuzzy candidates kept.
    pub max_candidates: usize,
}

/// Why a source could not answer.
///
/// `Display` and `Error` are written by hand: thiserror would take the
/// `source` field (the source id) for the underlying error.
#[derive(Debug)]
pub enum SourceError {
    SystemUnavailable { source: String, system: String },
    Net(NetError),
    Cache(CacheError),
    NotFound(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::SystemUnavailable { source, system } => {
                write!(f, "{system} is not available on {source}")
            }
            SourceError::Net(e) => e.fmt(f),
            SourceError::Cache(e) => e.fmt(f),
            SourceError::NotFound(url) => write!(f, "no image at {url}"),
        }
    }
}

impl std::error::Error for SourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // Transparent like thiserror's `#[error(transparent)]`.
        match self {
            SourceError::Net(e) => e.source(),
            SourceError::Cache(e) => e.source(),
            _ => None,
        }
    }
}

impl From<NetError> for SourceError {
    fn from(e: NetError) -> Self {
        SourceError::Net(e)
    }
}

impl From<CacheError> for SourceError {
    fn from(e: CacheError) -> Self {
        SourceError::Cache(e)
    }
}

/// An art source. Implementations are shared between worker threads.
pub trait ArtSource: Send + Sync {
    /// Stable id, stored in candidates and the journal.
    fn id(&self) -> &str;
    /// Candidates for `q`, sorted by [`sort_candidates`].
    fn candidates(&self, q: &ArtQuery) -> Result<Vec<Candidate>, SourceError>;
    /// Whether the `media` listing of `system` has `name` (case-insensitive).
    fn has_name(&self, system: &System, media: Media, name: &str) -> Result<bool, SourceError>;
    /// The image bytes of `c` (raw; the caller decodes and re-encodes them).
    fn fetch(&self, c: &Candidate) -> Result<Vec<u8>, SourceError>;
}

/// Orders candidates: strongest kind, then score, then name (deterministic).
pub fn sort_candidates(c: &mut [Candidate]) {
    c.sort_by(|a, b| {
        b.kind
            .cmp(&a.kind)
            .then(b.score.cmp(&a.score))
            .then_with(|| a.name.cmp(&b.name))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_read_like_the_interface_messages() {
        let e = SourceError::SystemUnavailable {
            source: "libretro".into(),
            system: "Super Nintendo".into(),
        };
        assert_eq!(e.to_string(), "Super Nintendo is not available on libretro");
        assert_eq!(
            SourceError::from(NetError::Timeout).to_string(),
            "timed out"
        );
        assert_eq!(
            SourceError::NotFound("u".into()).to_string(),
            "no image at u"
        );
    }
}
