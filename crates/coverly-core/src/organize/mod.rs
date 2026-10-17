//! Organising a pile of loose ROMs into one folder per system
//! (organise spec): inspection, identification, planning and apply.

pub mod apply;
pub mod evidence;
pub mod inspect;
pub mod online;
pub mod plan;
pub mod target;
pub mod walk;

pub use apply::{
    apply_organize, free_space, resume_organize, ApplyEnv, ApplyError, ItemOutcome, ItemStatus,
    OrganizeReport, SkipReason,
};
pub use plan::{
    plan_from_survey, plan_organize, probe_folder, survey, Choice, Companion, Counts, Decision,
    FolderProbe, ItemKind, ItemNote, LeftReason, NameMode, OrganizeAction, OrganizeEnv,
    OrganizeError, OrganizeItem, OrganizeOptions, OrganizePhase, OrganizePlan, OrganizeProgress,
    PlanWarning, RenameReason, SourceFp, Survey, SurveyEntry, TransferMode, DEFAULT_LAYOUT,
    PROBE_LIMIT,
};
pub use target::{
    assign_targets, file_system_is_fat32, folder_dir, mark_duplicates, DirIndex, TargetLayout,
};
pub use walk::{is_broad_root, MetaProbe, OsMetaProbe, WalkCandidate, WalkOptions, Walked};
