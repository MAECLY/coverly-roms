//! libretro-database DATs (clrmamepro format), downloaded on demand into
//! the cache (spec §16.6, §17 q3: never bundled). Rows are stored in SQLite
//! so a lookup is one indexed query; a DAT is re-checked after 30 days with
//! a conditional GET; a 404 is remembered for the same TTL. Any other
//! failed refresh falls back to older cached rows when there are some, and
//! is otherwise remembered for the rest of the process, so a library of N
//! ROMs never downloads a broken DAT N times. Rate limits and going offline
//! are not remembered: they pause the job, and a resume must try again.

use std::collections::HashMap;
use std::sync::Mutex;

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

use crate::cache::{now_unix, Cache, CacheError, DatRow};
use crate::identify::hash::HashOutcome;
use crate::net::{Http, HttpRequest, NetError};
use crate::systems::{is_safe_repo_path, System};

pub const DATABASE_BASE: &str =
    "https://raw.githubusercontent.com/libretro/libretro-database/master";
pub const DAT_TTL_SECS: i64 = 30 * 24 * 3600;
pub const MAX_DAT_BYTES: u64 = 64 * 1024 * 1024;
const NS: &str = "dat-meta";
/// Unreserved characters stay; everything else in a segment is escaped.
pub(crate) const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'(')
    .remove(b')');

/// One game of a DAT with its ROM entries (`DatRow::game == name`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatGame {
    pub name: String,
    pub roms: Vec<DatRow>,
}

/// Why a DAT could not be parsed, fetched or stored.
#[derive(Debug, thiserror::Error)]
pub enum DatError {
    #[error("DAT syntax error on line {line}: {message}")]
    Parse { line: usize, message: String },
    #[error(transparent)]
    Net(#[from] NetError),
    #[error(transparent)]
    Cache(#[from] CacheError),
    /// The DAT downloaded fine but holds no ROM entries (an empty body, a
    /// captive portal page): never replaces cached rows.
    #[error("DAT has no ROM entries")]
    Empty,
    /// An earlier refresh of `dat` in this process failed; not retried.
    #[error("DAT {dat} is unavailable: {reason}")]
    Unavailable { dat: String, reason: String },
}

/// Whether a DAT's rows can be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatStatus {
    /// Rows are cached and fresh (or revalidated).
    Ready,
    /// The DAT does not exist upstream (404, remembered for the TTL).
    Missing,
    /// The refresh failed; older cached rows are used.
    Stale,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DatMeta {
    found: bool,
}

/// `path` with each `/`-separated segment percent-encoded.
pub fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|s| utf8_percent_encode(s, SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// The DATs to identify `system`'s ROMs with: the explicit `dats` list, or
/// `metadat/no-intro/<libretro>.dat` (`metadat/redump/…` for disc systems).
/// Arcade systems without an explicit list have none, and so does a
/// libretro name that would not stay one path segment.
pub fn dat_sources(system: &System) -> Vec<String> {
    if !system.dats.is_empty() {
        return system.dats.clone();
    }
    let libretro = system
        .libretro
        .as_ref()
        .filter(|n| !n.contains('/') && is_safe_repo_path(n));
    match (libretro, system.arcade) {
        (Some(name), false) if system.multi_file => vec![format!("metadat/redump/{name}.dat")],
        (Some(name), false) => vec![format!("metadat/no-intro/{name}.dat")],
        _ => Vec::new(),
    }
}

#[derive(Debug, PartialEq)]
enum Tok {
    Open,
    Close,
    Word(String),
}

fn tokenize(text: &str) -> Result<Vec<(Tok, usize)>, DatError> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    let mut line = 1;
    while let Some(c) = chars.next() {
        match c {
            '\n' => line += 1,
            c if c.is_whitespace() => {}
            '(' => out.push((Tok::Open, line)),
            ')' => out.push((Tok::Close, line)),
            '"' => {
                let mut s = String::new();
                loop {
                    match chars.next() {
                        None => {
                            return Err(DatError::Parse {
                                line,
                                message: "unterminated string".into(),
                            })
                        }
                        Some('\\') => {
                            if let Some(n) = chars.next() {
                                s.push(n)
                            }
                        }
                        Some('"') => break,
                        Some('\n') => {
                            line += 1;
                            s.push('\n')
                        }
                        Some(o) => s.push(o),
                    }
                }
                out.push((Tok::Word(s), line));
            }
            _ => {
                let mut s = String::from(c);
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() || n == '(' || n == ')' {
                        break;
                    }
                    s.push(n);
                    chars.next();
                }
                out.push((Tok::Word(s), line));
            }
        }
    }
    Ok(out)
}

/// Parses `game ( … rom ( … ) … )` blocks; other top-level blocks
/// (`clrmamepro`, `resource`, `machine` without roms) are skipped.
pub fn parse_clrmamepro(text: &str) -> Result<Vec<DatGame>, DatError> {
    let toks = tokenize(text)?;
    let mut games = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let (Tok::Word(head), line) = &toks[i] else {
            return Err(DatError::Parse {
                line: toks[i].1,
                message: "expected a block name".into(),
            });
        };
        if toks.get(i + 1).map(|t| &t.0) != Some(&Tok::Open) {
            return Err(DatError::Parse {
                line: *line,
                message: format!("expected `(` after `{head}`"),
            });
        }
        let end = block_end(&toks, i + 1)?;
        if head == "game" || head == "machine" {
            games.push(parse_game(&toks[i + 2..end])?);
        }
        i = end + 1;
    }
    Ok(games)
}

/// Index of the `)` closing the `(` at `open`.
fn block_end(toks: &[(Tok, usize)], open: usize) -> Result<usize, DatError> {
    let mut depth = 0;
    for (j, (t, _)) in toks.iter().enumerate().skip(open) {
        match t {
            Tok::Open => depth += 1,
            Tok::Close => {
                depth -= 1;
                if depth == 0 {
                    return Ok(j);
                }
            }
            Tok::Word(_) => {}
        }
    }
    Err(DatError::Parse {
        line: toks.last().map(|t| t.1).unwrap_or(1),
        message: "unbalanced parentheses".into(),
    })
}

fn parse_game(body: &[(Tok, usize)]) -> Result<DatGame, DatError> {
    let mut name = String::new();
    let mut roms = Vec::new();
    let mut i = 0;
    while i < body.len() {
        match (&body[i].0, body.get(i + 1).map(|t| &t.0)) {
            (Tok::Word(k), Some(Tok::Open)) => {
                let end = block_end(body, i + 1)?;
                if k == "rom" {
                    roms.push(parse_rom(&body[i + 2..end]));
                }
                i = end + 1;
            }
            (Tok::Word(k), Some(Tok::Word(v))) => {
                if k == "name" {
                    name = v.clone();
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    for r in &mut roms {
        r.game = name.clone();
    }
    Ok(DatGame { name, roms })
}

fn parse_rom(body: &[(Tok, usize)]) -> DatRow {
    let mut row = DatRow {
        game: String::new(),
        rom_name: String::new(),
        size: None,
        crc32: None,
        md5: None,
        sha1: None,
    };
    for pair in body.windows(2) {
        if let (Tok::Word(k), Tok::Word(v)) = (&pair[0].0, &pair[1].0) {
            match k.as_str() {
                "name" if row.rom_name.is_empty() => row.rom_name = v.clone(),
                "size" => row.size = v.parse().ok(),
                "crc" => row.crc32 = Some(v.to_uppercase()),
                "md5" => row.md5 = Some(v.to_uppercase()),
                "sha1" => row.sha1 = Some(v.to_uppercase()),
                _ => {}
            }
        }
    }
    row
}

/// Downloads DATs on demand into the cache and answers lookups from it.
pub struct DatStore<'a> {
    http: &'a dyn Http,
    cache: &'a Cache,
    base: String,
    ttl: i64,
    max_bytes: u64,
    memo: Mutex<HashMap<String, Result<DatStatus, String>>>,
}

impl<'a> DatStore<'a> {
    pub fn new(http: &'a dyn Http, cache: &'a Cache, base: impl Into<String>) -> DatStore<'a> {
        DatStore {
            http,
            cache,
            base: base.into().trim_end_matches('/').to_string(),
            ttl: DAT_TTL_SECS,
            max_bytes: MAX_DAT_BYTES,
            memo: Mutex::default(),
        }
    }
    pub fn with_ttl_secs(mut self, secs: i64) -> Self {
        self.ttl = secs;
        self
    }
    pub fn with_max_bytes(mut self, n: u64) -> Self {
        self.max_bytes = n;
        self
    }

    /// Makes `dat`'s rows available, at most one check per process per DAT.
    /// A failure is remembered too (later calls return
    /// [`DatError::Unavailable`] without a request), except a rate limit or
    /// being offline, which pause the job and are retried on resume.
    pub fn ensure(&self, dat: &str) -> Result<DatStatus, DatError> {
        if let Some(m) = self.memo.lock().unwrap().get(dat) {
            return m.clone().map_err(|reason| DatError::Unavailable {
                dat: dat.to_string(),
                reason,
            });
        }
        let result = self.refresh(dat);
        let remembered = match &result {
            Ok(status) => Some(Ok(*status)),
            Err(DatError::Net(NetError::RateLimited { .. } | NetError::Offline(_))) => None,
            Err(e) => Some(Err(e.to_string())),
        };
        if let Some(m) = remembered {
            self.memo.lock().unwrap().insert(dat.to_string(), m);
        }
        result
    }

    fn refresh(&self, dat: &str) -> Result<DatStatus, DatError> {
        let meta = self.cache.get_json::<DatMeta>(NS, dat)?;
        if let Some(m) = &meta {
            if now_unix() - m.fetched_at < self.ttl {
                return Ok(if m.value.found {
                    DatStatus::Ready
                } else {
                    DatStatus::Missing
                });
            }
        }
        let etag = meta
            .as_ref()
            .filter(|m| m.value.found)
            .and_then(|m| m.etag.clone());
        match self.download(dat, etag) {
            // Older rows beat no rows: any failed refresh falls back to them.
            Err(DatError::Net(_) | DatError::Parse { .. } | DatError::Empty)
                if meta.as_ref().is_some_and(|m| m.value.found) =>
            {
                Ok(DatStatus::Stale)
            }
            other => other,
        }
    }

    fn download(&self, dat: &str, etag: Option<String>) -> Result<DatStatus, DatError> {
        let url = format!("{}/{}", self.base, encode_path(dat));
        let resp = self.http.get(
            &HttpRequest::get(url.clone())
                .max_bytes(self.max_bytes)
                .if_none_match(etag),
        )?;
        match resp.status {
            304 => {
                self.cache.touch_json(NS, dat)?;
                Ok(DatStatus::Ready)
            }
            404 => {
                self.cache
                    .put_json(NS, dat, &DatMeta { found: false }, None)?;
                Ok(DatStatus::Missing)
            }
            200..=299 => {
                let text = String::from_utf8_lossy(&resp.body);
                let rows: Vec<DatRow> = parse_clrmamepro(&text)?
                    .into_iter()
                    .flat_map(|g| g.roms)
                    .collect();
                if rows.is_empty() {
                    return Err(DatError::Empty);
                }
                self.cache.replace_dat(dat, &rows)?;
                self.cache
                    .put_json(NS, dat, &DatMeta { found: true }, resp.etag.as_deref())?;
                Ok(DatStatus::Ready)
            }
            status => Err(NetError::Status { status, url }.into()),
        }
    }

    /// The DAT game whose ROM matches `outcome`: zip CRC + size, or the
    /// headerless then the full digests (CRC + size, and SHA1 when both
    /// sides have one). Ties resolve to the first game by name.
    pub fn lookup(
        &self,
        dats: &[String],
        outcome: &HashOutcome,
    ) -> Result<Option<String>, DatError> {
        let probes: Vec<(&str, u64, Option<&str>)> = match outcome {
            HashOutcome::ZipEntry { crc32, size, .. } => vec![(crc32.as_str(), *size, None)],
            HashOutcome::Content(h) => h
                .headerless
                .iter()
                .chain(std::iter::once(&h.full))
                .map(|d| {
                    (
                        d.crc32.as_str(),
                        d.size,
                        Some(d.sha1.as_str()).filter(|s| !s.is_empty()),
                    )
                })
                .collect(),
            _ => return Ok(None),
        };
        for dat in dats {
            if self.ensure(dat)? == DatStatus::Missing {
                continue;
            }
            for (crc, size, sha1) in &probes {
                let hit = self.cache.dat_by_crc(dat, crc)?.into_iter().find(|r| {
                    r.size.is_none_or(|s| s == *size)
                        && match (sha1, &r.sha1) {
                            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                            _ => true,
                        }
                });
                if let Some(row) = hit {
                    return Ok(Some(row.game));
                }
            }
        }
        Ok(None)
    }

    /// The DAT game (description) whose ROM set is `short_name`, ignoring case.
    pub fn arcade_description(
        &self,
        dats: &[String],
        short_name: &str,
    ) -> Result<Option<String>, DatError> {
        for dat in dats {
            if self.ensure(dat)? == DatStatus::Missing {
                continue;
            }
            if let Some(row) = self
                .cache
                .dat_by_rom_stem(dat, short_name)?
                .into_iter()
                .next()
            {
                return Ok(Some(row.game));
            }
        }
        Ok(None)
    }
}
