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
    /// The parent game this one is a clone of (`cloneof`).
    pub cloneof: Option<String>,
    /// The DAT's `region` field, as written.
    pub region: Option<String>,
    /// `isbios "yes"`, or a `[BIOS]` mark in the name.
    pub is_bios: bool,
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
    let mut cloneof = None;
    let mut region = None;
    let mut serial = None;
    let mut isbios = false;
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
                match k.as_str() {
                    "name" => name = v.clone(),
                    "cloneof" if !v.is_empty() => cloneof = Some(v.clone()),
                    "region" if !v.is_empty() => region = Some(v.clone()),
                    "serial" => serial = first_serial(v),
                    "isbios" => isbios = v.eq_ignore_ascii_case("yes"),
                    _ => {}
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    for r in &mut roms {
        r.game = name.clone();
        if r.serial.is_none() {
            r.serial = serial.clone();
        }
    }
    let is_bios = isbios || name.contains("[BIOS]");
    Ok(DatGame {
        name,
        roms,
        cloneof,
        region,
        is_bios,
    })
}

/// A DAT CRC32 as eight upper-case hex digits: some `dat/` files (the
/// clean-cpc-db Amstrad CPC file at libretro-database 2c93952f) drop the
/// leading zero (`611E83F`), which would never match a computed CRC.
fn crc_hex(v: &str) -> String {
    let up = v.to_uppercase();
    if up.len() < 8 && !up.is_empty() && up.bytes().all(|b| b.is_ascii_hexdigit()) {
        format!("{up:0>8}")
    } else {
        up
    }
}

fn parse_rom(body: &[(Tok, usize)]) -> DatRow {
    let mut row = DatRow {
        game: String::new(),
        rom_name: String::new(),
        size: None,
        crc32: None,
        md5: None,
        sha1: None,
        serial: None,
    };
    for pair in body.windows(2) {
        if let (Tok::Word(k), Tok::Word(v)) = (&pair[0].0, &pair[1].0) {
            match k.as_str() {
                "name" if row.rom_name.is_empty() => row.rom_name = v.clone(),
                "size" => row.size = v.parse().ok(),
                "crc" => row.crc32 = Some(crc_hex(v)),
                "md5" => row.md5 = Some(v.to_uppercase()),
                "sha1" => row.sha1 = Some(v.to_uppercase()),
                "serial" => row.serial = first_serial(v),
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
    /// [`DatError::Unavailable`] without a request), except a rate limit, being
    /// offline or timing out ([`NetError::pauses`]), which pause the job and
    /// are retried on resume.
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
            Err(DatError::Net(e)) if e.pauses() => None,
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
                let games = parse_clrmamepro(&text)?;
                if games.iter().all(|g| g.roms.is_empty()) {
                    return Err(DatError::Empty);
                }
                self.cache.replace_dat_games(dat, &games)?;
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

    /// The DAT game whose ROM name starts with `prefix` (an Xbox 360
    /// STFS item's `<titleid8><contenttype8>`, spec §8.5), first by game
    /// name. See [`dat_by_name_prefix`].
    pub fn by_name_prefix(
        &self,
        dats: &[String],
        prefix: &str,
    ) -> Result<Option<String>, DatError> {
        for dat in dats {
            if self.ensure(dat)? == DatStatus::Missing {
                continue;
            }
            if let Some(game) = dat_by_name_prefix(self.cache, dat, prefix)
                .into_iter()
                .next()
            {
                return Ok(Some(game));
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

/// Length of the rom-name prefix of an Xbox 360 digital row: the title id
/// and the content type, 8 hex each (spec §8.5).
pub const NAME_PREFIX_LEN: usize = 16;

/// The games of `dat` whose ROM name starts with `prefix` (exactly
/// [`NAME_PREFIX_LEN`] hex characters, case ignored), each once, by game
/// name. Anything else matches nothing; a cache error reads as no hit.
pub fn dat_by_name_prefix(cache: &Cache, dat: &str, prefix: &str) -> Vec<String> {
    if prefix.len() != NAME_PREFIX_LEN || !prefix.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Vec::new();
    }
    let mut games: Vec<String> = cache
        .dat_by_rom_prefix(dat, prefix)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.game)
        .collect();
    games.dedup();
    games
}

/// The first serial of a DAT `serial` field, which may list several
/// (`"SLUS-00543, SLUS-00544"`).
fn first_serial(v: &str) -> Option<String> {
    v.split(',')
        .find_map(normalise_serial)
        .map(|s| nintendo_disc_key(&s).unwrap_or(s))
}

/// The game code of a Redump GameCube or Wii serial: `RVL-RMGE-USA`,
/// `RVL-SP3E-USA-B0` and `DL-DOL-GW7P-EUR` are indexed under `RMGE`,
/// `SP3E` and `GW7P`, the first four characters of the disc header's ID6
/// (spec §7: GameTDB DATs list the ID6 itself, Redump the middle key).
fn nintendo_disc_key(serial: &str) -> Option<String> {
    let parts: Vec<&str> = serial.split('-').collect();
    let at = parts.iter().position(|p| *p == "RVL" || *p == "DOL")?;
    let code = parts.get(at + 1)?;
    (code.len() == 4 && code.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| (*code).to_owned())
}

/// The serial keys to look `serial` up by, best first: a GameCube or Wii
/// ID6 is tried whole (GameTDB), then as its first four characters (the
/// Redump middle key). A Wii U ID6 (`ATWE01`, the product code plus the
/// maker of `meta.xml`) is tried whole, then as its ID4: GameTDB's Wii U
/// DAT lists retail discs by ID6 and eShop titles by ID4. Every other
/// system has one key.
pub fn serial_keys(system: &str, serial: &str) -> Vec<String> {
    let mut keys = vec![serial.to_owned()];
    if matches!(system, "gamecube" | "wii" | "wiiu") && serial.len() == 6 && serial.is_ascii() {
        keys.push(serial[..4].to_owned());
    }
    keys
}

/// A disc or product serial in one canonical form, or `None` when `raw` is
/// not one. Sony-style codes (four letters, five digits) become
/// `AAAA-NNNNN` whatever the separator: `SLUS_005.43`, `slus-00543` and
/// `SLUS00543` are all `SLUS-00543`. Other codes (Sega `T-5704H`,
/// `MK-81009`, `HDR-0001`) are only uppercased.
pub fn normalise_serial(raw: &str) -> Option<String> {
    let s = raw.trim().to_ascii_uppercase();
    if s.is_empty()
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_. ".contains(&b))
    {
        return None;
    }
    let compact: String = s.chars().filter(|c| !"-_. ".contains(*c)).collect();
    let b = compact.as_bytes();
    if b.len() == 9
        && b[..4].iter().all(u8::is_ascii_alphabetic)
        && b[4..].iter().all(u8::is_ascii_digit)
    {
        return Some(format!("{}-{}", &compact[..4], &compact[4..]));
    }
    if !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || !s.bytes().any(|b| b.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(s)
}
