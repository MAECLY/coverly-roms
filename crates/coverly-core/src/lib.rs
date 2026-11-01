//! Core library of Coverly ROMs.
//!
//! Pure Rust, no Tauri: everything the app and the `coverly` CLI share
//! lives here. Network access is confined to `net`.

pub mod archive;
pub mod art;
pub mod audit;
pub mod cache;
pub mod catalog;
pub mod consoles;
pub mod credentials;
mod csv_util;
pub mod dedupe;
pub mod disc;
pub mod fill;
pub mod gamelist;
pub mod gather;
pub mod identify;
pub mod image;
pub mod jobs;
pub mod matching;
pub mod net;
pub mod organize;
pub mod paths;
pub mod plan;
pub mod profiles;
pub mod progress;
pub mod scan;
pub mod settings;
pub mod sources;
pub mod systems;
mod time;
pub mod update;
pub mod volumes;
pub mod write;

/// Version of the core library, taken from the workspace package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_package() {
        // The workspace `Cargo.toml` is the one version source
        // (scripts/bump-version.sh); the constant must follow it.
        let manifest = include_str!("../../../Cargo.toml");
        let line = manifest
            .lines()
            .skip_while(|l| l.trim() != "[workspace.package]")
            .find(|l| l.trim_start().starts_with("version"))
            .expect("version in [workspace.package]");
        assert_eq!(line.split('"').nth(1), Some(VERSION));
    }
}
