//! Combining identification evidence into a verdict (organise spec §6):
//! per-system scores, the margin rule, content support, vetoes and the
//! auto/review thresholds (pure), and the [`Catalogue`] of DATs the
//! identification ladder asks for CRC hits, ROM stems and arcade sets.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::cache::Cache;
use crate::identify::dat::{dat_sources, DatStatus, DatStore};
use crate::identify::evidence::{Evidence, IdLevel};
use crate::identify::sniff::{Sniff, DECISIVE};
use crate::systems::{ExtClass, SystemDb};

/// Scores at or above `auto` (with margin and content support) are moved
/// without asking; scores at or above `review` go to the review list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thresholds {
    pub auto: f32,
    pub review: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            auto: 0.85,
            review: 0.50,
        }
    }
}

/// What happens to a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Auto,
    Review,
    Unknown,
}

/// The outcome of [`combine`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Combined {
    pub verdict: Verdict,
    pub system: Option<String>,
    pub confidence: f32,
    pub method: Option<IdLevel>,
    /// Evidence for `system`, strongest level first, at most 4.
    pub why: Vec<Evidence>,
    /// Other systems scoring at least 0.40, best first.
    pub alternatives: Vec<(String, f32)>,
}

impl Combined {
    /// Nothing points anywhere.
    pub fn unknown() -> Combined {
        Combined {
            verdict: Verdict::Unknown,
            system: None,
            confidence: 0.0,
            method: None,
            why: Vec::new(),
            alternatives: Vec::new(),
        }
    }
}

/// Everything [`combine`] weighs for one file.
pub struct CombineInput<'a> {
    pub evidence: &'a [Evidence],
    /// Hard vetoes: systems this file can never belong to.
    pub excluded: &'a [String],
    /// The sniff read bytes (not cloud-only, not a 7z/CHD).
    pub content_read: bool,
    /// The chosen folder is a broad root (home, Downloads…): reviews
    /// without content evidence are dropped to unknown.
    pub broad_root: bool,
    pub thresholds: Thresholds,
}

/// Bonus per extra agreeing evidence level.
const BONUS: f32 = 0.03;
/// Lead the best system needs over the runner-up to be automatic.
const MARGIN: f32 = 0.10;
/// Lowest score still named as a candidate or alternative.
const CANDIDATE: f32 = 0.40;
/// Slack for `f32` sums and differences (`0.95 - 0.85` is `0.0999…`).
const EPS: f32 = 1e-4;

/// Combines `input.evidence` into a verdict: the best system's score is
/// its strongest evidence plus [`BONUS`] per further agreeing level;
/// `Auto` needs the auto threshold, a [`MARGIN`] lead and content support,
/// or a user choice that no hard veto excluded.
pub fn combine(input: &CombineInput) -> Combined {
    let mut by_system: BTreeMap<&str, Vec<&Evidence>> = BTreeMap::new();
    for e in input
        .evidence
        .iter()
        .filter(|e| !input.excluded.contains(&e.system))
    {
        by_system.entry(e.system.as_str()).or_default().push(e);
    }
    let mut scored: Vec<(&str, f32, bool)> = by_system
        .iter()
        .map(|(system, evs)| {
            let max = evs.iter().map(|e| e.weight).fold(0.0f32, f32::max);
            let mut levels: Vec<IdLevel> = evs.iter().map(|e| e.kind).collect();
            levels.sort();
            levels.dedup();
            let score = (max + BONUS * (levels.len() as f32 - 1.0)).min(1.0);
            (
                *system,
                score,
                has_content_support(system, evs, input.content_read),
            )
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
    // A user choice (made now or remembered by content) settles the file
    // unless a hard veto excluded it above (spec §6.6: method = user).
    let chosen = input
        .evidence
        .iter()
        .find(|e| e.kind == IdLevel::User && by_system.contains_key(e.system.as_str()))
        .map(|e| e.system.as_str());
    if let Some(user) = chosen {
        if let Some(i) = scored.iter().position(|s| s.0 == user) {
            let picked = scored.remove(i);
            scored.insert(0, (picked.0, 1.0, true));
        }
    }
    let Some(&(best, score, content)) = scored.first() else {
        return Combined::unknown();
    };
    let second = scored.get(1).map_or(0.0, |s| s.1);
    let automatic =
        score + EPS >= input.thresholds.auto && score - second + EPS >= MARGIN && content;
    let verdict = if chosen.is_some() || automatic {
        Verdict::Auto
    } else if score + EPS >= input.thresholds.review && !(input.broad_root && !content) {
        Verdict::Review
    } else {
        Verdict::Unknown
    };
    let mut why: Vec<Evidence> = by_system[best].iter().map(|e| (*e).clone()).collect();
    why.sort_by(|a, b| {
        why_rank(a.kind)
            .cmp(&why_rank(b.kind))
            .then(b.weight.total_cmp(&a.weight))
    });
    why.truncate(4);
    Combined {
        verdict,
        system: (verdict != Verdict::Unknown || score + EPS >= CANDIDATE).then(|| best.to_string()),
        confidence: score,
        method: why.first().map(|e| e.kind),
        why,
        alternatives: scored
            .iter()
            .skip(1)
            .filter(|s| s.1 + EPS >= CANDIDATE)
            .map(|s| (s.0.to_string(), s.1))
            .collect(),
    }
}

/// Order of `why`: the user's choice first, then the ladder's precedence.
fn why_rank(kind: IdLevel) -> (bool, IdLevel) {
    (kind != IdLevel::User, kind)
}

/// A DAT hit, header evidence ≥ 0.85, a user choice, or a unique extension
/// of a system without a decisive signature whose contents were read.
fn has_content_support(system: &str, evs: &[&Evidence], content_read: bool) -> bool {
    evs.iter().any(|e| match e.kind {
        IdLevel::Hash | IdLevel::User => true,
        IdLevel::Header => e.weight + EPS >= 0.85,
        IdLevel::Extension => content_read && e.weight + EPS >= 0.85 && !DECISIVE.contains(&system),
        IdLevel::Name | IdLevel::Online => false,
    })
}

/// Extension evidence for `ext`: 0.85 when one system lists it, 0.50 when
/// several do, and 0.60 (detail suffixed `!`) when the extension is unique
/// to a system with a decisive signature that the sniffed bytes lack.
pub fn extension_evidence(db: &SystemDb, ext: &str, sniff: &Sniff) -> Vec<Evidence> {
    let class = db.extension_class(ext);
    let weight = match class {
        ExtClass::Unique => 0.85,
        ExtClass::Shared => 0.50,
        _ => return Vec::new(),
    };
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    let read_enough = sniff.read >= 0x200 && sniff.not_a_rom.is_none();
    db.systems_for_extension(&ext)
        .into_iter()
        .map(|s| {
            let contradicted = class == ExtClass::Unique
                && read_enough
                && DECISIVE.contains(&s.id.as_str())
                && !sniff.hits.iter().any(|h| h.system == s.id);
            let w = if contradicted { 0.60 } else { weight };
            let detail = if contradicted {
                format!("{ext}!")
            } else {
                ext.clone()
            };
            Evidence::new(IdLevel::Extension, &s.id, w, detail)
        })
        .collect()
}

/// DATs one plan may download on demand; the rest of the missing
/// catalogues are reported as not consulted.
pub const MAX_DAT_DOWNLOADS: usize = 4;

/// A DAT row that matched a file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogueHit {
    pub system: String,
    pub game: String,
    pub rom_name: String,
}

/// The DAT knowledge the identification ladder consults. Every lookup
/// degrades to "no hit" on errors.
pub trait Catalogue: Sync {
    /// Rows of `systems`' DATs with this CRC32 (any case) and `size` (or
    /// an unknown size).
    fn hits(&self, systems: &[String], crc32: &str, size: u64) -> Vec<CatalogueHit>;
    /// Rows of every cached DAT with this CRC32 and size.
    fn any_hits(&self, crc32: &str, size: u64) -> Vec<CatalogueHit>;
    /// `(rom stem, game)` of `system`'s DATs, stems lower-case.
    fn stems(&self, system: &str) -> Vec<(String, String)>;
    /// Systems with a cached DAT, sorted.
    fn stem_systems(&self) -> Vec<String>;
    /// An arcade set by short name (any case): the DAT game and the
    /// `(crc32, size)` of its ROMs.
    fn set_roms(&self, system: &str, set: &str) -> Option<(String, Vec<(String, u64)>)>;
    /// Systems asked about whose DAT was not available (offline, missing
    /// upstream, past the download cap, or no DAT at all), sorted.
    fn unconsulted(&self) -> Vec<String>;
}

#[derive(Default)]
struct CatalogueState {
    /// Whether a DAT's rows can be used; decided once per DAT.
    ready: HashMap<String, bool>,
    downloads: usize,
    unconsulted: BTreeSet<String>,
}

/// A [`Catalogue`] over the cache's DAT rows: DATs already cached are used
/// as they are; missing ones are downloaded through `store` (when given)
/// up to `max_downloads` per catalogue, i.e. per plan.
pub struct DatCatalogue<'a> {
    db: &'a SystemDb,
    cache: &'a Cache,
    store: Option<&'a DatStore<'a>>,
    cap: usize,
    cached: HashSet<String>,
    state: Mutex<CatalogueState>,
}

impl<'a> DatCatalogue<'a> {
    pub fn new(
        db: &'a SystemDb,
        cache: &'a Cache,
        store: Option<&'a DatStore<'a>>,
        max_downloads: usize,
    ) -> Self {
        DatCatalogue {
            db,
            cache,
            store,
            cap: max_downloads,
            cached: cache
                .cached_dats()
                .unwrap_or_default()
                .into_iter()
                .collect(),
            state: Mutex::default(),
        }
    }

    /// The usable DATs of `system`, downloading missing ones within the
    /// cap. The lock is held across a download so two workers never fetch
    /// the same DAT.
    fn ready_dats(&self, system: &str) -> Vec<String> {
        let Some(sys) = self.db.get(system) else {
            return Vec::new();
        };
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        for dat in dat_sources(sys) {
            let ready = match state.ready.get(&dat) {
                Some(r) => *r,
                None => {
                    let r = if self.cached.contains(&dat) {
                        true
                    } else {
                        match self.store {
                            Some(store) if state.downloads < self.cap => {
                                state.downloads += 1;
                                matches!(
                                    store.ensure(&dat),
                                    Ok(DatStatus::Ready | DatStatus::Stale)
                                )
                            }
                            _ => false,
                        }
                    };
                    state.ready.insert(dat.clone(), r);
                    r
                }
            };
            if ready {
                out.push(dat);
            }
        }
        if out.is_empty() {
            state.unconsulted.insert(sys.id.clone());
        }
        out
    }
}

impl Catalogue for DatCatalogue<'_> {
    fn hits(&self, systems: &[String], crc32: &str, size: u64) -> Vec<CatalogueHit> {
        let mut out = Vec::new();
        for system in systems {
            for dat in self.ready_dats(system) {
                let rows = self.cache.dat_by_crc(&dat, crc32).unwrap_or_default();
                out.extend(
                    rows.into_iter()
                        .filter(|r| r.size.is_none_or(|s| s == size))
                        .map(|r| CatalogueHit {
                            system: system.clone(),
                            game: r.game,
                            rom_name: r.rom_name,
                        }),
                );
            }
        }
        out
    }

    fn any_hits(&self, crc32: &str, size: u64) -> Vec<CatalogueHit> {
        self.cache
            .dat_by_crc_any(crc32, size)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(dat, r)| {
                let system = self.db.system_for_dat(&dat)?;
                Some(CatalogueHit {
                    system: system.id.clone(),
                    game: r.game,
                    rom_name: r.rom_name,
                })
            })
            .collect()
    }

    fn stems(&self, system: &str) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .ready_dats(system)
            .iter()
            .flat_map(|dat| self.cache.dat_stems(dat).unwrap_or_default())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn stem_systems(&self) -> Vec<String> {
        let systems: BTreeSet<String> = self
            .cache
            .cached_dats()
            .unwrap_or_default()
            .iter()
            .filter_map(|dat| self.db.system_for_dat(dat))
            .map(|s| s.id.clone())
            .collect();
        systems.into_iter().collect()
    }

    fn set_roms(&self, system: &str, set: &str) -> Option<(String, Vec<(String, u64)>)> {
        for dat in self.ready_dats(system) {
            // libretro DATs list a set as one ROM named `<set>.zip` under
            // the game's description; full DATs name the game by the set.
            let by_stem = self
                .cache
                .dat_by_rom_stem(&dat, set)
                .unwrap_or_default()
                .into_iter()
                .next()
                .map(|r| r.game);
            let candidates = by_stem
                .into_iter()
                .chain([set.to_string(), set.to_lowercase()]);
            for game in candidates {
                let rows = self.cache.dat_rows_of_game(&dat, &game).unwrap_or_default();
                if rows.is_empty() {
                    continue;
                }
                let roms = rows
                    .into_iter()
                    .filter_map(|r| Some((r.crc32?, r.size.unwrap_or(0))))
                    .collect();
                return Some((game, roms));
            }
        }
        None
    }

    fn unconsulted(&self) -> Vec<String> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.unconsulted.iter().cloned().collect()
    }
}
