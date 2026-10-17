//! Organising a pile of loose ROMs into one folder per system
//! (organise spec): inspection, identification, planning and apply.

pub mod evidence;
pub mod inspect;
pub mod plan;
pub mod walk;

pub use plan::{ItemKind, LeftReason, NameMode, TransferMode};
pub use walk::{is_broad_root, MetaProbe, OsMetaProbe, WalkCandidate, WalkOptions, Walked};
