//! Core library of Coverly ROMs.
//!
//! Pure Rust, no Tauri and no network code: everything the app and the
//! `coverly` CLI share lives here.

pub mod paths;
pub mod systems;

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
