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

/// Where the app keeps its cache (indexes, DATs, downloaded art) and its
/// data (job journals, backups).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppDirs {
    pub cache: std::path::PathBuf,
    pub data: std::path::PathBuf,
}

impl AppDirs {
    /// The platform dirs for `(QUALIFIER, ORG, APP)`, each replaced by its
    /// override when given. `None` only when no home dir is known and an
    /// override is missing.
    pub fn resolve(
        cache: Option<&std::path::Path>,
        data: Option<&std::path::Path>,
    ) -> Option<AppDirs> {
        let project = directories::ProjectDirs::from(QUALIFIER, ORG, APP);
        let cache = match cache {
            Some(p) => p.to_path_buf(),
            None => project.as_ref()?.cache_dir().to_path_buf(),
        };
        let data = match data {
            Some(p) => p.to_path_buf(),
            None => project.as_ref()?.data_dir().to_path_buf(),
        };
        Some(AppDirs { cache, data })
    }
}

/// Where the app keeps its settings (`library.json`), or `None` when no
/// home dir is known.
pub fn config_dir() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from(QUALIFIER, ORG, APP).map(|p| p.config_dir().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_dir_triple() {
        assert_eq!((ORG, QUALIFIER, APP), ("Maecly", "com", "CoverlyROMs"));
    }

    /// `directories` lowercases the app name on Linux (`~/.cache/coverlyroms`).
    fn names_the_app(path: &std::path::Path) -> bool {
        path.to_string_lossy()
            .to_lowercase()
            .contains(&APP.to_lowercase())
    }

    #[test]
    fn platform_dirs_are_used_without_overrides() {
        let dirs = AppDirs::resolve(None, Some(std::path::Path::new("/d"))).unwrap();
        assert!(names_the_app(&dirs.cache), "{dirs:?}");
        assert_eq!(dirs.data, std::path::Path::new("/d"));
    }

    #[test]
    fn config_dir_is_the_app_config_dir() {
        let dir = config_dir().expect("a home dir exists in tests");
        assert!(names_the_app(&dir), "{dir:?}");
    }
}
