//! Persisted settings (spec §9): one struct per group with the spec
//! defaults, stored as a versioned JSON file (`settings.json`) in the app
//! config dir (see [`crate::paths::config_dir`]). Loading never fails and
//! never loses a file: unknown fields are ignored, a bad field falls back
//! to its default, a file that cannot be read as settings is set aside,
//! and a copy is kept whenever loading changed something (D24).

mod apply;
mod load;
mod model;
mod patch;
mod validate;

pub use load::{
    backup_settings, load_settings, read_settings, save_settings, LoadNotice, LoadReport,
    LEGACY_LIBRARY_FILE, MAX_SETTINGS_BYTES, SETTINGS_FILE,
};
pub use model::*;
pub use validate::{FieldError, FieldErrorCode, Range, RANGES};
