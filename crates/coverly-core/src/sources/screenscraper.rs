//! ScreenScraper (M6-02, v1.1; research screenscraper-api.md, D36).
//! Hidden unless the build carries developer credentials. Candidate URLs
//! are built here without credentials; `fetch` appends them, so no secret
//! reaches the cache, a plan, a journal or the webview. Its own throttle:
//! ≥ 1.2 s between request starts, 1 thread until `ssuser.maxthreads`
//! says more (clamped 1–6). The daily quota (430/431, or `requeststoday ≥
//! maxrequestsperday`) stops this source for the run, never pauses it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    clean_text, sort_candidates, ArtQuery, ArtSource, Candidate, KeyCheck, MatchKind, SourceError,
};
use crate::cache::{now_unix, Cache};
use crate::credentials::{DevCredentials, UserCredentials};
use crate::identify::hash::HashOutcome;
use crate::identify::names::{parse_name, Region};
use crate::net::{host_of, Http, HttpRequest, HttpResponse, Limiter, NetError};
use crate::profiles::Media;
use crate::systems::System;

pub const SCREENSCRAPER_BASE: &str = crate::net::hosts::SCREENSCRAPER_BASE;
pub const SOURCE_ID: &str = "screenscraper";
pub const SOFTNAME: &str = concat!("CoverlyROMs-", env!("CARGO_PKG_VERSION"));
pub const MIN_INTERVAL: Duration = Duration::from_millis(1200);
pub const GAME_TTL_SECS: i64 = 7 * 24 * 3600;
const MAX_JSON_BYTES: u64 = 4 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_NAME_CHARS: usize = 200;
const MAX_THREADS: u64 = 6;
// Keys carry the base, the system and the request parameters (never a
// credential), so another server or ROM never reuses an answer.
const NS_GAME: &str = "ss-game-1";
const MEDIA_TYPES: [&str; 4] = ["box-2D", "flyer", "ss", "sstitle"];
/// Bodies `mediaJeu.php` sends instead of an image.
const NO_IMAGE_BODIES: [&[u8]; 4] = [b"NOMEDIA", b"CRCOK", b"MD5OK", b"SHA1OK"];
/// Query values: everything but unreserved characters is percent-encoded.
pub const QUERY_VALUE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

#[derive(Clone, Debug)]
pub struct SsConfig {
    pub base: String,
    pub min_interval: Duration,
    pub ttl_secs: i64,
}

impl Default for SsConfig {
    fn default() -> Self {
        SsConfig {
            base: SCREENSCRAPER_BASE.into(),
            min_interval: MIN_INTERVAL,
            ttl_secs: GAME_TTL_SECS,
        }
    }
}

/// The ScreenScraper media type for a Coverly media (arcade covers are
/// flyers).
pub fn media_type(media: Media, arcade: bool) -> &'static str {
    match media {
        Media::Cover if arcade => "flyer",
        Media::Cover => "box-2D",
        Media::Screenshot => "ss",
        Media::Title => "sstitle",
    }
}

/// A ScreenScraper region code as a Coverly region (`None` = unknown).
pub fn ss_region(code: &str) -> Option<Region> {
    use Region::*;
    Some(match code {
        "wor" => World,
        "us" => Usa,
        "eu" => Europe,
        "jp" => Japan,
        "asi" => Asia,
        "au" => Australia,
        "br" => Brazil,
        "ca" => Canada,
        "cn" => China,
        "fr" => France,
        "de" => Germany,
        "it" => Italy,
        "kr" => Korea,
        "nl" => Netherlands,
        "sp" => Spain,
        "se" => Sweden,
        "tw" => Taiwan,
        "uk" => UnitedKingdom,
        _ => return None,
    })
}

fn enc(v: &str) -> String {
    utf8_percent_encode(v, QUERY_VALUE).to_string()
}

fn bad(detail: impl Into<String>) -> SourceError {
    SourceError::BadResponse {
        source: SOURCE_ID.into(),
        detail: detail.into(),
    }
}

/// One game as kept (and cached): only what is used.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SsGame {
    id: String,
    /// (region, text)
    names: Vec<(String, String)>,
    /// (type, region)
    medias: Vec<(String, String)>,
    /// The game's `systeme.id` (absent in answers cached before it was kept).
    #[serde(default)]
    system: Option<u32>,
    /// Upper-case CRCs of the ROMs the answer says it matched (`rom`,
    /// `roms`); empty when it names none.
    #[serde(default)]
    rom_crcs: Vec<String>,
}

impl SsGame {
    /// One display name per game: `wor`, else `us`, else the first.
    fn name(&self) -> Option<&str> {
        ["wor", "us"]
            .iter()
            .find_map(|r| self.names.iter().find(|(nr, _)| nr == r))
            .or_else(|| self.names.first())
            .map(|(_, t)| t.as_str())
    }
}

/// A string or a number as text (the API sends numbers as strings).
fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn num(v: &Value) -> Option<u64> {
    text(v)?.trim().parse().ok()
}

fn region_ok(r: &str) -> bool {
    (2..=3).contains(&r.len()) && r.bytes().all(|b| b.is_ascii_lowercase())
}

/// A game object, hostile-safe: digits-only id (≤ 12), region codes of
/// 2–3 lowercase letters, known media types of parent `jeu` only, no
/// duplicates.
fn parse_game(v: &Value) -> Option<SsGame> {
    let id = text(v.get("id")?)?;
    if id.is_empty() || id.len() > 12 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let names = v
        .get("noms")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let region = text(n.get("region")?)?;
            let name = clean_text(&text(n.get("text")?)?, MAX_NAME_CHARS)?;
            region_ok(&region).then_some((region, name))
        })
        .collect();
    let mut medias: Vec<(String, String)> = Vec::new();
    for m in v
        .get("medias")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let field = |k: &str| m.get(k).and_then(text);
        let (Some(t), Some(r), Some(p)) = (field("type"), field("region"), field("parent")) else {
            continue;
        };
        let entry = (t, r);
        if p == "jeu"
            && MEDIA_TYPES.contains(&entry.0.as_str())
            && region_ok(&entry.1)
            && !medias.contains(&entry)
        {
            medias.push(entry);
        }
    }
    let system = v
        .get("systeme")
        .and_then(|s| s.get("id"))
        .and_then(num)
        .and_then(|n| u32::try_from(n).ok());
    let mut rom_crcs: Vec<String> = Vec::new();
    let roms = v.get("rom").into_iter().chain(
        v.get("roms")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
    );
    for r in roms.take(MAX_ROM_CRCS) {
        let Some(crc) = r.get("romcrc").and_then(text) else {
            continue;
        };
        let crc = crc.trim().to_ascii_uppercase();
        if crc.len() == 8 && crc.bytes().all(|b| b.is_ascii_hexdigit()) && !rom_crcs.contains(&crc)
        {
            rom_crcs.push(crc);
        }
    }
    Some(SsGame {
        id,
        names,
        medias,
        system,
        rom_crcs,
    })
}

/// Most ROM CRCs kept from one answer (hostile-safe bound).
const MAX_ROM_CRCS: usize = 64;

/// Query parameters that identify a ROM by its hashes; `None` when it was
/// not hashed (too large or name-only: search by title instead).
fn hash_params(file_name: &str, h: &HashOutcome) -> Option<Vec<(&'static str, String)>> {
    match h {
        HashOutcome::Content(r) => Some(vec![
            ("romtype", "rom".into()),
            ("romnom", file_name.into()),
            ("romtaille", r.full.size.to_string()),
            ("crc", r.full.crc32.to_ascii_uppercase()),
            ("md5", r.full.md5.to_ascii_uppercase()),
            ("sha1", r.full.sha1.to_ascii_uppercase()),
        ]),
        HashOutcome::ZipEntry {
            crc32,
            size,
            inner_name,
            ..
        } => Some(vec![
            ("romtype", "rom".into()),
            ("romnom", inner_name.clone()),
            ("romtaille", size.to_string()),
            ("crc", crc32.to_ascii_uppercase()),
        ]),
        HashOutcome::TooLarge { .. } | HashOutcome::NameOnly => None,
    }
}

pub struct ScreenScraperSource<'a> {
    http: &'a dyn Http,
    cache: &'a Cache,
    dev: DevCredentials,
    user: Option<UserCredentials>,
    cfg: SsConfig,
    /// At least `min_interval` between request starts; never replaced, so
    /// the spacing holds when the thread count changes.
    spacing: Arc<Limiter>,
    /// Requests in flight: 1 until `ssuser.maxthreads` says more.
    threads: Mutex<Arc<Limiter>>,
    exhausted: AtomicBool,
}

impl<'a> ScreenScraperSource<'a> {
    pub fn new(
        http: &'a dyn Http,
        cache: &'a Cache,
        dev: DevCredentials,
        user: Option<UserCredentials>,
        mut cfg: SsConfig,
    ) -> Self {
        cfg.base = cfg.base.trim_end_matches('/').to_string();
        ScreenScraperSource {
            http,
            cache,
            dev,
            user,
            spacing: Limiter::new(MAX_THREADS as usize, cfg.min_interval),
            threads: Mutex::new(Limiter::new(1, Duration::ZERO)),
            cfg,
            exhausted: AtomicBool::new(false),
        }
    }

    /// How many requests may run at once (from `ssuser.maxthreads`).
    pub fn threads(&self) -> usize {
        self.thread_limiter().capacity()
    }

    fn thread_limiter(&self) -> Arc<Limiter> {
        self.threads
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// The credential part of a query (dev, software, output, user).
    fn creds(&self) -> String {
        let mut s = format!(
            "devid={}&devpassword={}&softname={}&output=json",
            enc(self.dev.id.expose()),
            enc(self.dev.password.expose()),
            enc(SOFTNAME)
        );
        if let Some(u) = &self.user {
            s.push_str(&format!(
                "&ssid={}&sspassword={}",
                enc(u.user.expose()),
                enc(u.password.expose())
            ));
        }
        s
    }

    fn query(params: &[(&str, String)]) -> String {
        params
            .iter()
            .map(|(k, v)| format!("&{k}={}", enc(v)))
            .collect()
    }

    fn api_url(&self, endpoint: &str, params: &[(&str, String)]) -> String {
        format!(
            "{}/{endpoint}?{}{}",
            self.cfg.base,
            self.creds(),
            Self::query(params)
        )
    }

    /// `s` with every credential (raw and percent-encoded) replaced by `***`.
    fn scrub(&self, s: &str) -> String {
        let mut secrets = vec![self.dev.id.expose(), self.dev.password.expose()];
        if let Some(u) = &self.user {
            secrets.push(u.user.expose());
            secrets.push(u.password.expose());
        }
        let mut out = s.to_string();
        for secret in secrets.into_iter().filter(|s| !s.is_empty()) {
            out = out.replace(&enc(secret), "***").replace(secret, "***");
        }
        out
    }

    fn exhausted(&self, status: u16) -> SourceError {
        SourceError::Exhausted {
            source: SOURCE_ID.into(),
            status,
        }
    }

    /// One request, throttled, with errors that never show a credential.
    fn send(&self, url: &str, max: u64) -> Result<HttpResponse, SourceError> {
        if self.exhausted.load(Ordering::SeqCst) {
            return Err(self.exhausted(430));
        }
        let public = url.split('?').next().unwrap_or_default().to_string();
        let threads = self.thread_limiter();
        let resp = {
            let _slot = threads.acquire();
            let _start = self.spacing.acquire();
            self.http
                .get(&HttpRequest::get(url).forbidden_passes().max_bytes(max))
        };
        let resp = resp.map_err(|e| match e {
            NetError::Status { status, .. } => NetError::Status {
                status,
                url: public.clone(),
            },
            NetError::Offline(m) => NetError::Offline(self.scrub(&m)),
            NetError::Invalid(m) => NetError::Invalid(self.scrub(&m)),
            NetError::Insecure(_) => NetError::Insecure(self.cfg.base.clone()),
            NetError::Unregistered(_) => NetError::Unregistered(self.scrub(&public)),
            other => other,
        })?;
        match resp.status {
            200..=299 | 404 => Ok(resp),
            // The API is closed for now (or for non-members): pause.
            401 | 423 => Err(NetError::RateLimited {
                host: host_of(url),
                status: resp.status,
            }
            .into()),
            430 | 431 => {
                self.exhausted.store(true, Ordering::SeqCst);
                Err(self.exhausted(resp.status))
            }
            403 | 426 => Err(SourceError::Auth {
                source: SOURCE_ID.into(),
                status: resp.status,
            }),
            400 => Err(bad("request refused (HTTP 400)")),
            status => Err(NetError::Status {
                status,
                url: public,
            }
            .into()),
        }
    }

    /// The JSON of an answer (`None` for a 404), noting the account limits.
    fn read_json(&self, resp: &HttpResponse) -> Result<Option<Value>, SourceError> {
        if resp.status == 404 {
            return Ok(None);
        }
        let v: Value = serde_json::from_slice(&resp.body).map_err(|_| bad("not JSON"))?;
        self.note_user(&v["response"]["ssuser"]);
        Ok(Some(v))
    }

    /// `maxthreads` sets the thread count; a used-up daily quota stops the
    /// next request (the answer in hand is still used).
    fn note_user(&self, user: &Value) {
        if let Some(n) = user.get("maxthreads").and_then(num) {
            let n = n.clamp(1, MAX_THREADS) as usize;
            if n != self.threads() {
                *self.threads.lock().unwrap_or_else(|p| p.into_inner()) =
                    Limiter::new(n, Duration::ZERO);
            }
        }
        let today = user.get("requeststoday").and_then(num);
        let max = user.get("maxrequestsperday").and_then(num);
        if let (Some(today), Some(max)) = (today, max) {
            if max > 0 && today >= max {
                self.exhausted.store(true, Ordering::SeqCst);
            }
        }
    }

    /// A fresh cached answer, if any (stale or unreadable = a miss).
    fn cached(&self, key: &str) -> Option<Option<SsGame>> {
        match self.cache.get_json::<Option<SsGame>>(NS_GAME, key) {
            Ok(Some(s)) if now_unix() - s.fetched_at < self.cfg.ttl_secs => Some(s.value),
            _ => None,
        }
    }

    /// The game for `q`: by hash (canonical, 100) when the ROM was hashed,
    /// else by title when names may be used (title-exact, 95).
    fn game(&self, sid: u32, q: &ArtQuery) -> Result<Option<(SsGame, MatchKind, u8)>, SourceError> {
        if let Some(h) = q.hash.filter(|h| hash_params(q.file_name, h).is_some()) {
            let found = self.game_by_hash(sid, q.file_name, h)?;
            return Ok(found.map(|g| (g, MatchKind::Canonical, 100)));
        }
        let mut params = vec![("systemeid", sid.to_string())];
        if !q.by_name {
            return Ok(None);
        }
        let wanted = match q.canonical {
            Some(c) => parse_name(c, false),
            None => q.rom.clone(),
        };
        let title = wanted.title.trim().to_string();
        if wanted.key.is_empty() || title.is_empty() {
            return Ok(None);
        }
        params.push(("recherche", title));
        let found = self.lookup("jeuRecherche.php", &params, &wanted.key, |v| {
            v["response"]
                .get("jeux")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(parse_game)
                .find(|g| {
                    g.names
                        .iter()
                        .any(|(_, n)| parse_name(n, false).key == wanted.key)
                })
        })?;
        Ok(found.map(|g| (g, MatchKind::TitleExact, 95)))
    }

    /// The game `jeuInfos.php` names for a hashed ROM on system `sid`
    /// (`None` when the ROM was not hashed or the server does not know it).
    fn game_by_hash(
        &self,
        sid: u32,
        file_name: &str,
        hash: &HashOutcome,
    ) -> Result<Option<SsGame>, SourceError> {
        let Some(h) = hash_params(file_name, hash) else {
            return Ok(None);
        };
        let mut params = vec![("systemeid", sid.to_string())];
        params.extend(h);
        self.lookup("jeuInfos.php", &params, "", |v| {
            v["response"].get("jeu").and_then(parse_game)
        })
    }

    /// The game name for a ROM hash on system `sid` (cached like every
    /// ScreenScraper answer; organise asks it when offline levels are not
    /// enough). `None` for an unhashed ROM, an unknown hash, or an answer
    /// for another system or another ROM's CRC.
    pub fn identify_by_hash(
        &self,
        sid: u32,
        file_name: &str,
        hash: &HashOutcome,
    ) -> Result<Option<String>, SourceError> {
        let Some(game) = self.game_by_hash(sid, file_name, hash)? else {
            return Ok(None);
        };
        // The system is the question: an answer for another system (the
        // server may match across systems) says nothing about it.
        if game.system.is_some_and(|s| s != sid) {
            return Ok(None);
        }
        // An answer that names the ROMs it matched must name our CRC; else
        // the server matched something else (by file name, say).
        let crc = match hash {
            HashOutcome::Content(r) => r.full.crc32.to_ascii_uppercase(),
            HashOutcome::ZipEntry { crc32, .. } => crc32.to_ascii_uppercase(),
            HashOutcome::TooLarge { .. } | HashOutcome::NameOnly => return Ok(None),
        };
        if !game.rom_crcs.is_empty() && !game.rom_crcs.contains(&crc) {
            return Ok(None);
        }
        Ok(game.name().map(str::to_string))
    }

    /// A cached or fresh answer of `endpoint`, read by `pick`.
    fn lookup(
        &self,
        endpoint: &str,
        params: &[(&str, String)],
        wanted: &str,
        pick: impl Fn(&Value) -> Option<SsGame>,
    ) -> Result<Option<SsGame>, SourceError> {
        let key = format!(
            "{}/{endpoint}?{}|{wanted}",
            self.cfg.base,
            Self::query(params)
        );
        if let Some(g) = self.cached(&key) {
            return Ok(g);
        }
        let resp = self.send(&self.api_url(endpoint, params), MAX_JSON_BYTES)?;
        let game = self.read_json(&resp)?.as_ref().and_then(pick);
        let _ = self.cache.put_json(NS_GAME, &key, &game, None);
        Ok(game)
    }

    fn media_url(&self, sid: u32, game: &str, media: &str) -> String {
        format!(
            "{}/mediaJeu.php?systemeid={sid}&jeuid={game}&media={}",
            self.cfg.base,
            enc(media)
        )
    }
}

impl ArtSource for ScreenScraperSource<'_> {
    fn id(&self) -> &str {
        SOURCE_ID
    }

    fn candidates(&self, q: &ArtQuery) -> Result<Vec<Candidate>, SourceError> {
        let Some(sid) = q.system.screenscraper else {
            return Err(SourceError::SystemUnavailable {
                source: SOURCE_ID.into(),
                system: q.system.name.clone(),
            });
        };
        let Some((game, kind, score)) = self.game(sid, q)? else {
            return Ok(Vec::new());
        };
        let Some(name) = game.name().map(str::to_string) else {
            return Ok(Vec::new());
        };
        let wanted = media_type(q.media, q.system.arcade);
        let mut out: Vec<Candidate> = game
            .medias
            .iter()
            .filter(|(t, _)| t == wanted)
            .filter_map(|(t, r)| {
                let region = ss_region(r)?;
                Some(Candidate {
                    source: SOURCE_ID.into(),
                    media: q.media,
                    name: name.clone(),
                    url: self.media_url(sid, &game.id, &format!("{t}({r})")),
                    fallback_url: None,
                    regions: vec![region],
                    kind,
                    score,
                    votes: None,
                    consensus: 0,
                    origin: Some(format!("jeu {}", game.id)),
                })
            })
            .collect();
        sort_candidates(&mut out);
        Ok(out)
    }

    fn has_name(&self, _: &System, _: Media, _: &str) -> Result<bool, SourceError> {
        Ok(false)
    }

    fn fetch(&self, c: &Candidate) -> Result<Vec<u8>, SourceError> {
        let prefix = format!("{}/mediaJeu.php?", self.cfg.base);
        if !c.url.starts_with(&prefix) {
            return Err(SourceError::NotFound(c.url.clone()));
        }
        let resp = self.send(&format!("{}&{}", c.url, self.creds()), MAX_IMAGE_BYTES)?;
        let body = resp.body.trim_ascii();
        if !(200..300).contains(&resp.status) || body.is_empty() || NO_IMAGE_BODIES.contains(&body)
        {
            return Err(SourceError::NotFound(c.url.clone()));
        }
        Ok(resp.body)
    }
}

/// One cheap call with the account ("Test" in Settings): `ssuserInfos.php`.
/// No account → `Missing` without a request; never returns a credential.
pub fn check_account(
    http: &dyn Http,
    cfg: &SsConfig,
    dev: &DevCredentials,
    user: Option<&UserCredentials>,
) -> KeyCheck {
    let Some(user) = user else {
        return KeyCheck::Missing;
    };
    let Ok(cache) = Cache::in_memory() else {
        return KeyCheck::Unavailable;
    };
    let src = ScreenScraperSource::new(http, &cache, dev.clone(), Some(user.clone()), cfg.clone());
    match src.send(&src.api_url("ssuserInfos.php", &[]), MAX_JSON_BYTES) {
        Ok(r) if (200..300).contains(&r.status) => KeyCheck::Ok,
        Ok(_) => KeyCheck::Unavailable,
        Err(e) => KeyCheck::of_error(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_and_query_encoding() {
        assert_eq!(ss_region("wor"), Some(Region::World));
        assert_eq!(ss_region("uk"), Some(Region::UnitedKingdom));
        assert_eq!(ss_region("ss"), None);
        assert_eq!(enc("box-2D(us)"), "box-2D%28us%29");
        assert_eq!(enc("a b&c=d"), "a%20b%26c%3Dd");
    }

    #[test]
    fn games_with_hostile_ids_or_no_names_are_refused() {
        let v: Value = serde_json::from_str(r#"{"id":"12345678901234"}"#).unwrap();
        assert!(parse_game(&v).is_none());
        let v: Value =
            serde_json::from_str(r#"{"id":3,"noms":[{"region":"jp","text":"\u0007"}]}"#).unwrap();
        let g = parse_game(&v).unwrap();
        assert_eq!(g.id, "3");
        assert_eq!(g.name(), None);
    }
}
