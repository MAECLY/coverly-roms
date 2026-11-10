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

// --- The plan links addons and places them (mods spec D5–D13) ---

use coverly_core::addons::{HeaderFit, LinkVia};
use coverly_core::organize::{
    apply_organize, plan_organize, ApplyEnv, GameRole, ItemNote, ItemStatus, OrganizeAction,
    OrganizeItem, OrganizePlan,
};
use coverly_core::settings::OrganizeDedupeMode;
use coverly_core::write::trash::FakeTrash;
use coverly_core::write::undo_job;

const MON: u64 = 0x0004_0000_0005_5D00;
const MON_ID: &str = "0004000000055D00";
const CTR_CHEAT: &[u8] = b"[Infinite money]\nD3000000 00000000\n";

fn place_options(layout: &str, target: &Path) -> OrganizeOptions {
    OrganizeOptions {
        layout: layout.to_string(),
        target: Some(target.to_path_buf()),
        flat: true,
        offline: true,
        duplicates: OrganizeDedupeMode::Off,
        ..OrganizeOptions::default()
    }
}

fn plan_in(pile: &Path, opts: &OrganizeOptions) -> OrganizePlan {
    let db = SystemDb::builtin();
    let profiles = builtin_profiles();
    let cache = Cache::in_memory().unwrap();
    let env = OrganizeEnv {
        db: &db,
        profiles: &profiles,
        cache: &cache,
        dats: None,
        probe: &NoCloud,
        online: None,
        titles: None,
        now: now(),
    };
    plan_organize(pile, opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap()
}

fn lib_plan(pile: &Path, lib: &Path) -> OrganizePlan {
    plan_in(pile, &place_options("coverly-library", lib))
}

/// `rel`'s components as strings (never slash text: Windows CI).
fn parts(rel: &Path) -> Vec<String> {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect()
}

fn p(s: &[&str]) -> Vec<String> {
    s.iter().map(|c| c.to_string()).collect()
}

fn item<'a>(plan: &'a OrganizePlan, name: &str) -> &'a OrganizeItem {
    plan.items
        .iter()
        .find(|i| i.sources[0].file_name().is_some_and(|n| n == name))
        .unwrap_or_else(|| panic!("no item {name}: {:#?}", plan.items))
}

fn target_parts(it: &OrganizeItem) -> Vec<String> {
    parts(
        it.target
            .as_deref()
            .unwrap_or_else(|| panic!("{:?} has no target: {:?}", it.sources, it.action)),
    )
}

fn left(it: &OrganizeItem) -> LeftReason {
    match &it.action {
        OrganizeAction::Left { reason } => reason.clone(),
        other => panic!("{:?} is not left: {other:?}", it.sources),
    }
}

/// The game folder (by component) the base `name` was planned into.
fn folder_of(plan: &OrganizePlan, name: &str) -> Vec<String> {
    let base = item(plan, name);
    parts(
        base.game
            .as_ref()
            .and_then(|g| g.folder.as_deref())
            .unwrap_or_else(|| panic!("{name} has no game folder: {base:?}")),
    )
}

fn under(folder: &[String], rest: &[&str]) -> Vec<String> {
    folder.iter().cloned().chain(p(rest)).collect()
}

/// Every file below `dir`, by component, sorted.
fn files_below(dir: &Path) -> Vec<Vec<String>> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut out: Vec<Vec<String>> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| parts(e.path().strip_prefix(dir).unwrap()))
        .collect();
    out.sort();
    out
}

fn dirs_below(dir: &Path) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = walkdir::WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir())
        .map(|e| parts(e.path().strip_prefix(dir).unwrap()))
        .collect();
    out.sort();
    out
}

/// A pile with a 3DS base, its LayeredFS mod folder and its cheats.
fn pokemon_with_addons(pile: &Path) {
    put(
        pile,
        "Pokemon Y (World).cia",
        &ct::cia(MON, true, false).head_bytes(),
        1,
    );
    put(pile, &format!("{MON_ID}/romfs/a.bin"), &[1u8; 64], 2);
    put(pile, &format!("{MON_ID}.txt"), CTR_CHEAT, 3);
}

#[test]
fn coverly_library_places_mods_and_cheats_in_the_game_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_with_addons(&pile);
    fs::create_dir_all(&lib).unwrap();
    let plan = lib_plan(&pile, &lib);
    let base = item(&plan, "Pokemon Y (World).cia");
    assert_eq!(base.action, OrganizeAction::Auto, "{base:?}");
    let folder = folder_of(&plan, "Pokemon Y (World).cia");
    assert_eq!(folder[0], "3DS");

    let m = item(&plan, MON_ID);
    assert_eq!(m.action, OrganizeAction::Auto, "{m:?}");
    assert_eq!(target_parts(m), under(&folder, &["Mods", MON_ID]));
    let link = m.game.as_ref().unwrap();
    assert_eq!(link.role, GameRole::Mod);
    assert_eq!(link.of, Some(base.id));
    let addon = link.addon.as_ref().unwrap();
    assert_eq!(addon.key_text, MON_ID);
    assert_eq!(addon.via, LinkVia::Key);
    assert_eq!(addon.format, AddonFormat::LayeredFs);

    let c = item(&plan, &format!("{MON_ID}.txt"));
    assert_eq!(
        target_parts(c),
        under(&folder, &["Cheats", &format!("{MON_ID}.txt")])
    );
    let link = c.game.as_ref().unwrap();
    assert_eq!(link.role, GameRole::Cheat);
    assert_eq!(link.of, Some(base.id));
    assert_eq!(link.addon.as_ref().unwrap().via, LinkVia::Key);
    assert_eq!(m.system_id.as_deref(), Some("3ds"));
}

#[test]
fn no_mods_folder_without_mods() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, data) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("data"),
    );
    put(
        &pile,
        "Pokemon Y (World).cia",
        &ct::cia(MON, true, false).head_bytes(),
        1,
    );
    put(&pile, &format!("{MON_ID}.txt"), CTR_CHEAT, 2);
    fs::create_dir_all(&lib).unwrap();
    let plan = lib_plan(&pile, &lib);
    let folder = folder_of(&plan, "Pokemon Y (World).cia");
    for it in &plan.items {
        if let Some(t) = &it.target {
            assert!(!parts(t).contains(&"Mods".to_string()), "{it:?}");
        }
    }
    let report = apply_in(&plan, &data);
    assert!(report.paused.is_none(), "{report:?}");
    let dirs = dirs_below(&lib);
    assert!(dirs.contains(&under(&folder, &["Cheats"])), "{dirs:?}");
    assert!(
        !dirs.iter().any(|d| d.contains(&"Mods".to_string())),
        "{dirs:?}"
    );
}

const DAYS_GONE: &str = "UP9000-CUSA07023_00-BENDGAME00000000";
const BLOODBORNE_PLAN: &str = "UP9000-CUSA00900_00-BLOODBORNE000000";

#[test]
fn a_ps4_cheat_json_goes_to_cheats() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    ct::ps4_app(&pile.join("CUSA07023"), "CUSA07023", "gd", DAYS_GONE);
    put(&pile, "CUSA07023_01.03.json", b"{\"name\":\"x\"}", 1);
    fs::create_dir_all(&lib).unwrap();
    let plan = lib_plan(&pile, &lib);
    let base = item(&plan, "CUSA07023");
    assert!(base.moves_in(), "{base:?}");
    let game = target_parts(base);
    let c = item(&plan, "CUSA07023_01.03.json");
    assert_eq!(
        target_parts(c),
        under(&game, &["Cheats", "CUSA07023_01.03.json"])
    );
    let link = c.game.as_ref().unwrap();
    assert_eq!(link.role, GameRole::Cheat);
    assert_eq!(link.of, Some(base.id));
}

#[test]
fn the_shadps4_mods_overlay_follows_its_base() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    ct::ps4_app(&pile.join("CUSA00900"), "CUSA00900", "gd", BLOODBORNE_PLAN);
    put(&pile, "CUSA00900-mods/eboot.bin", &[1u8; 64], 1);
    fs::create_dir_all(&lib).unwrap();
    let plan = lib_plan(&pile, &lib);
    let base = item(&plan, "CUSA00900");
    let game = target_parts(base);
    let m = item(&plan, "CUSA00900-mods");
    assert_eq!(m.kind, coverly_core::organize::ItemKind::Folder);
    assert_eq!(target_parts(m), under(&game, &["Mods", "CUSA00900-mods"]));
    assert_eq!(m.game.as_ref().unwrap().of, Some(base.id));
}

#[test]
fn filed_pile_mods_move_with_their_base() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_pile(&pile);
    fs::create_dir_all(&lib).unwrap();
    let plan = lib_plan(&pile, &lib);
    let folder = folder_of(&plan, "Pokemon Y.cia");
    let m = item(&plan, "m.ips");
    assert_eq!(target_parts(m), under(&folder, &["Mods", "m.ips"]));
    let link = m.game.as_ref().unwrap();
    assert_eq!(link.role, GameRole::Mod);
    assert_eq!(link.addon.as_ref().unwrap().via, LinkVia::Folder);
    let c = item(&plan, "0004000000055E00.txt");
    assert_eq!(
        target_parts(c),
        under(&folder, &["Cheats", "0004000000055E00.txt"])
    );
    // What the sniff does not know keeps its folder's row.
    assert!(matches!(
        left(item(&plan, "notes.bin")),
        LeftReason::GameFolderContent { .. }
    ));
}

#[test]
fn a_retro_patch_has_no_place_in_coverly_library() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let rom = vec![0x11u8; 1 << 20];
    put(&pile, "Chrono Trigger (USA).sfc", &rom, 1);
    put(
        &pile,
        "Chrono Trigger (USA) [T-Es].bps",
        &at::bps(1 << 20, crc32fast::hash(&rom)),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = lib_plan(&pile, &lib);
    let rom_item = item(&plan, "Chrono Trigger (USA).sfc");
    assert!(rom_item.moves_in(), "{rom_item:?}");
    let patch = item(&plan, "Chrono Trigger (USA) [T-Es].bps");
    assert_eq!(
        left(patch),
        LeftReason::AddonNoPlace {
            kind: AddonKind::Patch
        }
    );
    let link = patch.game.as_ref().unwrap();
    assert_eq!(link.role, GameRole::Mod);
    assert_eq!(link.of, Some(rom_item.id));
    let addon = link.addon.as_ref().unwrap();
    assert_eq!(addon.via, LinkVia::SourceCrc);
    assert_eq!(addon.header, Some(HeaderFit::Matches));
    assert_eq!(
        addon.key_text,
        format!("CRC32 {:08X}", crc32fast::hash(&rom))
    );
}

#[test]
fn a_same_name_in_cheats_is_taken_not_numbered() {
    for same in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
        pokemon_with_addons(&pile);
        let existing: &[u8] = if same { CTR_CHEAT } else { b"[Other]\n" };
        put(
            &lib,
            &format!("3DS/Pokemon Y (World)/Cheats/{MON_ID}.txt"),
            existing,
            9,
        );
        let plan = lib_plan(&pile, &lib);
        let folder = folder_of(&plan, "Pokemon Y (World).cia");
        assert_eq!(folder, p(&["3DS", "Pokemon Y (World)"]));
        let c = item(&plan, &format!("{MON_ID}.txt"));
        let at = under(&folder, &["Cheats"]);
        match left(c) {
            LeftReason::AddonNameTaken { at: got } if !same => assert_eq!(parts(&got), at),
            LeftReason::AlreadyPresent { at: got } if same => {
                assert_eq!(parts(&got), under(&at, &[&format!("{MON_ID}.txt")]))
            }
            other => panic!("same = {same}: {other:?}"),
        }
        // The mod folder still goes in.
        assert_eq!(
            target_parts(item(&plan, MON_ID)),
            under(&folder, &["Mods", MON_ID])
        );
    }
}

#[test]
fn an_addon_already_in_place_is_no_item() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("lib");
    let game = "3DS/Pokemon Y (World)";
    put(
        &lib,
        &format!("{game}/Pokemon Y (World).cia"),
        &ct::cia(MON, true, false).head_bytes(),
        1,
    );
    put(
        &lib,
        &format!("{game}/Mods/{MON_ID}/romfs/a.bin"),
        &[1u8; 64],
        2,
    );
    put(&lib, &format!("{game}/Cheats/{MON_ID}.txt"), CTR_CHEAT, 3);
    let opts = OrganizeOptions {
        layout: "coverly-library".to_string(),
        offline: true,
        duplicates: OrganizeDedupeMode::Off,
        ..OrganizeOptions::default()
    };
    let plan = plan_in(&lib, &opts);
    for it in &plan.items {
        let names = parts(&it.sources[0]);
        assert!(
            !names.iter().any(|n| n == "Mods" || n == "Cheats"),
            "an addon in place became an item: {it:?}"
        );
    }
}

#[test]
fn unlinked_addons_stay_with_their_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(&pile, "random.ips", &at::ips(), 1);
    put(
        &pile,
        "patch.yml",
        b"BLUS30001: x\nBLES00001: y\nNPUA80001: z\n",
        2,
    );
    ct::wiiu_folder(
        &pile.join("Zelda US"),
        "0005000010101D00",
        "WUP-P-ALZE",
        "Zelda",
    );
    ct::wiiu_folder(
        &pile.join("Zelda EU"),
        "0005000010101E00",
        "WUP-P-ALZP",
        "Zelda",
    );
    put(
        &pile,
        "HD Pack/rules.txt",
        b"[Definition]\ntitleIds = 0005000010101D00,0005000010101E00\nname = HD\nversion = 6\n",
        3,
    );
    let mut pcm = b"MSU1".to_vec();
    pcm.extend([0u8; 60]);
    put(&pile, "Lone/lone-1.pcm", &pcm, 4);
    put(
        &pile,
        "A1B2C3D4E5F60718.txt",
        b"{Master}\n04000000 00000000\n",
        5,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = lib_plan(&pile, &lib);
    let why = |name: &str| match left(item(&plan, name)) {
        LeftReason::AddonUnlinked { why, .. } => why,
        other => panic!("{name}: {other:?}"),
    };
    assert_eq!(why("random.ips"), UnlinkedWhy::NoMatch);
    assert_eq!(why("patch.yml"), UnlinkedWhy::Many { count: 3 });
    assert_eq!(why("HD Pack"), UnlinkedWhy::Several { count: 2 });
    assert_eq!(why("lone-1.pcm"), UnlinkedWhy::Partial);
    assert_eq!(why("A1B2C3D4E5F60718.txt"), UnlinkedWhy::NoKey);
}

fn ps3_pkg(content_id: &str, tail: u8) -> Vec<u8> {
    let mut b = ct::pkg(1, content_id).head_bytes();
    b.extend([tail; 16]);
    b
}

const RESISTANCE: &str = "UP9000-BLUS30001_00-RESISTANCE000001";

#[test]
fn a_library_base_receives_its_addon_by_serial() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &lib,
        "PS3/Resistance (USA)/Resistance (USA).pkg",
        &ps3_pkg(RESISTANCE, 1),
        1,
    );
    put(&pile, "BLUS30001_patch.yml", b"Version: 1.2\n", 2);
    let plan = lib_plan(&pile, &lib);
    let c = item(&plan, "BLUS30001_patch.yml");
    assert_eq!(
        target_parts(c),
        p(&["PS3", "Resistance (USA)", "Cheats", "BLUS30001_patch.yml"])
    );
    assert!(
        c.notes.contains(&ItemNote::CheatOf {
            of: "Resistance (USA)".into()
        }),
        "{c:?}"
    );
    let link = c.game.as_ref().unwrap();
    assert_eq!(link.role, GameRole::Cheat);
    assert_eq!(link.of, None);
    assert!(link.folder_exists);
    assert_eq!(link.addon.as_ref().unwrap().key_text, "BLUS30001");
}

#[test]
fn a_bps_hashes_only_same_size_candidates() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let disc = put(
        &pile,
        "Zelda (USA).iso",
        &ct::gc_disc("GALE01", false).head_bytes(),
        1,
    );
    let f = fs::OpenOptions::new().write(true).open(&disc).unwrap();
    f.set_len(1_400_000_000).unwrap();
    drop(f);
    pin_mtime(&disc, 1);
    let rom = vec![0x13u8; 1 << 20];
    put(&pile, "Game (USA).sfc", &rom, 2);
    put(&pile, "hack.bps", &at::bps(1 << 20, 0xDEAD_BEEF), 3);
    put(
        &pile,
        "translation.bps",
        &at::bps(1 << 20, crc32fast::hash(&rom)),
        4,
    );
    fs::create_dir_all(&lib).unwrap();
    coverly_core::organize::addons::take_addon_hashes();
    let plan = lib_plan(&pile, &lib);
    let hashed = coverly_core::organize::addons::take_addon_hashes();
    assert!(
        !hashed.iter().any(|h| h.ends_with("Zelda (USA).iso")),
        "{hashed:?}"
    );
    assert!(matches!(
        left(item(&plan, "hack.bps")),
        LeftReason::AddonUnlinked {
            why: UnlinkedWhy::NoMatch,
            ..
        }
    ));
    // The ROM of the declared size is compared, and links.
    let linked = item(&plan, "translation.bps");
    assert_eq!(
        linked
            .game
            .as_ref()
            .and_then(|g| g.addon.as_ref())
            .map(|a| a.via),
        Some(LinkVia::SourceCrc),
        "{linked:?}"
    );
    assert_eq!(
        linked.game.as_ref().and_then(|g| g.of),
        Some(item(&plan, "Game (USA).sfc").id)
    );
}

#[test]
fn same_stem_companions_go_to_fixed_folders_with_game_folders_on() {
    for on in [true, false] {
        let tmp = tempfile::tempdir().unwrap();
        let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
        put(
            &pile,
            "Game.cia",
            &ct::cia(MON, true, false).head_bytes(),
            1,
        );
        put(&pile, "Game.cht", b"cheats = 1\n", 2);
        put(&pile, "Game.ips", &at::ips(), 3);
        fs::create_dir_all(&lib).unwrap();
        let mut opts = place_options("coverly-library", &lib);
        opts.game_folders = Some(on);
        let plan = plan_in(&pile, &opts);
        let base = item(&plan, "Game.cia");
        assert!(base.moves_in(), "{base:?}");
        let target = parts(base.target.as_deref().unwrap());
        let dir = &target[..target.len() - 1];
        let stem = Path::new(target.last().unwrap())
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for (name, fixed, ext) in [("Game.cht", "Cheats", "cht"), ("Game.ips", "Mods", "ips")] {
            let comp = base
                .companions
                .iter()
                .find(|c| c.source.file_name().is_some_and(|n| n == name))
                .unwrap_or_else(|| panic!("no companion {name}: {base:?}"));
            if on {
                assert_eq!(dir.len(), 2, "{target:?}");
                assert_eq!(parts(&comp.target), under(dir, &[fixed, name]));
            } else {
                assert_eq!(parts(&comp.target), under(dir, &[&format!("{stem}.{ext}")]));
            }
        }
    }
}

#[test]
fn addons_are_never_dedupe_units() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Pokemon Y/Pokemon Y.cia",
        &ct::cia(MON, true, false).head_bytes(),
        1,
    );
    put(&pile, "Pokemon Y/Cheats/same.txt", CTR_CHEAT, 2);
    put(
        &pile,
        "Pokemon X/Pokemon X.cia",
        &ct::cia(MON + 0x100, true, false).head_bytes(),
        3,
    );
    put(&pile, "Pokemon X/Cheats/same.txt", CTR_CHEAT, 4);
    fs::create_dir_all(&lib).unwrap();
    let mut opts = place_options("coverly-library", &lib);
    opts.duplicates = OrganizeDedupeMode::Report;
    let plan = plan_in(&pile, &opts);
    let json = serde_json::to_string(&plan.duplicates).unwrap();
    assert!(!json.contains("same.txt"), "{json}");
    for it in plan
        .items
        .iter()
        .filter(|i| i.sources[0].ends_with("same.txt"))
    {
        assert!(!it.is_set_aside(), "{it:?}");
    }
}

fn apply_in(plan: &OrganizePlan, data: &Path) -> coverly_core::organize::OrganizeReport {
    apply_organize(
        plan,
        &ApplyEnv {
            data_dir: data,
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
            trash: &FakeTrash::unavailable("no-trash"),
            tools: Some(&[]),
        },
    )
    .unwrap()
}

#[test]
fn apply_and_undo_round_trip_with_addons() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, data) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("data"),
    );
    ct::ps3_jb(&pile.join("Resistance"), "BLUS30001", "DG");
    let rap = put(
        &pile,
        "UP9000-BLUS30001_00-RESISTANCE000001.rap",
        &[0xA5; 16],
        2,
    );
    put(&pile, "BLUS30001_patch.yml", b"Version: 1.2\n", 3);
    pokemon_with_addons(&pile);
    fs::create_dir_all(&lib).unwrap();
    let before = files_below(&pile);
    let bytes: Vec<(Vec<String>, Vec<u8>)> = before
        .iter()
        .map(|f| {
            (
                f.clone(),
                fs::read(f.iter().fold(pile.clone(), |p, c| p.join(c))).unwrap(),
            )
        })
        .collect();

    let _turn = log_turn();
    start_open_log();
    let plan = lib_plan(&pile, &lib);
    let report = apply_in(&plan, &data);
    assert!(report.paused.is_none(), "{report:?}");
    assert!(
        report
            .items
            .iter()
            .all(|o| matches!(o.status, ItemStatus::Moved)),
        "{report:?}"
    );
    let ps3 = target_parts(item(&plan, "Resistance"));
    let mon = folder_of(&plan, "Pokemon Y (World).cia");
    let after = files_below(&lib);
    for want in [
        under(&ps3, &["Cheats", "BLUS30001_patch.yml"]),
        under(&mon, &["Cheats", &format!("{MON_ID}.txt")]),
        under(&mon, &["Mods", MON_ID, "romfs", "a.bin"]),
    ] {
        assert!(after.contains(&want), "{want:?} not in {after:?}");
    }
    let job = report.job.clone().unwrap();
    undo_job(&data, &job).unwrap();
    let opened = take_open_log();
    assert!(!opened.contains(&rap), "the licence was opened: {opened:?}");
    assert!(
        !opened.iter().any(|o| o
            .file_name()
            .is_some_and(|n| coverly_core::consoles::is_key_material(&n.to_string_lossy()))),
        "{opened:?}"
    );

    assert_eq!(files_below(&pile), before);
    for (f, b) in &bytes {
        assert_eq!(
            &fs::read(f.iter().fold(pile.clone(), |p, c| p.join(c))).unwrap(),
            b,
            "{f:?}"
        );
    }
    assert_eq!(files_below(&lib), Vec::<Vec<String>>::new());
    assert_eq!(fs::read_dir(&lib).unwrap().count(), 0, "every folder gone");
}

#[test]
fn a_mod_folder_still_being_written_stays_like_a_folder_game() {
    // The folder gates of a folder game hold for a mod folder too: one
    // written a moment ago, or holding a partial download, is not moved.
    for (file, mtime, want) in [
        ("a.bin", 100_000 - 5, LeftReason::RecentlyModified),
        ("b.bin.part", 2, LeftReason::PartialDownload),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
        pokemon_with_addons(&pile);
        put(&pile, &format!("{MON_ID}/romfs/{file}"), &[2u8; 8], mtime);
        fs::create_dir_all(&lib).unwrap();
        let plan = lib_plan(&pile, &lib);
        let m = item(&plan, MON_ID);
        assert_eq!(left(m), want, "{file}: {m:?}");
        // The cheat beside it still goes in.
        assert!(item(&plan, &format!("{MON_ID}.txt")).moves_in());
    }
}

#[cfg(unix)]
#[test]
fn a_mod_folder_with_a_link_inside_stays() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_with_addons(&pile);
    std::os::unix::fs::symlink(
        tmp.path().join("elsewhere"),
        join(&pile, &format!("{MON_ID}/romfs/link.bin")),
    )
    .unwrap();
    fs::create_dir_all(&lib).unwrap();
    let plan = lib_plan(&pile, &lib);
    assert_eq!(left(item(&plan, MON_ID)), LeftReason::Symlink);
}

// --- Organizar uses the translators (mods spec D16) ---

use coverly_core::organize::translate_plan::EmulatorDataWhy;
use coverly_core::organize::{EmulatorChoice, EmulatorId, OrganizeError, PlacementTo};

const MON_UPDATE: u64 = 0x0004_000E_0005_5D00;

fn plan_result(pile: &Path, opts: &OrganizeOptions) -> Result<OrganizePlan, OrganizeError> {
    let db = SystemDb::builtin();
    let profiles = builtin_profiles();
    let cache = Cache::in_memory().unwrap();
    let env = OrganizeEnv {
        db: &db,
        profiles: &profiles,
        cache: &cache,
        dats: None,
        probe: &NoCloud,
        online: None,
        titles: None,
        now: now(),
    };
    plan_organize(pile, opts, &env, &|_| {}, &AtomicBool::new(false))
}

fn choose(opts: &mut OrganizeOptions, system: &str, choice: EmulatorChoice) {
    opts.emulators.insert(system.to_string(), choice);
}

/// A PS4 game folder with its update beside it and a DLC folder.
fn bloodborne_pile(pile: &Path) {
    let sfo = |category: &str, content_id: &str, title: &str| {
        ct::param_sfo_with(
            &[
                ("APP_VER", "01.09"),
                ("CATEGORY", category),
                ("CONTENT_ID", content_id),
                ("TITLE", title),
                ("TITLE_ID", "CUSA00900"),
                ("VERSION", "01.00"),
            ],
            &[("APP_TYPE", 1), ("PUBTOOLVER", 0x0289_0000)],
        )
    };
    let base = pile.join("CUSA00900");
    ct::ps4_app(&base, "CUSA00900", "gd", BLOODBORNE_PLAN);
    ct::put_file(
        &base,
        "sce_sys/param.sfo",
        &sfo("gd", BLOODBORNE_PLAN, "Bloodborne"),
    );
    ct::ps4_app(
        &pile.join("CUSA00900-UPDATE"),
        "CUSA00900",
        "gp",
        BLOODBORNE_PLAN,
    );
    let dlc = pile.join("CUSA00900-DLC-ABC");
    ct::put_file(
        &dlc,
        "sce_sys/param.sfo",
        &sfo(
            "ac",
            "UP9000-CUSA00900_00-OLDHUNTERS000000",
            "The Old Hunters",
        ),
    );
    ct::put_file(&dlc, "content.dat", &[7u8; 64]);
    for f in walkdir::WalkDir::new(pile)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        pin_mtime(f.path(), 1);
    }
}

/// A 3DS base with its update, its LayeredFS mod, its cheats, and a
/// SNES ROM with a BPS made for it.
fn mixed_pile(pile: &Path) {
    pokemon_with_addons(pile);
    put(
        pile,
        "Pokemon Y Update.cia",
        &ct::cia(MON_UPDATE, true, false).head_bytes(),
        4,
    );
    let rom = vec![0x11u8; 1 << 20];
    put(pile, "Chrono Trigger (USA).sfc", &rom, 5);
    put(
        pile,
        "Chrono Trigger (USA) [T-Es].bps",
        &at::bps(1 << 20, crc32fast::hash(&rom)),
        6,
    );
}

fn is_addon(it: &OrganizeItem) -> bool {
    it.game
        .as_ref()
        .is_some_and(|g| matches!(g.role, GameRole::Mod | GameRole::Cheat))
}

#[test]
fn profile_defaults_keep_every_existing_target() {
    // D16, Q11: with no emulator in the request, every built-in profile
    // plans every base, update and DLC exactly as with no translator at
    // all (every system `none`); only addon rows and notes may change.
    let tmp = tempfile::tempdir().unwrap();
    let pile = tmp.path().join("pile");
    mixed_pile(&pile);
    bloodborne_pile(&pile);
    for (profile, game_folders) in builtin_profiles()
        .into_iter()
        .flat_map(|p| [(p.clone(), None), (p, Some(true))])
    {
        let lib = tmp
            .path()
            .join(format!("lib-{}-{game_folders:?}", profile.id));
        fs::create_dir_all(&lib).unwrap();
        let opts = OrganizeOptions {
            game_folders,
            ..place_options(&profile.id, &lib)
        };
        let mut off = opts.clone();
        for system in SystemDb::builtin().all().iter().map(|s| s.id.clone()) {
            choose(&mut off, &system, EmulatorChoice::None);
        }
        let with = plan_in(&pile, &opts);
        let without = plan_in(&pile, &off);
        assert!(without.translated.is_empty(), "{}", profile.id);
        assert_eq!(with.items.len(), without.items.len(), "{}", profile.id);
        for (a, b) in with.items.iter().zip(&without.items) {
            assert_eq!(a.sources, b.sources, "{}", profile.id);
            if is_addon(b) {
                continue;
            }
            assert_eq!(a.target, b.target, "{} {:?}", profile.id, a.sources);
            assert_eq!(a.action, b.action, "{} {:?}", profile.id, a.sources);
            assert_eq!(a.companions, b.companions, "{} {:?}", profile.id, a.sources);
            for note in &b.notes {
                assert!(a.notes.contains(note), "{} {:?}", profile.id, a.sources);
            }
        }
    }
}

#[test]
fn choosing_shadps4_uses_the_full_translator() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    bloodborne_pile(&pile);
    fs::create_dir_all(&lib).unwrap();
    // The ES-DE default (conservative): today's places, no `addcont`.
    let plan = plan_in(&pile, &place_options("esde", &lib));
    assert_eq!(
        target_parts(item(&plan, "CUSA00900-DLC-ABC")),
        p(&["ps4", "CUSA00900-DLC-ABC"])
    );
    assert!(plan.translated.is_empty(), "{:?}", plan.translated);

    let mut opts = place_options("esde", &lib);
    choose(
        &mut opts,
        "ps4",
        EmulatorChoice::Emulator(EmulatorId::Shadps4),
    );
    let plan = plan_in(&pile, &opts);
    let base = item(&plan, "CUSA00900");
    assert_eq!(target_parts(base), p(&["ps4", "CUSA00900"]));
    assert_eq!(
        target_parts(item(&plan, "CUSA00900-UPDATE")),
        p(&["ps4", "CUSA00900-UPDATE"])
    );
    let dlc = item(&plan, "CUSA00900-DLC-ABC");
    assert_eq!(dlc.action, OrganizeAction::Auto, "{dlc:?}");
    assert_eq!(
        target_parts(dlc),
        p(&["ps4", "addcont", "CUSA00900", "CUSA00900-DLC-ABC"])
    );
    assert!(
        dlc.notes.contains(&ItemNote::PointEmulatorHere {
            emulator: "shadPS4".into(),
            folder: "addcont".into(),
        }),
        "{:?}",
        dlc.notes
    );
    assert_eq!(plan.translated.len(), 1, "{:?}", plan.translated);
    let view = &plan.translated[0];
    assert_eq!(view.system, "ps4");
    assert_eq!(view.emulator, EmulatorId::Shadps4);
    assert_eq!(view.base, Some(base.id));
    let to: Vec<(Vec<String>, PlacementTo)> = view
        .placements
        .iter()
        .map(|pl| (parts(&pl.from), pl.to.clone()))
        .collect();
    let system = |s: &[&str]| PlacementTo::System {
        path: s.iter().collect(),
    };
    assert_eq!(
        to,
        vec![
            (p(&["CUSA00900"]), system(&["CUSA00900"])),
            (p(&["CUSA00900-UPDATE"]), system(&["CUSA00900-UPDATE"])),
            (
                p(&["CUSA00900-DLC-ABC"]),
                system(&["addcont", "CUSA00900", "CUSA00900-DLC-ABC"])
            ),
        ]
    );
}

#[test]
fn azahar_keeps_the_update_beside_its_base_and_says_install() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    mixed_pile(&pile);
    fs::create_dir_all(&lib).unwrap();
    let mut off = place_options("esde", &lib);
    choose(&mut off, "3ds", EmulatorChoice::None);
    let before = plan_in(&pile, &off);
    let plan = plan_in(&pile, &place_options("esde", &lib));
    let update = item(&plan, "Pokemon Y Update.cia");
    assert_eq!(
        update.target,
        item(&before, "Pokemon Y Update.cia").target,
        "{update:?}"
    );
    assert!(update.moves_in(), "{update:?}");
    assert!(
        update.notes.contains(&ItemNote::InstallFirst {
            emulator: "Azahar".into()
        }),
        "{:?}",
        update.notes
    );
}

#[test]
fn emulator_data_given_vs_not() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, data) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("data"),
    );
    pokemon_with_addons(&pile);
    fs::create_dir_all(&lib).unwrap();
    let cheat = format!("{MON_ID}.txt");
    // Not given: the cheat and the mod stay, saying where they go.
    let plan = plan_in(&pile, &place_options("esde", &lib));
    match left(item(&plan, &cheat)) {
        LeftReason::AddonForEmulator { emulator, path } => {
            assert_eq!(emulator, "Azahar");
            assert_eq!(parts(&path), p(&["cheats", &cheat]));
        }
        other => panic!("{other:?}"),
    }
    match left(item(&plan, MON_ID)) {
        LeftReason::AddonForEmulator { path, .. } => {
            assert_eq!(parts(&path), p(&["load", "mods", MON_ID]));
        }
        other => panic!("{other:?}"),
    }
    assert!(plan.emulator_roots.is_empty());
    let view = plan
        .translated
        .iter()
        .find(|v| v.system == "3ds")
        .expect("a 3DS view");
    assert!(view.placements.iter().any(|pl| pl.to
        == PlacementTo::EmulatorData {
            path: ["cheats", cheat.as_str()].iter().collect()
        }));

    // Given (and not there yet): under it, journaled, undone.
    let emu = tmp.path().join("azahar");
    let mut opts = place_options("esde", &lib);
    opts.emulator_data.insert(EmulatorId::Azahar, emu.clone());
    let plan = plan_in(&pile, &opts);
    let c = item(&plan, &cheat);
    assert_eq!(c.action, OrganizeAction::Auto, "{c:?}");
    assert_eq!(
        c.target.as_deref(),
        Some(emu.join("cheats").join(&cheat).as_path())
    );
    let m = item(&plan, MON_ID);
    assert_eq!(
        m.target.as_deref(),
        Some(emu.join("load").join("mods").join(MON_ID).as_path())
    );
    assert_eq!(plan.emulator_roots.len(), 1);
    assert_eq!(plan.emulator_roots[0].path, emu);
    assert!(plan.emulator_roots[0].anchor.is_some());
    let before = files_below(&pile);
    let report = apply_in(&plan, &data);
    assert!(report.paused.is_none(), "{report:?}");
    assert_eq!(report.failed, 0, "{report:?}");
    let written = files_below(&emu);
    assert!(written.contains(&p(&["cheats", &cheat])), "{written:?}");
    assert!(
        written.contains(&p(&["load", "mods", MON_ID, "romfs", "a.bin"])),
        "{written:?}"
    );
    undo_job(&data, report.job.as_deref().unwrap()).unwrap();
    assert_eq!(files_below(&pile), before);
    assert!(!emu.exists(), "the emulator folder it made is gone");
}

#[test]
fn a_missing_target_and_emulator_folder_under_one_new_parent_are_both_created() {
    // A fresh card: the ROMs folder and the emulator's folder side by side
    // (or one inside the other) under a parent that does not exist yet.
    // Creating the first must not make the second read as a pulled volume.
    for nested in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let (pile, data) = (tmp.path().join("pile"), tmp.path().join("data"));
        pokemon_with_addons(&pile);
        let card = tmp.path().join("card");
        let lib = card.join("roms");
        let emu = if nested {
            lib.join("emu").join("azahar")
        } else {
            card.join("azahar")
        };
        let cheat = format!("{MON_ID}.txt");
        let mut opts = place_options("esde", &lib);
        opts.emulator_data.insert(EmulatorId::Azahar, emu.clone());
        let plan = plan_in(&pile, &opts);
        assert_eq!(item(&plan, &cheat).action, OrganizeAction::Auto);
        let before = files_below(&pile);
        let report = apply_in(&plan, &data);
        assert!(report.paused.is_none(), "nested {nested}: {report:?}");
        assert_eq!(report.failed, 0, "nested {nested}: {report:?}");
        assert!(
            files_below(&emu).contains(&p(&["cheats", &cheat])),
            "nested {nested}"
        );
        undo_job(&data, report.job.as_deref().unwrap()).unwrap();
        assert_eq!(files_below(&pile), before);
        assert!(
            !card.exists(),
            "nested {nested}: the folders it made are gone"
        );
    }
}

#[test]
fn an_emulator_data_folder_inside_the_target_shows_as_emulator_data() {
    // A handheld card holds the ROMs and the emulator's own folder side
    // by side: the data folder may sit below the target root. The preview
    // still says the cheat goes to the emulator, not to the system folder.
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_with_addons(&pile);
    fs::create_dir_all(&lib).unwrap();
    let cheat = format!("{MON_ID}.txt");
    let mut opts = place_options("esde", &lib);
    opts.emulator_data
        .insert(EmulatorId::Azahar, lib.join("emu").join("azahar"));
    let plan = plan_in(&pile, &opts);
    let c = item(&plan, &cheat);
    assert_eq!(c.action, OrganizeAction::Auto, "{c:?}");
    assert_eq!(
        parts(&lib.join(c.target.as_deref().unwrap())),
        parts(&lib.join("emu").join("azahar").join("cheats").join(&cheat))
    );
    let view = plan
        .translated
        .iter()
        .find(|v| v.system == "3ds")
        .expect("a 3DS view");
    assert!(
        view.placements.iter().any(|pl| pl.to
            == PlacementTo::EmulatorData {
                path: ["cheats", cheat.as_str()].iter().collect()
            }),
        "{:?}",
        view.placements
    );
}

#[test]
fn a_run_emulator_that_does_not_run_the_system_falls_back_to_the_default() {
    // shadPS4 does not run 3DS games: the choice is ignored and the
    // profile's default (Azahar) still lays the cheat out.
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_with_addons(&pile);
    fs::create_dir_all(&lib).unwrap();
    let cheat = format!("{MON_ID}.txt");
    let default = plan_in(&pile, &place_options("esde", &lib));
    let mut opts = place_options("esde", &lib);
    choose(
        &mut opts,
        "3ds",
        EmulatorChoice::Emulator(EmulatorId::Shadps4),
    );
    let plan = plan_in(&pile, &opts);
    assert_eq!(item(&plan, &cheat).action, item(&default, &cheat).action);
    assert!(matches!(
        left(item(&plan, &cheat)),
        LeftReason::AddonForEmulator { .. }
    ));
    assert_eq!(plan.translated, default.translated);
}

#[test]
fn none_returns_the_master_layout() {
    // Game folders on: the master puts the cheat and the mod in the game
    // folder. `none` never sends them to the emulator, even when the run
    // names its folder; the default does, and keeps the master place when
    // no folder is named (Q11).
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_with_addons(&pile);
    fs::create_dir_all(&lib).unwrap();
    let cheat = format!("{MON_ID}.txt");
    let mut opts = place_options("esde", &lib);
    opts.game_folders = Some(true);
    let plan = plan_in(&pile, &opts);
    let folder = folder_of(&plan, "Pokemon Y (World).cia");
    assert_eq!(
        target_parts(item(&plan, &cheat)),
        under(&folder, &["Cheats", &cheat])
    );
    assert!(!plan.translated.is_empty());
    let emu = tmp.path().join("azahar");
    opts.emulator_data.insert(EmulatorId::Azahar, emu.clone());
    let plan = plan_in(&pile, &opts);
    assert_eq!(
        item(&plan, &cheat).target.as_deref(),
        Some(emu.join("cheats").join(&cheat).as_path())
    );
    choose(&mut opts, "3ds", EmulatorChoice::None);
    let plan = plan_in(&pile, &opts);
    let folder = folder_of(&plan, "Pokemon Y (World).cia");
    assert_eq!(
        target_parts(item(&plan, &cheat)),
        under(&folder, &["Cheats", &cheat])
    );
    assert_eq!(
        target_parts(item(&plan, MON_ID)),
        under(&folder, &["Mods", MON_ID])
    );
    assert!(plan.translated.is_empty(), "{:?}", plan.translated);
    assert!(plan.emulator_roots.is_empty());
}

#[test]
fn a_linked_retro_patch_under_the_retroarch_default_is_patch_not_applied() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    mixed_pile(&pile);
    fs::create_dir_all(&lib).unwrap();
    let patch = "Chrono Trigger (USA) [T-Es].bps";
    let plan = plan_in(&pile, &place_options("esde", &lib));
    assert_eq!(
        left(item(&plan, patch)),
        LeftReason::PatchNotApplied {
            emulator: "RetroArch".into()
        }
    );
    let rom = target_parts(item(&plan, "Chrono Trigger (USA).sfc"));
    let mut opts = place_options("esde", &lib);
    choose(
        &mut opts,
        "snes",
        EmulatorChoice::Emulator(EmulatorId::RetroArch),
    );
    let plan = plan_in(&pile, &opts);
    let it = item(&plan, patch);
    assert_eq!(it.action, OrganizeAction::Auto, "{it:?}");
    let mut beside = rom.clone();
    *beside.last_mut().unwrap() = "Chrono Trigger (USA).bps".into();
    assert_eq!(target_parts(it), beside);
    assert_eq!(
        target_parts(item(&plan, "Chrono Trigger (USA).sfc")),
        rom,
        "the ROM never moves"
    );
}

#[test]
fn emulator_data_inside_the_source_or_holding_keys_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_with_addons(&pile);
    fs::create_dir_all(&lib).unwrap();
    let keys = tmp.path().join("keys");
    put(&keys, "prod.keys", &[9u8; 32], 7);
    for (path, want) in [
        (lib.clone(), EmulatorDataWhy::IsTarget),
        (pile.join("emu"), EmulatorDataWhy::InsideSource),
        (keys.clone(), EmulatorDataWhy::HoldsKeys),
        (PathBuf::from("relative/emu"), EmulatorDataWhy::Relative),
    ] {
        let mut opts = place_options("esde", &lib);
        opts.emulator_data.insert(EmulatorId::Azahar, path.clone());
        let _turn = log_turn();
        start_open_log();
        match plan_result(&pile, &opts) {
            Err(OrganizeError::EmulatorData { emulator, why, .. }) => {
                assert_eq!(emulator, EmulatorId::Azahar);
                assert_eq!(why, want, "{path:?}");
            }
            other => panic!("{path:?}: {other:?}"),
        }
        let opened = take_open_log();
        assert!(
            !opened.iter().any(|o| o.starts_with(&keys)),
            "a key was opened: {opened:?}"
        );
    }
}

#[test]
fn es_de_with_game_folders_names_the_folder_after_the_base_file() {
    // D15 frontend: ES-DE «directories interpreted as files», only with
    // game folders on and an emulator chosen in the run (full scope). The
    // update follows the folder, still with Azahar's install note.
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Pokemon Y (World).cia",
        &ct::cia(MON, true, false).head_bytes(),
        1,
    );
    put(
        &pile,
        "Pokemon Y Update.cia",
        &ct::cia(MON_UPDATE, true, false).head_bytes(),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let mut opts = place_options("esde", &lib);
    opts.game_folders = Some(true);
    // The default (conservative) keeps the part A folder name.
    let plan = plan_in(&pile, &opts);
    assert_eq!(
        folder_of(&plan, "Pokemon Y (World).cia")
            .last()
            .map(String::as_str),
        Some("Pokemon Y (World)")
    );
    choose(
        &mut opts,
        "3ds",
        EmulatorChoice::Emulator(EmulatorId::Azahar),
    );
    let plan = plan_in(&pile, &opts);
    let base = item(&plan, "Pokemon Y (World).cia");
    let t = target_parts(base);
    let n = t.len();
    assert_eq!(
        t[n - 2..],
        p(&["Pokemon Y (World).cia", "Pokemon Y (World).cia"]),
        "{t:?}"
    );
    let folder = folder_of(&plan, "Pokemon Y (World).cia");
    assert_eq!(
        folder.last().map(String::as_str),
        Some("Pokemon Y (World).cia")
    );
    let update = item(&plan, "Pokemon Y Update.cia");
    let u = target_parts(update);
    assert!(u.starts_with(&folder), "{u:?} not under {folder:?}");
    assert!(update.notes.contains(&ItemNote::InstallFirst {
        emulator: "Azahar".into()
    }));
    // Without game folders nothing is renamed.
    let plan = plan_in(&pile, &place_options("esde", &lib));
    let t = target_parts(item(&plan, "Pokemon Y (World).cia"));
    assert_eq!(t.len(), 2, "{t:?}");
}
