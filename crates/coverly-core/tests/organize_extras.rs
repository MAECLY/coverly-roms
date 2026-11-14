//! Official extras and tagged music rips in Organizar (extras spec D5, D6,
//! D7, D15): the plan files them into `<Game>/Extras/<Category>/`, or
//! leaves them where they are with why.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use coverly_core::addons::LinkVia;
use coverly_core::cache::Cache;
use coverly_core::consoles::testing as ct;
use coverly_core::consoles::{start_open_log, take_open_log};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::extras::testing as xt;
use coverly_core::extras::{ExtraFormat, ExtrasCategory, Provenance};
use coverly_core::identify::sniff::NotARom;
use coverly_core::organize::plan::GameRole;
use coverly_core::organize::{
    plan_organize, ItemNote, LeftReason, MetaProbe, OrganizeAction, OrganizeEnv, OrganizeItem,
    OrganizeOptions, OrganizePlan,
};
use coverly_core::profiles::builtin_profiles;
use coverly_core::settings::OrganizeDedupeMode;
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

fn options(layout: &str, target: &Path) -> OrganizeOptions {
    OrganizeOptions {
        layout: layout.to_string(),
        target: Some(target.to_path_buf()),
        flat: true,
        offline: true,
        duplicates: OrganizeDedupeMode::Off,
        ..OrganizeOptions::default()
    }
}

fn plan_with(pile: &Path, opts: &OrganizeOptions, cache: &Cache) -> OrganizePlan {
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
    plan_organize(pile, opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap()
}

fn plan_of(pile: &Path, opts: &OrganizeOptions) -> OrganizePlan {
    plan_with(pile, opts, &Cache::in_memory().unwrap())
}

fn item<'a>(plan: &'a OrganizePlan, name: &str) -> &'a OrganizeItem {
    plan.items
        .iter()
        .find(|i| i.sources[0].file_name().is_some_and(|n| n == name))
        .unwrap_or_else(|| panic!("no item {name}: {:#?}", plan.items))
}

/// `rel`'s components as strings (never slash text: Windows CI).
fn parts(rel: &Path) -> Vec<String> {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect()
}

fn target_parts(it: &OrganizeItem) -> Vec<String> {
    parts(
        it.target
            .as_deref()
            .unwrap_or_else(|| panic!("{:?} has no target: {:?}", it.sources, it.action)),
    )
}

fn p(s: &[&str]) -> Vec<String> {
    s.iter().map(|c| c.to_string()).collect()
}

fn left(it: &OrganizeItem) -> Option<&LeftReason> {
    match &it.action {
        OrganizeAction::Left { reason } => Some(reason),
        _ => None,
    }
}

fn ps3_game(content_id: &str, tail: u8) -> Vec<u8> {
    let mut b = ct::pkg(1, content_id).head_bytes();
    b.extend([tail; 16]);
    b
}

/// The folder a layout gives `system` (as the plan of a pile holding
/// `name` with `bytes` names it).
fn system_folder(name: &str, bytes: &[u8]) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(&pile, name, bytes, 1);
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    item(&plan, name)
        .folder
        .clone()
        .unwrap_or_else(|| panic!("no folder for {name}: {:#?}", plan.items))
}

const RESISTANCE: &str = "UP9000-BCUS98107_00-RESISTANCE000001";

/// A PS3 disc folder (JB layout) `Resistance (USA)/` in `pile`.
fn ps3_disc_folder(pile: &Path) {
    let game = pile.join("Resistance (USA)");
    ct::ps3_jb(&game, "BCUS98107", "DG");
    pin_tree(&game);
}

fn pin_tree(dir: &Path) {
    for e in walkdir::WalkDir::new(dir) {
        let e = e.unwrap();
        if e.file_type().is_file() {
            pin_mtime(e.path(), 1);
        }
    }
}

#[test]
fn ps3_theme_pkg_of_its_game_is_planned_into_extras_themes() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    ps3_disc_folder(&pile);
    put(
        &pile,
        "Themes/Tidewalker Theme.pkg",
        &xt::pkg_with_type(1, 0x09, "UP9000-BCUS98107_00-THEME0000000001"),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "Resistance (USA)");
    let theme = item(&plan, "Tidewalker Theme.pkg");
    assert!(theme.moves_in(), "{theme:#?}");
    let game = theme.game.as_ref().unwrap();
    assert_eq!(game.role, GameRole::Extra);
    let link = game.extra.as_ref().unwrap();
    assert_eq!(link.category, ExtrasCategory::Themes);
    assert_eq!(link.format, ExtraFormat::Ps3ThemePkg);
    assert_eq!(link.provenance, Provenance::Official);
    assert_eq!(link.via, LinkVia::Key);
    assert_eq!(link.content_type, Some(0x09));
    let mut want = target_parts(base);
    assert_eq!(want.last().map(String::as_str), Some("Resistance (USA)"));
    want.extend(p(&["Extras", "Themes", "Tidewalker Theme.pkg"]));
    assert_eq!(target_parts(theme), want);
    assert!(theme.notes.contains(&ItemNote::ExtraOf {
        category: ExtrasCategory::Themes,
        of: "Resistance (USA)".into(),
        format: ExtraFormat::Ps3ThemePkg,
        content_type: Some(0x09),
    }));
}

#[test]
fn a_soundtrack_cia_filed_beside_its_game_goes_into_its_game_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Pokemon Y/Pokemon Y.cia",
        &ct::cia_with_code(0x0004_0000_0005_5D00, "CTR-P-EK2A", false, false).head_bytes(),
        1,
    );
    // Its own title id and a name no game has: linked by where it is.
    put(
        &pile,
        "Pokemon Y/Kalos Soundtrack.cia",
        &xt::cia_titled(0x0004_0000_0009_9900, None),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "Pokemon Y.cia");
    let ost = item(&plan, "Kalos Soundtrack.cia");
    assert!(ost.moves_in(), "{ost:#?}");
    let link = ost.game.as_ref().and_then(|g| g.extra.as_ref()).unwrap();
    assert_eq!(link.format, ExtraFormat::N3dsSoundtrackCia);
    assert_eq!(link.category, ExtrasCategory::Soundtrack);
    assert_eq!(link.via, LinkVia::Folder);
    let base_target = target_parts(base);
    let mut want = base_target[..base_target.len() - 1].to_vec();
    want.extend(p(&["Extras", "Soundtrack", "Kalos Soundtrack.cia"]));
    assert_eq!(target_parts(ost), want);
    // The same pile at its root: no folder of its own, no link.
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Pokemon Y.cia",
        &ct::cia_with_code(0x0004_0000_0005_5D00, "CTR-P-EK2A", false, false).head_bytes(),
        1,
    );
    put(
        &pile,
        "Kalos Soundtrack.cia",
        &xt::cia_titled(0x0004_0000_0009_9900, None),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    assert_eq!(
        left(item(&plan, "Kalos Soundtrack.cia")),
        Some(&LeftReason::ExtraWithoutGame {
            category: ExtrasCategory::Soundtrack
        })
    );
}

#[test]
fn x360_gamerpic_links_by_title_id_to_its_game() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let tid = 0x5841_0A00;
    let hash = ct::STFS_HASH;
    put(
        &pile,
        &format!("games/58410A00/000D0000/{hash}"),
        &ct::stfs_head(0x000D_0000, tid, 1, 0).head_bytes(),
        1,
    );
    put(
        &pile,
        "Gamerpics/FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        &xt::stfs_extra(0x0002_0000, tid),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "58410A00");
    let pic = item(&plan, "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF");
    assert!(pic.moves_in(), "{pic:#?}");
    let link = pic.game.as_ref().and_then(|g| g.extra.as_ref()).unwrap();
    assert_eq!(link.format, ExtraFormat::X360Gamerpic);
    assert_eq!(link.category, ExtrasCategory::Themes);
    assert_eq!(link.via, LinkVia::Key);
    let mut want = target_parts(base);
    want.extend(p(&[
        "Extras",
        "Themes",
        "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
    ]));
    assert_eq!(target_parts(pic), want);
    assert_eq!(pic.system_id.as_deref(), Some("xbox360"));
}

#[test]
fn vita_theme_links_by_title_key_of_its_name_only() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    ct::vita_app(&pile.join("Gravity Rush (Europe)"), "PCSF00011");
    // A game that carries the theme's own title id: never linked by it
    // (Vita themes carry their own id, D7).
    ct::vita_app(&pile.join("Tearaway (Europe)"), "PCSB00001");
    pin_tree(&pile.join("Gravity Rush (Europe)"));
    pin_tree(&pile.join("Tearaway (Europe)"));
    put(
        &pile,
        "Gravity Rush (USA).pkg",
        &xt::pkg_with_type(2, 0x1F, "EP0001-PCSB00001_00-THEME0000000001"),
        3,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let theme = item(&plan, "Gravity Rush (USA).pkg");
    assert!(theme.moves_in(), "{theme:#?}");
    let link = theme.game.as_ref().and_then(|g| g.extra.as_ref()).unwrap();
    assert_eq!(link.format, ExtraFormat::VitaThemePkg);
    assert_eq!(link.via, LinkVia::Name);
    let game = item(&plan, "Gravity Rush (Europe)");
    let mut want = target_parts(game);
    want.extend(p(&["Extras", "Themes", "Gravity Rush (USA).pkg"]));
    assert_eq!(target_parts(theme), want);
}

#[test]
fn ambiguous_or_unknown_game_is_left_extra_without_game() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Ape Escape (USA).pkg",
        &ps3_game("UP9000-BCUS00001_00-APEESCAPE0000001", 1),
        1,
    );
    put(
        &pile,
        "Ape Escape (Europe).pkg",
        &ps3_game("EP9000-BCES00002_00-APEESCAPE0000001", 2),
        2,
    );
    put(&pile, "Ape.p3t", &xt::p3t("Ape Escape"), 3);
    put(&pile, "Mystery.p3t", &xt::p3t("Nobody Knows"), 4);
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    for name in ["Ape.p3t", "Mystery.p3t"] {
        let it = item(&plan, name);
        assert_eq!(
            left(it),
            Some(&LeftReason::ExtraWithoutGame {
                category: ExtrasCategory::Themes
            }),
            "{name}: {it:#?}"
        );
        assert!(it.target.is_none());
    }
}

/// The SNES folder of the library layout, with `Super Metroid (Japan,
/// USA)/` as a game folder in it.
fn snes_library(lib: &Path) -> String {
    let snes = system_folder("Probe (USA).sfc", &vec![0x11u8; 1 << 16]);
    put(
        lib,
        &format!("{snes}/Super Metroid (Japan, USA)/Super Metroid (Japan, USA).sfc"),
        &vec![0x22u8; 1 << 16],
        1,
    );
    snes
}

#[test]
fn spc_with_a_matching_game_tag_goes_to_soundtrack_as_user_content() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let snes = snes_library(&lib);
    put(
        &pile,
        "track01.spc",
        &xt::spc("Boss", "Super Metroid", "Kenji Yamamoto", true),
        2,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let rip = item(&plan, "track01.spc");
    assert!(rip.moves_in(), "{rip:#?}");
    let game = rip.game.as_ref().unwrap();
    assert_eq!(game.role, GameRole::Extra);
    assert!(game.folder_exists);
    let link = game.extra.as_ref().unwrap();
    assert_eq!(link.provenance, Provenance::User);
    assert_eq!(link.category, ExtrasCategory::Soundtrack);
    assert_eq!(link.format, ExtraFormat::Spc);
    assert_eq!(
        link.rip.as_ref().and_then(|r| r.game.as_deref()),
        Some("Super Metroid")
    );
    assert_eq!(
        target_parts(rip),
        p(&[
            &snes,
            "Super Metroid (Japan, USA)",
            "Extras",
            "Soundtrack",
            "track01.spc"
        ])
    );
    assert!(rip.notes.contains(&ItemNote::ExtraRip {
        of: "Super Metroid (Japan, USA)".into(),
        format: ExtraFormat::Spc,
        tag: "Super Metroid".into(),
    }));
}

#[test]
fn spc_without_a_match_is_left_as_music() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    snes_library(&lib);
    put(
        &pile,
        "other.spc",
        &xt::spc("Theme", "Unknown Game", "Someone", true),
        2,
    );
    put(
        &pile,
        "untagged.spc",
        &xt::spc("Boss", "Super Metroid", "Kenji Yamamoto", false),
        3,
    );
    // A pile ROM has no game folder on a retro system: the rip stays.
    put(&pile, "Chrono Trigger (USA).sfc", &vec![0x33u8; 1 << 16], 4);
    put(
        &pile,
        "chrono.spc",
        &xt::spc("Theme", "Chrono Trigger", "Mitsuda", true),
        5,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    for name in ["other.spc", "untagged.spc", "chrono.spc"] {
        assert_eq!(
            left(item(&plan, name)),
            Some(&LeftReason::NotARom {
                what: NotARom::Music
            }),
            "{name}"
        );
    }
}

#[test]
fn a_3ds_cia_with_video_in_its_name_stays_a_game() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Video Poker (USA).cia",
        &xt::cia_titled(0x0004_0000_0001_2300, None),
        1,
    );
    put(
        &pile,
        "Zelda Original Soundtrack.cia",
        &xt::cia_titled(0x0004_0000_0001_2400, None),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let poker = item(&plan, "Video Poker (USA).cia");
    assert_eq!(poker.system_id.as_deref(), Some("3ds"));
    assert_ne!(poker.game.as_ref().map(|g| g.role), Some(GameRole::Extra));
    assert!(left(poker).is_none_or(|r| !matches!(r, LeftReason::ExtraWithoutGame { .. })));
    // A soundtrack word makes an extra; no game of it here.
    assert_eq!(
        left(item(&plan, "Zelda Original Soundtrack.cia")),
        Some(&LeftReason::ExtraWithoutGame {
            category: ExtrasCategory::Soundtrack
        })
    );
}

/// A library holding `Resistance (USA)/` with its PKG; returns the PS3
/// folder and the game folder (relative to `lib`).
fn ps3_library(lib: &Path) -> (String, PathBuf) {
    let ps3 = system_folder("Probe (USA).pkg", &ps3_game(RESISTANCE, 9));
    put(
        lib,
        &format!("{ps3}/Resistance (USA)/Resistance (USA).pkg"),
        &ps3_game(RESISTANCE, 1),
        1,
    );
    let folder = PathBuf::from(&ps3).join("Resistance (USA)");
    (ps3, folder)
}

#[test]
fn a_theme_pkg_already_in_a_library_dlc_folder_is_never_moved() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let (ps3, folder) = ps3_library(&lib);
    let filed = put(
        &lib,
        &format!("{ps3}/Resistance (USA)/DLC/old theme.pkg"),
        &xt::pkg_with_type(1, 0x09, "UP9000-BCUS98107_00-THEME0000000001"),
        2,
    );
    put(
        &pile,
        "new theme.pkg",
        &xt::pkg_with_type(1, 0x09, "UP9000-BCUS98107_00-THEME0000000002"),
        3,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    assert!(
        !plan.items.iter().any(|i| lib.join(&i.sources[0]) == filed
            || i.sources[0]
                .file_name()
                .is_some_and(|n| n == "old theme.pkg")),
        "{:#?}",
        plan.items
    );
    let new = item(&plan, "new theme.pkg");
    assert!(new.moves_in(), "{new:#?}");
    let mut want = parts(&folder);
    want.extend(p(&["Extras", "Themes", "new theme.pkg"]));
    assert_eq!(target_parts(new), want);
    assert_eq!(
        new.game
            .as_ref()
            .and_then(|g| g.extra.as_ref())
            .map(|l| l.via),
        Some(LinkVia::Key)
    );
}

#[test]
fn a_newer_extras_json_leaves_the_extra_in_the_pile() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let (_, folder) = ps3_library(&lib);
    put(
        &lib.join(&folder),
        "Extras/extras.json",
        br#"{"version": 2, "files": []}"#,
        2,
    );
    put(
        &pile,
        "theme.pkg",
        &xt::pkg_with_type(1, 0x09, "UP9000-BCUS98107_00-THEME0000000001"),
        3,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let theme = item(&plan, "theme.pkg");
    assert_eq!(
        left(theme),
        Some(&LeftReason::ExtrasIndexNewer),
        "{theme:#?}"
    );
    assert!(theme.target.is_none());
}

#[test]
fn a_malformed_extras_json_plans_with_a_backup_note() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let (_, folder) = ps3_library(&lib);
    put(&lib.join(&folder), "Extras/extras.json", b"{not json", 2);
    put(
        &pile,
        "theme.pkg",
        &xt::pkg_with_type(1, 0x09, "UP9000-BCUS98107_00-THEME0000000001"),
        3,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let theme = item(&plan, "theme.pkg");
    assert!(theme.moves_in(), "{theme:#?}");
    assert!(theme.notes.contains(&ItemNote::ExtrasBackupFirst));
    let mut want = parts(&folder);
    want.extend(p(&["Extras", "Themes", "theme.pkg"]));
    assert_eq!(target_parts(theme), want);
}

#[test]
fn a_name_taken_in_the_category_gets_a_number() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let (_, folder) = ps3_library(&lib);
    put(&lib.join(&folder), "Extras/Themes/theme.pkg", b"other", 2);
    put(
        &pile,
        "theme.pkg",
        &xt::pkg_with_type(1, 0x09, "UP9000-BCUS98107_00-THEME0000000001"),
        3,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let theme = item(&plan, "theme.pkg");
    let mut want = parts(&folder);
    want.extend(p(&["Extras", "Themes", "theme (2).pkg"]));
    assert_eq!(target_parts(theme), want);
}

#[test]
fn key_material_is_never_opened_by_the_extras_hook() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let keys = ["x.rap", "act.dat", "kv.bin", "prod.keys", "title.tik"];
    for (n, name) in keys.iter().enumerate() {
        put(&pile, name, &[0x5Au8; 0x2000], n as u64 + 1);
    }
    put(&pile, "theme.p3t", &xt::p3t("Tidewalker"), 9);
    fs::create_dir_all(&lib).unwrap();
    let _turn = log_turn();
    start_open_log();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let opened = take_open_log();
    for name in keys {
        assert!(
            !opened
                .iter()
                .any(|p| p.file_name().is_some_and(|n| n == name)),
            "{name} opened: {opened:?}"
        );
    }
    assert_eq!(
        left(item(&plan, "theme.p3t")),
        Some(&LeftReason::ExtraWithoutGame {
            category: ExtrasCategory::Themes
        })
    );
}

#[test]
fn disc_game_folders_are_never_split_into_extras() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let game = pile.join("Resistance (USA)");
    ct::ps3_jb(&game, "BCUS98107", "DG");
    ct::put_file(&game, "PS3_EXTRA/theme.p3t", &xt::p3t("Tidewalker"));
    pin_tree(&game);
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    assert!(
        !plan
            .items
            .iter()
            .any(|i| i.sources[0].file_name().is_some_and(|n| n == "theme.p3t")),
        "{:#?}",
        plan.items
    );
    assert!(plan
        .items
        .iter()
        .all(|i| i.game.as_ref().map(|g| g.role) != Some(GameRole::Extra)));
}

#[test]
fn without_game_folders_extras_are_left_as_today() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    ps3_disc_folder(&pile);
    put(
        &pile,
        "theme.pkg",
        &xt::pkg_with_type(1, 0x09, "UP9000-BCUS98107_00-THEME0000000001"),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let off = OrganizeOptions {
        game_folders: Some(false),
        ..options("coverly-library", &lib)
    };
    for opts in [off, options("anbernic-stock", &lib)] {
        let plan = plan_of(&pile, &opts);
        let theme = item(&plan, "theme.pkg");
        assert!(!theme.moves_in(), "{}: {theme:#?}", opts.layout);
        assert!(theme.target.is_none());
        assert_ne!(theme.game.as_ref().map(|g| g.role), Some(GameRole::Extra));
    }
}

#[test]
fn device_translators_still_skip_extras() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    ps3_disc_folder(&pile);
    put(
        &pile,
        "theme.pkg",
        &xt::pkg_with_type(1, 0x09, "UP9000-BCUS98107_00-THEME0000000001"),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let on = OrganizeOptions {
        game_folders: Some(true),
        ..options("esde", &lib)
    };
    let plan = plan_of(&pile, &on);
    let theme = item(&plan, "theme.pkg");
    assert!(theme.moves_in(), "{theme:#?}");
    let t = target_parts(theme);
    assert_eq!(
        t[t.len() - 3..].to_vec(),
        p(&["Extras", "Themes", "theme.pkg"])
    );
    let json = serde_json::to_string(&plan.translated).unwrap();
    assert!(!json.contains("theme.pkg"), "{json}");
}

#[test]
fn facts_cache_from_an_earlier_run_still_finds_the_extra() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    ps3_disc_folder(&pile);
    put(
        &pile,
        "theme.pkg",
        &xt::pkg_with_type(1, 0x09, "UP9000-BCUS98107_00-THEME0000000001"),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let cache = Cache::in_memory().unwrap();
    let opts = options("coverly-library", &lib);
    let first = plan_with(&pile, &opts, &cache);
    let second = plan_with(&pile, &opts, &cache);
    for plan in [first, second] {
        let theme = item(&plan, "theme.pkg");
        assert_eq!(
            theme.game.as_ref().map(|g| g.role),
            Some(GameRole::Extra),
            "{theme:#?}"
        );
    }
}

#[test]
fn a_library_organised_in_place_never_moves_what_its_game_folders_hold() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("lib");
    let (ps3, _) = ps3_library(&lib);
    put(
        &lib,
        &format!("{ps3}/Resistance (USA)/DLC/old theme.pkg"),
        &xt::pkg_with_type(1, 0x09, "UP9000-BCUS98107_00-THEME0000000001"),
        2,
    );
    put(
        &lib,
        &format!("{ps3}/Resistance (USA)/Extras/Themes/placed.p3t"),
        &xt::p3t("Resistance"),
        3,
    );
    let plan = plan_of(&lib, &options("coverly-library", &lib));
    for name in ["old theme.pkg", "placed.p3t"] {
        for it in plan
            .items
            .iter()
            .filter(|i| i.sources[0].file_name().is_some_and(|n| n == name))
        {
            assert!(!it.moves_in(), "{name}: {it:#?}");
        }
    }
}

#[test]
fn a_rip_beside_another_game_links_only_by_its_tag() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let snes = snes_library(&lib);
    // The rip's tag names a game of the library; beside it sits another
    // game's file of the same stem (where it sits must not win over the
    // tag: rips link by their tag only, D15).
    put(&pile, "Chrono Trigger (USA).sfc", &vec![0x33u8; 1 << 16], 4);
    put(
        &pile,
        "Chrono Trigger (USA).spc",
        &xt::spc("Boss", "Super Metroid", "Kenji Yamamoto", true),
        5,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let rip = item(&plan, "Chrono Trigger (USA).spc");
    let link = rip.game.as_ref().and_then(|g| g.extra.as_ref());
    assert!(rip.moves_in(), "{rip:#?}");
    assert_eq!(link.map(|l| l.via), Some(LinkVia::Name), "{rip:#?}");
    assert_eq!(
        target_parts(rip),
        p(&[
            &snes,
            "Super Metroid (Japan, USA)",
            "Extras",
            "Soundtrack",
            "Chrono Trigger (USA).spc"
        ])
    );
}

/// D7 (1) names a pile game folder's `Extras/`, but part A leaves that
/// folder whole as game folder content: an official extra in it is never
/// split from the user's own files there (the safe default; owner note).
#[test]
fn a_pile_game_folders_own_extras_folder_stays_whole() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Pokemon Y/Pokemon Y.cia",
        &ct::cia_with_code(0x0004_0000_0005_5D00, "CTR-P-EK2A", false, false).head_bytes(),
        1,
    );
    put(
        &pile,
        "Pokemon Y/Extras/Soundtrack/Kalos Soundtrack.cia",
        &xt::cia_titled(0x0004_0000_0009_9900, None),
        2,
    );
    put(&pile, "Pokemon Y/Extras/Manuals/notes.pdf", b"%PDF-1.4", 3);
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    assert!(item(&plan, "Pokemon Y.cia").moves_in());
    let extras = item(&plan, "Extras");
    assert_eq!(parts(&extras.sources[0]), p(&["Pokemon Y", "Extras"]));
    assert!(
        matches!(left(extras), Some(LeftReason::GameFolderContent { .. })),
        "{extras:#?}"
    );
    assert!(plan
        .items
        .iter()
        .all(|i| i.game.as_ref().map(|g| g.role) != Some(GameRole::Extra)));
}

// --- Apply writes `extras.json` in the organise journal (extras spec D3,
// D5, D10) ---

mod apply {
    use super::*;

    use coverly_core::extras::index::list;
    use coverly_core::extras::ops::{set_tags, ExtrasCtx};
    use coverly_core::organize::apply::user_available;
    use coverly_core::organize::{apply_organize, ApplyEnv, OrganizeReport};
    use coverly_core::write::trash::FakeTrash;
    use coverly_core::write::{later_jobs, same_volume, undo_job, undo_jobs_in_order, JobKind};
    use serde_json::Value;

    /// Pile and library under one resolved temp folder (journal roots are
    /// compared as paths, and macOS temp folders sit behind a link).
    struct Lib {
        _tmp: tempfile::TempDir,
        data: tempfile::TempDir,
        pile: PathBuf,
        lib: PathBuf,
        /// The game folder, relative to `lib`.
        folder: PathBuf,
    }

    impl Lib {
        fn new() -> Lib {
            let tmp = tempfile::tempdir().unwrap();
            let base = tmp.path().canonicalize().unwrap();
            let (pile, lib) = (base.join("pile"), base.join("lib"));
            let (_, folder) = ps3_library(&lib);
            put(
                &pile,
                "theme.pkg",
                &xt::pkg_with_type(1, 0x09, "UP9000-BCUS98107_00-THEME0000000001"),
                3,
            );
            Lib {
                _tmp: tmp,
                data: tempfile::tempdir().unwrap(),
                pile,
                lib,
                folder,
            }
        }

        fn game(&self) -> PathBuf {
            self.lib.join(&self.folder)
        }

        fn extras(&self) -> PathBuf {
            self.game().join("Extras")
        }

        fn plan(&self) -> OrganizePlan {
            plan_of(&self.pile, &options("coverly-library", &self.lib))
        }

        fn apply(&self) -> OrganizeReport {
            let plan = self.plan();
            assert!(item(&plan, "theme.pkg").moves_in(), "{plan:#?}");
            apply_organize(
                &plan,
                &ApplyEnv {
                    data_dir: self.data.path(),
                    progress: &|_| {},
                    cancel: &AtomicBool::new(false),
                    trash: &FakeTrash::unavailable("no-trash"),
                    tools: Some(&[]),
                },
            )
            .unwrap()
        }

        fn index(&self) -> Value {
            serde_json::from_slice(&fs::read(self.extras().join("extras.json")).unwrap()).unwrap()
        }

        /// `(op kind, target file name)` of every journal entry of `job`.
        fn ops(&self, job: &str) -> Vec<(String, String)> {
            let journal = self.data.path().join("jobs").join(format!("{job}.jsonl"));
            fs::read_to_string(journal)
                .unwrap()
                .lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .filter_map(|l| {
                    let kind = l["op"]["kind"].as_str()?.to_string();
                    let target = PathBuf::from(l["target"].as_str()?);
                    let name = target.file_name()?.to_string_lossy().into_owned();
                    Some((kind, name))
                })
                .collect()
        }
    }

    fn record<'a>(index: &'a Value, path: &str) -> &'a Value {
        index["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["path"] == path)
            .unwrap_or_else(|| panic!("no record {path}: {index:#}"))
    }

    fn has(ops: &[(String, String)], kind: &str, name: &str) -> bool {
        ops.iter().any(|(k, n)| k == kind && n == name)
    }

    #[test]
    fn apply_places_the_extra_and_writes_the_index_in_the_same_journal() {
        let fx = Lib::new();
        let report = fx.apply();
        assert_eq!(report.moved, 1, "{report:#?}");
        let placed = fx.extras().join("Themes").join("theme.pkg");
        assert!(placed.is_file());
        let job = report.job.clone().unwrap();
        let ops = fx.ops(&job);
        assert!(has(&ops, "dir-created", "Extras"), "{ops:?}");
        assert!(has(&ops, "dir-created", "Themes"), "{ops:?}");
        assert!(has(&ops, "moved", "theme.pkg"), "{ops:?}");
        assert!(has(&ops, "created", "extras.json"), "{ops:?}");
        // The index is the job's last write, and only one.
        assert_eq!(
            ops.last().map(|(k, n)| (k.as_str(), n.as_str())),
            Some(("created", "extras.json"))
        );
        assert_eq!(ops.iter().filter(|(_, n)| n == "extras.json").count(), 1);
        let index = fx.index();
        assert_eq!(index["version"], 1);
        let r = record(&index, "Themes/theme.pkg");
        assert_eq!(r["provenance"], "official");
        assert_eq!(r["type"], "other");
        assert_eq!(r["origin"]["how"], "organize");
        assert_eq!(r["origin"]["format"], "ps3-theme-pkg");
        assert_eq!(r["origin"]["from_name"], "theme.pkg");
        assert_eq!(r["tags"], serde_json::json!([]));
        assert!(report.extras_saved.is_empty());
        assert!(report.extras_unwritten.is_empty());
        // The listing reads it back as the file's own record.
        let listing = list(&fx.extras());
        assert_eq!(listing.files.len(), 1);
        assert_eq!(
            listing.files[0].matched,
            coverly_core::extras::index::Match::Path
        );
    }

    #[test]
    fn a_tagged_rip_is_recorded_as_the_users_with_its_tags() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let (pile, lib) = (base.join("pile"), base.join("lib"));
        let snes = snes_library(&lib);
        put(
            &pile,
            "track01.spc",
            &xt::spc("Boss", "Super Metroid", "Kenji Yamamoto", true),
            2,
        );
        let data = tempfile::tempdir().unwrap();
        let plan = plan_of(&pile, &options("coverly-library", &lib));
        let report = apply_organize(
            &plan,
            &ApplyEnv {
                data_dir: data.path(),
                progress: &|_| {},
                cancel: &AtomicBool::new(false),
                trash: &FakeTrash::unavailable("no-trash"),
                tools: Some(&[]),
            },
        )
        .unwrap();
        assert_eq!(report.moved, 1, "{report:#?}");
        let extras = lib
            .join(&snes)
            .join("Super Metroid (Japan, USA)")
            .join("Extras");
        let index: Value =
            serde_json::from_slice(&fs::read(extras.join("extras.json")).unwrap()).unwrap();
        let r = record(&index, "Soundtrack/track01.spc");
        assert_eq!(r["provenance"], "user");
        assert_eq!(r["origin"]["how"], "organize");
        assert_eq!(r["origin"]["format"], "spc");
        assert_eq!(r["origin"]["rip"]["game"], "Super Metroid");
        assert_eq!(r["origin"]["rip"]["artist"], "Kenji Yamamoto");
        // A rip's tags are offered, never added without the user (D15).
        assert_eq!(r["tags"], serde_json::json!([]));
    }

    #[test]
    fn apply_merges_into_an_existing_index_with_backup() {
        let fx = Lib::new();
        put(&fx.extras(), "Soundtrack/a.flac", b"fLaC synthetic", 4);
        let before = serde_json::json!({
            "version": 1,
            "categories": {},
            "files": [{
                "path": "Soundtrack/a.flac",
                "type": "audio",
                "provenance": "user",
                "origin": { "how": "added" },
                "tags": ["tema del jefe final"],
                "size": 14,
                "mtime_ns": 0,
                "added_at": "2026-11-01T10:00:00Z",
                "kept": "from a newer build"
            }],
            "future": true
        });
        let before = serde_json::to_vec_pretty(&before).unwrap();
        put(&fx.extras(), "extras.json", &before, 5);
        let report = fx.apply();
        let job = report.job.clone().unwrap();
        let ops = fx.ops(&job);
        assert!(has(&ops, "replaced", "extras.json"), "{ops:?}");
        assert!(!has(&ops, "created", "extras.json"), "{ops:?}");
        let index = fx.index();
        assert_eq!(index["future"], true);
        let a = record(&index, "Soundtrack/a.flac");
        assert_eq!(a["tags"], serde_json::json!(["tema del jefe final"]));
        assert_eq!(a["origin"]["how"], "added");
        assert_eq!(a["kept"], "from a newer build");
        assert_eq!(record(&index, "Themes/theme.pkg")["provenance"], "official");
        // Undo brings the previous file back byte for byte.
        undo_job(fx.data.path(), &job).unwrap();
        assert_eq!(fs::read(fx.extras().join("extras.json")).unwrap(), before);
        assert!(!fx.extras().join("Themes").exists());
    }

    #[test]
    fn apply_backs_up_a_malformed_index_first() {
        let fx = Lib::new();
        put(&fx.extras(), "extras.json", b"{not json", 4);
        let plan = fx.plan();
        assert!(item(&plan, "theme.pkg")
            .notes
            .contains(&ItemNote::ExtrasBackupFirst));
        let report = fx.apply();
        assert_eq!(report.extras_saved.len(), 1, "{report:#?}");
        let saved = report.extras_saved[0].clone();
        assert_eq!(saved.parent(), Some(fx.extras().as_path()));
        let name = saved.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("extras.broken-") && name.ends_with(".json"),
            "{name}"
        );
        assert_eq!(fs::read(&saved).unwrap(), b"{not json");
        let index = fx.index();
        assert_eq!(record(&index, "Themes/theme.pkg")["provenance"], "official");
        let job = report.job.clone().unwrap();
        let ops = fx.ops(&job);
        assert!(has(&ops, "created", &name), "{ops:?}");
        assert!(has(&ops, "replaced", "extras.json"), "{ops:?}");
        undo_job(fx.data.path(), &job).unwrap();
        assert_eq!(
            fs::read(fx.extras().join("extras.json")).unwrap(),
            b"{not json"
        );
        assert!(!saved.exists());
    }

    #[test]
    fn a_newer_index_at_apply_time_keeps_the_extra_in_the_pile() {
        let fx = Lib::new();
        let plan = fx.plan();
        // Written by a newer build between the dry run and apply.
        put(
            &fx.extras(),
            "extras.json",
            br#"{"version": 2, "files": []}"#,
            4,
        );
        let report = apply_organize(
            &plan,
            &ApplyEnv {
                data_dir: fx.data.path(),
                progress: &|_| {},
                cancel: &AtomicBool::new(false),
                trash: &FakeTrash::unavailable("no-trash"),
                tools: Some(&[]),
            },
        )
        .unwrap();
        assert_eq!(report.moved, 0, "{report:#?}");
        assert!(fx.pile.join("theme.pkg").is_file());
        assert!(!fx.extras().join("Themes").exists());
        assert_eq!(
            fs::read(fx.extras().join("extras.json")).unwrap(),
            br#"{"version": 2, "files": []}"#
        );
    }

    #[test]
    fn undo_removes_the_extra_the_index_and_empty_extras() {
        let fx = Lib::new();
        let report = fx.apply();
        let job = report.job.clone().unwrap();
        let undo = undo_job(fx.data.path(), &job).unwrap();
        assert!(undo.skipped_modified.is_empty(), "{undo:#?}");
        assert!(fx.pile.join("theme.pkg").is_file());
        assert!(!fx.extras().exists(), "Extras/ stays: {undo:#?}");
        assert!(fx.game().join("Resistance (USA).pkg").is_file());
    }

    fn tag_it(fx: &Lib) -> String {
        let trash = FakeTrash::unavailable("no-trash");
        let game = fx.game();
        let ctx = ExtrasCtx {
            library_root: &fx.lib,
            game_dir: &game,
            data_dir: fx.data.path(),
            trash: &trash,
            now: now(),
            same_volume,
            available: user_available,
        };
        let listing = list(&fx.extras());
        let out = set_tags(&ctx, &listing, 0, &["tema del jefe final".to_string()]).unwrap();
        out.job.unwrap()
    }

    #[test]
    fn undo_keeps_an_index_a_later_extras_job_changed() {
        let fx = Lib::new();
        let organise = fx.apply().job.unwrap();
        let tags = tag_it(&fx);
        let later = later_jobs(fx.data.path(), &organise).unwrap();
        assert_eq!(later.len(), 1, "{later:#?}");
        assert_eq!(later[0].job, tags);
        assert_eq!(later[0].kind, JobKind::Extras);
        // Undone alone, the organise job keeps the json the user changed,
        // and with it Extras/.
        let undo = undo_job(fx.data.path(), &organise).unwrap();
        let json = fx.extras().join("extras.json");
        assert!(json.is_file());
        assert!(undo.skipped_modified.contains(&json), "{undo:#?}");
        assert!(fx.pile.join("theme.pkg").is_file());
        assert!(!fx.extras().join("Themes").exists());
        assert_eq!(
            record(&fx.index(), "Themes/theme.pkg")["tags"],
            serde_json::json!(["tema del jefe final"])
        );
    }

    #[test]
    fn undoing_the_later_extras_job_first_undoes_both_cleanly() {
        let fx = Lib::new();
        let organise = fx.apply().job.unwrap();
        let tags = tag_it(&fx);
        let reports =
            undo_jobs_in_order(fx.data.path(), &[organise.clone(), tags.clone()]).unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].job, tags);
        for r in &reports {
            assert!(r.skipped_modified.is_empty(), "{r:#?}");
        }
        assert!(fx.pile.join("theme.pkg").is_file());
        assert!(!fx.extras().exists());
    }

    #[test]
    fn key_material_never_opened_on_apply_or_undo() {
        let fx = Lib::new();
        let keys = ["x.rap", "title.tik", "prod.keys", "kv.bin"];
        for (n, name) in keys.iter().enumerate() {
            put(&fx.pile, name, &[0x5Au8; 0x2000], 10 + n as u64);
        }
        // Key material already inside the game's Extras/ (listed, never read).
        put(&fx.extras(), "Guides/act.dat", &[0x5Au8; 0x2000], 20);
        let plan = fx.plan();
        let _turn = log_turn();
        start_open_log();
        let report = apply_organize(
            &plan,
            &ApplyEnv {
                data_dir: fx.data.path(),
                progress: &|_| {},
                cancel: &AtomicBool::new(false),
                trash: &FakeTrash::unavailable("no-trash"),
                tools: Some(&[]),
            },
        )
        .unwrap();
        undo_job(fx.data.path(), report.job.as_deref().unwrap()).unwrap();
        let opened = take_open_log();
        for name in keys.iter().chain(&["act.dat"]) {
            assert!(
                !opened
                    .iter()
                    .any(|p| p.file_name().is_some_and(|n| n == *name)),
                "{name} opened: {opened:?}"
            );
        }
        assert!(fx.extras().join("Guides").join("act.dat").is_file());
    }
}
