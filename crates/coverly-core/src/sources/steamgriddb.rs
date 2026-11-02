//! SteamGridDB (spec §6, D9, D35, M6-01): v2 API with the user's own
//! Bearer key. Covers only: `/search/autocomplete/{title}` → game ids,
//! `/grids/game/{id}` (static, portrait 600x900 then 342x482, nsfw/humor/
//! epilepsy off) → one candidate per grid with its votes. Answers are
//! hostile: parsed element by element, capped, image URLs only on
//! steamgriddb.com hosts. 401/403 = this source refuses the key (never a
//! pause); a 429 or going offline pauses like libretro.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::libretro::encode_segment;
use super::{
    clean_text, sort_candidates, ArtQuery, ArtSource, Candidate, KeyCheck, MatchKind, SourceError,
    Votes,
};
use crate::cache::{now_unix, Cache};
use crate::credentials::Secret;
use crate::identify::names::{compatible, parse_name, score, ParsedName};
use crate::net::{host_of, plain_authority, Http, HttpRequest, NetError};
use crate::profiles::Media;
use crate::systems::System;

pub const STEAMGRIDDB_BASE: &str = crate::net::hosts::STEAMGRIDDB_BASE;
pub const SOURCE_ID: &str = "steamgriddb";
pub const COVER_DIMENSIONS: [&str; 2] = ["600x900", "342x482"];
/// Score of an exact title that several games share (review, D35).
pub const AMBIGUOUS_SCORE: u8 = 85;
pub const SEARCH_TTL_SECS: i64 = 7 * 24 * 3600;
pub const GRIDS_TTL_SECS: i64 = 2 * 24 * 3600;
pub const MAX_JSON_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
/// Most games whose grids are taken for one ROM.
const MAX_GAMES: usize = 2;
/// Most search results kept (and cached) per term.
const MAX_SEARCH_RESULTS: usize = 20;
/// Most grids kept (and cached) per game: a hostile answer with thousands
/// of entries never fills the cache.
pub const MAX_GRIDS: usize = 50;
const MAX_NAME_CHARS: usize = 200;
const MAX_STYLE_CHARS: usize = 32;
// Keys carry the base and the request path (with the filters), so a
// changed setting or server never reuses another answer.
const NS_SEARCH: &str = "sgdb-search-2";
const NS_GRIDS: &str = "sgdb-grids-2";

#[derive(Clone, Debug)]
pub struct SgdbConfig {
    pub base: String,
    pub search_ttl_secs: i64,
    pub grids_ttl_secs: i64,
    pub nsfw: bool,
    pub humor: bool,
    pub epilepsy: bool,
}

impl Default for SgdbConfig {
    fn default() -> Self {
        SgdbConfig {
            base: STEAMGRIDDB_BASE.into(),
            search_ttl_secs: SEARCH_TTL_SECS,
            grids_ttl_secs: GRIDS_TTL_SECS,
            nsfw: false,
            humor: false,
            epilepsy: false,
        }
    }
}

/// `/search/autocomplete/<term>` with the term as one encoded segment.
pub fn search_path(term: &str) -> String {
    format!("/search/autocomplete/{}", encode_segment(term))
}

/// `/grids/game/<id>?…` for portrait static covers with the filters.
pub fn grids_path(game_id: u64, cfg: &SgdbConfig) -> String {
    format!(
        "/grids/game/{game_id}?dimensions={}&types=static&nsfw={}&humor={}&epilepsy={}",
        COVER_DIMENSIONS.join(","),
        cfg.nsfw,
        cfg.humor,
        cfg.epilepsy
    )
}

/// One cheap authenticated request ("Test key" in Settings): `GET
/// {base}/search/autocomplete/mario`. A 2xx JSON answer with `"success":
/// true` → `Ok` (any other 2xx, such as a captive portal page, cannot
/// confirm the key → `Unavailable`), 401/403 → `Invalid`, a
/// rate limit → `RateLimited`, offline/timeout → `Offline`, anything else
/// → `Unavailable`; no key → `Missing` without any request. Never returns
/// or logs the key; a key that cannot be sent as a header is `Invalid`
/// without a request, like [`SteamGridDbSource`] does.
pub fn check_key(http: &dyn Http, base: &str, key: Option<&Secret>) -> KeyCheck {
    let key = match key {
        Some(k) if !k.expose().is_empty() => k,
        _ => return KeyCheck::Missing,
    };
    if !key.expose().chars().all(|c| c.is_ascii_graphic()) {
        return KeyCheck::Invalid;
    }
    let url = format!("{}{}", base.trim_end_matches('/'), search_path("mario"));
    let req = HttpRequest::get(url)
        .bearer(key.clone())
        .forbidden_passes()
        .max_bytes(MAX_JSON_BYTES);
    match http.get(&req) {
        Ok(r) if (200..300).contains(&r.status) => {
            let success = serde_json::from_slice::<Value>(&r.body)
                .ok()
                .and_then(|v| v.get("success").and_then(Value::as_bool));
            if success == Some(true) {
                KeyCheck::Ok
            } else {
                KeyCheck::Unavailable
            }
        }
        Ok(r) if r.status == 401 || r.status == 403 => KeyCheck::Invalid,
        Ok(_) => KeyCheck::Unavailable,
        Err(e) => KeyCheck::of_error(&SourceError::Net(e)),
    }
}

/// One search result (only what is used is kept, and cached).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Game {
    id: u64,
    name: String,
}

/// One grid (only what is used is kept, and cached).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Grid {
    id: u64,
    url: String,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    style: String,
    #[serde(default)]
    mime: String,
    #[serde(default)]
    nsfw: bool,
    #[serde(default)]
    humor: bool,
    #[serde(default)]
    epilepsy: bool,
    #[serde(default)]
    upvotes: Option<u32>,
    #[serde(default)]
    downvotes: Option<u32>,
    #[serde(default)]
    score: Option<i64>,
}

impl Grid {
    /// Up/down votes; an answer with only a net `score` reads as that.
    fn votes(&self) -> Votes {
        match (self.upvotes, self.downvotes, self.score) {
            (None, None, Some(s)) => Votes {
                up: u32::try_from(s.max(0)).unwrap_or(u32::MAX),
                down: u32::try_from(s.saturating_neg().max(0)).unwrap_or(u32::MAX),
            },
            (up, down, _) => Votes {
                up: up.unwrap_or(0),
                down: down.unwrap_or(0),
            },
        }
    }

    /// Position of the grid's size in [`COVER_DIMENSIONS`] (others last).
    fn dimension_rank(&self) -> usize {
        let d = format!("{}x{}", self.width, self.height);
        COVER_DIMENSIONS
            .iter()
            .position(|x| *x == d)
            .unwrap_or(COVER_DIMENSIONS.len())
    }
}

/// Whitespace, control and bidi characters never belong in an image URL
/// that is shown, cached and fetched.
fn unsafe_url_char(c: char) -> bool {
    c.is_whitespace()
        || c.is_control()
        || ('\u{202A}'..='\u{202E}').contains(&c)
        || ('\u{2066}'..='\u{2069}').contains(&c)
        || matches!(c, '\u{200E}' | '\u{200F}' | '\u{061C}')
}

pub struct SteamGridDbSource<'a> {
    http: &'a dyn Http,
    cache: &'a Cache,
    key: Option<Secret>,
    cfg: SgdbConfig,
}

fn bad(detail: impl Into<String>) -> SourceError {
    SourceError::BadResponse {
        source: SOURCE_ID.into(),
        detail: detail.into(),
    }
}

impl<'a> SteamGridDbSource<'a> {
    pub fn new(
        http: &'a dyn Http,
        cache: &'a Cache,
        key: Option<Secret>,
        mut cfg: SgdbConfig,
    ) -> Self {
        cfg.base = cfg.base.trim_end_matches('/').to_string();
        SteamGridDbSource {
            http,
            cache,
            key,
            cfg,
        }
    }

    /// The stored key, if it can be sent: an empty one is missing, and one
    /// with anything but visible ASCII (a pasted line break, a space) is
    /// refused here like the API would, never sent as a broken header.
    fn usable_key(&self) -> Result<Secret, SourceError> {
        let key = match &self.key {
            Some(k) if !k.expose().is_empty() => k.clone(),
            _ => {
                return Err(SourceError::MissingKey {
                    source: SOURCE_ID.into(),
                })
            }
        };
        if !key.expose().chars().all(|c| c.is_ascii_graphic()) {
            return Err(SourceError::Auth {
                source: SOURCE_ID.into(),
                status: 0,
            });
        }
        Ok(key)
    }

    /// The `data` list of an API answer; `None` for a 404.
    fn get_data(&self, path: &str) -> Result<Option<Vec<Value>>, SourceError> {
        let key = self.usable_key()?;
        let url = format!("{}{path}", self.cfg.base);
        let req = HttpRequest::get(url.clone())
            .bearer(key)
            .forbidden_passes()
            .max_bytes(MAX_JSON_BYTES);
        let resp = self.http.get(&req)?;
        match resp.status {
            200..=299 => {
                let v: Value = serde_json::from_slice(&resp.body).map_err(|_| bad("not JSON"))?;
                if v.get("success").and_then(Value::as_bool) != Some(true) {
                    return Err(bad("the answer is not a success"));
                }
                match v.get("data") {
                    Some(Value::Array(a)) => Ok(Some(a.clone())),
                    _ => Err(bad("no data list")),
                }
            }
            401 | 403 => Err(SourceError::Auth {
                source: SOURCE_ID.into(),
                status: resp.status,
            }),
            404 => Ok(None),
            status => Err(NetError::Status { status, url }.into()),
        }
    }

    /// A fresh cached value, if any (a stale or unreadable one is a miss).
    fn cached<T: serde::de::DeserializeOwned>(&self, ns: &str, key: &str, ttl: i64) -> Option<T> {
        match self.cache.get_json::<T>(ns, key) {
            Ok(Some(s)) if now_unix() - s.fetched_at < ttl => Some(s.value),
            _ => None,
        }
    }

    fn search(&self, term: &str) -> Result<Vec<Game>, SourceError> {
        let key = format!("{}{}", self.cfg.base, search_path(&term.to_lowercase()));
        if let Some(g) = self.cached(NS_SEARCH, &key, self.cfg.search_ttl_secs) {
            return Ok(g);
        }
        // A 404 is not remembered: only real answers are cached.
        let Some(data) = self.get_data(&search_path(term))? else {
            return Ok(Vec::new());
        };
        let games: Vec<Game> = data
            .into_iter()
            .filter_map(|v| serde_json::from_value::<Game>(v).ok())
            .filter_map(|g| {
                Some(Game {
                    name: clean_text(&g.name, MAX_NAME_CHARS)?,
                    ..g
                })
            })
            .take(MAX_SEARCH_RESULTS)
            .collect();
        let _ = self.cache.put_json(NS_SEARCH, &key, &games, None);
        Ok(games)
    }

    fn grids(&self, game: u64) -> Result<Vec<Grid>, SourceError> {
        let path = grids_path(game, &self.cfg);
        let key = format!("{}{path}", self.cfg.base);
        if let Some(g) = self.cached(NS_GRIDS, &key, self.cfg.grids_ttl_secs) {
            return Ok(g);
        }
        let Some(data) = self.get_data(&path)? else {
            return Ok(Vec::new());
        };
        let grids: Vec<Grid> = data
            .into_iter()
            .filter_map(|v| serde_json::from_value::<Grid>(v).ok())
            .filter(|g| self.allowed_image_url(&g.url))
            .take(MAX_GRIDS)
            .collect();
        let _ = self.cache.put_json(NS_GRIDS, &key, &grids, None);
        Ok(grids)
    }

    /// Image URLs only on steamgriddb.com over HTTPS, under a registered
    /// endpoint prefix (`net::hosts`, so the client never refuses a kept
    /// candidate). A base that is not
    /// HTTPS can only be a loopback mock (the client refuses anything else),
    /// so only then are images on the base's own host accepted (tests).
    /// The authority must be a plain `host[:port]` ([`plain_authority`]):
    /// `evil.com\\.steamgriddb.com` would otherwise pass the suffix test.
    /// Whitespace, control or bidi characters anywhere drop the URL.
    fn allowed_image_url(&self, url: &str) -> bool {
        let host = host_of(url);
        let https = url.to_ascii_lowercase().starts_with("https://");
        let sgdb = host == "steamgriddb.com" || host.ends_with(".steamgriddb.com");
        let mock_base = !self.cfg.base.to_ascii_lowercase().starts_with("https://")
            && host == host_of(&self.cfg.base);
        plain_authority(url)
            && !host.is_empty()
            && !url.chars().any(unsafe_url_char)
            && ((https && sgdb && crate::net::hosts::allowed(url, &[])) || mock_base)
    }

    fn grid_candidate(
        &self,
        game: &Game,
        g: &Grid,
        kind: MatchKind,
        score: u8,
    ) -> Option<Candidate> {
        let filtered = (g.nsfw && !self.cfg.nsfw)
            || (g.humor && !self.cfg.humor)
            || (g.epilepsy && !self.cfg.epilepsy);
        let image = matches!(g.mime.as_str(), "image/png" | "image/jpeg" | "image/webp");
        if filtered || !image || !self.allowed_image_url(&g.url) {
            return None;
        }
        let style = clean_text(&g.style, MAX_STYLE_CHARS).unwrap_or_default();
        let origin = format!(
            "game {} · grid {} · {}x{} {style}",
            game.id, g.id, g.width, g.height
        );
        Some(Candidate {
            source: SOURCE_ID.into(),
            media: Media::Cover,
            name: game.name.clone(),
            url: g.url.clone(),
            fallback_url: None,
            regions: Vec::new(),
            kind,
            score,
            votes: Some(g.votes()),
            consensus: 0,
            origin: Some(origin.trim_end().to_string()),
        })
    }

    /// The games to take grids from, with how they matched: one exact
    /// title → title-exact; several → fuzzy at [`AMBIGUOUS_SCORE`]; none →
    /// the best fuzzy titles when fuzzy matching is on.
    fn matched_games(
        &self,
        wanted: &ParsedName,
        games: &[Game],
        q: &ArtQuery,
    ) -> Vec<(Game, MatchKind, u8)> {
        let parsed: Vec<(Game, ParsedName)> = games
            .iter()
            .map(|g| (g.clone(), parse_name(&g.name, false)))
            .collect();
        let exact: Vec<&Game> = parsed
            .iter()
            .filter(|(_, p)| p.key == wanted.key)
            .map(|(g, _)| g)
            .collect();
        match exact.len() {
            1 => vec![(exact[0].clone(), MatchKind::TitleExact, 95)],
            0 if q.fuzzy => {
                let mut scored: Vec<(u8, &Game)> = parsed
                    .iter()
                    .filter(|(_, p)| compatible(wanted, p))
                    .map(|(g, p)| (score(&wanted.key, &p.key), g))
                    .filter(|(s, _)| *s >= q.min_score)
                    .collect();
                scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
                scored
                    .into_iter()
                    .take(MAX_GAMES)
                    .map(|(s, g)| (g.clone(), MatchKind::Fuzzy, s.min(94)))
                    .collect()
            }
            0 => Vec::new(),
            _ => exact
                .into_iter()
                .take(MAX_GAMES)
                .map(|g| (g.clone(), MatchKind::Fuzzy, AMBIGUOUS_SCORE))
                .collect(),
        }
    }
}

impl ArtSource for SteamGridDbSource<'_> {
    fn id(&self) -> &str {
        SOURCE_ID
    }

    fn candidates(&self, q: &ArtQuery) -> Result<Vec<Candidate>, SourceError> {
        if q.media != Media::Cover || !q.by_name {
            return Ok(Vec::new());
        }
        let wanted = match q.canonical {
            Some(c) => parse_name(c, false),
            None => q.rom.clone(),
        };
        if wanted.key.is_empty() || wanted.title.trim().is_empty() {
            return Ok(Vec::new());
        }
        let games = self.search(wanted.title.trim())?;
        let mut out = Vec::new();
        for (game, kind, score) in self.matched_games(&wanted, &games, q) {
            let mut grids = self.grids(game.id)?;
            grids.sort_by(|a, b| {
                b.votes()
                    .score()
                    .cmp(&a.votes().score())
                    .then(a.dimension_rank().cmp(&b.dimension_rank()))
                    .then(a.id.cmp(&b.id))
            });
            out.extend(
                grids
                    .iter()
                    .filter_map(|g| self.grid_candidate(&game, g, kind, score))
                    .take(q.max_candidates.max(1)),
            );
        }
        // Stable: grids keep their vote order inside one kind and score.
        sort_candidates(&mut out);
        Ok(out)
    }

    fn has_name(&self, _: &System, _: Media, _: &str) -> Result<bool, SourceError> {
        Ok(false)
    }

    /// The image from the CDN, without the key.
    fn fetch(&self, c: &Candidate) -> Result<Vec<u8>, SourceError> {
        if !self.allowed_image_url(&c.url) {
            return Err(SourceError::NotFound(c.url.clone()));
        }
        let r = self
            .http
            .get(&HttpRequest::get(c.url.clone()).max_bytes(MAX_IMAGE_BYTES))?;
        if (200..300).contains(&r.status) && !r.body.is_empty() {
            Ok(r.body)
        } else {
            Err(SourceError::NotFound(c.url.clone()))
        }
    }
}
