//! Name-based rules: which entries are skipped and which files are ROMs.

use std::path::Path;

use crate::systems::System;

use super::ScanOptions;

/// OS metadata files ignored everywhere, compared case-insensitively.
const JUNK_FILES: [&str; 3] = [".ds_store", "thumbs.db", "desktop.ini"];

/// Suffixes of files a browser or download manager is still writing
/// (Chrome, Firefox, Safari, Opera, µTorrent, qBittorrent, aria2, WinSCP).
pub(crate) const PARTIAL_DOWNLOAD_EXTS: [&str; 10] = [
    "crdownload",
    "part",
    "partial",
    "download",
    "opdownload",
    "tmp",
    "!ut",
    "!qb",
    "aria2",
    "filepart",
];

/// Extensions that follow a ROM with the same stem when it moves
/// (default of `settings.organize.companion_exts`; `state*` matches
/// `state`, `state1`, `state.auto`…).
#[allow(dead_code)] // consumed by the organise planner
pub(crate) const COMPANION_EXTS: [&str; 12] = [
    "sav", "srm", "rtc", "state*", "cht", "ips", "bps", "ups", "txt", "nfo", "png", "jpg",
];

/// BIOS and firmware files, by exact lowercase name.
const BIOS_NAMES: [&str; 27] = [
    "gba_bios.bin",
    "gb_bios.bin",
    "gbc_bios.bin",
    "sgb_bios.bin",
    "bios7.bin",
    "bios9.bin",
    "firmware.bin",
    "neogeo.zip",
    "disksys.rom",
    "dc_boot.bin",
    "dc_flash.bin",
    "sega_101.bin",
    "lynxboot.img",
    "5200.rom",
    "7800 bios (u).rom",
    "bios.min",
    "pgm.zip",
    "naomi.zip",
    "bios.gg",
    "bios_e.sms",
    "bios_u.sms",
    "bios_j.sms",
    "panafz10.bin",
    "psxonpsp660.bin",
    "ps1_rom.bin",
    "uni-bios.rom",
    "colecovision.rom",
];

/// BIOS families by lowercase prefix and the extension they must have.
/// Amiga Kickstarts are `kick<digits>…` or `Kickstart …` (TOSEC): a bare
/// `kick` prefix would also catch MSX/ColecoVision games (`Kick Off.rom`).
const BIOS_PREFIXES: [(&str, &str); 6] = [
    ("scph", "bin"),
    ("syscard", "pce"),
    ("bios_cd_", "bin"),
    ("mpr-", "bin"),
    ("kickstart", "rom"),
    ("kick#", "rom"),
];

/// Whether `name` is a file a download is still writing.
pub(crate) fn is_partial_download(name: &str) -> bool {
    let lower = name.to_lowercase();
    PARTIAL_DOWNLOAD_EXTS
        .iter()
        .any(|ext| lower.len() > ext.len() + 1 && lower.ends_with(&format!(".{ext}")))
}

/// Windows `Zone.Identifier` alternate stream copied as a regular file
/// (through WSL or exFAT): `name.zip:Zone.Identifier`.
pub(crate) fn is_zone_identifier(name: &str) -> bool {
    name.to_lowercase().ends_with(":zone.identifier")
}

/// Whether `name` is a BIOS or firmware file (left in place by organise:
/// moving it into a game folder makes it a fake game).
#[allow(dead_code)] // consumed by the organise planner
pub(crate) fn is_bios_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    if BIOS_NAMES.contains(&lower.as_str()) {
        return true;
    }
    let ext = lower.rsplit_once('.').map_or("", |(_, e)| e);
    BIOS_PREFIXES.iter().any(|(prefix, want)| {
        let head = match prefix.strip_suffix('#') {
            // `#` stands for "followed by a digit".
            Some(head) => lower
                .strip_prefix(head)
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit())),
            None => lower.starts_with(prefix),
        };
        head && ext == *want
    })
}

/// OS metadata file (`.DS_Store`, `Thumbs.db`, `desktop.ini`), any case.
pub(crate) fn is_junk_file(name: &str) -> bool {
    JUNK_FILES.iter().any(|j| name.eq_ignore_ascii_case(j))
}

/// macOS AppleDouble twin (`._Game.zip`): never a ROM, never art.
pub(crate) fn is_appledouble(name: &str) -> bool {
    name.starts_with("._")
}

/// Dot-prefixed entry (`.hidden`, `.media`).
pub(crate) fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Whether an entry named `name` is skipped before looking at its type:
/// AppleDouble twins, OS metadata, partial downloads and copied
/// `Zone.Identifier` streams always, hidden entries unless `include_hidden`.
pub(crate) fn is_skipped_entry(name: &str, opts: &ScanOptions) -> bool {
    is_appledouble(name)
        || is_junk_file(name)
        || is_partial_download(name)
        || is_zone_identifier(name)
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
            dats: Vec::new(),
            screenscraper: None,
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

    #[test]
    fn partial_downloads_by_suffix_any_case() {
        for name in [
            "Tetris.zip.crdownload",
            "Tetris.zip.part",
            "x.PARTIAL",
            "x.download",
            "x.opdownload",
            "x.tmp",
            "x.!ut",
            "x.!qb",
            "x.zip.aria2",
            "x.filepart",
        ] {
            assert!(is_partial_download(name), "{name}");
        }
        for name in ["Tetris.zip", "Party.gba", "partial.gb", "download.nes"] {
            assert!(!is_partial_download(name), "{name}");
        }
    }

    #[test]
    fn zone_identifier_streams() {
        assert!(is_zone_identifier("Tetris.zip:Zone.Identifier"));
        assert!(is_zone_identifier("a.gba:zone.identifier"));
        assert!(!is_zone_identifier("Zone.Identifier.gba"));
    }

    #[test]
    fn bios_names_exact_and_by_prefix() {
        for name in [
            "gba_bios.bin",
            "GBA_BIOS.BIN",
            "bios7.bin",
            "bios9.bin",
            "firmware.bin",
            "scph1001.bin",
            "SCPH5501.BIN",
            "neogeo.zip",
            "syscard3.pce",
            "disksys.rom",
            "dc_boot.bin",
            "dc_flash.bin",
            "bios_CD_U.bin",
            "sega_101.bin",
            "mpr-17933.bin",
            "kick34005.A500.rom",
            "Kickstart v1.3 rev 34.5 (1987)(Commodore)(A500).rom",
        ] {
            assert!(is_bios_name(name), "{name}");
        }
        for name in [
            "Tetris.gb",
            "scph.gba",
            "kickoff.zip",
            "mpr-17933.zip",
            "biosphere.gba",
            // MSX and ColecoVision games are `.rom` too.
            "Kick Off.rom",
            "Kickle Cubicle.rom",
        ] {
            assert!(!is_bios_name(name), "{name}");
        }
    }

    #[test]
    fn common_libretro_bios_names() {
        for name in [
            "bios.gg",
            "bios_E.sms",
            "bios_U.sms",
            "bios_J.sms",
            "panafz10.bin",
            "PSXONPSP660.bin",
            "ps1_rom.bin",
            "uni-bios.rom",
            "colecovision.rom",
        ] {
            assert!(is_bios_name(name), "{name}");
        }
    }

    #[test]
    fn the_scan_skips_partial_downloads_and_streams() {
        let opts = ScanOptions::default();
        assert!(is_skipped_entry("Tetris.zip.crdownload", &opts));
        assert!(is_skipped_entry("Tetris.zip:Zone.Identifier", &opts));
        assert!(!is_skipped_entry("Tetris.zip", &opts));
    }
}
