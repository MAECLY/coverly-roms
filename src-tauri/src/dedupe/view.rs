//! Wire types of the duplicates screen (duplicates spec §11, §12): snake_case
//! fields, kebab-case enums, mirrored in `src/ipc/dedupeTypes.ts`.

use coverly_core::dedupe::{DedupePhase, DedupePlan, DedupeReport, QuarantineChoice};
use coverly_core::settings::{DedupeLevel, RuleToggle};
use serde::{Deserialize, Serialize, Serializer};

use crate::error::IpcError;

pub use crate::organize::view::OrganizeJobState as DedupeJobState;

/// Event carrying a [`DedupeProgressEvent`] while a plan is made or applied.
pub const DEDUPE_PROGRESS_EVENT: &str = "dedupe-progress";
/// Event carrying the [`DedupeJobView`] when an apply or resume ends.
pub const DEDUPE_CHANGED_EVENT: &str = "dedupe-changed";

/// What to look for; `None` fields take `settings.dedupe`. A regroup reads
/// only `levels`, `one_game_one_rom`, `rules` and `quarantine` (the rest
/// would need the library read again).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct DedupeRequest {
    /// Profile id of the library's layout (`None` = the open library's, the
    /// default profile, else detected).
    pub profile: Option<String>,
    pub levels: Option<Vec<DedupeLevel>>,
    pub one_game_one_rom: Option<bool>,
    /// Keeper rules in the order they are weighed.
    pub rules: Option<Vec<RuleToggle>>,
    /// System ids (`None` = every system).
    pub systems: Option<Vec<String>>,
    /// Cached catalogues only: no download.
    pub offline: Option<bool>,
    pub quarantine: Option<QuarantineChoice>,
    /// Copies that must stay, whatever the rules say (absolute paths).
    pub forced_keepers: Vec<String>,
    /// Forget the keepers chosen in earlier runs.
    pub reset_choices: bool,
}

/// A duplicates plan kept by the backend under `plan_id`: the webview sends
/// the id and its picks back, never the plan. The plan's fields sit next to
/// `plan_id` on the wire.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DedupePlanView {
    pub plan_id: u64,
    #[serde(flatten)]
    pub plan: DedupePlan,
    /// The library's volume has a Trash: false on a Windows removable
    /// drive or a FAT volume without `.Trashes`, and the screen disables
    /// the Trash option with that reason.
    pub trash_available: bool,
}

/// The step a [`DedupeProgressEvent`] reports: a planning phase, or
/// `apply` while copies are set aside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DedupeEventPhase {
    Plan(DedupePhase),
    Apply,
}

impl Serialize for DedupeEventPhase {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            DedupeEventPhase::Plan(p) => p.serialize(s),
            DedupeEventPhase::Apply => s.serialize_str("apply"),
        }
    }
}

/// Planning or apply progress of plan `plan_id` (at most every 100 ms;
/// phase changes and ends always).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct DedupeProgressEvent {
    pub plan_id: u64,
    pub phase: DedupeEventPhase,
    pub done: usize,
    pub total: usize,
}

/// The duplicates apply of the app (at most one at a time, and never while
/// a fill or an organise job writes).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DedupeJobView {
    /// The journal, once known (always for a resume).
    pub job: Option<String>,
    /// The plan applied; a resume gets a new id for its progress events.
    pub plan_id: u64,
    pub state: DedupeJobState,
    /// The last progress, while applying.
    pub progress: Option<DedupeProgressEvent>,
    /// What was done, once it ended (`done`, `paused`, `cancelled`).
    pub report: Option<DedupeReport>,
    /// The library the copies came from.
    pub root: String,
    /// Where they went: the quarantine folder (also set for the Trash, as
    /// the plan named it).
    pub quarantine: String,
    /// The copies went to the OS Trash instead of the quarantine.
    pub trash: bool,
    /// Why it failed (`failed` only), as an IPC error (`no-space`, …).
    pub error: Option<IpcError>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn requests_read_partial_json() {
        let req: DedupeRequest = serde_json::from_value(json!({
            "levels": ["exact"],
            "quarantine": { "kind": "trash" },
            "offline": true
        }))
        .unwrap();
        assert_eq!(req.levels, Some(vec![DedupeLevel::Exact]));
        assert_eq!(req.quarantine, Some(QuarantineChoice::Trash));
        assert_eq!(req.offline, Some(true));
        assert_eq!(req.profile, None);
        assert!(!req.reset_choices);
    }

    #[test]
    fn progress_phases_are_kebab_case_with_apply() {
        let plan = DedupeProgressEvent {
            plan_id: 2,
            phase: DedupeEventPhase::Plan(DedupePhase::Fingerprints),
            done: 3,
            total: 9,
        };
        assert_eq!(
            serde_json::to_value(plan).unwrap(),
            json!({ "plan_id": 2, "phase": "fingerprints", "done": 3, "total": 9 })
        );
        let apply = DedupeProgressEvent {
            phase: DedupeEventPhase::Apply,
            ..plan
        };
        assert_eq!(serde_json::to_value(apply).unwrap()["phase"], "apply");
    }
}
