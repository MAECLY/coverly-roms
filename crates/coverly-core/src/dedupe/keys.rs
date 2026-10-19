//! Content keys (duplicates spec §5.2–§5.4). This step: level 1, the exact
//! copy, whatever the name, container, copier header or byte order.
//!
//! The work is ordered so that most files are never read (spec §10): units
//! are bucketed per system by payload size (after the header rule, not the
//! file size), and only units that share a bucket are looked at. Inside a
//! bucket the cheapest test runs first: the cache (`content_keys`, then the
//! scan's `quick_ids`), the zip central-directory CRC, a 128 KiB prefix CRC,
//! and only then a full SHA1. A singleton bucket is never read. Every
//! computed fact is written back to `content_keys`, so a warm run reads
//! nothing.
//!
//! No key reads a file name: names never group (spec §1).

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::cache::{Cache, ContentRow};
use crate::identify::hash::{
    hash_stream, hex_upper, n64_to_z64, HashOutcome, HeaderRule, N64Order, DEFAULT_HASH_MAX_BYTES,
};
use crate::identify::sniff::PREFIX;
use crate::scan::classify::extension;
use crate::systems::SystemDb;

use super::source::{EntryListing, ListedEntry};
use super::units::{Unit, UnitKind};
use super::DedupePhase;

/// Worker threads on internal disks, and on removable or network volumes
/// (the organise rule).
pub const WORKERS: usize = 4;
pub const SLOW_VOLUME_WORKERS: usize = 2;

/// Version of the facts stored in `content_keys.keys_json`; other versions
/// are a miss.
const FACTS_VERSION: u32 = 1;
/// The largest header any rule takes off (SNES/SMS copier).
const MAX_HEADER: usize = 512;
/// Bytes read from a CHD for its header (v5 is 124).
const CHD_HEAD: usize = 128;
/// A cue/gdi/ccd sheet larger than this is not a sheet.
const MAX_SHEET: u64 = 1024 * 1024;

/// A level 1 key: equal keys in one system mean the same game data.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ExactKey {
    /// SHA1 (upper-case hex) of the game data after the header rule and
    /// the N64 byte-order fix, and its size: loose ROMs, one-ROM zips,
    /// disc images compared whole.
    Payload { sha1: String, size: u64 },
    /// A zip entry too big to decode within the budget: its directory CRC
    /// and size only. Unconfirmed, so report-only.
    ZipCrc { crc32: u32, size: u64 },
    /// An arcade set: the sorted multiset of `(crc32, size)` of its
    /// entries, whatever they are called.
    Arcade { entries: Vec<(u32, u64)> },
    /// A cue/gdi/ccd set: the SHA1 of every track in order and the sheet's
    /// layout ([`track_layout`]).
    DiscTracks { tracks: Vec<String>, layout: String },
    /// The combined data and metadata SHA1 of a CHD header.
    Chd { sha1: String },
    /// A CHD whose header gives only the raw data SHA1. Unconfirmed.
    ChdRaw { rawsha1: String },
}

impl ExactKey {
    /// The key as one string, `system` first, for the cache's reverse
    /// index and the group id. Long parts are hashed.
    pub fn key_string(&self, system: &str) -> String {
        match self {
            ExactKey::Payload { sha1, size } => format!("l1:{system}:payload:{sha1}:{size}"),
            ExactKey::ZipCrc { crc32, size } => format!("l1:{system}:zipcrc:{crc32:08X}:{size}"),
            ExactKey::Arcade { entries } => {
                let list: Vec<String> = entries
                    .iter()
                    .map(|(crc, size)| format!("{crc:08X}/{size}"))
                    .collect();
                format!("l1:{system}:arcade:{}", sha1_of(list.join(",").as_bytes()))
            }
            ExactKey::DiscTracks { tracks, layout } => format!(
                "l1:{system}:disc:{}:{}",
                sha1_of(tracks.join(",").as_bytes()),
                sha1_of(layout.as_bytes())
            ),
            ExactKey::Chd { sha1 } => format!("l1:{system}:chd:{sha1}"),
            ExactKey::ChdRaw { rawsha1 } => format!("l1:{system}:chdraw:{rawsha1}"),
        }
    }

    /// Whether the key proves the bytes are equal (not a CRC-only or a
    /// raw-data-only match).
    pub fn is_confirmed(&self) -> bool {
        !matches!(self, ExactKey::ZipCrc { .. } | ExactKey::ChdRaw { .. })
    }
}

/// What level 1 found out about one unit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyedUnit {
    /// Index of the unit.
    pub unit: usize,
    /// `None` when nothing else in its system can be the same bytes (so it
    /// was not read), or when it could not be read.
    pub exact: Option<ExactKey>,
    /// The key proves equal bytes.
    pub confirmed: bool,
    /// Sorted SHA1s of the unit's soft patches: part of the group key.
    pub patch_set: Vec<String>,
    /// The functional header (iNES, Lynx, Atari 7800) when present: two
    /// copies with other headers may play differently.
    pub header: Option<Vec<u8>>,
    /// `(system, game)` of a catalogue hit. Not part of the level 1 key;
    /// set by the catalogue step (level 2), `None` here.
    pub dat_game: Option<(String, String)>,
    /// A CHD's raw data SHA1, when its header has one: same raw data with
    /// another combined SHA1 is a report-only match.
    pub chd_raw: Option<String>,
}

impl KeyedUnit {
    /// The key two units must share to be exact copies in `system`: the
    /// level 1 key plus the soft-patch set.
    pub fn group_key(&self, system: &str) -> Option<String> {
        let key = self.exact.as_ref()?.key_string(system);
        if self.patch_set.is_empty() {
            Some(key)
        } else {
            Some(format!("{key}+patches:{}", self.patch_set.join(",")))
        }
    }
}

/// Opens files for reading content. A seam so tests can count reads.
pub trait FileBytes: Send + Sync {
    fn open(&self, path: &Path) -> io::Result<Box<dyn Read + Send>>;
}

/// [`FileBytes`] over the file system.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiskBytes;

impl FileBytes for DiskBytes {
    fn open(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(File::open(path)?))
    }
}

static DISK_BYTES: DiskBytes = DiskBytes;

/// What level 1 reads with.
pub struct KeyCtx<'a> {
    /// Header rules and arcade flags per system.
    pub db: &'a SystemDb,
    /// Lists and decodes archive entries.
    pub listing: &'a dyn EntryListing,
    /// Opens loose files.
    pub bytes: &'a dyn FileBytes,
    /// `quick_ids` reuse and the `content_keys` side table.
    pub cache: Option<&'a Cache>,
    /// The volume's UUID: `content_keys` rows are read and written only
    /// when it is known.
    pub volume_uuid: Option<&'a str>,
    /// The per-file budget for decoding an archive entry (`max_hash_mb`).
    /// Loose files in a colliding bucket are hashed whatever their size
    /// (spec §5.2).
    pub max_hash_bytes: u64,
    /// Worker threads ([`workers_for`]).
    pub workers: usize,
}

impl<'a> KeyCtx<'a> {
    /// A context reading from disk, with no cache, the default budget and
    /// [`WORKERS`] threads.
    pub fn new(db: &'a SystemDb, listing: &'a dyn EntryListing) -> KeyCtx<'a> {
        KeyCtx {
            db,
            listing,
            bytes: &DISK_BYTES,
            cache: None,
            volume_uuid: None,
            max_hash_bytes: DEFAULT_HASH_MAX_BYTES,
            workers: WORKERS,
        }
    }

    fn rule(&self, system: &str) -> HeaderRule {
        self.db.get(system).map(|s| s.header).unwrap_or_default()
    }
}

/// Worker threads for a library at `root`.
pub fn workers_for(root: &Path) -> usize {
    if crate::write::on_removable_volume(root) {
        SLOW_VOLUME_WORKERS
    } else {
        WORKERS
    }
}

/// The facts gathered about one unit, as cached in `content_keys`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Facts {
    v: u32,
    /// Size of the game data after the header rule, once known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payload_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    header: Option<Vec<u8>>,
    /// CRC32 of the first [`PREFIX`] payload bytes (one per track for a
    /// disc set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prefix: Option<Vec<u32>>,
    /// Directory CRC32 of a zip's ROM entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    entry_crc: Option<u32>,
    /// A disc set's layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    exact: Option<ExactKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chd_raw: Option<String>,
}

/// How a unit of the payload class is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum How {
    /// A loose file with a header rule.
    Raw(HeaderRule),
    /// The ROM entry of a one-ROM zip.
    Zip(HeaderRule),
}

impl How {
    fn rule(self) -> HeaderRule {
        match self {
            How::Raw(r) | How::Zip(r) => r,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    /// Compared by payload: loose ROMs, one-ROM zips, whole images.
    Payload(How),
    Arcade,
    Disc,
    Chd,
}

/// Level 1 keys of `units` (index-aligned: `out[i].unit == i`).
///
/// `progress(phase, done, total)` is called from this thread, unthrottled.
/// On `cancel` the work stops between units and what is known so far is
/// returned; the caller discards it. The cache keeps what was written.
pub fn level_one(
    units: &[Unit],
    ctx: &KeyCtx,
    progress: &dyn Fn(DedupePhase, usize, usize),
    cancel: &AtomicBool,
) -> Vec<KeyedUnit> {
    let n = units.len();
    progress(DedupePhase::Sizes, 0, n);
    let classes: Vec<Class> = units.iter().map(|u| class_of(u, ctx)).collect();
    let mut facts: Vec<Facts> = Vec::with_capacity(n);
    let mut fresh = vec![false; n];
    for (i, u) in units.iter().enumerate() {
        match cached_facts(u, ctx) {
            Some(f) => facts.push(f),
            None => {
                let f = quick_facts(u, classes[i], ctx).unwrap_or_default();
                fresh[i] = f != Facts::default();
                facts.push(f);
            }
        }
    }
    progress(DedupePhase::Sizes, n, n);

    let mut stage = Stage {
        units,
        ctx,
        classes: &classes,
        facts: &mut facts,
        fresh: &mut fresh,
        progress,
        cancel,
    };
    let passes: [fn(&mut Stage); 4] = [payload_class, arcade_class, disc_class, chd_class];
    for pass in passes {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        pass(&mut stage);
    }

    let mut out = Vec::with_capacity(n);
    for (i, (u, f)) in units.iter().zip(&facts).enumerate() {
        if fresh[i] {
            store_facts(u, f, ctx);
        }
        let patch_set = match &f.exact {
            Some(_) if !u.patches.is_empty() => patch_set(&u.patches),
            _ => Vec::new(),
        };
        out.push(KeyedUnit {
            unit: i,
            confirmed: f.exact.as_ref().is_some_and(ExactKey::is_confirmed),
            exact: f.exact.clone(),
            patch_set,
            header: f.header.clone(),
            dat_game: None,
            chd_raw: f.chd_raw.clone(),
        });
    }
    out
}

/// Shared state of the class passes.
struct Stage<'s, 'a> {
    units: &'s [Unit],
    ctx: &'s KeyCtx<'a>,
    classes: &'s [Class],
    facts: &'s mut Vec<Facts>,
    fresh: &'s mut Vec<bool>,
    progress: &'s dyn Fn(DedupePhase, usize, usize),
    cancel: &'s AtomicBool,
}

impl Stage<'_, '_> {
    /// Runs `f` on `todo` with the worker pool and merges each result into
    /// the unit's facts; `Err` and `None` change nothing.
    fn read_all<F>(&mut self, todo: &[usize], f: F)
    where
        F: Fn(usize, &Facts) -> io::Result<Option<Facts>> + Sync,
    {
        let facts: &Vec<Facts> = self.facts;
        let results = par_map(todo, self.ctx.workers, self.cancel, self.progress, |i| {
            f(i, &facts[i])
        });
        for (i, r) in results {
            if let Ok(Some(new)) = r {
                if new != self.facts[i] {
                    self.facts[i] = new;
                    self.fresh[i] = true;
                }
            }
        }
    }

    fn of_class(&self, want: impl Fn(Class) -> bool) -> Vec<usize> {
        (0..self.units.len())
            .filter(|&i| want(self.classes[i]))
            .collect()
    }
}

fn class_of(u: &Unit, ctx: &KeyCtx) -> Class {
    let plain = Class::Payload(How::Raw(HeaderRule::Plain));
    match u.kind {
        UnitKind::Single if matches!(u.ext.as_str(), "7z" | "rar") => plain,
        UnitKind::Single => Class::Payload(How::Raw(ctx.rule(&u.system))),
        UnitKind::Image => plain,
        UnitKind::Zip if u.payload_size.is_some() => Class::Payload(How::Zip(ctx.rule(&u.system))),
        UnitKind::Zip => plain,
        UnitKind::Arcade if u.ext == "zip" => Class::Arcade,
        UnitKind::Arcade => plain,
        UnitKind::DiscSet | UnitKind::MultiDisc => Class::Disc,
        UnitKind::Chd => Class::Chd,
    }
}

/// Size of a loose unit's file.
fn file_len(u: &Unit) -> u64 {
    if u.files.len() == 1 {
        u.size
    } else {
        std::fs::metadata(&u.primary)
            .map(|m| m.len())
            .unwrap_or(u.size)
    }
}

// --- Cache ---

fn cached_facts(u: &Unit, ctx: &KeyCtx) -> Option<Facts> {
    let (cache, volume) = (ctx.cache?, ctx.volume_uuid?);
    let row = cache
        .content_row(volume, &u.primary, u.size, u.mtime_ns)
        .ok()??;
    serde_json::from_str::<Facts>(&row.keys_json)
        .ok()
        .filter(|f| f.v == FACTS_VERSION)
}

/// Facts the scan's `quick_ids` memo already holds, read nothing.
fn quick_facts(u: &Unit, class: Class, ctx: &KeyCtx) -> Option<Facts> {
    let Class::Payload(how) = class else {
        return None;
    };
    let outcome = ctx.cache?.quick_id(&u.primary, u.size, u.mtime_ns).ok()??;
    let rule = how.rule();
    let mut f = Facts {
        v: FACTS_VERSION,
        ..Facts::default()
    };
    match (how, outcome) {
        (How::Raw(_), HashOutcome::Content(h)) => {
            // A functional header's bytes are not memoised: read it later.
            if h.headerless.is_some() && functional(rule) {
                return None;
            }
            let sha1 = h.payload_sha1?;
            let size = h.headerless.as_ref().unwrap_or(&h.full).size;
            f.payload_size = Some(size);
            f.exact = Some(ExactKey::Payload { sha1, size });
        }
        (
            How::Zip(_),
            HashOutcome::ZipEntry {
                crc32,
                size,
                payload_sha1,
                ..
            },
        ) => {
            f.entry_crc = u32::from_str_radix(&crc32, 16).ok();
            // Without the entry's first bytes only a size rule is known.
            if magic_header(rule) {
                return Some(f);
            }
            let payload = size - size_rule_header(rule, size) as u64;
            f.payload_size = Some(payload);
            f.exact = payload_sha1.map(|sha1| ExactKey::Payload {
                sha1,
                size: payload,
            });
        }
        _ => return None,
    }
    Some(f)
}

fn store_facts(u: &Unit, f: &Facts, ctx: &KeyCtx) {
    let (Some(cache), Some(volume)) = (ctx.cache, ctx.volume_uuid) else {
        return;
    };
    let Ok(keys_json) = serde_json::to_string(f) else {
        return;
    };
    let row = ContentRow {
        volume_uuid: volume.to_string(),
        path: u.primary.clone(),
        size: u.size,
        mtime_ns: u.mtime_ns,
        file_id: u.file_id,
        keys_json,
        serials_json: None,
        volume_fp: None,
        index_keys: f.exact.iter().map(|k| k.key_string(&u.system)).collect(),
    };
    // The cache is an optimisation: a failed write only costs a re-read.
    let _ = cache.put_content_row(&row);
}

// --- Header rules ---

/// The header tells the emulator how to run the game (spec §5.2).
fn functional(rule: HeaderRule) -> bool {
    matches!(rule, HeaderRule::Nes | HeaderRule::Lynx | HeaderRule::A7800)
}

/// The header is found by its magic, not by the file size.
fn magic_header(rule: HeaderRule) -> bool {
    matches!(
        rule,
        HeaderRule::Nes | HeaderRule::Fds | HeaderRule::Lynx | HeaderRule::A7800
    )
}

fn magic_len(rule: HeaderRule) -> u64 {
    match rule {
        HeaderRule::Nes | HeaderRule::Fds => 16,
        HeaderRule::Lynx => 64,
        HeaderRule::A7800 => 128,
        _ => 0,
    }
}

/// The header length a size-only rule takes off a `len`-byte file.
fn size_rule_header(rule: HeaderRule, len: u64) -> usize {
    if magic_header(rule) {
        0
    } else {
        rule.header_len(&[], len)
    }
}

/// Every payload size a `len`-byte file may have under `rule`, before its
/// first bytes are read.
fn candidate_sizes(rule: HeaderRule, len: u64) -> Vec<u64> {
    if magic_header(rule) {
        let h = magic_len(rule);
        if len > h {
            return vec![len, len - h];
        }
        return vec![len];
    }
    vec![len - size_rule_header(rule, len) as u64]
}

// --- Payload class: loose ROMs, one-ROM zips, whole images ---

fn payload_class(s: &mut Stage) {
    let (units, ctx) = (s.units, s.ctx);
    let hows: Vec<Option<How>> = s
        .classes
        .iter()
        .map(|c| match c {
            Class::Payload(h) => Some(*h),
            _ => None,
        })
        .collect();
    let how = |i: usize| hows[i].expect("payload class only");
    let todo = s.of_class(|c| matches!(c, Class::Payload(_)));

    // 1. Buckets by every candidate payload size; read the first bytes of
    //    units that share one.
    let colliders = {
        let facts: &Vec<Facts> = s.facts;
        colliding(&todo, units, |i| match facts[i].payload_size {
            Some(p) => vec![p],
            None => match how(i) {
                How::Raw(rule) => candidate_sizes(rule, file_len(&units[i])),
                How::Zip(rule) => candidate_sizes(rule, units[i].payload_size.unwrap_or(0)),
            },
        })
    };
    let heads: Vec<usize> = colliders
        .into_iter()
        .filter(|&i| {
            let f = &s.facts[i];
            f.payload_size.is_none()
                || (f.exact.is_none()
                    && match how(i) {
                        How::Raw(_) => f.prefix.is_none(),
                        How::Zip(_) => f.entry_crc.is_none(),
                    })
        })
        .collect();
    s.read_all(&heads, |i, f| match how(i) {
        How::Raw(rule) => read_raw_head(&units[i], rule, f, ctx).map(Some),
        How::Zip(rule) => read_zip_head(&units[i], rule, f, ctx),
    });
    if s.cancel.load(Ordering::Relaxed) {
        return;
    }

    // 2. Buckets by the actual payload size; the cheap fingerprints decide
    //    who still needs a full hash.
    let known: Vec<usize> = todo
        .iter()
        .copied()
        .filter(|&i| s.facts[i].payload_size.is_some())
        .collect();
    let mut need = Vec::new();
    {
        let facts: &Vec<Facts> = s.facts;
        for bucket in buckets(&known, units, |i| vec![facts[i].payload_size.unwrap_or(0)]) {
            for &a in &bucket {
                if facts[a].exact.is_some() {
                    continue;
                }
                if bucket
                    .iter()
                    .any(|&b| b != a && might_match(&facts[a], how(a), &facts[b], how(b)))
                {
                    need.push(a);
                }
            }
        }
    }
    need.sort_unstable();
    need.dedup();
    s.read_all(&need, |i, f| match how(i) {
        How::Raw(rule) => hash_raw(&units[i], rule, f, ctx).map(Some),
        How::Zip(rule) => hash_zip(&units[i], rule, f, ctx),
    });
}

/// Whether `a` (still unhashed) may hold the same payload as `b`, by
/// what is known without a full read.
fn might_match(a: &Facts, ha: How, b: &Facts, hb: How) -> bool {
    match (ha, hb) {
        (How::Raw(_), How::Raw(_)) => match (&a.prefix, &b.prefix) {
            (Some(pa), Some(pb)) => pa == pb,
            _ => true,
        },
        // A whole-entry CRC only speaks for the payload when no header
        // can differ between the two.
        (How::Zip(HeaderRule::Plain), How::Zip(HeaderRule::Plain)) => {
            match (a.entry_crc, b.entry_crc) {
                (Some(ca), Some(cb)) => ca == cb,
                _ => true,
            }
        }
        _ => true,
    }
}

/// Reads the header and the payload prefix of a loose file; a payload
/// that fits the prefix is hashed in the same read.
fn read_raw_head(u: &Unit, rule: HeaderRule, f: &Facts, ctx: &KeyCtx) -> io::Result<Facts> {
    let len = file_len(u);
    let want = (MAX_HEADER + PREFIX) as u64;
    let mut head = Vec::with_capacity(want.min(len) as usize);
    ctx.bytes
        .open(&u.primary)?
        .take(want)
        .read_to_end(&mut head)?;
    let hl = rule.header_len(&head, len).min(head.len());
    let payload_size = len - hl as u64;
    let mut out = f.clone();
    out.v = FACTS_VERSION;
    out.payload_size = Some(payload_size);
    out.header = (functional(rule) && hl > 0).then(|| head[..hl].to_vec());
    if (head.len() as u64) == len {
        // The whole file is in hand: hash it now.
        let h = hash_stream(Cursor::new(&head), len, rule)?;
        out.exact = h.payload_sha1.map(|sha1| ExactKey::Payload {
            sha1,
            size: payload_size,
        });
    }
    out.prefix = Some(vec![prefix_crc(&head[hl..], rule)]);
    Ok(out)
}

/// CRC32 of the first [`PREFIX`] bytes of `payload`, N64 order fixed.
fn prefix_crc(payload: &[u8], rule: HeaderRule) -> u32 {
    let mut prefix = payload[..payload.len().min(PREFIX)].to_vec();
    if rule == HeaderRule::N64 {
        if let Some(order) = n64_to_z64(&prefix).filter(|o| *o != N64Order::Z64) {
            order.normalise(&mut prefix);
        }
    }
    crc32fast::hash(&prefix)
}

fn hash_raw(u: &Unit, rule: HeaderRule, f: &Facts, ctx: &KeyCtx) -> io::Result<Facts> {
    let len = file_len(u);
    let h = hash_stream(ctx.bytes.open(&u.primary)?, len, rule)?;
    let mut out = f.clone();
    let size = f.payload_size.unwrap_or(len);
    out.exact = h.payload_sha1.map(|sha1| ExactKey::Payload { sha1, size });
    Ok(out)
}

/// The ROM entry of a one-ROM zip unit: its index in the listing and its
/// listed entry.
fn zip_entry(u: &Unit, ctx: &KeyCtx) -> io::Result<Option<(usize, ListedEntry)>> {
    let Some(entries) = ctx.listing.list(&u.primary)? else {
        return Ok(None);
    };
    let rom_like = |e: &ListedEntry| {
        let ext = extension(Path::new(&e.name));
        !matches!(ext.as_str(), "zip" | "7z" | "rar") && ctx.db.is_rom_like_extension(&ext)
    };
    let pick = entries
        .iter()
        .position(|e| Some(e.size) == u.payload_size && rom_like(e))
        .or_else(|| entries.iter().position(|e| Some(e.size) == u.payload_size))
        .or_else(|| (entries.len() == 1).then_some(0));
    Ok(pick.map(|i| (i, entries[i].clone())))
}

fn read_zip_head(u: &Unit, rule: HeaderRule, f: &Facts, ctx: &KeyCtx) -> io::Result<Option<Facts>> {
    let Some((index, entry)) = zip_entry(u, ctx)? else {
        return Ok(None);
    };
    let mut out = f.clone();
    out.v = FACTS_VERSION;
    out.entry_crc = entry.crc32;
    let hl = if magic_header(rule) {
        let head = ctx
            .listing
            .head(&u.primary, index, magic_len(rule) as usize)?
            .unwrap_or_default();
        let hl = rule.header_len(&head, entry.size).min(head.len());
        out.header = (functional(rule) && hl > 0).then(|| head[..hl].to_vec());
        hl
    } else {
        size_rule_header(rule, entry.size)
    };
    out.payload_size = Some(entry.size - hl as u64);
    Ok(Some(out))
}

fn hash_zip(u: &Unit, rule: HeaderRule, f: &Facts, ctx: &KeyCtx) -> io::Result<Option<Facts>> {
    let Some((index, entry)) = zip_entry(u, ctx)? else {
        return Ok(None);
    };
    let mut out = f.clone();
    out.entry_crc = entry.crc32;
    match ctx
        .listing
        .payload_sha1(&u.primary, index, rule, ctx.max_hash_bytes)?
    {
        Some(sha1) => {
            let size = f.payload_size.unwrap_or(entry.size);
            out.exact = Some(ExactKey::Payload { sha1, size });
        }
        None => {
            out.exact = entry.crc32.map(|crc32| ExactKey::ZipCrc {
                crc32,
                size: entry.size,
            });
        }
    }
    Ok(Some(out))
}

// --- Arcade sets ---

fn arcade_class(s: &mut Stage) {
    let todo = s.of_class(|c| c == Class::Arcade);
    let colliders = colliding(&todo, s.units, |i| {
        vec![s.units[i].payload_size.unwrap_or(s.units[i].size)]
    });
    let need: Vec<usize> = colliders
        .into_iter()
        .filter(|&i| s.facts[i].exact.is_none())
        .collect();
    let (units, ctx) = (s.units, s.ctx);
    s.read_all(&need, |i, f| {
        let Some(entries) = ctx.listing.list(&units[i].primary)? else {
            return Ok(None);
        };
        let mut pairs = Vec::with_capacity(entries.len());
        for e in &entries {
            let Some(crc) = e.crc32 else {
                return Ok(None);
            };
            pairs.push((crc, e.size));
        }
        pairs.sort_unstable();
        let mut out = f.clone();
        out.v = FACTS_VERSION;
        out.exact = Some(ExactKey::Arcade { entries: pairs });
        Ok(Some(out))
    });
}

// --- Disc sets ---

fn is_sheet(path: &Path) -> bool {
    matches!(extension(path).as_str(), "cue" | "gdi" | "ccd")
}

/// Total bytes of a disc unit's tracks (everything but sheets and
/// playlists), from metadata only.
fn track_bytes(u: &Unit) -> u64 {
    u.files
        .iter()
        .filter(|f| !is_sheet(f) && extension(f) != "m3u")
        .map(|f| std::fs::metadata(f).map(|m| m.len()).unwrap_or(0))
        .sum()
}

fn disc_class(s: &mut Stage) {
    let todo = s.of_class(|c| c == Class::Disc);
    let colliders = colliding(&todo, s.units, |i| vec![track_bytes(&s.units[i])]);
    let (units, ctx) = (s.units, s.ctx);

    // 1. Layout and per-track prefix CRCs.
    let probe: Vec<usize> = colliders
        .iter()
        .copied()
        .filter(|&i| s.facts[i].exact.is_none() && s.facts[i].prefix.is_none())
        .collect();
    s.read_all(&probe, |i, f| {
        let Some((layout, tracks)) = disc_layout(&units[i], ctx)? else {
            return Ok(None);
        };
        let mut prefix = Vec::with_capacity(tracks.len());
        for t in &tracks {
            let mut head = Vec::with_capacity(PREFIX);
            ctx.bytes
                .open(t)?
                .take(PREFIX as u64)
                .read_to_end(&mut head)?;
            prefix.push(crc32fast::hash(&head));
        }
        let mut out = f.clone();
        out.v = FACTS_VERSION;
        out.layout = Some(layout);
        out.prefix = Some(prefix);
        Ok(Some(out))
    });
    if s.cancel.load(Ordering::Relaxed) {
        return;
    }

    // 2. Full track hashes where layout and prefixes still collide.
    let mut by_cheap: HashMap<(String, u64, String, Vec<u32>), Vec<usize>> = HashMap::new();
    for &i in &colliders {
        let f = &s.facts[i];
        if let (Some(layout), Some(prefix)) = (&f.layout, &f.prefix) {
            by_cheap
                .entry((
                    units[i].system.clone(),
                    track_bytes(&units[i]),
                    layout.clone(),
                    prefix.clone(),
                ))
                .or_default()
                .push(i);
        }
    }
    let mut need: Vec<usize> = by_cheap
        .into_values()
        .filter(|b| b.len() > 1)
        .flatten()
        .filter(|&i| s.facts[i].exact.is_none())
        .collect();
    need.sort_unstable();
    s.read_all(&need, |i, f| {
        let Some((layout, tracks)) = disc_layout(&units[i], ctx)? else {
            return Ok(None);
        };
        let mut sums = Vec::with_capacity(tracks.len());
        for t in &tracks {
            let len = std::fs::metadata(t)?.len();
            sums.push(
                hash_stream(ctx.bytes.open(t)?, len, HeaderRule::Plain)?
                    .full
                    .sha1,
            );
        }
        let mut out = f.clone();
        out.exact = Some(ExactKey::DiscTracks {
            tracks: sums,
            layout,
        });
        Ok(Some(out))
    });
}

/// The layout of a disc unit's sheets and its track files in sheet order.
/// `None` when a sheet names a file the unit does not have.
fn disc_layout(u: &Unit, ctx: &KeyCtx) -> io::Result<Option<(String, Vec<PathBuf>)>> {
    let sheets: Vec<&PathBuf> = u.files.iter().filter(|f| is_sheet(f)).collect();
    let by_name = |name: &str| -> Option<PathBuf> {
        let name = Path::new(name)
            .file_name()?
            .to_string_lossy()
            .to_lowercase();
        u.files
            .iter()
            .find(|f| {
                f.file_name()
                    .is_some_and(|n| n.to_string_lossy().to_lowercase() == name)
            })
            .cloned()
    };
    let mut layouts = Vec::new();
    let mut tracks: Vec<PathBuf> = Vec::new();
    if sheets.is_empty() {
        layouts.push("m3u".to_string());
        tracks.extend(u.files.iter().filter(|f| extension(f) != "m3u").cloned());
    }
    for sheet in sheets {
        let ext = extension(sheet);
        let names: Vec<PathBuf> = if ext == "ccd" {
            let stem = sheet
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase());
            ["img", "sub"]
                .iter()
                .filter_map(|want| {
                    u.files
                        .iter()
                        .find(|f| {
                            extension(f) == *want
                                && f.file_stem().map(|s| s.to_string_lossy().to_lowercase()) == stem
                        })
                        .cloned()
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut text = Vec::new();
        ctx.bytes
            .open(sheet)?
            .take(MAX_SHEET)
            .read_to_end(&mut text)?;
        let text = String::from_utf8_lossy(&text);
        let (layout, files) = match ext.as_str() {
            "cue" => (track_layout(&text), cue_files(&text)),
            "gdi" => gdi_layout(&text),
            _ => (ccd_layout(&text), Vec::new()),
        };
        layouts.push(layout);
        let mut found = names;
        for name in files {
            match by_name(&name) {
                Some(p) => found.push(p),
                None => return Ok(None),
            }
        }
        for p in found {
            if !tracks.contains(&p) {
                tracks.push(p);
            }
        }
    }
    if tracks.is_empty() {
        return Ok(None);
    }
    Ok(Some((layouts.join("|"), tracks)))
}

/// The normalised track layout of a cue sheet: the track count, each
/// file's type, each track's number and mode, and its `INDEX`, `PREGAP`
/// and `POSTGAP` entries. File names, comments, `REM` lines and CD-Text
/// are left out, and case and spacing do not matter.
pub fn track_layout(cue_text: &str) -> String {
    let mut tokens = Vec::new();
    let mut tracks = 0usize;
    for line in cue_text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        let mut words = line.split_whitespace();
        let Some(cmd) = words.next() else {
            continue;
        };
        match cmd.to_ascii_uppercase().as_str() {
            "FILE" => {
                let ty = line.split_whitespace().last().unwrap_or("");
                tokens.push(format!("F {}", ty.to_ascii_uppercase()));
            }
            "TRACK" => {
                tracks += 1;
                let num = words.next().map(number).unwrap_or_default();
                let mode = words.next().unwrap_or("").to_ascii_uppercase();
                tokens.push(format!("T{num} {mode}"));
            }
            "INDEX" => {
                let num = words.next().map(number).unwrap_or_default();
                let at = words.next().unwrap_or("");
                tokens.push(format!("I{num} {at}"));
            }
            gap @ ("PREGAP" | "POSTGAP") => {
                tokens.push(format!("{gap} {}", words.next().unwrap_or("")));
            }
            _ => {}
        }
    }
    format!("cue;{tracks};{}", tokens.join(";"))
}

/// `07` → `7`; anything else as is, upper-cased.
fn number(word: &str) -> String {
    word.parse::<u32>()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| word.to_ascii_uppercase())
}

/// File names a cue sheet references, in order.
fn cue_files(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim().trim_start_matches('\u{feff}');
            line.get(..5).filter(|c| c.eq_ignore_ascii_case("FILE "))?;
            let rest = line[5..].trim_start();
            if let Some(quoted) = rest.strip_prefix('"') {
                quoted.split('"').next().map(str::to_string)
            } else {
                rest.split_whitespace().next().map(str::to_string)
            }
        })
        .collect()
}

/// A gdi sheet's layout (track, LBA, type, sector size, offset per line)
/// and its track file names in order.
fn gdi_layout(text: &str) -> (String, Vec<String>) {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let count = lines.next().unwrap_or("").to_string();
    let mut parts = vec![format!("gdi;{count}")];
    let mut files = Vec::new();
    for line in lines {
        let mut fields: Vec<&str> = line.splitn(5, char::is_whitespace).collect();
        if fields.len() < 5 {
            parts.push(line.to_string());
            continue;
        }
        let rest = fields.pop().unwrap_or("").trim_start();
        let (name, offset) = if let Some(quoted) = rest.strip_prefix('"') {
            let mut split = quoted.splitn(2, '"');
            (
                split.next().unwrap_or("").to_string(),
                split.next().unwrap_or("").trim().to_string(),
            )
        } else {
            let mut split = rest.splitn(2, char::is_whitespace);
            (
                split.next().unwrap_or("").to_string(),
                split.next().unwrap_or("").trim().to_string(),
            )
        };
        parts.push(format!("{} {offset}", fields.join(" ")));
        files.push(name);
    }
    (parts.join(";"), files)
}

/// A CloneCD sheet holds no file names: its trimmed, upper-cased lines.
fn ccd_layout(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_ascii_uppercase())
        .filter(|l| !l.is_empty())
        .collect();
    format!("ccd;{}", lines.join(";"))
}

// --- CHD ---

fn chd_class(s: &mut Stage) {
    let todo = s.of_class(|c| c == Class::Chd);
    // A CHD's file size says nothing about its data: any two in a system
    // collide, and only their headers are read.
    let mut per_system: HashMap<&str, Vec<usize>> = HashMap::new();
    for &i in &todo {
        per_system.entry(&s.units[i].system).or_default().push(i);
    }
    let mut need: Vec<usize> = per_system
        .into_values()
        .filter(|v| v.len() > 1)
        .flatten()
        .filter(|&i| s.facts[i].exact.is_none())
        .collect();
    need.sort_unstable();
    let (units, ctx) = (s.units, s.ctx);
    s.read_all(&need, |i, f| {
        let mut head = Vec::with_capacity(CHD_HEAD);
        ctx.bytes
            .open(&units[i].primary)?
            .take(CHD_HEAD as u64)
            .read_to_end(&mut head)?;
        let Some((_, sha1, raw)) = parse_chd_header(&head) else {
            return Ok(None);
        };
        let mut out = f.clone();
        out.v = FACTS_VERSION;
        out.exact = match (sha1, &raw) {
            (Some(sha1), _) => Some(ExactKey::Chd { sha1 }),
            (None, Some(rawsha1)) => Some(ExactKey::ChdRaw {
                rawsha1: rawsha1.clone(),
            }),
            (None, None) => None,
        };
        out.chd_raw = raw;
        Ok(Some(out))
    });
}

/// Version, combined SHA1 and raw data SHA1 (upper-case hex) of a CHD
/// header; `None` when `head` is not a whole v3, v4 or v5 header. An
/// all-zero hash counts as absent. v3 has only its data SHA1.
pub fn parse_chd_header(head: &[u8]) -> Option<(u32, Option<String>, Option<String>)> {
    if head.get(..8)? != b"MComprHD" {
        return None;
    }
    let be32 = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes(head.get(at..at + 4)?.try_into().ok()?))
    };
    let (len, version) = (be32(8)? as usize, be32(12)?);
    let (sha1_at, raw_at, need) = match version {
        3 => (80, None, 120),
        4 => (48, Some(88), 108),
        5 => (84, Some(64), 124),
        _ => return None,
    };
    if head.len() < need || len < need {
        return None;
    }
    let hash = |at: usize| {
        let b = &head[at..at + 20];
        (!b.iter().all(|x| *x == 0)).then(|| hex_upper(b))
    };
    Some((version, hash(sha1_at), raw_at.and_then(hash)))
}

// --- Shared helpers ---

/// Units of `todo` sharing a `(system, size)` bucket with another unit,
/// for any of their candidate sizes. Sorted.
fn colliding(todo: &[usize], units: &[Unit], sizes: impl Fn(usize) -> Vec<u64>) -> Vec<usize> {
    let mut count: HashMap<(&str, u64), usize> = HashMap::new();
    let per: Vec<Vec<u64>> = todo.iter().map(|&i| sizes(i)).collect();
    for (&i, s) in todo.iter().zip(&per) {
        for &size in s {
            *count.entry((units[i].system.as_str(), size)).or_default() += 1;
        }
    }
    todo.iter()
        .zip(&per)
        .filter(|(&i, s)| {
            s.iter()
                .any(|&size| count[&(units[i].system.as_str(), size)] > 1)
        })
        .map(|(&i, _)| i)
        .collect()
}

/// The `(system, size)` buckets of two or more units.
fn buckets(todo: &[usize], units: &[Unit], sizes: impl Fn(usize) -> Vec<u64>) -> Vec<Vec<usize>> {
    let mut map: HashMap<(&str, u64), Vec<usize>> = HashMap::new();
    for &i in todo {
        for size in sizes(i) {
            map.entry((units[i].system.as_str(), size))
                .or_default()
                .push(i);
        }
    }
    let mut out: Vec<Vec<usize>> = map.into_values().filter(|b| b.len() > 1).collect();
    out.sort();
    out
}

/// Sorted SHA1s of the soft patches; an unreadable patch counts by its
/// path so it never matches another.
fn patch_set(patches: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = patches
        .iter()
        .map(|p| {
            File::open(p)
                .and_then(|f| {
                    let len = f.metadata()?.len();
                    hash_stream(f, len, HeaderRule::Plain)
                })
                .map(|h| h.full.sha1)
                .unwrap_or_else(|_| format!("unreadable:{}", p.display()))
        })
        .collect();
    out.sort();
    out
}

fn sha1_of(bytes: &[u8]) -> String {
    hex_upper(&Sha1::digest(bytes))
}

/// Runs `f` over `items` on up to `workers` threads, reporting progress
/// from the calling thread. Stops taking new items on `cancel`.
fn par_map<T, F>(
    items: &[usize],
    workers: usize,
    cancel: &AtomicBool,
    progress: &dyn Fn(DedupePhase, usize, usize),
    f: F,
) -> Vec<(usize, T)>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
{
    let total = items.len();
    if total == 0 {
        return Vec::new();
    }
    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel();
    let mut out = Vec::with_capacity(total);
    std::thread::scope(|scope| {
        for _ in 0..workers.clamp(1, total) {
            let (tx, next, f) = (tx.clone(), &next, &f);
            scope.spawn(move || loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let k = next.fetch_add(1, Ordering::Relaxed);
                let Some(&i) = items.get(k) else {
                    break;
                };
                if tx.send((i, f(i))).is_err() {
                    break;
                }
            });
        }
        drop(tx);
        progress(DedupePhase::Fingerprints, 0, total);
        for item in rx {
            out.push(item);
            progress(DedupePhase::Fingerprints, out.len(), total);
        }
    });
    out.sort_by_key(|(i, _)| *i);
    out
}
