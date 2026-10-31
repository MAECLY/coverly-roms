//! Combining identification evidence into a verdict (organise spec §6):
//! per-system scores, the margin rule, content support, vetoes and the
//! auto/review thresholds (pure), and the [`Catalogue`] of DATs the
//! identification ladder asks for CRC hits, ROM stems and arcade sets,
//! and the ladder itself ([`identify_candidate`]): hash, header,
//! extension, name and (when configured) online evidence per file.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::cache::Cache;
use crate::dedupe::source::DiscId;
use crate::identify::dat::{dat_sources, DatError, DatStatus, DatStore};
use crate::identify::evidence::{Evidence, IdLevel};
use crate::identify::hash::{hash_rom, HashOutcome, HeaderRule, DEFAULT_HASH_MAX_BYTES};
use crate::identify::names::{parse_name, score, strip_numbering_prefix};
use crate::identify::sniff::{Sniff, DECISIVE};
use crate::organize::inspect::{Container, DiscSetInfo, FoundSystem, Inspection, NOTE_ENCRYPTED};
use crate::organize::plan::SheetCheck;
use crate::organize::plan::{ItemNote, LeftReason};
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

/// Header evidence at or above this weight for a system that lists the
/// extension as weak contradicts the extension's strong owner (spec D24).
const WEAK_OWNER_PROOF: f32 = 0.95;

/// Weight of a weak extension that agrees with another signal (`ext~`).
const WEAK_SUPPORT: f32 = 0.50;
/// A header names a weak owner only from the review line up: a tie-break
/// hit under it (the 0.40 `.prg` load address, spec §8) never lifts a
/// weak owner over the extension's strong owners (D24).
const WEAK_NAMING_HEADER: f32 = 0.50;

/// Extension evidence for `ext`: 0.85 when one system lists it, 0.50 when
/// several do, and 0.60 (detail suffixed `!`) when the extension is unique
/// to a system with a decisive signature that the sniffed bytes lack, or
/// when a ≥ 0.95 header proves another system that lists `ext` as weak
/// (spec D24). Weak owners add nothing here: see [`weak_extension_support`].
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
            let lacks_signature = class == ExtClass::Unique
                && read_enough
                && DECISIVE.contains(&s.id.as_str())
                && !sniff.hits.iter().any(|h| h.system == s.id);
            let contradicted = lacks_signature || weak_owner_proven(db, &ext, &s.id, &sniff.hits);
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

/// Whether `hits` hold a ≥ 0.95 header for a system other than `owner`
/// that lists `ext` as weak (spec D24): `owner`'s extension is then
/// contradicted.
fn weak_owner_proven(db: &SystemDb, ext: &str, owner: &str, hits: &[Evidence]) -> bool {
    hits.iter().any(|h| {
        h.kind == IdLevel::Header
            && h.weight + EPS >= WEAK_OWNER_PROOF
            && h.system != owner
            && db
                .get(&h.system)
                .is_some_and(|w| w.weak_extensions.iter().any(|x| x == ext))
    })
}

/// Weak-extension support (spec D3, D4): a system that lists `ext` as
/// weak gains 0.50 (detail `ext~`) only when a header of at least 0.50, a
/// DAT hit or a user choice already names it. A weak extension alone never assigns a system.
pub fn weak_extension_support(db: &SystemDb, ext: &str, evidence: &mut Vec<Evidence>) {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    if ext.is_empty() {
        return;
    }
    let detail = format!("{ext}~");
    for s in db.systems_accepting(&ext) {
        if !s.weak_extensions.contains(&ext) {
            continue;
        }
        let named = evidence.iter().any(|e| {
            e.system == s.id
                && match e.kind {
                    IdLevel::Header => e.weight + EPS >= WEAK_NAMING_HEADER,
                    IdLevel::Hash | IdLevel::User => true,
                    IdLevel::Extension | IdLevel::Name | IdLevel::Online => false,
                }
        });
        let known = evidence
            .iter()
            .any(|e| e.kind == IdLevel::Extension && e.system == s.id && e.detail == detail);
        if named && !known {
            evidence.push(Evidence::new(
                IdLevel::Extension,
                &s.id,
                WEAK_SUPPORT,
                detail.clone(),
            ));
        }
    }
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
    /// Told `(attempted, cap)` around each download.
    on_download: Option<&'a (dyn Fn(usize, usize) + Sync)>,
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
            on_download: None,
        }
    }

    /// Reports `(attempted, cap)` before and after each DAT download, so
    /// a slow download shows as its own phase.
    pub fn with_download_progress(mut self, f: &'a (dyn Fn(usize, usize) + Sync)) -> Self {
        self.on_download = Some(f);
        self
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
                                if let Some(f) = self.on_download {
                                    f(state.downloads, self.cap);
                                }
                                state.downloads += 1;
                                let attempted = state.downloads;
                                let outcome = store.ensure(&dat);
                                if let Some(f) = self.on_download {
                                    f(attempted, self.cap);
                                }
                                match outcome {
                                    Ok(DatStatus::Ready | DatStatus::Stale) => true,
                                    // Offline, timed out or rate limited: the
                                    // next download would fail the same way,
                                    // so the rest use cached DATs only.
                                    Err(DatError::Net(e)) if e.pauses() => {
                                        state.downloads = self.cap;
                                        false
                                    }
                                    _ => false,
                                }
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
            // Full DATs (MAME) name the game by the set: that wins, since a
            // clone may ship a parent's chip ROM whose stem is the set name
            // (`mspacman` has `pacman.6e`). libretro DATs list a set as one
            // ROM named `<set>.zip` under the game's description; only an
            // archive-named row stands for a whole set.
            let by_archive = self
                .cache
                .dat_by_rom_stem(&dat, set)
                .unwrap_or_default()
                .into_iter()
                .find(|r| is_set_archive(&r.rom_name))
                .map(|r| r.game);
            let candidates = [set.to_string(), set.to_lowercase()]
                .into_iter()
                .chain(by_archive);
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

/// Whether a DAT ROM name is a whole arcade set packed as an archive.
fn is_set_archive(rom_name: &str) -> bool {
    rom_name
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("zip") || ext.eq_ignore_ascii_case("7z"))
}

// --- The identification ladder per file (organise spec §6) ---

/// What is known about one candidate before any DAT is consulted:
/// memoised in `organize_ids` (O-D6).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Facts {
    pub inspection: Inspection,
    /// CRC32 (upper-case hex) and size of a hashed raw file: the whole
    /// file first, then the file without its copier/emulator header when
    /// one was found.
    pub raw_crcs: Vec<(String, u64)>,
    /// Hashing was attempted (`raw_crcs` may stay empty: too large).
    pub hashed: bool,
}

/// What the ladder decided for one candidate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Identification {
    pub combined: Combined,
    /// Decided before combining (not a ROM, damaged, several games…).
    pub left: Option<LeftReason>,
    /// DAT game name of the deciding hit.
    pub catalogue_name: Option<String>,
    /// CRC32 + size for duplicates and remembered choices.
    pub content_key: Option<(String, u64)>,
    /// ROM entries in a zip (multi-ROM note); 1 for a raw file.
    pub rom_count: usize,
    pub notes: Vec<ItemNote>,
    /// The disc of a disc archive: serials and fingerprint for the item
    /// (archives spec §6.2 step 5).
    #[serde(default)]
    pub disc: Option<DiscId>,
}

/// An online source asked by CRC when the offline levels leave a file
/// below Auto (implemented in Task 24, only when configured).
pub trait OnlineLookup: Sync {
    /// `(system, game)` matches among `systems`; nothing is asked when
    /// `systems` is empty (nothing bounds the question).
    fn lookup(
        &self,
        systems: &[String],
        file_name: &str,
        crc32: &str,
        size: u64,
    ) -> Vec<(String, String)>;
}

/// Everything the ladder consults besides the file itself.
pub struct LadderEnv<'a> {
    pub db: &'a SystemDb,
    pub catalogue: &'a dyn Catalogue,
    pub cache: &'a Cache,
    pub thresholds: Thresholds,
    pub broad_root: bool,
    /// Hash raw files even when they are Auto (catalogue names, a
    /// duplicate bucket).
    pub need_hash: bool,
    /// Arcade systems in the layout's preference order.
    pub arcade_order: &'a [String],
    pub online: Option<&'a dyn OnlineLookup>,
}

/// Weight of name tags and scene-group hints (spec §6.5).
const HINT: f32 = 0.60;
/// Weight of a CRC hit in a DAT of a candidate system.
const HASH_CANDIDATE: f32 = 1.00;
/// Weight of a CRC hit in a DAT outside the candidates.
const HASH_ELSEWHERE: f32 = 0.97;
/// Weight of a later arcade system confirming the same set.
const ARCADE_ALTERNATIVE: f32 = 0.99;
/// Weight of an arcade set found by name only.
const ARCADE_NAME: f32 = 0.70;
/// Weight of an online match.
const ONLINE: f32 = 0.80;

/// GBA release groups whose names appear as a `(Group)` tag.
const SCENE_GROUPS: [&str; 12] = [
    "Venom",
    "Paracox",
    "BatMan",
    "Independent",
    "Rising Sun",
    "Eurasia",
    "Trashman",
    "Mode7",
    "Megaroms",
    "Suxxors",
    "Menace",
    "LightForce",
];

/// `gba` when `name` carries a GBA scene group tag (`(Venom)`, …).
pub fn scene_group_hint(name: &str) -> Option<&'static str> {
    scene_group(name).map(|_| "gba")
}

fn scene_group(name: &str) -> Option<&'static str> {
    paren_groups(name).find_map(|g| {
        SCENE_GROUPS
            .iter()
            .find(|s| g.eq_ignore_ascii_case(s))
            .copied()
    })
}

/// Trimmed contents of every `( … )` group of `name`.
fn paren_groups(name: &str) -> impl Iterator<Item = &str> {
    name.split('(')
        .skip(1)
        .filter_map(|rest| rest.split_once(')').map(|(g, _)| g.trim()))
}

/// Name signals of one stem: tags and scene groups (hints, never Auto).
fn name_hints(name: &str, evidence: &mut Vec<Evidence>) {
    for group in paren_groups(name) {
        if group.eq_ignore_ascii_case("SGB Enhanced") {
            evidence.push(Evidence::new(IdLevel::Name, "gb", HINT, group));
            evidence.push(Evidence::new(IdLevel::Name, "gbc", HINT, group));
        } else if group.eq_ignore_ascii_case("GB Compatible") {
            evidence.push(Evidence::new(IdLevel::Name, "gbc", HINT, group));
        }
    }
    if let Some(group) = scene_group(name) {
        evidence.push(Evidence::new(IdLevel::Name, "gba", HINT, group));
    }
}

/// File name without folders or extension.
fn stem_of(name: &str) -> &str {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    Path::new(base)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(base)
}

/// Why a candidate stays put whatever its evidence.
fn left_before_combining(insp: &Inspection) -> Option<LeftReason> {
    if let Some(issue) = &insp.issue {
        return Some(crate::organize::plan::left_reason_for(issue));
    }
    if let Some(name) = insp
        .payloads
        .iter()
        .find_map(|p| p.sniff.unsupported.clone())
    {
        return Some(LeftReason::UnsupportedSystem { name });
    }
    if let Some(FoundSystem::Unsupported { name }) = insp
        .disc_set
        .as_ref()
        .and_then(|s| s.identity.as_ref())
        .map(|f| &f.system)
    {
        return Some(LeftReason::UnsupportedSystem { name: name.clone() });
    }
    if insp.container == Container::Raw {
        if let Some(what) = insp.payloads.first().and_then(|p| p.sniff.not_a_rom) {
            return Some(LeftReason::NotARom { what });
        }
    }
    None
}

/// Systems with any evidence so far that no veto excluded, sorted.
fn candidates(evidence: &[Evidence], excluded: &[String]) -> Vec<String> {
    let set: BTreeSet<&str> = evidence
        .iter()
        .map(|e| e.system.as_str())
        .filter(|s| !excluded.iter().any(|x| x == s))
        .collect();
    set.into_iter().map(str::to_string).collect()
}

/// A remembered user choice for this content (spec §6.6).
fn remembered(env: &LadderEnv, key: Option<&(String, u64)>, evidence: &mut Vec<Evidence>) {
    if evidence.iter().any(|e| e.kind == IdLevel::User) {
        return;
    }
    let Some((crc, size)) = key else { return };
    if let Ok(Some(system)) = env.cache.organize_choice(crc, *size) {
        evidence.push(Evidence::new(IdLevel::User, &system, 1.0, "remembered"));
    }
}

/// Level 1: CRC hits in the candidates' DATs (1.00), else in any cached
/// DAT (0.97). Several systems hitting are narrowed to those the header
/// names, when it names any (spec §6.2).
fn dat_level(
    env: &LadderEnv,
    crcs: &[(String, u64)],
    evidence: &mut Vec<Evidence>,
    excluded: &[String],
    asked: &mut BTreeSet<String>,
) -> Vec<CatalogueHit> {
    if crcs.is_empty() {
        return Vec::new();
    }
    let systems = candidates(evidence, excluded);
    let mut found: Vec<(String, CatalogueHit)> = Vec::new();
    if !systems.is_empty() {
        asked.extend(systems.iter().cloned());
        for (crc, size) in crcs {
            found.extend(
                env.catalogue
                    .hits(&systems, crc, *size)
                    .into_iter()
                    .map(|h| (crc.clone(), h)),
            );
        }
    }
    let mut weight = HASH_CANDIDATE;
    if found.is_empty() {
        weight = HASH_ELSEWHERE;
        for (crc, size) in crcs {
            found.extend(
                env.catalogue
                    .any_hits(crc, *size)
                    .into_iter()
                    .map(|h| (crc.clone(), h)),
            );
        }
    }
    let header: BTreeSet<&str> = evidence
        .iter()
        .filter(|e| e.kind == IdLevel::Header)
        .map(|e| e.system.as_str())
        .collect();
    let hit_systems: BTreeSet<&str> = found.iter().map(|(_, h)| h.system.as_str()).collect();
    if hit_systems.len() > 1 && hit_systems.iter().any(|s| header.contains(s)) {
        found.retain(|(_, h)| header.contains(h.system.as_str()));
    }
    for (crc, hit) in &found {
        let known = evidence
            .iter()
            .any(|e| e.kind == IdLevel::Hash && e.system == hit.system);
        if !known {
            evidence.push(Evidence::new(
                IdLevel::Hash,
                &hit.system,
                weight,
                format!("{crc} {}", hit.game),
            ));
        }
    }
    found.into_iter().map(|(_, h)| h).collect()
}

/// Fuzzy title match against the DAT stems of `systems`: ≥ 95 → 0.80,
/// ≥ 85 → 0.70 (spec §6.5).
fn fuzzy_level(
    env: &LadderEnv,
    systems: &[String],
    keys: &[String],
    evidence: &mut Vec<Evidence>,
    asked: &mut BTreeSet<String>,
) {
    if keys.is_empty() {
        return;
    }
    for system in systems {
        asked.insert(system.clone());
        let mut best: Option<(u8, String)> = None;
        for (stem, game) in env.catalogue.stems(system) {
            let stem_key = parse_name(&stem, false).key;
            for key in keys {
                // score ≤ 100·(l − |Δlen|)/l: skip pairs that cannot reach 85.
                let (a, b) = (key.chars().count(), stem_key.chars().count());
                let l = a.max(b);
                if l > 0 && 100 * (l - a.min(b)) > 15 * l {
                    continue;
                }
                let sc = score(key, &stem_key);
                if best.as_ref().is_none_or(|(s, _)| sc > *s) {
                    best = Some((sc, game.clone()));
                }
            }
        }
        if let Some((sc, game)) = best {
            let weight = match sc {
                95.. => 0.80,
                85.. => 0.70,
                _ => continue,
            };
            evidence.push(Evidence::new(IdLevel::Name, system, weight, game));
        }
    }
}

/// Evidence of a disc archive (archives spec §6.2 step 4): the primary
/// track's header verdict at its spec §7 confidence, and the extensions
/// of the set's sheets and images (one system: 0.85, several: 0.50;
/// generic ones none).
fn disc_evidence(db: &SystemDb, set: &DiscSetInfo, evidence: &mut Vec<Evidence>) {
    let mut header: Vec<Evidence> = Vec::new();
    if let Some(found) = &set.identity {
        if let FoundSystem::Known { id } = &found.system {
            header.push(Evidence::new(
                IdLevel::Header,
                id,
                found.confidence,
                found.detail.clone(),
            ));
        }
    }
    evidence.extend(header.iter().cloned());
    let mut exts: BTreeSet<String> = BTreeSet::new();
    for name in &set.members {
        let ext = crate::scan::classify::extension(Path::new(name));
        // Tracks and playlists say nothing of the system.
        if !matches!(ext.as_str(), "bin" | "raw" | "m3u" | "sub" | "") {
            exts.insert(ext);
        }
    }
    for ext in exts {
        // Generic image extensions (`iso`, `chd`, `img`) name no system.
        let weight = match db.extension_class(&ext) {
            ExtClass::Unique => 0.85,
            ExtClass::Shared => 0.50,
            _ => continue,
        };
        for system in db.systems_for_extension(&ext) {
            // D24: the disc verdict of a weak owner contradicts it.
            let (weight, detail) = if weak_owner_proven(db, &ext, &system.id, &header) {
                (0.60, format!("{ext}!"))
            } else {
                (weight, ext.clone())
            };
            evidence.push(Evidence::new(
                IdLevel::Extension,
                &system.id,
                weight,
                detail,
            ));
        }
    }
}

/// Identifies one candidate (organise spec §6): pre-checks from the
/// inspection, a remembered choice, extension and header per payload,
/// name hints, DAT hits by CRC, raw hashing only when it can change the
/// outcome, fuzzy names, arcade sets, the optional online lookup, then
/// [`combine`]. `facts` gains the raw CRCs when the file is hashed.
pub fn identify_candidate(
    path: &Path,
    outer_name: &str,
    size: u64,
    facts: &mut Facts,
    cloud_only: bool,
    env: &LadderEnv,
) -> Identification {
    let insp = facts.inspection.clone();
    // Memoised digests of a raw file that changed size are stale.
    if insp.container == Container::Raw && facts.raw_crcs.first().is_some_and(|(_, s)| *s != size) {
        facts.raw_crcs.clear();
        facts.hashed = false;
    }
    let mut id = Identification {
        combined: Combined::unknown(),
        left: None,
        catalogue_name: None,
        content_key: None,
        rom_count: insp.payloads.len(),
        notes: Vec::new(),
        disc: None,
    };
    if insp.notes.iter().any(|n| n == NOTE_ENCRYPTED) {
        id.notes.push(ItemNote::Encrypted);
    }
    if let Some((ext, format)) = &insp.mismatch {
        id.notes.push(ItemNote::ContainerMismatch {
            ext: ext.clone(),
            actual: format.short_name(),
        });
    }
    // 1. Pre-checks.
    if let Some(left) = left_before_combining(&insp) {
        id.left = Some(left);
        return id;
    }
    let content_read = !cloud_only
        && insp.payloads.iter().any(|p| p.sniff.read > 0)
        && (insp.container == Container::Raw || insp.container.is_archive());
    let decide = |evidence: &[Evidence], excluded: &[String]| {
        combine(&CombineInput {
            evidence,
            excluded,
            content_read,
            broad_root: env.broad_root,
            thresholds: env.thresholds,
        })
    };
    let mut evidence: Vec<Evidence> = Vec::new();
    let mut excluded: Vec<String> = Vec::new();
    match insp.container {
        c if c.is_archive() && insp.payloads.len() == 1 => {
            let p = &insp.payloads[0];
            id.content_key = p.crc32.clone().map(|crc| (crc, p.size));
        }
        Container::Raw if facts.hashed => id.content_key = facts.raw_crcs.first().cloned(),
        _ => {}
    }
    // 2. Remembered choice.
    remembered(env, id.content_key.as_ref(), &mut evidence);
    // 3. Extension and header per payload; several payloads must agree.
    let mut payload_systems: BTreeSet<String> = BTreeSet::new();
    for p in &insp.payloads {
        let mut own = extension_evidence(env.db, &p.ext, &p.sniff);
        own.extend(p.sniff.hits.iter().cloned());
        if insp.payloads.len() > 1 {
            let alone = combine(&CombineInput {
                evidence: &own,
                excluded: &p.sniff.excluded,
                content_read: !cloud_only && p.sniff.read > 0,
                broad_root: false,
                thresholds: env.thresholds,
            });
            payload_systems.extend(alone.system);
        }
        evidence.extend(own);
        excluded.extend(p.sniff.excluded.iter().cloned());
    }
    if let Some(set) = &insp.disc_set {
        disc_evidence(env.db, set, &mut evidence);
        id.disc = set.identity.as_ref().map(|f| f.id.clone());
        if set.sheet_check == SheetCheck::Deferred && set.extractable {
            id.notes.push(ItemNote::SheetNotReadYet);
        }
        if !set.twins.is_empty() {
            id.notes.push(ItemNote::TwinSheets {
                kept: set.twins.clone(),
            });
        }
    }
    excluded.sort();
    excluded.dedup();
    if payload_systems.len() > 1 {
        id.left = Some(LeftReason::MultiGameArchive {
            count: insp.payloads.len(),
        });
        return id;
    }
    if insp.payloads.len() > 1 {
        id.notes.push(ItemNote::MultiRomZip {
            count: insp.payloads.len(),
        });
    }
    // 4. Name signals on the outer and inner stems.
    let mut stems: Vec<&str> = vec![strip_numbering_prefix(stem_of(outer_name))];
    if insp.container.is_archive() {
        stems.extend(
            insp.payloads
                .iter()
                .map(|p| strip_numbering_prefix(stem_of(&p.name))),
        );
    }
    stems.dedup();
    for stem in &stems {
        name_hints(stem, &mut evidence);
    }
    // 5. DAT by CRC.
    let mut crcs: Vec<(String, u64)> = match insp.container {
        c if c.is_archive() => insp
            .payloads
            .iter()
            .filter_map(|p| p.crc32.clone().map(|c| (c, p.size)))
            .collect(),
        Container::Raw if facts.hashed => facts.raw_crcs.clone(),
        _ => Vec::new(),
    };
    // Systems whose catalogue this file asked for: only those can carry a
    // "catalogue missing" note, so the note never depends on which other
    // files of the pile were identified first.
    let mut asked: BTreeSet<String> = BTreeSet::new();
    let mut hits = dat_level(env, &crcs, &mut evidence, &excluded, &mut asked);
    // 6. Raw hashing, only when it can change the outcome.
    if insp.container == Container::Raw
        && !cloud_only
        && !facts.hashed
        && (env.need_hash || decide(&evidence, &excluded).verdict != Verdict::Auto)
    {
        let rule = decide(&evidence, &excluded)
            .system
            .and_then(|s| env.db.get(&s))
            .map_or(HeaderRule::Plain, |s| s.header);
        let ext = insp
            .payloads
            .first()
            .map_or_else(|| crate::scan::classify::extension(path), |p| p.ext.clone());
        #[cfg(any(test, feature = "test-util"))]
        crate::organize::plan::HASH_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match hash_rom(path, &ext, rule, DEFAULT_HASH_MAX_BYTES) {
            Ok(HashOutcome::Content(h)) => {
                facts.raw_crcs = std::iter::once(h.full)
                    .chain(h.headerless)
                    .map(|d| (d.crc32, d.size))
                    .collect();
                facts.hashed = true;
            }
            Ok(_) => facts.hashed = true,
            Err(_) => {}
        }
        if !facts.raw_crcs.is_empty() {
            id.content_key = facts.raw_crcs.first().cloned();
            remembered(env, id.content_key.as_ref(), &mut evidence);
            crcs = facts.raw_crcs.clone();
            hits.extend(dat_level(env, &crcs, &mut evidence, &excluded, &mut asked));
        }
    }
    // 7. Fuzzy name against DAT stems.
    if decide(&evidence, &excluded).verdict != Verdict::Auto {
        let mut systems = candidates(&evidence, &excluded);
        if systems.is_empty() {
            systems = env.catalogue.stem_systems();
        }
        let mut keys: Vec<String> = stems
            .iter()
            .map(|s| parse_name(s, false).key)
            .filter(|k| !k.is_empty())
            .collect();
        keys.sort();
        keys.dedup();
        fuzzy_level(env, &systems, &keys, &mut evidence, &mut asked);
    }
    // 8. Arcade sets: a zip with entries but no console ROM.
    let mut arcade_games: BTreeMap<String, String> = BTreeMap::new();
    let mut arcade_alternatives: Vec<(String, f32)> = Vec::new();
    if insp.container == Container::Zip
        && insp.disc_set.is_none()
        && insp.payloads.is_empty()
        && !insp.entries.is_empty()
    {
        let set = stem_of(outer_name);
        let inside: HashSet<String> = insp
            .entries
            .iter()
            .filter_map(|e| e.crc32.as_deref())
            .map(str::to_ascii_uppercase)
            .collect();
        let mut winner = false;
        for system in env.arcade_order {
            asked.insert(system.clone());
            let Some((game, roms)) = env.catalogue.set_roms(system, set) else {
                continue;
            };
            let matched = roms
                .iter()
                .filter(|(crc, _)| inside.contains(&crc.to_ascii_uppercase()))
                .count();
            let confirmed = !roms.is_empty() && matched * 4 >= roms.len() * 3;
            if confirmed && winner {
                arcade_alternatives.push((system.clone(), ARCADE_ALTERNATIVE));
                continue;
            }
            let (level, weight) = if confirmed {
                winner = true;
                (IdLevel::Hash, HASH_CANDIDATE)
            } else {
                (IdLevel::Name, ARCADE_NAME)
            };
            evidence.push(Evidence::new(level, system, weight, game.clone()));
            arcade_games.insert(system.clone(), game);
        }
    }
    // 9. Online, when configured and a CRC is known.
    if let Some(online) = env.online {
        let known = id.content_key.clone().or_else(|| crcs.first().cloned());
        if let Some((crc, crc_size)) = known {
            if decide(&evidence, &excluded).verdict != Verdict::Auto {
                let file_name = match insp.container {
                    c if c.is_archive() => insp
                        .payloads
                        .first()
                        .map_or(outer_name, |p| p.name.as_str()),
                    _ => outer_name,
                };
                let systems = candidates(&evidence, &excluded);
                let mut seen = BTreeSet::new();
                for (system, game) in online.lookup(&systems, file_name, &crc, crc_size) {
                    if env.db.get(&system).is_some() && seen.insert(system.clone()) {
                        evidence.push(Evidence::new(IdLevel::Online, &system, ONLINE, game));
                    }
                }
            }
        }
    }
    // 10. Combine. A weak extension supports a system that a header, a
    // DAT hit or a user choice already names.
    let mut weak_exts: Vec<&str> = insp.payloads.iter().map(|p| p.ext.as_str()).collect();
    weak_exts.sort_unstable();
    weak_exts.dedup();
    for ext in weak_exts {
        weak_extension_support(env.db, ext, &mut evidence);
    }
    let mut seen = HashSet::new();
    evidence.retain(|e| seen.insert((e.kind, e.system.clone(), e.detail.clone())));
    let mut combined = decide(&evidence, &excluded);
    // A cloud-only placeholder is never moved without asking (spec §7.1),
    // even when facts memoised while it was local name its system.
    if cloud_only && combined.verdict == Verdict::Auto && combined.method != Some(IdLevel::User) {
        combined.verdict = Verdict::Review;
    }
    // An archive not fully read within the plan budget is asked about,
    // never moved on its own (archives spec §4.2).
    if insp.budget_spent {
        if combined.verdict == Verdict::Auto && combined.method != Some(IdLevel::User) {
            combined.verdict = Verdict::Review;
        }
        id.notes.push(ItemNote::ArchiveNotFullyRead);
    }
    // A disc set guessed from a directory (its sheet unread and off the
    // Redump pattern, or no sheet) is asked about, never extracted alone.
    if insp.disc_set.as_ref().is_some_and(|s| !s.extractable)
        && combined.verdict == Verdict::Auto
        && combined.method != Some(IdLevel::User)
    {
        combined.verdict = Verdict::Review;
    }
    if let Some(system) = combined.system.clone() {
        id.catalogue_name = hits
            .iter()
            .find(|h| h.system == system)
            .map(|h| h.game.clone())
            .or_else(|| arcade_games.get(&system).cloned());
        if asked.contains(&system) && env.catalogue.unconsulted().contains(&system) {
            id.notes.push(ItemNote::CatalogueMissing { system });
        }
    }
    for (system, weight) in arcade_alternatives {
        if combined.system.as_ref() != Some(&system)
            && !combined.alternatives.iter().any(|(s, _)| *s == system)
        {
            combined.alternatives.push((system, weight));
        }
    }
    combined
        .alternatives
        .sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    // 11. Notes.
    for e in &evidence {
        if e.kind == IdLevel::Extension {
            if let Some(ext) = e.detail.strip_suffix('!') {
                let note = ItemNote::ExtensionContradicted {
                    ext: ext.to_string(),
                };
                if !id.notes.contains(&note) {
                    id.notes.push(note);
                }
            }
        }
    }
    id.combined = combined;
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disc set of `members` whose primary track names `system`.
    fn disc_set(members: &[&str], system: &str, confidence: f32) -> DiscSetInfo {
        serde_json::from_value(serde_json::json!({
            "sheets": [],
            "members": members,
            "primary": members.first(),
            "sheet_check": "read",
            "identity": {
                "system": { "kind": "known", "id": system },
                "id": { "serials": [], "disc_no": null, "volume": null, "source": "ip-bin" },
                "confidence": confidence,
                "detail": "ZZ CD",
            },
            "multi_disc": false,
            "twins": [],
            "extractable": true,
        }))
        .unwrap()
    }

    #[test]
    fn a_disc_header_of_a_weak_owner_contradicts_the_strong_extension() {
        // D24 «sniff or disc verdict»: a zipped `.cdi` whose primary track
        // names a system listing `cdi` as weak drops dreamcast to 0.60.
        let db = SystemDb::builtin()
            .with_overrides("[[system]]\nid = \"zz\"\nname = \"ZZ\"\nweak_extensions = [\"cdi\"]\n")
            .unwrap();
        let mut evidence = Vec::new();
        disc_evidence(&db, &disc_set(&["Game.cdi"], "zz", 0.95), &mut evidence);
        let dc = evidence.iter().find(|e| e.system == "dreamcast").unwrap();
        assert_eq!((dc.weight, dc.detail.as_str()), (0.60, "cdi!"));

        // A weaker verdict leaves the strong owner alone.
        let mut evidence = Vec::new();
        disc_evidence(&db, &disc_set(&["Game.cdi"], "zz", 0.90), &mut evidence);
        let dc = evidence.iter().find(|e| e.system == "dreamcast").unwrap();
        assert_eq!((dc.weight, dc.detail.as_str()), (0.85, "cdi"));
    }
}
