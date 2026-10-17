//! Wire types of organising a loose pile (organise spec §4, §11): snake_case
//! fields, kebab-case enums, mirrored in `src/ipc/types.ts`.

use coverly_core::organize::{FolderProbe, NameMode, OrganizePhase, OrganizePlan, TransferMode};
use serde::{Deserialize, Serialize};

/// Event carrying an [`OrganizeProgressEvent`] while a plan is made.
pub const ORGANIZE_PROGRESS_EVENT: &str = "organize-progress";

/// What a quick, name-only look at a folder found (start-screen offer).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FolderProbeView {
    pub rom_like: usize,
    pub partial_downloads: usize,
    pub cloud_only: usize,
    /// Names of folders that already resolve to a system.
    pub sorted_folders: Vec<String>,
    pub writable: bool,
    pub broad_root: bool,
    /// Stopped after 5 000 entries: counts are "5 000+".
    pub truncated: bool,
}

impl From<FolderProbe> for FolderProbeView {
    fn from(p: FolderProbe) -> Self {
        FolderProbeView {
            rom_like: p.rom_like,
            partial_downloads: p.partial_downloads,
            cloud_only: p.cloud_only,
            sorted_folders: p.sorted_folders,
            writable: p.writable,
            broad_root: p.broad_root,
            truncated: p.truncated,
        }
    }
}

/// What to plan; `None` fields take `settings.organize`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct OrganizeRequest {
    /// The folder holding the loose ROMs.
    pub source: String,
    /// Profile id of the target layout.
    pub layout: Option<String>,
    /// Where the system folders go (`None` = the source).
    pub target: Option<String>,
    /// System folders straight in the target, without the ROMs folder.
    pub flat: Option<bool>,
    /// `None` = move on the same volume, copy across volumes (or the setting).
    pub mode: Option<TransferMode>,
    pub names: Option<NameMode>,
    /// Cached catalogues only: no download, no online lookup.
    pub offline: Option<bool>,
}

/// A dry run kept by the backend under `plan_id`: the webview only sends
/// the id and its decisions back, never the plan.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OrganizePlanView {
    pub plan_id: u64,
    pub plan: OrganizePlan,
}

/// Planning progress of plan `plan_id` (at most every 100 ms; phase
/// changes and ends always).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct OrganizeProgressEvent {
    pub plan_id: u64,
    pub phase: OrganizePhase,
    pub done: usize,
    pub total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn requests_read_partial_json() {
        let req: OrganizeRequest = serde_json::from_value(json!({
            "source": "/Users/me/ROMs-Downloaded",
            "layout": "anbernic-stock",
            "mode": "copy",
            "names": "catalogue",
            "offline": true
        }))
        .unwrap();
        assert_eq!(req.source, "/Users/me/ROMs-Downloaded");
        assert_eq!(req.layout.as_deref(), Some("anbernic-stock"));
        assert_eq!(req.mode, Some(TransferMode::Copy));
        assert_eq!(req.names, Some(NameMode::Catalogue));
        assert_eq!(req.offline, Some(true));
        assert_eq!(req.target, None);
        assert_eq!(req.flat, None);
    }

    #[test]
    fn progress_events_are_snake_case_with_kebab_phases() {
        let e = OrganizeProgressEvent {
            plan_id: 3,
            phase: OrganizePhase::Identify,
            done: 4,
            total: 17,
        };
        assert_eq!(
            serde_json::to_value(e).unwrap(),
            json!({ "plan_id": 3, "phase": "identify", "done": 4, "total": 17 })
        );
    }
}
