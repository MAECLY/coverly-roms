//! Name-based rules: which entries are skipped and which files are ROMs.

use std::path::Path;

use crate::systems::System;

use super::ScanOptions;

/// OS metadata files ignored everywhere, compared case-insensitively.
const JUNK_FILES: [&str; 3] = [".ds_store", "thumbs.db", "desktop.ini"];

/// macOS AppleDouble twin (`._Game.zip`): never a ROM, never art.
pub(crate) fn is_appledouble(name: &str) -> bool {
    name.starts_with("._")
}

/// Dot-prefixed entry (`.hidden`, `.media`).
pub(crate) fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Whether an entry named `name` is skipped before looking at its type:
/// AppleDouble twins and OS metadata always, hidden entries unless
/// `include_hidden`.
pub(crate) fn is_skipped_entry(name: &str, opts: &ScanOptions) -> bool {
    is_appledouble(name)
        || JUNK_FILES.iter().any(|j| name.eq_ignore_ascii_case(j))
        || (!opts.include_hidden && is_hidden(name))
}

/// Whether a sub-folder named `name` inside a system dir is skipped:
/// the profile's art folders and the ignored folders, case-insensitively.
pub(crate) fn is_skipped_dir(name: &str, art_dir_names: &[String], opts: &ScanOptions) -> bool {
    let key = name.to_lowercase();
    art_dir_names
        .iter()
        .chain(&opts.ignored_dirs)
        .any(|d| d.to_lowercase() == key)
}

/// Lowercase extension of `path` without the dot (`""` when it has none).
pub(crate) fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Whether a file with extension `ext` (lowercase) is a ROM of `system`.
///
/// A system with an extension list accepts exactly those, even one that is
/// also ignored (`md` is a Mega Drive ROM, not Markdown). A system with an
/// empty list accepts any extension that is not ignored.
pub(crate) fn is_rom_extension(ext: &str, system: &System, opts: &ScanOptions) -> bool {
    if ext.is_empty() {
        return false;
    }
    if system.extensions.is_empty() {
        !opts
            .ignored_exts
            .iter()
            .any(|e| e.eq_ignore_ascii_case(ext))
    } else {
        system.extensions.iter().any(|e| e == ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(extensions: &[&str]) -> System {
        System {
            id: "x".into(),
            name: "X".into(),
            aliases: vec![],
            extensions: extensions.iter().map(|e| e.to_string()).collect(),
            libretro: None,
            arcade: false,
            multi_file: false,
            header: crate::identify::hash::HeaderRule::Plain,
        }
    }

    #[test]
    fn skipped_entries() {
        let opts = ScanOptions::default();
        assert!(is_skipped_entry("._Game.zip", &opts));
        assert!(is_skipped_entry(".DS_Store", &opts));
        assert!(is_skipped_entry("THUMBS.DB", &opts));
        assert!(is_skipped_entry(".media", &opts));
        assert!(!is_skipped_entry("Game.zip", &opts));

        let hidden = ScanOptions {
            include_hidden: true,
            ..ScanOptions::default()
        };
        assert!(!is_skipped_entry(".media", &hidden));
        assert!(
            is_skipped_entry("._Game.zip", &hidden),
            "AppleDouble always"
        );
    }

    #[test]
    fn skipped_dirs_ignore_case() {
        let opts = ScanOptions::default();
        let art = vec!["Imgs".to_string()];
        assert!(is_skipped_dir("IMGS", &art, &opts));
        assert!(is_skipped_dir("imgs", &art, &opts));
        assert!(is_skipped_dir("BIOS", &art, &opts));
        assert!(is_skipped_dir("shortcuts", &art, &opts));
        assert!(!is_skipped_dir("Hacks", &art, &opts));
    }

    #[test]
    fn extensions_are_lowercased() {
        assert_eq!(extension(Path::new("GAME.ZIP")), "zip");
        assert_eq!(extension(Path::new("Game")), "");
        assert_eq!(extension(Path::new("Zelda (USA).Sfc")), "sfc");
    }

    #[test]
    fn rom_extension_rules() {
        let opts = ScanOptions::default();
        let snes = system(&["sfc", "smc", "zip"]);
        assert!(is_rom_extension("sfc", &snes, &opts));
        assert!(!is_rom_extension("nes", &snes, &opts));
        assert!(!is_rom_extension("", &snes, &opts));

        let genesis = system(&["md", "bin"]);
        assert!(
            is_rom_extension("md", &genesis, &opts),
            "listed beats ignored"
        );

        let any = system(&[]);
        assert!(is_rom_extension("whatever", &any, &opts));
        assert!(!is_rom_extension("txt", &any, &opts));
        assert!(!is_rom_extension("png", &any, &opts));
    }
}
