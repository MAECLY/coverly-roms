//! Wire types of fill jobs, drawer actions and exports, mirrored by
//! `src/ipc/types.ts` and pinned by `tests/contract.rs`. Paths never
//! cross: items are addressed by index and entries by id.

use coverly_core::identify::names::Region;
use coverly_core::jobs::{ItemErrorKind, ItemStatus, JobReport};
use coverly_core::matching::StrategyKind;
use coverly_core::plan::{
    confidence, Counts, Decision, NoneReason, Plan, PlanAction, PlanItem, PlanNote,
};
use coverly_core::profiles::Media;
use coverly_core::progress::{PauseKind, Phase};
use coverly_core::sources::{Candidate, MatchKind};
use coverly_core::write::{JobSummary, OverwritePolicy, UndoReport};
use serde::{Deserialize, Serialize};

use crate::library::refresh::EntryArtUpdate;

/// What the wizard asks for.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct FillRequest {
    /// The selection or the current filter; `None` = the whole library.
    pub entry_ids: Option<Vec<u32>>,
    pub media: Vec<Media>,
    pub strategy: StrategyKind,
    /// `rom`, `world`, `usa`, … (spec §9 region order).
    pub regions: Vec<String>,
    /// `never` = only missing art; `backup` = replace existing art too.
    pub overwrite: OverwritePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobState {
    Gathering,
    Planned,
    Applying,
    Paused,
    Cancelled,
    Done,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobProgressView {
    pub phase: Phase,
    pub done: usize,
    pub total: usize,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PauseView {
    pub phase: Phase,
    pub kind: PauseKind,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SystemCountsView {
    pub folder: String,
    pub auto: usize,
    pub review: usize,
    pub none: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanSummaryView {
    pub totals: Counts,
    pub per_system: Vec<SystemCountsView>,
    pub warnings: Vec<String>,
}

impl PlanSummaryView {
    pub fn of(plan: &Plan) -> PlanSummaryView {
        PlanSummaryView {
            totals: plan.totals,
            per_system: plan
                .per_system
                .iter()
                .map(|(folder, c)| SystemCountsView {
                    folder: folder.clone(),
                    auto: c.auto,
                    review: c.review,
                    none: c.none,
                })
                .collect(),
            warnings: plan.warnings.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct JobTotalsView {
    pub filled: usize,
    pub replaced: usize,
    pub skipped: usize,
    pub no_art: usize,
    pub failed: usize,
}

impl JobTotalsView {
    pub fn of(r: &JobReport) -> JobTotalsView {
        JobTotalsView {
            filled: r.filled,
            replaced: r.replaced,
            skipped: r.skipped,
            no_art: r.no_art,
            failed: r.failed,
        }
    }
}

/// The current job, as the webview sees it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct JobView {
    pub id: u64,
    /// The library generation the job belongs to.
    pub generation: u64,
    pub state: JobState,
    pub progress: Option<JobProgressView>,
    pub paused: Option<PauseView>,
    pub plan: Option<PlanSummaryView>,
    /// Review decisions recorded so far.
    pub decided: usize,
    /// The journal id, once something was written.
    pub journal: Option<String>,
    pub result: Option<JobTotalsView>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CandidateView {
    /// 0 = the chosen image, `n` = alternative `n - 1` (see `Decision::Pick`).
    pub index: u32,
    pub source: String,
    pub name: String,
    pub regions: Vec<Region>,
    pub kind: MatchKind,
    pub score: u8,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionKind {
    Auto,
    Review,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoneKind {
    NoCandidates,
    BelowThreshold,
    SourceError,
    NoTarget,
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutcomeKind {
    Filled,
    Replaced,
    SkippedExists,
    SkippedReview,
    SkippedNoMatch,
    NotAttempted,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ItemOutcomeView {
    pub status: OutcomeKind,
    pub error: Option<ItemErrorKind>,
    pub message: Option<String>,
}

impl ItemOutcomeView {
    pub fn of(status: &ItemStatus) -> ItemOutcomeView {
        let simple = |status| ItemOutcomeView {
            status,
            error: None,
            message: None,
        };
        match status {
            ItemStatus::Filled { .. } => simple(OutcomeKind::Filled),
            ItemStatus::Replaced { .. } => simple(OutcomeKind::Replaced),
            ItemStatus::SkippedExists => simple(OutcomeKind::SkippedExists),
            ItemStatus::SkippedReview => simple(OutcomeKind::SkippedReview),
            ItemStatus::SkippedNoMatch => simple(OutcomeKind::SkippedNoMatch),
            ItemStatus::NotAttempted => simple(OutcomeKind::NotAttempted),
            ItemStatus::Failed { kind, message } => ItemOutcomeView {
                status: OutcomeKind::Failed,
                error: Some(*kind),
                message: Some(message.clone()),
            },
        }
    }
}

/// One plan item: one ROM and one media.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct JobItemView {
    /// Index in the plan (what decisions and candidate art name).
    pub index: u32,
    /// The library entry, when it is still in the open session.
    pub entry_id: Option<u32>,
    pub folder: String,
    /// The ROM file name without extension.
    pub name: String,
    pub media: Media,
    pub action: ActionKind,
    pub confidence: Option<f32>,
    pub note: Option<PlanNote>,
    pub none_reason: Option<NoneKind>,
    pub candidates: Vec<CandidateView>,
    pub replaces_existing: bool,
    pub decision: Option<Decision>,
    pub outcome: Option<ItemOutcomeView>,
}

/// The chosen image then the alternatives (empty for `none`).
pub fn candidates_of(item: &PlanItem) -> Vec<&Candidate> {
    match &item.action {
        PlanAction::Auto {
            chosen,
            alternatives,
            ..
        }
        | PlanAction::Review {
            chosen,
            alternatives,
            ..
        } => std::iter::once(chosen).chain(alternatives.iter()).collect(),
        PlanAction::None { .. } => Vec::new(),
    }
}

pub fn candidate_views(item: &PlanItem) -> Vec<CandidateView> {
    candidates_of(item)
        .into_iter()
        .enumerate()
        .map(|(i, c)| CandidateView {
            index: i as u32,
            source: c.source.clone(),
            name: c.name.clone(),
            regions: c.regions.clone(),
            kind: c.kind,
            score: c.score,
            confidence: confidence(c),
        })
        .collect()
}

impl JobItemView {
    pub fn new(
        index: usize,
        item: &PlanItem,
        entry_id: Option<u32>,
        decision: Option<Decision>,
        outcome: Option<&ItemStatus>,
    ) -> JobItemView {
        let (action, confidence, none_reason) = match &item.action {
            PlanAction::Auto { confidence, .. } => (ActionKind::Auto, Some(*confidence), None),
            PlanAction::Review { confidence, .. } => (ActionKind::Review, Some(*confidence), None),
            PlanAction::None { reason } => (
                ActionKind::None,
                None,
                Some(match reason {
                    NoneReason::NoCandidates => NoneKind::NoCandidates,
                    NoneReason::BelowThreshold { .. } => NoneKind::BelowThreshold,
                    NoneReason::SourceError { .. } => NoneKind::SourceError,
                    NoneReason::NoTarget => NoneKind::NoTarget,
                    NoneReason::Skipped => NoneKind::Skipped,
                }),
            ),
        };
        JobItemView {
            index: index as u32,
            entry_id,
            folder: item.folder.clone(),
            name: item.stem.clone(),
            media: item.media,
            action,
            confidence,
            note: item.note,
            none_reason,
            candidates: candidate_views(item),
            replaces_existing: item.replaces_existing,
            decision,
            outcome: outcome.map(ItemOutcomeView::of),
        }
    }
}

/// Which items `fill_job_items` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemFilter {
    All,
    Auto,
    Review,
    Failed,
}

/// One review decision from the webview: `{ "index": 3, "choice": "pick", "candidate": 1 }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct DecisionIn {
    pub index: u32,
    #[serde(flatten)]
    pub decision: Decision,
}

/// Payload of `fill-job-progress`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobProgressEvent {
    pub job: u64,
    pub phase: Phase,
    pub done: usize,
    pub total: usize,
    pub label: String,
}

/// Payload of `library-entries-updated`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EntriesUpdatedEvent {
    pub generation: u64,
    pub entries: Vec<EntryArtUpdate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UndoView {
    pub job: String,
    pub removed: usize,
    pub restored: usize,
    /// Files the user changed after the job: left as they are.
    pub kept: usize,
    pub entries: Vec<EntryArtUpdate>,
}

impl UndoView {
    pub fn new(r: &UndoReport, entries: Vec<EntryArtUpdate>) -> UndoView {
        UndoView {
            job: r.job.clone(),
            removed: r.removed.len(),
            restored: r.restored.len(),
            kept: r.skipped_modified.len(),
            entries,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobHistoryView {
    pub job: String,
    pub root: String,
    pub created_at: String,
    pub entries: usize,
    pub complete: bool,
    pub undone: bool,
}

impl From<JobSummary> for JobHistoryView {
    fn from(s: JobSummary) -> Self {
        JobHistoryView {
            job: s.job,
            root: s.root.display().to_string(),
            created_at: s.created_at,
            entries: s.entries,
            complete: s.complete,
            undone: s.undone,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Csv,
    Json,
}
