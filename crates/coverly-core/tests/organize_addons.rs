//! Mods, patches and cheats in Organizar (mods spec D2–D4, D7, D13, D18):
//! what the survey's walk finds in a pile.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use coverly_core::addons::testing as at;
use coverly_core::addons::{AddonFormat, AddonKey, AddonKind, UnlinkedWhy};
use coverly_core::cache::Cache;
use coverly_core::consoles::testing as ct;
use coverly_core::consoles::{start_open_log, take_open_log};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::game_folder::FixedFolder;
use coverly_core::identify::sniff::NotARom;
use coverly_core::organize::addons::AddonCandidate;
use coverly_core::organize::plan::Survey;
use coverly_core::organize::{survey, LeftReason, MetaProbe, OrganizeEnv, OrganizeOptions};
use coverly_core::profiles::builtin_profiles;
use coverly_core::systems::SystemDb;

struct NoCloud;

impl MetaProbe for NoCloud {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }
}

/// The open log is one per process: the tests that read it take turns.
static OPEN_LOG: Mutex<()> = Mutex::new(());

fn log_turn() -> MutexGuard<'static, ()> {
    OPEN_LOG.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The plan's clock: well after every pinned fixture mtime.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000 + 100_000)
}

fn put(root: &Path, rel: &str, bytes: &[u8], n: u64) -> PathBuf {
    let p = rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c));
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    pin_mtime(&p, n);
    p
}

fn join(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c))
}

fn survey_with(dir: &Path, cache: &Cache) -> Survey {
    let db = SystemDb::builtin();
    let profiles = builtin_profiles();
    let env = OrganizeEnv {
        db: &db,
        profiles: &profiles,
        cache,
        dats: None,
        probe: &NoCloud,
        online: None,
        titles: None,
        now: now(),
    };
    let opts = OrganizeOptions {
        layout: "coverly-library".to_string(),
        offline: true,
        ..OrganizeOptions::default()
    };
    survey(dir, &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap()
}

fn survey_of(dir: &Path) -> Survey {
    survey_with(dir, &Cache::in_memory().unwrap())
}

fn addon<'a>(s: &'a Survey, path: &Path) -> &'a AddonCandidate {
    s.walked
        .addons
        .iter()
        .find(|a| a.path == path)
        .unwrap_or_else(|| panic!("no addon {path:?}: {:#?}", s.walked.addons))
}

fn left_of(s: &Survey, path: &Path) -> Option<LeftReason> {
    s.entries
        .iter()
        .find(|e| e.sources[0] == path)
        .and_then(|e| e.left.clone())
}

fn has_entry_below(s: &Survey, dir: &Path) -> bool {
    s.entries
        .iter()
        .any(|e| e.sources[0] != dir && e.sources[0].starts_with(dir))
}

#[test]
fn a_bps_beside_a_rom_is_an_addon_candidate_not_other_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    put(root, "Chrono Trigger (USA).sfc", &vec![0x11u8; 1 << 20], 1);
    let patch = put(
        root,
        "Chrono Trigger (USA) [T-Es].bps",
        &at::bps(1 << 20, 0x2D20_6BF7),
        2,
    );
    let s = survey_of(root);
    let a = addon(&s, &patch);
    assert_eq!(a.kind, AddonKind::Patch);
    assert_eq!(a.format, AddonFormat::Bps);
    assert_eq!(
        a.key,
        AddonKey::SourceCrc {
            crc32: 0x2D20_6BF7,
            size: 1 << 20
        }
    );
    assert!(!a.is_dir);
    assert_eq!(a.filed_under, None);
    assert!(!s.walked.other_files.contains(&patch));
    assert!(!s.walked.companions.contains(&patch));
    // Until a game takes it, it stays where it is (D7).
    assert_eq!(
        left_of(&s, &patch),
        Some(LeftReason::AddonUnlinked {
            kind: AddonKind::Patch,
            format: AddonFormat::Bps,
            why: UnlinkedWhy::NoMatch
        })
    );
}

#[test]
fn a_same_stem_patch_is_still_a_companion() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    put(root, "Game.sfc", &vec![0x11u8; 1 << 16], 1);
    let ips = put(root, "Game.ips", &at::ips(), 2);
    let cht = put(root, "Game.cht", b"cheats = 1\n", 3);
    let s = survey_of(root);
    assert!(s.walked.addons.is_empty(), "{:#?}", s.walked.addons);
    assert!(s.walked.companions.contains(&ips));
    assert!(s.walked.companions.contains(&cht));
}

#[test]
fn a_same_stem_patch_in_another_normal_form_is_still_a_companion() {
    // The companion pass matches stems by NFC and case (`name_key`): a
    // patch whose name a Mac wrote decomposed still travels with its ROM.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    put(
        root,
        "Pok\u{e9}mon Snap (USA).sfc",
        &vec![0x11u8; 1 << 16],
        1,
    );
    let ips = put(root, "POKE\u{301}MON SNAP (USA).ips", &at::ips(), 2);
    let s = survey_of(root);
    assert!(s.walked.addons.is_empty(), "{:#?}", s.walked.addons);
    assert!(s.walked.companions.contains(&ips));
}

#[test]
fn a_keyless_patch_carries_the_readme_beside_it() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let ips = put(root, "Super Mario Bros. 3 (USA) [T-Es].ips", &at::ips(), 1);
    let readme = put(
        root,
        "readme.txt",
        b"Apply to Super Mario Bros. 3 (USA).nes\n",
        2,
    );
    let s = survey_of(root);
    let a = addon(&s, &ips);
    assert_eq!(a.format, AddonFormat::Ips);
    assert_eq!(a.key, AddonKey::None);
    assert_eq!(a.readme.as_deref(), Some(readme.as_path()));
    // The readme is prose, not a cheat.
    assert!(!s.walked.addons.iter().any(|a| a.path == readme));
}

#[test]
fn a_mod_folder_is_one_candidate_and_not_descended() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    put(root, "0004000000055D00/romfs/a.bin", &[1u8; 64], 1);
    put(root, "0004000000055D00/romfs/b.sfc", &[1u8; 64], 2);
    let dir = root.join("0004000000055D00");
    let _turn = log_turn();
    start_open_log();
    let s = survey_of(root);
    let opened = take_open_log();
    assert!(
        !opened.iter().any(|p| p.starts_with(&dir)),
        "opened inside the mod: {opened:?}"
    );
    assert_eq!(s.walked.addons.len(), 1, "{:#?}", s.walked.addons);
    let a = addon(&s, &dir);
    assert!(a.is_dir);
    assert_eq!(a.format, AddonFormat::LayeredFs);
    assert_eq!(
        a.key,
        AddonKey::TitleId {
            id: "0004000000055D00".into()
        }
    );
    assert!(!has_entry_below(&s, &dir));
}

#[test]
fn a_texture_pack_under_textures_is_one_candidate() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    put(
        root,
        "textures/SLUS-00594/replacements/a.png",
        &[1u8; 64],
        1,
    );
    let dir = join(root, "textures/SLUS-00594");
    let s = survey_of(root);
    let a = addon(&s, &dir);
    assert_eq!(a.format, AddonFormat::TexturePack);
    assert_eq!(
        a.key,
        AddonKey::Serial {
            serial: "SLUS-00594".into()
        }
    );
    assert!(!has_entry_below(&s, &dir));
}

const BASE: u64 = 0x0004_0000_0005_5E00;
const UPDATE: u64 = 0x0004_000E_0005_5E00;

/// Part A's `Pokemon Y/` with filed `Mods/` and `Cheats/` content.
fn pokemon_pile(root: &Path) {
    let files: Vec<(&str, Vec<u8>)> = vec![
        (
            "Pokemon Y/Pokemon Y.cia",
            ct::cia(BASE, true, false).head_bytes(),
        ),
        (
            "Pokemon Y/Updates/u.cia",
            ct::cia(UPDATE, true, false).head_bytes(),
        ),
        ("Pokemon Y/Mods/m.ips", b"PATCH\0\0\0EOF".to_vec()),
        ("Pokemon Y/Mods/notes.bin", vec![9u8; 64]),
        (
            "Pokemon Y/Cheats/0004000000055E00.txt",
            b"[Infinite money]\nD3000000 00000000\n".to_vec(),
        ),
        ("Pokemon Y/Saves/s.sav", vec![1u8; 512]),
        ("Pokemon Y/Extras/ost.mp3", vec![2u8; 512]),
    ];
    for (n, (rel, bytes)) in files.iter().enumerate() {
        put(root, rel, bytes, n as u64 + 1);
    }
}

#[test]
fn a_pile_game_folder_s_mods_and_cheats_entries_are_filed_addons() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    pokemon_pile(root);
    let base = join(root, "Pokemon Y/Pokemon Y.cia");
    let s = survey_of(root);

    let m = addon(&s, &join(root, "Pokemon Y/Mods/m.ips"));
    assert_eq!(m.format, AddonFormat::Ips);
    assert_eq!(m.filed_under.as_deref(), Some(base.as_path()));
    let c = addon(&s, &join(root, "Pokemon Y/Cheats/0004000000055E00.txt"));
    assert_eq!(c.kind, AddonKind::Cheat);
    assert_eq!(c.format, AddonFormat::CtrCheats);
    assert_eq!(c.filed_under.as_deref(), Some(base.as_path()));
    assert_eq!(s.walked.addons.len(), 2, "{:#?}", s.walked.addons);

    // An entry the sniff does not know stays with its folder's row.
    let content = |folder| LeftReason::GameFolderContent {
        folder,
        of: "Pokemon Y".into(),
    };
    assert_eq!(
        left_of(&s, &join(root, "Pokemon Y/Mods/notes.bin")),
        Some(content(FixedFolder::Mods))
    );
    // `Extras/` and `Saves/` keep part A's one row each.
    assert_eq!(
        left_of(&s, &join(root, "Pokemon Y/Extras")),
        Some(content(FixedFolder::Extras))
    );
    assert_eq!(
        left_of(&s, &join(root, "Pokemon Y/Saves")),
        Some(content(FixedFolder::Saves))
    );
    assert_eq!(left_of(&s, &join(root, "Pokemon Y/Mods")), None);
    assert!(!has_entry_below(&s, &join(root, "Pokemon Y/Extras")));
}

#[test]
fn a_filed_folder_with_nothing_known_keeps_its_one_row() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    put(
        root,
        "Pokemon Y/Pokemon Y.cia",
        &ct::cia(BASE, true, false).head_bytes(),
        1,
    );
    put(root, "Pokemon Y/Mods/notes.bin", &[9u8; 64], 2);
    let s = survey_of(root);
    assert!(s.walked.addons.is_empty(), "{:#?}", s.walked.addons);
    assert_eq!(
        left_of(&s, &join(root, "Pokemon Y/Mods")),
        Some(LeftReason::GameFolderContent {
            folder: FixedFolder::Mods,
            of: "Pokemon Y".into()
        })
    );
}

const BLOODBORNE: &str = "UP9000-CUSA00900_00-BLOODBORNE000000";

#[test]
fn the_shadps4_mods_overlay_is_an_addon_when_its_base_is_here() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    ct::ps4_app(&root.join("CUSA00900"), "CUSA00900", "gd", BLOODBORNE);
    put(root, "CUSA00900-mods/eboot.bin", &[1u8; 64], 1);
    let mods = root.join("CUSA00900-mods");
    let s = survey_of(root);
    let a = addon(&s, &mods);
    assert!(a.is_dir);
    assert_eq!(a.format, AddonFormat::Shadps4Mods);
    assert_eq!(
        a.key,
        AddonKey::TitleId {
            id: "CUSA00900".into()
        }
    );
    assert_eq!(
        a.filed_under.as_deref(),
        Some(root.join("CUSA00900").as_path())
    );
    // Until the plan links it, it stays beside its base as before.
    assert_eq!(
        left_of(&s, &mods),
        Some(LeftReason::GameOverlay {
            of: "CUSA00900".into()
        })
    );
    assert!(!has_entry_below(&s, &mods));

    // No base beside it: an ordinary folder, as before.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    put(root, "CUSA00900-mods/x.bin", &[1u8; 64], 1);
    let s = survey_of(root);
    assert!(s.walked.addons.is_empty(), "{:#?}", s.walked.addons);
}

#[test]
fn global_cheat_databases_are_emulator_config() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let dat = put(root, "usrcheat.dat", &[1u8; 256], 1);
    let db = put(root, "cheat.db", &[1u8; 256], 2);
    let s = survey_of(root);
    for p in [dat, db] {
        assert_eq!(
            left_of(&s, &p),
            Some(LeftReason::NotARom {
                what: NotARom::EmulatorConfig
            })
        );
    }
    assert!(s.walked.addons.is_empty());
}

#[test]
fn cheats_and_msu1_sets_are_found_by_name_and_content() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let pnach = put(
        root,
        "SLUS-20062_ABCD1234.pnach",
        b"patch=1,EE,0,word,0\n",
        1,
    );
    let yml = put(
        root,
        "patch.yml",
        b"BLUS30001:\nBLES00001:\nNPUA80001:\n",
        2,
    );
    let mut pcm = b"MSU1".to_vec();
    pcm.extend([0u8; 60]);
    let msu = put(root, "Zelda MSU/zelda.msu", &[0u8; 16], 3);
    put(root, "Zelda MSU/zelda-1.pcm", &pcm, 4);
    put(root, "Zelda MSU/zelda-2.pcm", &pcm, 5);
    let stray = put(root, "Lone/lone-1.pcm", &pcm, 6);
    let prose = put(root, "notes.txt", b"Remember to buy milk.\n", 7);
    let s = survey_of(root);
    assert_eq!(
        addon(&s, &pnach).key,
        AddonKey::Serial {
            serial: "SLUS-20062".into()
        }
    );
    assert_eq!(
        left_of(&s, &yml),
        Some(LeftReason::AddonUnlinked {
            kind: AddonKind::Cheat,
            format: AddonFormat::Rpcs3Patch,
            why: UnlinkedWhy::Many { count: 3 }
        })
    );
    let set = addon(&s, &msu);
    assert_eq!(set.format, AddonFormat::Msu1);
    assert_eq!(
        set.members,
        vec![PathBuf::from("zelda-1.pcm"), PathBuf::from("zelda-2.pcm")]
    );
    assert_eq!(
        left_of(&s, &stray),
        Some(LeftReason::AddonUnlinked {
            kind: AddonKind::Mod,
            format: AddonFormat::Msu1,
            why: UnlinkedWhy::Partial
        })
    );
    assert!(!s.walked.addons.iter().any(|a| a.path == prose));
    assert!(s.walked.companions.contains(&prose));
}

#[test]
fn a_mod_folder_with_a_key_is_left_and_the_key_never_opened() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    put(
        root,
        "atmosphere/contents/0100000000010000/romfs/x",
        &[1u8; 64],
        1,
    );
    let tik = put(
        root,
        "atmosphere/contents/0100000000010000/title.tik",
        &[2u8; 64],
        2,
    );
    // A folder that is no mod keeps walking: its key is left on its own.
    put(root, "Stuff/game.sfc", &[3u8; 1024], 3);
    let tik2 = put(root, "Stuff/other.tik", &[4u8; 64], 4);
    let dir = join(root, "atmosphere/contents/0100000000010000");
    let _turn = log_turn();
    start_open_log();
    let s = survey_of(root);
    let opened = take_open_log();
    assert!(!opened.contains(&tik), "{opened:?}");
    assert!(!opened.contains(&tik2), "{opened:?}");
    assert_eq!(left_of(&s, &dir), Some(LeftReason::KeyMaterial));
    assert!(!has_entry_below(&s, &dir));
    assert!(s.walked.addons.is_empty(), "{:#?}", s.walked.addons);
    assert_eq!(left_of(&s, &tik2), Some(LeftReason::KeyMaterial));
}

#[test]
fn walking_2000_text_files_and_500_patches_stays_within_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    for i in 0..2000u64 {
        put(
            root,
            &format!("notes/{}/note {i}.txt", i % 20),
            b"Just some words.\n",
            i + 1,
        );
    }
    for i in 0..500u64 {
        put(
            root,
            &format!("patches/{}/hack {i}.bps", i % 10),
            &at::bps(1 << 20, i as u32),
            i + 1,
        );
    }
    let cache = Cache::in_memory().unwrap();
    let _turn = log_turn();
    start_open_log();
    let s = survey_with(root, &cache);
    let opened = take_open_log();
    let mine: Vec<&PathBuf> = opened.iter().filter(|p| p.starts_with(root)).collect();
    let patches = mine
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "bps"))
        .count();
    let texts = mine
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .count();
    assert_eq!(patches, 500, "each patch opened once");
    assert_eq!(texts, 2000, "each text opened once");
    assert_eq!(s.walked.addons.len(), 500);

    // A second survey takes the memoised verdicts: nothing is read again.
    start_open_log();
    let again = survey_with(root, &cache);
    let opened = take_open_log();
    assert!(
        !opened.iter().any(|p| p.starts_with(root)),
        "read again: {}",
        opened.len()
    );
    assert_eq!(again.walked.addons, s.walked.addons);
}
