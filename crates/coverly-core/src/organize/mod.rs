//! Organising a pile of loose ROMs into one folder per system
//! (organise spec): inspection, identification, planning and apply.

pub mod inspect;
pub mod plan;

pub use plan::{ItemKind, LeftReason, NameMode, TransferMode};
