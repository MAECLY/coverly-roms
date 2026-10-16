//! The plan (spec §7 step 6): one item per ROM and media, decided before
//! anything is written. `auto` when the chosen candidate's confidence
//! reaches the auto threshold, `review` between the thresholds, `none`
//! otherwise. Shown as the dry run; applied by `jobs::apply_plan`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::art::target_path;
use crate::gather::Gathered;
use crate::identify::IdMethod;
use crate::matching::{default_region_order, strategy_for, RankContext, RegionPref, StrategyKind};
use crate::profiles::{Media, Profile};
use crate::sources::{Candidate, MatchKind};
use crate::systems::SystemDb;

/// Plan settings (spec §9 defaults).
#[derive(Clone, Debug)]
pub struct PlanOptions {
    pub strategy: StrategyKind,
    /// Region order for the region-aware strategies.
    pub regions: Vec<RegionPref>,
    /// Source ids in priority order.
    pub sources: Vec<String>,
    /// Confidence at or above which an item is `auto`.
    pub auto_threshold: f32,
    /// Confidence at or above which an item is `review` (below: `none`).
    pub review_threshold: f32,
    /// How many runner-up candidates each item keeps.
    pub max_alternatives: usize,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            strategy: StrategyKind::RomRegion,
            regions: default_region_order(),
            sources: vec!["libretro".to_string()],
            auto_threshold: 0.90,
            review_threshold: 0.65,
            max_alternatives: 4,
        }
    }
}

/// Why an item gets no art.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum NoneReason {
    /// No source offered a candidate.
    NoCandidates,
    /// The best candidate is below the review threshold.
    BelowThreshold { best: f32 },
    /// A source failed and nothing else was found.
    SourceError { message: String },
    /// The profile cannot store this media for this ROM.
    NoTarget,
}

/// What the plan does for one ROM and media.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum PlanAction {
    /// Written without asking.
    Auto {
        chosen: Candidate,
        confidence: f32,
        alternatives: Vec<Candidate>,
    },
    /// Written only after a review (or by `--apply` without `--auto-only`).
    Review {
        chosen: Candidate,
        confidence: f32,
        alternatives: Vec<Candidate>,
    },
    /// Nothing is written.
    None {
        #[serde(flatten)]
        reason: NoneReason,
    },
}

/// One ROM and media of the plan.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanItem {
    pub system_id: String,
    /// The system folder name as on disk.
    pub folder: String,
    /// The ROM path relative to the system folder.
    pub rel_path: PathBuf,
    pub stem: String,
    pub media: Media,
    /// Where the art goes; empty for [`NoneReason::NoTarget`].
    pub target: PathBuf,
    pub identity_method: IdMethod,
    pub canonical: Option<String>,
    /// Whether a file already exists at `target` (exact spelling).
    pub replaces_existing: bool,
    #[serde(flatten)]
    pub action: PlanAction,
}

/// Item counts per action.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub auto: usize,
    pub review: usize,
    pub none: usize,
}

impl Counts {
    fn add(&mut self, action: &PlanAction) {
        match action {
            PlanAction::Auto { .. } => self.auto += 1,
            PlanAction::Review { .. } => self.review += 1,
            PlanAction::None { .. } => self.none += 1,
        }
    }
}

/// A fill plan: the dry run, and the input of `jobs::apply_plan`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Plan {
    pub root: PathBuf,
    pub profile_id: String,
    /// RFC 3339 UTC.
    pub created_at: String,
    pub strategy: StrategyKind,
    pub items: Vec<PlanItem>,
    pub totals: Counts,
    /// Counts per system folder.
    pub per_system: BTreeMap<String, Counts>,
    pub warnings: Vec<String>,
    /// Why gathering paused, when it did (the plan is partial).
    pub paused: Option<String>,
}

/// How sure the plan is of a candidate: 1.0 for a canonical or exact file
/// name match, 0.95 for the same title, and the fuzzy score / 100 (capped
/// at 0.94, so a fuzzy match is never as sure as a title match).
pub fn confidence(c: &Candidate) -> f32 {
    match c.kind {
        MatchKind::Canonical | MatchKind::NameExact => 1.0,
        MatchKind::TitleExact => 0.95,
        MatchKind::Fuzzy => f32::from(c.score.min(94)) / 100.0,
    }
}

/// Builds the plan for `gathered`: ranks each media's candidates with the
/// strategy, picks the best and classifies it by the thresholds. Items of
/// a system missing from `systems` are skipped. Reads the disk only to set
/// [`PlanItem::replaces_existing`].
pub fn build_plan(
    root: &Path,
    profile: &Profile,
    systems: &SystemDb,
    gathered: &Gathered,
    opts: &PlanOptions,
) -> Plan {
    let strategy = strategy_for(opts.strategy);
    let mut items = Vec::new();
    let mut totals = Counts::default();
    let mut per_system = BTreeMap::<String, Counts>::new();
    for g in &gathered.items {
        let t = &g.target;
        let Some(system) = systems.get(&t.system_id) else {
            continue;
        };
        for &media in &t.media {
            let target = target_path(
                root,
                profile,
                &t.system_dir,
                system,
                &t.rel_dir,
                &t.stem,
                media,
            );
            let cands = g.candidates.get(&media).map(Vec::as_slice).unwrap_or(&[]);
            let action = match &target {
                None => PlanAction::None {
                    reason: NoneReason::NoTarget,
                },
                Some(_) if cands.is_empty() => match g.errors.get(&media) {
                    Some(message) => PlanAction::None {
                        reason: NoneReason::SourceError {
                            message: message.clone(),
                        },
                    },
                    None => PlanAction::None {
                        reason: NoneReason::NoCandidates,
                    },
                },
                Some(_) => {
                    let ctx = RankContext {
                        rom_regions: &g.identity.parsed.regions,
                        regions: &opts.regions,
                        sources: &opts.sources,
                    };
                    let order = strategy.rank(&ctx, cands);
                    choose(cands, &order, opts)
                }
            };
            totals.add(&action);
            per_system.entry(t.folder.clone()).or_default().add(&action);
            let rel_path = t
                .path
                .strip_prefix(&t.system_dir)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| t.path.clone());
            items.push(PlanItem {
                system_id: t.system_id.clone(),
                folder: t.folder.clone(),
                rel_path,
                stem: t.stem.clone(),
                media,
                replaces_existing: target.as_deref().is_some_and(Path::exists),
                target: target.unwrap_or_default(),
                identity_method: g.identity.method,
                canonical: g.identity.canonical.clone(),
                action,
            });
        }
    }
    Plan {
        root: root.to_path_buf(),
        profile_id: profile.id.clone(),
        created_at: crate::time::rfc3339_utc(std::time::SystemTime::now()),
        strategy: opts.strategy,
        items,
        totals,
        per_system,
        warnings: gathered.warnings.clone(),
        paused: gathered.paused.clone(),
    }
}

/// Picks the first of `order` (falling back to the first candidate if the
/// strategy returned nothing) and classifies it by the thresholds.
fn choose(cands: &[Candidate], order: &[usize], opts: &PlanOptions) -> PlanAction {
    let mut order: Vec<usize> = order.iter().copied().filter(|&i| i < cands.len()).collect();
    if order.is_empty() {
        order = (0..cands.len()).collect();
    }
    let chosen = cands[order[0]].clone();
    let alternatives: Vec<Candidate> = order[1..]
        .iter()
        .take(opts.max_alternatives)
        .map(|&i| cands[i].clone())
        .collect();
    let c = confidence(&chosen);
    if c >= opts.auto_threshold {
        PlanAction::Auto {
            chosen,
            confidence: c,
            alternatives,
        }
    } else if c >= opts.review_threshold {
        PlanAction::Review {
            chosen,
            confidence: c,
            alternatives,
        }
    } else {
        PlanAction::None {
            reason: NoneReason::BelowThreshold { best: c },
        }
    }
}
