//! Phase one of a fill (spec §7 steps 3–5, §16.1): pick the ROMs and media
//! to fill, identify each ROM and gather art candidates from every enabled
//! source into the SQLite cache. Nothing is downloaded but indexes and DATs;
//! nothing is written to the library. A rate limit or a transport failure
//! (offline, timed out) pauses the phase (spec §10).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::art::{ArtState, SystemArt};
use crate::cache::{now_unix, Cache};
use crate::identify::dat::DatError;
use crate::identify::names;
use crate::identify::{IdMethod, Identifier, Identity};
use crate::profiles::Media;
use crate::progress::{Phase, ProgressEvent, ProgressSink};
use crate::scan::RomEntry;
use crate::sources::libretro::INDEX_TTL_SECS;
use crate::sources::{sort_candidates, ArtQuery, ArtSource, Candidate, SourceError};
use crate::systems::{System, SystemDb};

/// Cache namespace of gathered candidate lists.
const NS: &str = "candidates";

/// Which ROMs get art: only those missing it, or every one (overwrite).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    #[default]
    Missing,
    All,
}

/// What to fill: system folders or ids (empty = all), media and scope.
#[derive(Clone, Debug, Default)]
pub struct TargetFilter {
    pub systems: Vec<String>,
    pub media: Vec<Media>,
    pub scope: Scope,
}

/// One ROM to gather art for, with the media it needs.
#[derive(Clone, Debug)]
pub struct GatherTarget {
    pub system_id: String,
    /// The system folder name as on disk.
    pub folder: String,
    pub system_dir: PathBuf,
    pub rom: RomEntry,
    pub media: Vec<Media>,
    /// The folder numbers its ROMs (`001 Name`): strip before matching.
    pub strip_numbering: bool,
}

/// The ROMs and media of `art` that `filter` selects. Unknown folders and
/// media the profile cannot store are never targets.
pub fn select_targets(art: &[SystemArt], filter: &TargetFilter) -> Vec<GatherTarget> {
    let wanted = |s: &SystemArt| {
        filter.systems.is_empty()
            || filter.systems.iter().any(|f| {
                f.eq_ignore_ascii_case(&s.scan.folder_name)
                    || s.scan
                        .system_id
                        .as_deref()
                        .is_some_and(|id| f.eq_ignore_ascii_case(id))
            })
    };
    let mut out = Vec::new();
    for system in art.iter().filter(|s| wanted(s)) {
        let Some(system_id) = system.scan.system_id.clone() else {
            continue;
        };
        let strip = names::detect_numbering(system.roms.iter().map(|r| r.rom.stem.as_str()));
        for rom_art in &system.roms {
            let media: Vec<Media> = filter
                .media
                .iter()
                .copied()
                .filter(|m| match rom_art.art.get(m) {
                    Some(ArtState::Missing) => true,
                    Some(ArtState::Present { .. }) => filter.scope == Scope::All,
                    _ => false,
                })
                .collect();
            if media.is_empty() {
                continue;
            }
            out.push(GatherTarget {
                system_id: system_id.clone(),
                folder: system.scan.folder_name.clone(),
                system_dir: system.scan.dir.clone(),
                rom: rom_art.rom.clone(),
                media,
                strip_numbering: strip,
            });
        }
    }
    out
}

/// Candidate gathering settings (spec §9 defaults).
#[derive(Clone, Debug)]
pub struct GatherOptions {
    /// Lowest fuzzy score kept.
    pub min_score: u8,
    /// Whether fuzzy matching may run.
    pub fuzzy: bool,
    /// Most fuzzy candidates per source and media.
    pub max_candidates: usize,
    /// Count in how many listings each variant appears (Popularity, D9 b).
    pub consensus: bool,
    /// Ignore cached candidate lists.
    pub refresh: bool,
    /// How long a cached candidate list stays fresh.
    pub ttl_secs: i64,
}

impl Default for GatherOptions {
    fn default() -> Self {
        GatherOptions {
            min_score: 65,
            fuzzy: true,
            max_candidates: 5,
            consensus: false,
            refresh: false,
            ttl_secs: INDEX_TTL_SECS,
        }
    }
}

/// The target of a [`GatherItem`], without the ROM's file metadata.
#[derive(Clone, Debug, Serialize)]
pub struct GatherTargetSummary {
    pub system_id: String,
    pub folder: String,
    pub system_dir: PathBuf,
    pub rel_dir: PathBuf,
    pub path: PathBuf,
    pub stem: String,
    pub media: Vec<Media>,
}

/// One ROM: its identity and the candidates of each media.
#[derive(Clone, Debug, Serialize)]
pub struct GatherItem {
    pub target: GatherTargetSummary,
    pub identity: Identity,
    /// Candidates of every source, sorted strongest first.
    pub candidates: BTreeMap<Media, Vec<Candidate>>,
    /// Why a source could not answer for a media. A media with no
    /// `candidates` entry was not searched to the end (`paused: …`).
    pub errors: BTreeMap<Media, String>,
}

/// Result of [`gather`].
#[derive(Clone, Debug, Default, Serialize)]
pub struct Gathered {
    pub items: Vec<GatherItem>,
    pub warnings: Vec<String>,
    /// Why the phase paused (rate limit, offline); resume by running again.
    pub paused: Option<String>,
    pub cancelled: bool,
}

/// Identifies each target and gathers its candidates from `sources`,
/// caching each candidate list. Stops at a cancel, a rate limit or a
/// transport failure; the items before it are kept.
#[allow(clippy::too_many_arguments)]
pub fn gather(
    targets: &[GatherTarget],
    systems: &SystemDb,
    sources: &[&dyn ArtSource],
    identifier: &Identifier,
    cache: &Cache,
    opts: &GatherOptions,
    progress: &dyn ProgressSink,
    cancel: &AtomicBool,
) -> Gathered {
    let mut out = Gathered::default();
    let total = targets.len();
    progress.event(ProgressEvent::Started {
        phase: Phase::Gather,
        total,
    });
    for (n, t) in targets.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            out.cancelled = true;
            break;
        }
        let Some(system) = systems.get(&t.system_id) else {
            continue;
        };
        progress.event(ProgressEvent::Item {
            phase: Phase::Gather,
            done: n,
            total,
            label: t.rom.stem.clone(),
        });
        let identity = match identifier.identify(&t.rom, system, t.strip_numbering) {
            Ok(i) => i,
            Err(DatError::Net(e)) if e.pauses() => {
                out.paused = Some(e.to_string());
                break;
            }
            Err(e) => {
                let w = format!("{}: identification failed: {e}", t.rom.path.display());
                progress.event(ProgressEvent::Warning(w.clone()));
                out.warnings.push(w);
                Identity {
                    canonical: None,
                    method: IdMethod::Name,
                    confidence: 0.0,
                    parsed: names::parse_name(&t.rom.stem, t.strip_numbering),
                    hash: None,
                }
            }
        };
        let mut item = GatherItem {
            target: summary(t),
            identity,
            candidates: BTreeMap::new(),
            errors: BTreeMap::new(),
        };
        let mut paused = None;
        'media: for &media in &t.media {
            let mut all = Vec::new();
            for &src in sources {
                match source_candidates(src, system, media, &item.identity, t, cache, opts) {
                    Ok(found) => all.extend(found),
                    Err(SourceError::Net(e)) if e.pauses() => {
                        // "rate limited by … (HTTP 429)" / "offline or unreachable: …"
                        paused = Some(e.to_string());
                        break 'media;
                    }
                    Err(e) => {
                        item.errors.insert(media, e.to_string());
                    }
                }
            }
            sort_candidates(&mut all);
            item.candidates.insert(media, all);
        }
        // A paused item keeps what was gathered before the pause; the media
        // not searched to the end say so, so they never read as "no art".
        if let Some(reason) = &paused {
            for &media in &t.media {
                if !item.candidates.contains_key(&media) {
                    item.errors.insert(media, format!("paused: {reason}"));
                }
            }
        }
        out.items.push(item);
        if paused.is_some() {
            out.paused = paused;
            break;
        }
    }
    progress.event(ProgressEvent::Finished {
        phase: Phase::Gather,
    });
    out
}

/// The candidates of one source for one ROM and media: from the cache when
/// fresh, else asked and stored.
fn source_candidates(
    src: &dyn ArtSource,
    system: &System,
    media: Media,
    identity: &Identity,
    t: &GatherTarget,
    cache: &Cache,
    opts: &GatherOptions,
) -> Result<Vec<Candidate>, SourceError> {
    let key = cache_key(src.id(), system, media, identity, &t.rom.stem, opts);
    if !opts.refresh {
        // An unreadable cached value is a miss.
        if let Ok(Some(s)) = cache.get_json::<Vec<Candidate>>(NS, &key) {
            if now_unix() - s.fetched_at < opts.ttl_secs {
                return Ok(s.value);
            }
        }
    }
    let q = ArtQuery {
        system,
        media,
        canonical: identity.canonical.as_deref(),
        rom: &identity.parsed,
        stem: &t.rom.stem,
        by_name: identity.method != IdMethod::Unknown || identity.canonical.is_some(),
        fuzzy: opts.fuzzy,
        min_score: opts.min_score,
        max_candidates: opts.max_candidates,
    };
    let mut found = src.candidates(&q)?;
    if opts.consensus {
        add_consensus(src, system, media, &mut found)?;
    }
    // The cache only saves work: failing to store is not an error.
    let _ = cache.put_json(NS, &key, &found, None);
    Ok(found)
}

/// Every option that changes the candidate list or its fields is part of
/// the key, so a cached run without consensus is never reused by
/// `--strategy popularity`.
fn cache_key(
    src: &str,
    system: &System,
    media: Media,
    identity: &Identity,
    stem: &str,
    opts: &GatherOptions,
) -> String {
    // The stem is always part of it: a source also matches the file name
    // itself, so two ROMs with one DAT identity can get different lists.
    let who = match &identity.canonical {
        Some(c) => format!("{c}|{stem}"),
        None => format!("{}|{stem}", identity.parsed.key),
    };
    format!(
        "{src}\u{1f}{}\u{1f}{media:?}\u{1f}{who}\u{1f}{:?}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        system.id, identity.method, opts.fuzzy, opts.min_score, opts.max_candidates, opts.consensus
    )
}

/// `consensus` = 1 + the number of the source's other media listings that
/// carry the same variant (D9 part b, measured inside one source in M2).
/// A listing that cannot be read counts as not carrying it, except a rate
/// limit or going offline, which pause the phase.
fn add_consensus(
    src: &dyn ArtSource,
    system: &System,
    media: Media,
    found: &mut [Candidate],
) -> Result<(), SourceError> {
    for c in found.iter_mut() {
        let mut n: u8 = 1;
        for other in Media::ALL.into_iter().filter(|&m| m != media) {
            match src.has_name(system, other, &c.name) {
                Ok(true) => n = n.saturating_add(1),
                Ok(false) => {}
                Err(SourceError::Net(e)) if e.pauses() => return Err(SourceError::Net(e)),
                Err(_) => {}
            }
        }
        c.consensus = n;
    }
    Ok(())
}

/// The summary of `t` stored in a [`GatherItem`].
fn summary(t: &GatherTarget) -> GatherTargetSummary {
    GatherTargetSummary {
        system_id: t.system_id.clone(),
        folder: t.folder.clone(),
        system_dir: t.system_dir.clone(),
        rel_dir: t.rom.rel_dir.clone(),
        path: t.rom.path.clone(),
        stem: t.rom.stem.clone(),
        media: t.media.clone(),
    }
}
