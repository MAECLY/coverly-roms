//! Table-driven checks for the systems of batch 3 (plan P-1): the current
//! generation (PS4, PS5, Xbox One, Xbox Series). Each system task appends
//! rows; every check runs over every row. Names are checked against the
//! research listings committed in `tests/fixtures/research`, so a typo
//! fails offline.

use std::collections::{BTreeMap, BTreeSet};

use coverly_core::identify::evidence::Evidence;
use coverly_core::identify::hash::{hash_rom, HashOutcome, HeaderRule, DEFAULT_HASH_MAX_BYTES};
use coverly_core::identify::sniff::{sniff, SliceAt};
use coverly_core::profiles::builtin_profiles;
use coverly_core::profiles::naming::system_folder;
use coverly_core::sources::libretro::parse_apache_dirs;
use coverly_core::systems::{ExtClass, Family, SystemDb};

/// (id, family, multi_file)
const SYSTEMS: &[(&str, Family, bool)] = &[
    ("ps4", Family::Console, false),
    ("ps5", Family::Console, false),
    ("xboxone", Family::Console, false),
    ("xboxseries", Family::Console, false),
];
/// (id, embedded_first, no_emulator): spec §7 flags.
const FLAGS: &[(&str, bool, bool)] = &[
    ("ps4", true, false),
    ("ps5", true, false),
    ("xboxone", true, false),
    ("xboxseries", true, true),
];
/// (id, display name, ScreenScraper id): spec §7.
const NAMES: &[(&str, &str, Option<u32>)] = &[
    ("ps4", "Sony PlayStation 4", Some(60)),
    ("ps5", "Sony PlayStation 5", Some(284)),
    ("xboxone", "Microsoft Xbox One", Some(34)),
    // No ScreenScraper id found (spec §13).
    ("xboxseries", "Microsoft Xbox Series X|S", None),
];
/// (profile, id, expected folder, on device): spot checks of spec §6.
const TARGETS: &[(&str, &str, &str, Option<bool>)] = &[
    ("esde", "ps4", "ps4", None),
    ("batocera", "ps4", "ps4", None),
    ("esde", "xboxone", "xboxone", None),
    ("batocera", "xboxone", "xboxone", Some(false)),
    ("esde", "ps5", "ps5", Some(false)),
    ("batocera", "xboxseries", "xboxseries", Some(false)),
    ("muos", "ps4", "ps4", Some(false)),
    ("onion", "ps5", "PS5", Some(false)),
    ("minui", "xboxone", "XBOXONE", Some(false)),
];
/// (profile, id): the `Named` cells of spec §6; every other built-in ×
/// new system cell is Absent.
const NAMED: &[(&str, &str)] = &[("esde", "ps4"), ("batocera", "ps4"), ("esde", "xboxone")];
/// Builds a sparse file whose far offsets matter.
type Sparse = fn() -> coverly_core::consoles::testing::SparseAt;
/// (id, ext, sparse file, expected system, expected serial): one
/// raw-file header rule of the batch.
type ConsoleHeader = (
    &'static str,
    &'static str,
    Sparse,
    Option<&'static str>,
    Option<&'static str>,
);
/// The batch's raw-file header rules, through `sniff` like a real file.
const CONSOLE_HEADERS: &[ConsoleHeader] = &[
    // Task 4: the ES-DE «game serial» launcher (D17); PS4 has no DAT,
    // so no serial.
    ("ps4", "ps4", ps4_launcher, Some("ps4"), None),
    // Task 5: a CNT package header names the PS4 (D9) under the weak
    // `.pkg` extension.
    ("ps4", "pkg", ps4_cnt_pkg, Some("ps4"), None),
    // Task 6: a PS5 store package names the PS5 through its embedded CNT
    // header (D10), and an `.ffpkg` its UFS2 superblock.
    ("ps5", "pkg", ps5_fih_pkg, Some("ps5"), None),
    ("ps5", "ffpkg", ps5_ufs2, Some("ps5"), None),
    // Task 7: a `.zar` is decided by its tree (D12), never by the
    // ZArchive magic alone.
    ("ps4", "zar", ps4_zar, Some("ps4"), None),
    ("ps5", "zar", ps5_zar, Some("ps5"), None),
    ("ps4", "zar", bare_zar, None, None),
];

/// A PS4 ZArchive: `eboot.bin` and `sce_sys/param.sfo`.
fn ps4_zar() -> coverly_core::consoles::testing::SparseAt {
    use coverly_core::consoles::testing as t;
    let sfo = t::ps4_sfo("CUSA04447", "gd", "EP0002-CUSA04447_00-GRAVITYRUSH00000", 1);
    t::zarchive(&[("eboot.bin", &[0u8; 64]), ("sce_sys/param.sfo", &sfo)])
}

/// A PS5 ZArchive: `eboot.bin` and `sce_sys/param.json`.
fn ps5_zar() -> coverly_core::consoles::testing::SparseAt {
    use coverly_core::consoles::testing as t;
    let json = t::ps5_param_json("PPSA01284", "Returnal", false);
    t::zarchive(&[("eboot.bin", &[0u8; 64]), ("sce_sys/param.json", &json)])
}

/// A ZArchive whose tree names no system: no header verdict.
fn bare_zar() -> coverly_core::consoles::testing::SparseAt {
    coverly_core::consoles::testing::zarchive(&[("media/a.bin", &[0u8; 8])])
}

/// A PS5 store package head (FIH, then CNT with a `PPSA` content id).
fn ps5_fih_pkg() -> coverly_core::consoles::testing::SparseAt {
    coverly_core::consoles::testing::fih_pkg("UP9000-PPSA01284_00-SAMPLEGAME000000", 0x1A)
}

/// An `.ffpkg` head with the UFS2 superblock magic.
fn ps5_ufs2() -> coverly_core::consoles::testing::SparseAt {
    coverly_core::consoles::testing::ufs2_image(true)
}

/// A PS4 CNT package head (content id `CUSA` title id, type GD).
fn ps4_cnt_pkg() -> coverly_core::consoles::testing::SparseAt {
    coverly_core::consoles::testing::cnt_pkg("EP0002-CUSA04447_00-SAMPLEGAME000000", 0x1A, 0, &[])
}

/// A `.ps4` launcher: one line with a title id.
fn ps4_launcher() -> coverly_core::consoles::testing::SparseAt {
    let text = b"CUSA07010\n";
    let mut f = coverly_core::consoles::testing::SparseAt::new(text.len() as u64);
    f.put(0, text);
    f
}

fn research(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/research")
            .join(name),
    )
    .unwrap()
}

/// The strongest hit; on a tie the first listed (the likelier).
fn best(hits: &[Evidence]) -> Option<&str> {
    hits.iter()
        .fold(None, |best: Option<&Evidence>, h| match best {
            Some(b) if b.weight >= h.weight => Some(b),
            _ => Some(h),
        })
        .map(|h| h.system.as_str())
}

#[test]
fn batch_systems_load_with_name_family_and_disc_flag() {
    let db = SystemDb::builtin();
    for (id, family, multi) in SYSTEMS {
        let s = db.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!((s.family, s.multi_file), (*family, *multi), "{id}");
    }
    for (id, name, ss) in NAMES {
        let s = db.get(id).unwrap();
        assert_eq!((s.name.as_str(), s.screenscraper), (*name, *ss), "{id}");
    }
}

#[test]
fn the_catalogue_has_131_systems() {
    assert_eq!(SystemDb::builtin().all().len(), 131);
}

#[test]
fn flags_match_the_research_table() {
    let db = SystemDb::builtin();
    for (id, embedded_first, no_emulator) in FLAGS {
        let s = db.get(id).unwrap();
        assert_eq!(
            (s.embedded_first, s.no_emulator),
            (*embedded_first, *no_emulator),
            "{id}"
        );
    }
    // Only this batch sets them (spec §7).
    let flagged: BTreeSet<&str> = db
        .all()
        .iter()
        .filter(|s| s.embedded_first || s.no_emulator)
        .map(|s| s.id.as_str())
        .collect();
    let batch: BTreeSet<&str> = FLAGS.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(flagged, batch);
}

#[test]
fn libretro_names_exist_on_the_thumbnail_server() {
    let dirs: BTreeSet<String> = parse_apache_dirs(&research("thumbnails-root.html"))
        .into_iter()
        .collect();
    let db = SystemDb::builtin();
    let named: Vec<&str> = SYSTEMS
        .iter()
        .filter_map(|(id, _, _)| db.get(id).unwrap().libretro.as_deref())
        .collect();
    // Only PS4 has a libretro-thumbnails folder (spec §7).
    assert_eq!(named, ["Sony - PlayStation 4"]);
    for name in named {
        assert!(
            dirs.contains(name),
            "`{name}` not on thumbnails.libretro.com"
        );
    }
}

#[test]
fn every_alias_resolves_to_its_system() {
    let db = SystemDb::builtin();
    for (id, _, _) in SYSTEMS {
        let s = db.get(id).unwrap();
        assert!(!s.aliases.is_empty(), "{id}");
        for alias in &s.aliases {
            assert_eq!(
                db.resolve_folder(alias).map(|s| s.id.as_str()),
                Some(*id),
                "{alias}"
            );
            assert_eq!(
                db.resolve_folder(&alias.to_lowercase())
                    .map(|s| s.id.as_str()),
                Some(*id),
                "{alias} lowercased"
            );
        }
    }
}

/// `Microsoft Xbox Series X|S` is a display name only, never a folder.
#[test]
fn display_names_are_not_folders() {
    let db = SystemDb::builtin();
    for (_, name, _) in NAMES {
        assert!(db.resolve_folder(name).is_none(), "{name}");
    }
}

#[test]
fn organise_targets_match_the_research_table() {
    let db = SystemDb::builtin();
    let ps = builtin_profiles();
    let none = BTreeMap::new();
    for (profile, id, folder, on_device) in TARGETS {
        let p = ps.iter().find(|p| &p.id == profile).unwrap();
        let c = system_folder(p, db.get(id).unwrap(), &db, &none);
        assert_eq!(
            (c.folder.as_str(), c.on_device),
            (*folder, *on_device),
            "{profile} {id}"
        );
    }
}

/// Spec §6: every built-in × every new system is `Named` exactly where the
/// research table has a name; everywhere else the frontend has no folder
/// (`on_device == Some(false)`), except RetroArch, which lists none.
#[test]
fn every_builtin_names_only_the_researched_folders() {
    use coverly_core::profiles::ProfileFolder;
    let db = SystemDb::builtin();
    let none = BTreeMap::new();
    for p in builtin_profiles() {
        for (id, _, _) in SYSTEMS {
            let named = NAMED.contains(&(p.id.as_str(), *id));
            let c = system_folder(&p, db.get(id).unwrap(), &db, &none);
            if p.id == "retroarch" {
                assert!(!p.folders.contains_key(*id), "retroarch {id}");
                assert_eq!(
                    (c.folder.as_str(), c.on_device),
                    (*id, None),
                    "retroarch {id}"
                );
                continue;
            }
            if named {
                assert_eq!(
                    p.folders.get(*id),
                    Some(&ProfileFolder::Named(id.to_string())),
                    "{} {id}",
                    p.id
                );
                assert_eq!(c.on_device, None, "{} {id}", p.id);
            } else {
                assert_eq!(
                    p.folders.get(*id),
                    Some(&ProfileFolder::Absent),
                    "{} {id}",
                    p.id
                );
                assert_eq!(c.on_device, Some(false), "{} {id}", p.id);
            }
        }
    }
}

/// The batch's raw-file header rules go through `sniff` like a real file:
/// the strongest hit names the system and the serial is read.
#[test]
fn console_header_rules() {
    for (id, ext, file, want, serial) in CONSOLE_HEADERS {
        let mut f = file();
        let len = f.len;
        let s = sniff(&mut f, len, ext);
        assert_eq!(best(&s.hits), *want, "{id} .{ext}");
        assert_eq!(s.serial.as_deref(), *serial, "{id} .{ext} serial");
    }
}

/// Spec §7 and §12: `.zar` is Shared by three systems, the XVC family by
/// the two Xbox systems; `pkg` and `exfat` are weak (Unknown class) but
/// still organise candidates, so their header (or a `PPSA` token, D10)
/// can name the system.
#[test]
fn extension_classes_and_candidates() {
    let db = SystemDb::builtin();
    let owners = |ext: &str| -> Vec<String> {
        db.systems_for_extension(ext)
            .iter()
            .map(|s| s.id.clone())
            .collect()
    };
    assert_eq!(owners("zar"), ["xbox360", "ps4", "ps5"]);
    for ext in ["xvc", "xvd", "msixvc"] {
        assert_eq!(db.extension_class(ext), ExtClass::Shared, "{ext}");
        assert_eq!(owners(ext), ["xboxone", "xboxseries"], "{ext}");
    }
    for (ext, id) in [
        ("ps4", "ps4"),
        ("ffpkg", "ps5"),
        ("ffpfs", "ps5"),
        ("ffpfsc", "ps5"),
    ] {
        assert_eq!(db.extension_class(ext), ExtClass::Unique, "{ext}");
        assert_eq!(owners(ext), [id], "{ext}");
    }
    for ext in ["pkg", "exfat", "EXFAT"] {
        assert_eq!(db.extension_class(ext), ExtClass::Unknown, "{ext}");
        assert!(db.is_rom_like_extension(ext), "{ext} is a candidate");
    }
    let accepting: Vec<String> = db
        .systems_accepting("exfat")
        .iter()
        .map(|s| s.id.clone())
        .collect();
    assert_eq!(accepting, ["ps5"]);
}

/// D17: the ES-DE «game serial» `.ps4` file is text-native, so a one-line
/// title id is not taken for a stray text file; a web page still is.
#[test]
fn a_ps4_serial_file_is_not_a_text_file() {
    for body in [&b"CUSA07010\n"[..], b"CUSA07010"] {
        let s = sniff(&mut SliceAt(body), body.len() as u64, "ps4");
        assert!(s.not_a_rom.is_none(), "{body:?}");
    }
    let page = b"<!DOCTYPE html><html><body>404</body></html>";
    let s = sniff(&mut SliceAt(page), page.len() as u64, "ps4");
    assert!(s.not_a_rom.is_some());
}

/// §8.5: nothing of this batch is hashed; the file is never opened for it.
#[test]
fn current_generation_files_are_name_only() {
    let missing = std::path::Path::new("does-not-exist");
    for ext in [
        "ps4", "ffpkg", "ffpfs", "ffpfsc", "exfat", "xvc", "xvd", "msixvc", "zar", "pkg", "XVC",
    ] {
        assert_eq!(
            hash_rom(missing, ext, HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap(),
            HashOutcome::NameOnly,
            "{ext}"
        );
    }
}

/// D10: an exFAT image names the PS5 only with a `PPSA` token in its
/// file name; the sniff sees the name.
#[test]
fn an_exfat_image_needs_a_ppsa_token() {
    use coverly_core::identify::sniff::sniff_named;
    let mut f = coverly_core::consoles::testing::exfat_image();
    let len = f.len;
    let s = sniff_named(&mut f, len, "exfat", "PPSA09519.exfat");
    assert_eq!(best(&s.hits), Some("ps5"));
    let mut f = coverly_core::consoles::testing::exfat_image();
    let s = sniff_named(&mut f, len, "exfat", "backup.exfat");
    assert_eq!(best(&s.hits), None);
}
