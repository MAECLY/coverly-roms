//! libretro-thumbnails (spec §16.5, ADR-0005): the system list comes from
//! the live server root, each `<System>/<Named_Type>/` Apache listing is
//! cached in SQLite for two days and revalidated with its ETag; names are
//! matched case-insensitively after the libretro sanitising; images come
//! from thumbnails.libretro.com with raw.githubusercontent.com as fallback.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use percent_encoding::{percent_decode_str, utf8_percent_encode};
use unicode_normalization::UnicodeNormalization;

use super::{sort_candidates, ArtQuery, ArtSource, Candidate, MatchKind, SourceError};
use crate::cache::{now_unix, Cache};
use crate::identify::dat::SEGMENT;
use crate::identify::names::{
    compatible, libretro_key, parse_name, sanitise_libretro, score, ParsedName,
};
use crate::net::{Http, HttpRequest, NetError};
use crate::profiles::Media;
use crate::systems::System;

pub const LIBRETRO_BASE: &str = "https://thumbnails.libretro.com";
pub const LIBRETRO_RAW_BASE: &str = "https://raw.githubusercontent.com/libretro-thumbnails";
pub const INDEX_TTL_SECS: i64 = 2 * 24 * 3600;
pub const MAX_INDEX_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const SOURCE_ID: &str = "libretro";
const NS_ROOT: &str = "libretro-root";
const NS_INDEX: &str = "libretro-index";
/// Server folders whose GitHub repo has another name (research, correction 2).
const RAW_RENAMES: [(&str, &str); 2] = [
    ("Atari - 8-bit", "Atari_-_8-bit_Family"),
    ("NEC - PC-8001 - PC-8801", "NEC_-_PC-88"),
];

/// The libretro folder of a media type.
pub fn named_dir(media: Media) -> &'static str {
    match media {
        Media::Cover => "Named_Boxarts",
        Media::Screenshot => "Named_Snaps",
        Media::Title => "Named_Titles",
    }
}

/// The libretro-thumbnails GitHub repo of a server system folder.
pub fn raw_repo_name(server_system: &str) -> String {
    RAW_RENAMES
        .iter()
        .find(|(s, _)| *s == server_system)
        .map(|(_, r)| r.to_string())
        .unwrap_or_else(|| server_system.replace(' ', "_"))
}

/// Percent-encodes one URL path segment.
pub fn encode_segment(s: &str) -> String {
    utf8_percent_encode(s, SEGMENT).to_string()
}

/// Every `href="…"` value, in order.
fn hrefs(html: &str) -> impl Iterator<Item = &str> {
    html.split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
}

/// Bidirectional overrides and isolates (Trojan Source style spoofing).
fn is_bidi_control(c: char) -> bool {
    ('\u{202A}'..='\u{202E}').contains(&c) || ('\u{2066}'..='\u{2069}').contains(&c)
}

/// Decodes an href into a safe single path segment, or `None`.
fn safe_name(href: &str) -> Option<String> {
    if href.starts_with('?') || href.starts_with('/') || href.contains("://") {
        return None;
    }
    let decoded = percent_decode_str(href).decode_utf8().ok()?;
    let name: String = decoded.nfc().collect();
    // A single segment cannot traverse without a separator, so titles with
    // "..." (e.g. "Wait... What (USA).png") stay valid.
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
        || name.chars().any(|c| c.is_control() || is_bidi_control(c));
    (!bad).then_some(name)
}

/// The decoded `*.png` file names of an Apache listing; hostile ones dropped.
pub fn parse_apache_files(html: &str) -> Vec<String> {
    hrefs(html)
        .filter_map(safe_name)
        .filter(|n| n.to_lowercase().ends_with(".png"))
        .collect()
}

/// The decoded folder names of an Apache listing, without `/` and `stats`.
pub fn parse_apache_dirs(html: &str) -> Vec<String> {
    hrefs(html)
        .filter_map(|h| h.strip_suffix('/'))
        .filter_map(safe_name)
        .filter(|n| n != "stats")
        .collect()
}

/// Where libretro-thumbnails lives, and how long a listing stays fresh.
#[derive(Clone, Debug)]
pub struct LibretroConfig {
    pub base: String,
    pub raw_base: String,
    pub ttl_secs: i64,
}

impl Default for LibretroConfig {
    fn default() -> Self {
        LibretroConfig {
            base: LIBRETRO_BASE.into(),
            raw_base: LIBRETRO_RAW_BASE.into(),
            ttl_secs: INDEX_TTL_SECS,
        }
    }
}

/// One file of a parsed listing.
struct Entry {
    file: String,
    parsed: ParsedName,
}

/// A parsed listing with a case-insensitive lookup by libretro key.
struct Index {
    entries: Vec<Entry>,
    by_key: HashMap<String, usize>,
}

/// A listing, or why it could not be read in this run.
type Remembered<T> = Result<Arc<T>, NetError>;
type IndexMap = HashMap<(String, Media), Remembered<Index>>;

/// The libretro-thumbnails source.
pub struct LibretroSource<'a> {
    http: &'a dyn Http,
    cache: &'a Cache,
    cfg: LibretroConfig,
    root: Mutex<Option<Remembered<Vec<String>>>>,
    indexes: Mutex<IndexMap>,
}

/// Locks a mutex, ignoring poisoning: the guarded data are plain caches.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl<'a> LibretroSource<'a> {
    pub fn new(http: &'a dyn Http, cache: &'a Cache, mut cfg: LibretroConfig) -> Self {
        cfg.base = cfg.base.trim_end_matches('/').to_string();
        cfg.raw_base = cfg.raw_base.trim_end_matches('/').to_string();
        LibretroSource {
            http,
            cache,
            cfg,
            root: Mutex::default(),
            indexes: Mutex::default(),
        }
    }

    /// GET with the SQLite copy: fresh → cached; stale → conditional GET
    /// (304 refreshes, 200 replaces, 404 → empty list); network error with
    /// a stale copy → the stale copy. A rate limit always surfaces.
    fn listing(
        &self,
        ns: &str,
        key: &str,
        url: &str,
        dirs: bool,
    ) -> Result<Vec<String>, SourceError> {
        let stored = self.cache.get_json::<Vec<String>>(ns, key)?;
        if let Some(s) = &stored {
            if now_unix() - s.fetched_at < self.cfg.ttl_secs {
                return Ok(s.value.clone());
            }
        }
        let req = HttpRequest::get(url)
            .max_bytes(MAX_INDEX_BYTES)
            .if_none_match(stored.as_ref().and_then(|s| s.etag.clone()));
        let resp = match self.http.get(&req) {
            Ok(r) => r,
            Err(e @ NetError::RateLimited { .. }) => return Err(e.into()),
            Err(e) => return stored.map(|s| s.value).ok_or(e.into()),
        };
        match (resp.status, stored) {
            (304, Some(stored)) => {
                self.cache.touch_json(ns, key)?;
                Ok(stored.value)
            }
            (404, _) => {
                self.cache.put_json(ns, key, &Vec::<String>::new(), None)?;
                Ok(Vec::new())
            }
            (200..=299, _) => {
                let html = String::from_utf8_lossy(&resp.body);
                let names = if dirs {
                    parse_apache_dirs(&html)
                } else {
                    parse_apache_files(&html)
                };
                self.cache.put_json(ns, key, &names, resp.etag.as_deref())?;
                Ok(names)
            }
            (status, _) => Err(NetError::Status {
                status,
                url: url.to_string(),
            }
            .into()),
        }
    }

    fn root(&self) -> Result<Arc<Vec<String>>, SourceError> {
        let mut root = lock(&self.root);
        if let Some(r) = root.as_ref() {
            return r.clone().map_err(SourceError::Net);
        }
        let list = self.listing(NS_ROOT, "root", &format!("{}/", self.cfg.base), true);
        let (result, keep) = remember(list.map(Arc::new));
        if keep.is_some() {
            *root = keep;
        }
        result
    }

    /// The server folder for `system`, matched case-insensitively against the live root.
    pub fn server_system(&self, system: &System) -> Result<String, SourceError> {
        let unavailable = || SourceError::SystemUnavailable {
            source: SOURCE_ID.into(),
            system: system.name.clone(),
        };
        let wanted = system
            .libretro
            .as_deref()
            .ok_or_else(unavailable)?
            .to_lowercase();
        self.root()?
            .iter()
            .find(|s| s.to_lowercase() == wanted)
            .cloned()
            .ok_or_else(unavailable)
    }

    fn index(&self, server: &str, media: Media) -> Result<Arc<Index>, SourceError> {
        let map_key = (server.to_string(), media);
        if let Some(i) = lock(&self.indexes).get(&map_key) {
            return i.clone().map_err(SourceError::Net);
        }
        let url = format!(
            "{}/{}/{}/",
            self.cfg.base,
            encode_segment(server),
            named_dir(media)
        );
        let files = match self.listing(
            NS_INDEX,
            &format!("{server}/{}", named_dir(media)),
            &url,
            false,
        ) {
            Ok(files) => files,
            Err(e) => {
                let (result, keep) = remember::<Index>(Err(e));
                if let Some(keep) = keep {
                    lock(&self.indexes).insert(map_key, keep);
                }
                return result;
            }
        };
        let mut entries = Vec::with_capacity(files.len());
        let mut by_key = HashMap::new();
        for file in files {
            let parsed = parse_name(&key_display(&file), false);
            by_key.entry(libretro_key(&file)).or_insert(entries.len());
            entries.push(Entry { file, parsed });
        }
        let index = Arc::new(Index { entries, by_key });
        lock(&self.indexes).insert(map_key, Ok(index.clone()));
        Ok(index)
    }

    fn candidate(
        &self,
        server: &str,
        media: Media,
        e: &Entry,
        kind: MatchKind,
        score: u8,
    ) -> Candidate {
        let dir = named_dir(media);
        let file = encode_segment(&e.file);
        Candidate {
            source: SOURCE_ID.into(),
            media,
            name: key_display(&e.file),
            url: format!("{}/{}/{dir}/{file}", self.cfg.base, encode_segment(server)),
            fallback_url: Some(format!(
                "{}/{}/master/{dir}/{file}",
                self.cfg.raw_base,
                encode_segment(&raw_repo_name(server))
            )),
            regions: e.parsed.regions.clone(),
            kind,
            score,
            votes: None,
            consensus: 0,
        }
    }
}

/// What to remember of a listing fetch for the rest of the run: a listing,
/// or a network failure that does not pause (an HTTP 500, an oversized
/// listing), so it is not asked again for every ROM. A pausing failure is
/// not remembered (the re-run retries it), nor is a cache error.
fn remember<T>(
    r: Result<Arc<T>, SourceError>,
) -> (Result<Arc<T>, SourceError>, Option<Remembered<T>>) {
    match r {
        Ok(v) => (Ok(v.clone()), Some(Ok(v))),
        Err(SourceError::Net(e)) if !e.pauses() => (Err(SourceError::Net(e.clone())), Some(Err(e))),
        Err(e) => (Err(e), None),
    }
}

/// File name without its `.png` extension (any case), as shown to users.
fn key_display(file: &str) -> String {
    // `get` instead of indexing: never slice inside a multi-byte character.
    match file
        .len()
        .checked_sub(4)
        .and_then(|i| Some((i, file.get(i..)?)))
    {
        Some((i, ext)) if i > 0 && ext.eq_ignore_ascii_case(".png") => file[..i].to_string(),
        _ => file.to_string(),
    }
}

impl ArtSource for LibretroSource<'_> {
    fn id(&self) -> &str {
        SOURCE_ID
    }

    fn candidates(&self, q: &ArtQuery) -> Result<Vec<Candidate>, SourceError> {
        let server = self.server_system(q.system)?;
        let index = self.index(&server, q.media)?;
        let mut out: Vec<Candidate> = Vec::new();
        let mut taken = vec![false; index.entries.len()];
        let mut push = |i: usize, kind: MatchKind, score: u8, out: &mut Vec<Candidate>| {
            if !taken[i] {
                taken[i] = true;
                out.push(self.candidate(&server, q.media, &index.entries[i], kind, score));
            }
        };
        // 1. canonical (DAT / arcade) name, then its other regions.
        if let Some(canon) = q.canonical {
            let key = libretro_key(&sanitise_libretro(canon));
            if let Some(&i) = index.by_key.get(&key) {
                push(i, MatchKind::Canonical, 100, &mut out);
            }
            let title = parse_name(canon, false).key;
            for (i, e) in index.entries.iter().enumerate() {
                if !title.is_empty() && e.parsed.key == title {
                    push(i, MatchKind::TitleExact, 95, &mut out);
                }
            }
        }
        if q.by_name {
            // 2. the ROM file name as is.
            let key = libretro_key(&sanitise_libretro(q.stem));
            if let Some(&i) = index.by_key.get(&key) {
                push(i, MatchKind::NameExact, 100, &mut out);
            }
            // 3. same title, any region.
            for (i, e) in index.entries.iter().enumerate() {
                if !q.rom.key.is_empty() && e.parsed.key == q.rom.key {
                    push(i, MatchKind::TitleExact, 95, &mut out);
                }
            }
            // 4. fuzzy, only when nothing exact was found.
            if out.is_empty() && q.fuzzy && !q.rom.key.is_empty() {
                let mut scored: Vec<(u8, usize)> = index
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| compatible(q.rom, &e.parsed))
                    .map(|(i, e)| {
                        let full = score(&q.rom.key, &e.parsed.key);
                        let short = score(&q.rom.short_key, &e.parsed.short_key).saturating_sub(10);
                        (full.max(short), i)
                    })
                    .filter(|(s, _)| *s >= q.min_score)
                    .collect();
                scored.sort_by(|a, b| {
                    b.0.cmp(&a.0)
                        .then_with(|| index.entries[a.1].file.cmp(&index.entries[b.1].file))
                });
                for (s, i) in scored.into_iter().take(q.max_candidates) {
                    push(i, MatchKind::Fuzzy, s.min(94), &mut out);
                }
            }
        }
        sort_candidates(&mut out);
        out.truncate(q.max_candidates.max(1) + 8); // keep every exact variant, bounded
        Ok(out)
    }

    fn has_name(&self, system: &System, media: Media, name: &str) -> Result<bool, SourceError> {
        let server = self.server_system(system)?;
        Ok(self
            .index(&server, media)?
            .by_key
            .contains_key(&libretro_key(name)))
    }

    fn fetch(&self, c: &Candidate) -> Result<Vec<u8>, SourceError> {
        let mut last = SourceError::NotFound(c.url.clone());
        for url in std::iter::once(&c.url).chain(c.fallback_url.iter()) {
            match self
                .http
                .get(&HttpRequest::get(url.clone()).max_bytes(MAX_IMAGE_BYTES))
            {
                Ok(r) if (200..300).contains(&r.status) && !r.body.is_empty() => return Ok(r.body),
                Ok(_) => last = SourceError::NotFound(url.clone()),
                Err(e @ NetError::RateLimited { .. }) => return Err(e.into()),
                Err(e) => last = e.into(),
            }
        }
        Err(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_display_strips_only_a_png_extension() {
        assert_eq!(key_display("Zelda (USA).PNG"), "Zelda (USA)");
        assert_eq!(key_display(".png"), ".png");
        assert_eq!(key_display("ab"), "ab");
        assert_eq!(key_display("éé"), "éé", "no slicing inside a character");
    }

    #[test]
    fn safe_name_keeps_ellipses_and_drops_bidi_and_urls() {
        assert_eq!(
            safe_name("Wait...%20What%20(USA).png").as_deref(),
            Some("Wait... What (USA).png")
        );
        assert_eq!(safe_name("https://evil/x.png"), None);
        assert_eq!(safe_name("%E2%81%A6x.png"), None);
        assert_eq!(safe_name("%FF.png"), None, "invalid UTF-8");
    }
}
