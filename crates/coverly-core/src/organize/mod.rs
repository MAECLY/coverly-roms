//! Organising a pile of loose ROMs into one folder per system
//! (organise spec): inspection, identification, planning and apply.

pub mod apply;
pub mod discsets;
pub mod evidence;
pub mod extract;
pub mod inspect;
pub mod library_bases;
pub mod online;
pub mod plan;
pub mod target;
pub mod walk;

pub use apply::{
    apply_organize, cluster_size, free_space, gate, large_transfer, large_transfer_threshold,
    load_plan, resume_organize, user_available, ApplyEnv, ApplyError, HugeItem, ItemOutcome,
    ItemStatus, LargeConfirm, LargeTransfer, OrganizeReport, SkipReason, LARGE_TRANSFER,
};
pub use plan::{
    count_library_loose_roms, count_loose_roms, plan_from_survey, plan_organize, probe_folder,
    survey, ArchiveAfter, Choice, Companion, Counts, Decision, ExtractEntry, ExtractSource,
    FolderProbe, GameLink, GameRole, ItemKind, ItemNote, LeftReason, LooseDiscSet, NameMode,
    OrganizeAction, OrganizeEnv, OrganizeError, OrganizeItem, OrganizeOptions, OrganizePhase,
    OrganizePlan, OrganizeProgress, PlanWarning, Playlist, RenameReason, SheetCheck, SourceFp,
    Survey, SurveyEntry, Transfer, TransferMode, DEFAULT_LAYOUT, PROBE_LIMIT,
};
pub use target::{assign_targets, file_system_is_fat32, folder_dir, DirIndex, TargetLayout};
pub use walk::{is_broad_root, MetaProbe, OsMetaProbe, WalkCandidate, WalkOptions, Walked};
