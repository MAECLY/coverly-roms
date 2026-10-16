//! Core library of Coverly ROMs.
//!
//! Pure Rust, no Tauri: everything the app and the `coverly` CLI share
//! lives here. Network access is confined to `net`.

pub mod art;
pub mod audit;
pub mod cache;
pub mod gather;
pub mod identify;
pub mod image;
pub mod jobs;
pub mod matching;
pub mod net;
pub mod paths;
pub mod plan;
pub mod profiles;
pub mod progress;
pub mod scan;
pub mod sources;
pub mod systems;
mod time;
pub mod write;

/// Version of the core library, taken from the workspace package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_package() {
        assert_eq!(VERSION, "0.0.1");
    }
}
