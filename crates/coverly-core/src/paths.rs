//! Application directory identity.
//!
//! The `(QUALIFIER, ORG, APP)` triple is what `directories::ProjectDirs`
//! resolves app config/data/cache dirs from. Forks must change it (see
//! `TRADEMARKS.md`), so it is spelled out only here.

/// Reverse-DNS qualifier of the app directories.
pub const QUALIFIER: &str = "com";
/// Organisation name of the app directories.
pub const ORG: &str = "Maecly";
/// Application name of the app directories.
pub const APP: &str = "CoverlyROMs";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_dir_triple() {
        assert_eq!((ORG, QUALIFIER, APP), ("Maecly", "com", "CoverlyROMs"));
    }
}
