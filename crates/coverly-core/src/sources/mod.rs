//! Art sources (spec §6 `sources/`, ADR-0005): a small trait each source
//! implements, the query it answers and the candidates it returns. M2 ships
//! libretro-thumbnails; ScreenScraper, SteamGridDB and others arrive in M6
//! behind the same trait.

pub mod libretro;
pub mod steamgriddb;

use serde::{Deserialize, Serialize};

use crate::cache::CacheError;
use crate::identify::hash::HashOutcome;
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

/// Community votes on one image (SteamGridDB).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Votes {
    pub up: u32,
    pub down: u32,
}

impl Votes {
    /// Net votes: up − down.
    pub fn score(self) -> i64 {
        i64::from(self.up) - i64::from(self.down)
    }
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
    /// Community votes, when the source has them (SteamGridDB).
    pub votes: Option<Votes>,
    /// How many listings/sources/regions carry the same variant (D9 b, D34).
    pub consensus: u8,
    /// Where it came from inside the source (e.g. `game 5248 · grid 80211`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
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
    /// The ROM's hashes, when it was hashed (ScreenScraper matches by them).
    pub hash: Option<&'a HashOutcome>,
    /// ROM size in bytes (0 = unknown).
    pub size: u64,
    /// ROM file name with extension, no folders.
    pub file_name: &'a str,
}

/// Why a source could not answer.
///
/// `Display` and `Error` are written by hand: thiserror would take the
/// `source` field (the source id) for the underlying error.
#[derive(Debug)]
pub enum SourceError {
    SystemUnavailable {
        source: String,
        system: String,
    },
    Net(NetError),
    Cache(CacheError),
    NotFound(String),
    /// The source refused the key or account (HTTP 401/403).
    Auth {
        source: String,
        status: u16,
    },
    /// The source needs a key and none is stored.
    MissingKey {
        source: String,
    },
    /// The answer could not be read (not JSON, `success: false`, …).
    BadResponse {
        source: String,
        detail: String,
    },
    /// The source's daily quota is used up (ScreenScraper 430/431).
    Exhausted {
        source: String,
        status: u16,
    },
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
            SourceError::Auth { source, status } => {
                write!(
                    f,
                    "{source} refused access (HTTP {status}): check the key or account"
                )
            }
            SourceError::MissingKey { source } => write!(f, "{source} needs an API key; skipped"),
            SourceError::BadResponse { source, detail } => {
                write!(f, "{source} sent an unreadable answer: {detail}")
            }
            SourceError::Exhausted { source, status } => {
                write!(
                    f,
                    "{source} daily quota used up (HTTP {status}); skipped for this run"
                )
            }
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

impl SourceError {
    /// Whether the error is about the source itself (its key, account or
    /// daily quota), not this ROM: gathering stops asking that source for
    /// the rest of the run, and the run goes on with the other sources
    /// (never a pause).
    pub fn is_source_wide(&self) -> bool {
        matches!(
            self,
            SourceError::Auth { .. }
                | SourceError::MissingKey { .. }
                | SourceError::Exhausted { .. }
        )
    }
}

/// What one cheap authenticated request said about a key ("Test key").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyCheck {
    Ok,
    Invalid,
    Missing,
    Offline,
    RateLimited,
    Unavailable,
}

impl KeyCheck {
    /// The "Test key" answer for the error a check request ended with.
    pub fn of_error(e: &SourceError) -> KeyCheck {
        match e {
            SourceError::Auth { .. } => KeyCheck::Invalid,
            SourceError::MissingKey { .. } => KeyCheck::Missing,
            SourceError::Exhausted { .. } | SourceError::Net(NetError::RateLimited { .. }) => {
                KeyCheck::RateLimited
            }
            SourceError::Net(NetError::Offline(_) | NetError::Timeout) => KeyCheck::Offline,
            _ => KeyCheck::Unavailable,
        }
    }
}

/// Text from the network made safe to show: control and bidi-override
/// characters dropped, trimmed, at most `max_chars`; `None` when empty.
pub fn clean_text(s: &str, max_chars: usize) -> Option<String> {
    let bidi =
        |c: char| ('\u{202A}'..='\u{202E}').contains(&c) || ('\u{2066}'..='\u{2069}').contains(&c);
    let t: String = s.chars().filter(|&c| !c.is_control() && !bidi(c)).collect();
    let t: String = t.trim().chars().take(max_chars).collect();
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
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

    #[test]
    fn new_errors_read_well_and_only_key_errors_are_source_wide() {
        let auth = SourceError::Auth {
            source: "steamgriddb".into(),
            status: 401,
        };
        assert_eq!(
            auth.to_string(),
            "steamgriddb refused access (HTTP 401): check the key or account"
        );
        let missing = SourceError::MissingKey {
            source: "steamgriddb".into(),
        };
        assert_eq!(missing.to_string(), "steamgriddb needs an API key; skipped");
        let bad = SourceError::BadResponse {
            source: "screenscraper".into(),
            detail: "not JSON".into(),
        };
        assert_eq!(
            bad.to_string(),
            "screenscraper sent an unreadable answer: not JSON"
        );
        let quota = SourceError::Exhausted {
            source: "screenscraper".into(),
            status: 430,
        };
        assert_eq!(
            quota.to_string(),
            "screenscraper daily quota used up (HTTP 430); skipped for this run"
        );
        assert!(auth.is_source_wide() && missing.is_source_wide() && quota.is_source_wide());
        assert!(!bad.is_source_wide());
        assert!(!SourceError::from(NetError::Timeout).is_source_wide());
    }

    #[test]
    fn votes_score_is_up_minus_down() {
        assert_eq!(Votes { up: 3, down: 5 }.score(), -2);
        assert_eq!(
            Votes {
                up: u32::MAX,
                down: 0
            }
            .score(),
            i64::from(u32::MAX)
        );
    }

    #[test]
    fn a_candidate_cached_before_m6_still_reads_and_origin_is_omitted_when_empty() {
        let old = r#"{"source":"libretro","media":"cover","name":"Zelda (USA)","url":"https://x/z.png",
            "fallback_url":null,"regions":["usa"],"kind":"canonical","score":100,"votes":null,"consensus":0}"#;
        let c: Candidate = serde_json::from_str(old).unwrap();
        assert_eq!(c.votes, None);
        assert_eq!(c.origin, None);
        assert!(!serde_json::to_string(&c).unwrap().contains("origin"));
    }

    #[test]
    fn key_check_of_errors() {
        use KeyCheck::*;
        assert_eq!(
            KeyCheck::of_error(&SourceError::Auth {
                source: "s".into(),
                status: 401
            }),
            Invalid
        );
        assert_eq!(
            KeyCheck::of_error(&SourceError::MissingKey { source: "s".into() }),
            Missing
        );
        assert_eq!(KeyCheck::of_error(&NetError::Timeout.into()), Offline);
        assert_eq!(
            KeyCheck::of_error(
                &NetError::RateLimited {
                    host: "h".into(),
                    status: 429
                }
                .into()
            ),
            RateLimited
        );
        assert_eq!(
            KeyCheck::of_error(&SourceError::Exhausted {
                source: "s".into(),
                status: 430
            }),
            RateLimited
        );
        assert_eq!(
            KeyCheck::of_error(&SourceError::NotFound("u".into())),
            Unavailable
        );
        assert_eq!(
            serde_json::to_string(&RateLimited).unwrap(),
            "\"rate-limited\""
        );
    }

    #[test]
    fn clean_text_drops_controls_and_bidi_and_caps() {
        assert_eq!(
            clean_text("  Doom\u{202E}\u{0007} ", 200).as_deref(),
            Some("Doom")
        );
        assert_eq!(clean_text("\u{0000}", 200), None);
        assert_eq!(
            clean_text(&"x".repeat(300), 200).unwrap().chars().count(),
            200
        );
    }
}
