//! Organising a pile of loose ROMs into one folder per system
//! (organise spec): inspection, identification, planning and apply.

pub mod evidence;
pub mod inspect;
pub mod plan;
pub mod target;
pub mod walk;

pub use plan::{
    Companion, ItemKind, ItemNote, LeftReason, NameMode, OrganizeAction, OrganizeItem,
    RenameReason, SourceFp, TransferMode,
};
pub use target::{
    assign_targets, file_system_is_fat32, folder_dir, mark_duplicates, DirIndex, TargetLayout,
};
pub use walk::{is_broad_root, MetaProbe, OsMetaProbe, WalkCandidate, WalkOptions, Walked};
