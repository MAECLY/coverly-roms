//! The game as a folder in Organizar (game folders spec D7, §8.1): the
//! survey respects a pile's fixed folders whatever the layout.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use coverly_core::cache::Cache;
use coverly_core::consoles::testing as ct;
use coverly_core::consoles::TitleKind;
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::game_folder::{ChildRole, FixedFolder};
use coverly_core::organize::plan::{effective_kind, Survey, SurveyEntry};
use coverly_core::organize::{survey, LeftReason, MetaProbe, OrganizeEnv, OrganizeOptions};
use coverly_core::profiles::builtin_profiles;
use coverly_core::systems::SystemDb;

struct NoCloud;

impl MetaProbe for NoCloud {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }
}

fn put(root: &Path, rel: &str, bytes: &[u8], n: u64) -> PathBuf {
    let p = rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c));
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    pin_mtime(&p, n);
    p
}

const BASE: u64 = 0x0004_0000_0005_5E00;
const UPDATE: u64 = 0x0004_000E_0005_5E00;
const DLC: u64 = 0x0004_008C_0005_5E00;

/// `Pokemon Y/` with its base, an update in `Updates/`, a DLC and an
/// update header misfiled in `DLC/`, a file no header names in `DLC/`,
/// and `Mods/`, `Saves/`, `Extras/` content.
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
        (
            "Pokemon Y/DLC/d.cia",
            ct::cia(DLC, true, false).head_bytes(),
        ),
        (
            "Pokemon Y/DLC/misfiled.cia",
            ct::cia(UPDATE + 1, true, false).head_bytes(),
        ),
        ("Pokemon Y/DLC/unknown.cia", vec![0x55u8; 4096]),
        ("Pokemon Y/Mods/m.ips", b"PATCH\0\0\0EOF".to_vec()),
        ("Pokemon Y/Saves/s.sav", vec![1u8; 512]),
        (
            "Pokemon Y/Extras/ost.cia",
            ct::cia(BASE, true, false).head_bytes(),
        ),
    ];
    for (n, (rel, bytes)) in files.iter().enumerate() {
        put(root, rel, bytes, n as u64);
    }
}

fn survey_of(dir: &Path, layout: &str) -> Survey {
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
        now: SystemTime::now(),
    };
    let opts = OrganizeOptions {
        layout: layout.to_string(),
        ..OrganizeOptions::default()
    };
    survey(dir, &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap()
}

fn entry<'a>(s: &'a Survey, name: &str) -> &'a SurveyEntry {
    s.entries
        .iter()
        .find(|e| e.sources[0].file_name().is_some_and(|n| n == name))
        .unwrap_or_else(|| panic!("no entry {name}"))
}

fn has_entry_below(s: &Survey, folder: &str) -> bool {
    s.entries.iter().any(|e| {
        let p = &e.sources[0];
        p.file_name().is_some_and(|n| n != folder)
            && p.components().any(|c| c.as_os_str() == folder)
    })
}

#[test]
fn a_pile_game_folder_gives_roles_and_leaves_its_content() {
    // The walk ignores the layout and every option: the same with a
    // library profile and a handheld one (D1, D7).
    for layout in ["esde", "muos"] {
        let tmp = tempfile::tempdir().unwrap();
        pokemon_pile(tmp.path());
        let s = survey_of(tmp.path(), layout);
        let game = tmp.path().join("Pokemon Y");

        let base = entry(&s, "Pokemon Y.cia");
        assert_eq!(base.role_hint, None, "{layout}");
        assert_eq!(effective_kind(base), TitleKind::Base, "{layout}");

        let update = entry(&s, "u.cia");
        assert_eq!(update.role_hint, Some(ChildRole::Update), "{layout}");
        assert_eq!(update.game_dir.as_deref(), Some(game.as_path()), "{layout}");
        assert_eq!(effective_kind(update), TitleKind::Update, "{layout}");

        let dlc = entry(&s, "d.cia");
        assert_eq!(dlc.role_hint, Some(ChildRole::Dlc), "{layout}");
        assert_eq!(effective_kind(dlc), TitleKind::Dlc, "{layout}");

        // The header wins over the folder.
        let misfiled = entry(&s, "misfiled.cia");
        assert_eq!(misfiled.role_hint, Some(ChildRole::Dlc), "{layout}");
        assert_eq!(effective_kind(misfiled), TitleKind::Update, "{layout}");

        // No header kind: the folder's role.
        let unknown = entry(&s, "unknown.cia");
        assert_ne!(
            unknown.console.as_ref().map(|c| c.kind),
            Some(TitleKind::Dlc),
            "{layout}"
        );
        assert_eq!(unknown.role_hint, Some(ChildRole::Dlc), "{layout}");
        assert_eq!(effective_kind(unknown), TitleKind::Dlc, "{layout}");

        let mut rows: Vec<(String, FixedFolder, String)> = s
            .entries
            .iter()
            .filter_map(|e| match &e.left {
                Some(LeftReason::GameFolderContent { folder, of }) => Some((
                    e.sources[0]
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    *folder,
                    of.clone(),
                )),
                _ => None,
            })
            .collect();
        rows.sort();
        assert_eq!(
            rows,
            vec![
                ("Extras".into(), FixedFolder::Extras, "Pokemon Y".into()),
                ("Saves".into(), FixedFolder::Saves, "Pokemon Y".into()),
                // A patch filed in `Mods/` is an addon of its game (mods
                // spec D13), left with its folder's row until linked.
                ("m.ips".into(), FixedFolder::Mods, "Pokemon Y".into()),
            ],
            "{layout}"
        );
        for folder in ["Saves", "Extras"] {
            assert!(!has_entry_below(&s, folder), "{layout}: inside {folder}");
        }
    }
}

#[test]
fn a_folder_of_several_games_gives_roles_without_a_game() {
    let tmp = tempfile::tempdir().unwrap();
    put(
        tmp.path(),
        "Mix/a.cia",
        &ct::cia(BASE, true, false).head_bytes(),
        1,
    );
    put(
        tmp.path(),
        "Mix/b.cia",
        &ct::cia(BASE + 0x100, true, false).head_bytes(),
        2,
    );
    put(tmp.path(), "Mix/DLC/x.cia", &[0x55u8; 4096], 3);
    put(tmp.path(), "Mix/Mods/y.cia", &[0x66u8; 4096], 4);
    let s = survey_of(tmp.path(), "esde");
    let x = entry(&s, "x.cia");
    assert_eq!(x.role_hint, Some(ChildRole::Dlc));
    assert_eq!(x.game_dir, None);
    // Not a game folder: `Mods/` is walked as today.
    assert!(has_entry_below(&s, "Mods"));
    assert!(s
        .entries
        .iter()
        .all(|e| !matches!(e.left, Some(LeftReason::GameFolderContent { .. }))));
}

// --- Planning bases, updates and DLC into game folders (Task 10) ---

use coverly_core::organize::plan::{GameRole, RenameReason};
use coverly_core::organize::{plan_organize, OrganizeAction, OrganizeItem, OrganizePlan};
use coverly_core::settings::OrganizeDedupeMode;

const PY_BASE: u64 = 0x0004_0000_0005_5D00;
const PY_UPDATE: u64 = 0x0004_000E_0005_5D00;
const PY_DLC: u64 = 0x0004_008C_0005_5D00;

/// Header bytes of `s` plus `tail` distinct bytes, so two files with one
/// header are not the same content.
fn with_tail(s: &ct::SparseAt, tail: u8) -> Vec<u8> {
    let mut b = s.head_bytes();
    b.extend([tail; 16]);
    b
}

/// A 3DS CIA update of `tid` with TMD title version `version`.
fn cia_update(tid: u64, version: u16) -> ct::SparseAt {
    let mut s = ct::cia(tid, true, false);
    s.put(ct::CIA_TMD + 0x1DC, &version.to_be_bytes());
    s
}

/// Pokemon Y (`CTR-P-EK2A`, region A = World), its update (TMD 1040)
/// and two DLC files, flat in `pile`.
fn pokemon_flat(pile: &Path) {
    let files: Vec<(&str, Vec<u8>)> = vec![
        (
            "Pokemon Y.cia",
            ct::cia_with_code(PY_BASE, "CTR-P-EK2A", false, false).head_bytes(),
        ),
        (
            "0004000E00055D00.cia",
            with_tail(&cia_update(PY_UPDATE, 1040), 1),
        ),
        (
            "Pokemon Y - Extra Pack (USA).cia",
            with_tail(&ct::cia(PY_DLC, true, false), 2),
        ),
        (
            "Pokemon Y [DLC2].cia",
            with_tail(&ct::cia(PY_DLC, true, false), 3),
        ),
    ];
    for (n, (rel, bytes)) in files.iter().enumerate() {
        put(pile, rel, bytes, n as u64);
    }
}

fn options(layout: &str, target: &Path) -> OrganizeOptions {
    OrganizeOptions {
        layout: layout.to_string(),
        target: Some(target.to_path_buf()),
        flat: true,
        duplicates: OrganizeDedupeMode::Off,
        ..OrganizeOptions::default()
    }
}

fn plan_of(pile: &Path, opts: &OrganizeOptions) -> OrganizePlan {
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
        now: SystemTime::now(),
    };
    plan_organize(pile, opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap()
}

fn item<'a>(plan: &'a OrganizePlan, name: &str) -> &'a OrganizeItem {
    plan.items
        .iter()
        .find(|i| i.sources[0].file_name().is_some_and(|n| n == name))
        .unwrap_or_else(|| panic!("no item {name}"))
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

/// The plan rows a snapshot compares: sources, action, target, rename
/// and game link, paths by component.
fn rows(plan: &OrganizePlan) -> serde_json::Value {
    let join = |p: &Path| parts(p).join("/");
    serde_json::Value::Array(
        plan.items
            .iter()
            .map(|it| {
                let mut v = serde_json::to_value(it).unwrap();
                let game = v.get("game").cloned();
                let mut game = game.unwrap_or(serde_json::Value::Null);
                if let Some(folder) = it.game.as_ref().and_then(|g| g.folder.as_deref()) {
                    game["folder"] = join(folder).into();
                }
                serde_json::json!({
                    "id": it.id,
                    "source": join(&it.sources[0]),
                    "action": v["action"].take(),
                    "reason": v.get_mut("reason").map(|r| r.take()),
                    "target": it.target.as_deref().map(join),
                    "folder": it.folder,
                    "rename": it.rename,
                    "game": game,
                })
            })
            .collect(),
    )
}

#[test]
fn bases_updates_and_dlc_go_into_their_game_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_flat(&pile);
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));

    let base = item(&plan, "Pokemon Y.cia");
    assert_eq!(base.action, OrganizeAction::Auto, "{base:?}");
    let game = "Pokemon Y (World)";
    assert_eq!(
        target_parts(base),
        p(&["3DS", game, "Pokemon Y (World).cia"])
    );
    assert_eq!(
        base.rename,
        Some(RenameReason::GameFolder {
            from: "Pokemon Y.cia".into()
        })
    );
    let link = base.game.as_ref().unwrap();
    assert_eq!(link.role, GameRole::Base);
    assert_eq!(parts(link.folder.as_deref().unwrap()), p(&["3DS", game]));
    assert!(!link.folder_exists);

    let update = item(&plan, "0004000E00055D00.cia");
    assert_eq!(
        target_parts(update),
        p(&["3DS", game, "Updates", "Pokemon Y (World) (v1.1.0).cia"])
    );
    let link = update.game.as_ref().unwrap();
    assert_eq!(
        (
            link.role,
            link.of,
            link.version.as_deref(),
            link.newest_here
        ),
        (GameRole::Update, Some(base.id), Some("1.1.0"), true)
    );
    assert_eq!(
        update.rename,
        Some(RenameReason::GameFolder {
            from: "0004000E00055D00.cia".into()
        })
    );

    let dlc = item(&plan, "Pokemon Y - Extra Pack (USA).cia");
    assert_eq!(
        target_parts(dlc),
        p(&["3DS", game, "DLC", "Pokemon Y (World) - Extra Pack.cia"])
    );
    assert_eq!(dlc.game.as_ref().unwrap().of, Some(base.id));
    let bare = item(&plan, "Pokemon Y [DLC2].cia");
    assert_eq!(
        target_parts(bare),
        p(&[
            "3DS",
            game,
            "DLC",
            "Pokemon Y (World) - DLC 0004008C00055D00.cia"
        ])
    );
    insta::assert_json_snapshot!("game_folders_3ds_plan", rows(&plan));
}

/// `parts` without the system folder (its spelling is the profile's).
fn below_system(it: &OrganizeItem) -> Vec<String> {
    target_parts(it)[1..].to_vec()
}

#[test]
fn the_option_off_or_a_handheld_profile_keeps_the_flat_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_flat(&pile);
    fs::create_dir_all(&lib).unwrap();
    let off = OrganizeOptions {
        game_folders: Some(false),
        ..options("coverly-library", &lib)
    };
    for opts in [off, options("esde", &lib), options("anbernic-stock", &lib)] {
        let plan = plan_of(&pile, &opts);
        for it in plan.items.iter().filter(|i| i.moves_in()) {
            assert_eq!(it.game, None, "{}: {:?}", opts.layout, it.sources);
            assert_eq!(
                target_parts(it).len(),
                2,
                "{}: {:?}",
                opts.layout,
                it.target
            );
            assert!(
                !matches!(it.rename, Some(RenameReason::GameFolder { .. })),
                "{}",
                opts.layout
            );
        }
        let base = item(&plan, "Pokemon Y.cia");
        assert_eq!(below_system(base), p(&["Pokemon Y.cia"]), "{}", opts.layout);
    }
    // The option on for a handheld profile through the run's override.
    let on = OrganizeOptions {
        game_folders: Some(true),
        ..options("esde", &lib)
    };
    let plan = plan_of(&pile, &on);
    assert_eq!(
        below_system(item(&plan, "0004000E00055D00.cia")),
        p(&[
            "Pokemon Y (World)",
            "Updates",
            "Pokemon Y (World) (v1.1.0).cia"
        ])
    );
}

#[test]
fn a_switch_update_keeps_its_raw_version_and_no_region() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let nca = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55u8; 64][..]);
    let tik = "01007EF00011E0000000000000000000.tik";
    put(
        &pile,
        "Zelda.nsp",
        &ct::nsp(&[nca, (tik, &[0x66; 0x2C0])]).bytes(),
        1,
    );
    put(
        &pile,
        "Zelda [01007EF00011E800][v131072].nsp",
        &ct::nsp(&[nca]).bytes(),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    assert_eq!(
        below_system(item(&plan, "Zelda.nsp")),
        p(&["Zelda", "Zelda.nsp"])
    );
    let update = item(&plan, "Zelda [01007EF00011E800][v131072].nsp");
    assert_eq!(
        below_system(update),
        p(&["Zelda", "Updates", "Zelda (v131072).nsp"])
    );
    assert_eq!(update.game.as_ref().unwrap().region, None);
}

#[test]
fn a_folder_game_is_its_own_game_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    ct::ps3_jb(&pile.join("Resistance"), "BLUS30001", "DG");
    ct::ps3_hdd(&pile.join("BLUS30001 Patch"), "BLUS30001", "GD");
    for f in walk_files(&pile) {
        pin_mtime(&f, 7);
    }
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "Resistance");
    assert_eq!(below_system(base), p(&["Resistance"]), "{base:?}");
    let link = base.game.as_ref().unwrap();
    assert_eq!(
        link.folder.as_deref().map(parts),
        Some(target_parts(base)),
        "a folder game's folder is itself"
    );
    let update = item(&plan, "BLUS30001 Patch");
    assert_eq!(
        below_system(update),
        p(&["Resistance", "Updates", "BLUS30001 Patch"]),
        "{update:?}"
    );
    assert_eq!(update.rename, None);
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_files(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn an_existing_game_folder_is_reused_and_a_file_in_its_place_is_a_collision() {
    // Reused with its own casing.
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_flat(&pile);
    fs::create_dir_all(lib.join("3DS").join("pokemon y (world)").join("updates")).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "Pokemon Y.cia");
    assert_eq!(
        below_system(base),
        p(&["pokemon y (world)", "pokemon y (world).cia"])
    );
    assert!(base.game.as_ref().unwrap().folder_exists);
    // The case-different `updates/` is reused, never a second one.
    let update = item(&plan, "0004000E00055D00.cia");
    assert_eq!(
        below_system(update),
        p(&[
            "pokemon y (world)",
            "updates",
            "pokemon y (world) (v1.1.0).cia"
        ])
    );
    assert!(update.game.as_ref().unwrap().folder_exists);

    // A file named like the folder: the next free name, children follow.
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_flat(&pile);
    put(&lib, "3DS/Pokemon Y (World)", b"not a folder", 9);
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let game = "Pokemon Y (World) (2)";
    assert_eq!(
        below_system(item(&plan, "Pokemon Y.cia")),
        p(&[game, "Pokemon Y (World) (2).cia"])
    );
    assert_eq!(
        below_system(item(&plan, "0004000E00055D00.cia")),
        p(&[game, "Updates", "Pokemon Y (World) (2) (v1.1.0).cia"])
    );
    assert_eq!(
        below_system(item(&plan, "Pokemon Y - Extra Pack (USA).cia")),
        p(&[game, "DLC", "Pokemon Y (World) (2) - Extra Pack.cia"])
    );
}

#[test]
fn a_file_named_like_a_fixed_folder_leaves_the_child() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_flat(&pile);
    put(&lib, "3DS/Pokemon Y (World)/Updates", b"a file", 9);
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let update = item(&plan, "0004000E00055D00.cia");
    assert_eq!(
        update.action,
        OrganizeAction::Left {
            reason: LeftReason::UnsafeTarget
        }
    );
    // The base and the DLC still go into the existing folder.
    assert_eq!(
        below_system(item(&plan, "Pokemon Y - Extra Pack (USA).cia")),
        p(&[
            "Pokemon Y (World)",
            "DLC",
            "Pokemon Y (World) - Extra Pack.cia"
        ])
    );
}

#[test]
fn nfc_and_nfd_names_meet_in_one_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Pok\u{e9}mon Y.cia",
        &ct::cia_with_code(PY_BASE, "CTR-P-EK2A", false, false).head_bytes(),
        1,
    );
    let nfd = "Poke\u{301}mon Y (World)";
    fs::create_dir_all(lib.join("3DS").join(nfd)).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "Pok\u{e9}mon Y.cia");
    assert!(base.game.as_ref().unwrap().folder_exists, "{base:?}");
    let entries: Vec<_> = fs::read_dir(lib.join("3DS")).unwrap().flatten().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        target_parts(base)[1],
        entries[0].file_name().to_string_lossy().into_owned()
    );
}

#[test]
fn a_wii_dlc_wad_folds_under_its_disc() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Fortune Street.iso",
        &ct::gc_disc("WF4E01", true).head_bytes(),
        1,
    );
    put(
        &pile,
        "wF4E.wad",
        &ct::wad_head(0x0001_0005_7746_3445, 0).head_bytes(),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "Fortune Street.iso");
    assert_eq!(
        below_system(base),
        p(&["Fortune Street (USA)", "Fortune Street (USA).iso"]),
        "{base:?}"
    );
    let dlc = item(&plan, "wF4E.wad");
    assert_eq!(
        dlc.game.as_ref().and_then(|g| g.of),
        Some(base.id),
        "{dlc:?}"
    );
    assert_eq!(target_parts(dlc)[1..3], p(&["Fortune Street (USA)", "DLC"]));
}

#[test]
fn a_demo_with_its_base_title_id_is_not_folded() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let tid = 0x5841_0A00;
    let hash = ct::STFS_HASH;
    // GOD/XBLA layout `<tid>/<content type>/<40 hex>`, one per folder.
    put(
        &pile,
        &format!("base/58410A00/000D0000/{hash}"),
        &ct::stfs_head(0x000D_0000, tid, 1, 0).head_bytes(),
        1,
    );
    put(
        &pile,
        &format!("demo/58410A00/00080000/{hash}"),
        &ct::stfs_head(0x0008_0000, tid, 1, 0).head_bytes(),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = plan
        .items
        .iter()
        .find(|i| i.sources[0].starts_with("base"))
        .unwrap();
    assert_eq!(
        base.game.as_ref().map(|g| g.role),
        Some(GameRole::Base),
        "{base:?}"
    );
    let demo = plan
        .items
        .iter()
        .find(|i| i.sources[0].starts_with("demo"))
        .unwrap();
    assert_ne!(demo.game.as_ref().map(|g| g.role), Some(GameRole::Update));
    assert!(demo.game.as_ref().and_then(|g| g.of).is_none(), "{demo:?}");
    if let Some(t) = &demo.target {
        assert!(
            !parts(t).iter().any(|c| c == "Updates" || c == "DLC"),
            "{t:?}"
        );
    }
}

#[test]
fn two_bases_with_one_title_keep_their_own_children() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let (a, b) = (0x0004_0000_0001_1100u64, 0x0004_0000_0002_2200u64);
    let upd = |tid: u64| tid | 0x0000_000E_0000_0000;
    put(
        &pile,
        "a/Tetris.cia",
        &ct::cia_with_code(a, "CTR-P-ATRE", false, false).head_bytes(),
        1,
    );
    put(
        &pile,
        "b/Tetris.cia",
        &ct::cia_with_code(b, "CTR-P-ATRE", false, false).head_bytes(),
        2,
    );
    put(
        &pile,
        "a/u.cia",
        &with_tail(&cia_update(upd(a), 1040), 1),
        3,
    );
    put(
        &pile,
        "b/u.cia",
        &with_tail(&cia_update(upd(b), 2080), 2),
        4,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let by_source = |rel: &[&str]| {
        plan.items
            .iter()
            .find(|i| parts(&i.sources[0]) == p(rel))
            .unwrap()
    };
    let (base_a, base_b) = (
        by_source(&["a", "Tetris.cia"]),
        by_source(&["b", "Tetris.cia"]),
    );
    let (ua, ub) = (by_source(&["a", "u.cia"]), by_source(&["b", "u.cia"]));
    assert_eq!(ua.game.as_ref().unwrap().of, Some(base_a.id));
    assert_eq!(ub.game.as_ref().unwrap().of, Some(base_b.id));
    let folder = |it: &OrganizeItem| target_parts(it)[1].clone();
    assert_ne!(folder(base_a), folder(base_b));
    assert_eq!(folder(ua), folder(base_a));
    assert_eq!(folder(ub), folder(base_b));
    let mut names = vec![folder(base_a), folder(base_b)];
    names.sort();
    assert_eq!(names, p(&["Tetris (USA)", "Tetris (USA) (2)"]));
}

#[test]
fn the_highest_update_is_the_newest_here() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_flat(&pile);
    put(
        &pile,
        "newer.cia",
        &with_tail(&cia_update(PY_UPDATE, 2080), 9),
        9,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let old = item(&plan, "0004000E00055D00.cia").game.clone().unwrap();
    let new = item(&plan, "newer.cia").game.clone().unwrap();
    assert!(!old.newest_here);
    assert!(new.newest_here);
    assert_eq!(new.version.as_deref(), Some("2.2.0"));
    assert_eq!(
        below_system(item(&plan, "newer.cia")),
        p(&[
            "Pokemon Y (World)",
            "Updates",
            "Pokemon Y (World) (v2.2.0).cia"
        ])
    );
}

#[test]
fn an_update_already_in_its_folder_is_present_and_another_gets_a_number() {
    let name = "Pokemon Y (World) (v1.1.0).cia";
    // The same bytes already filed: nothing to move.
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_flat(&pile);
    let same = with_tail(&cia_update(PY_UPDATE, 1040), 1);
    put(
        &lib,
        &format!("3DS/Pokemon Y (World)/Updates/{name}"),
        &same,
        1,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    match &item(&plan, "0004000E00055D00.cia").action {
        OrganizeAction::Left {
            reason: LeftReason::AlreadyPresent { at },
        } => assert_eq!(parts(at), p(&["3DS", "Pokemon Y (World)", "Updates", name])),
        other => panic!("{other:?}"),
    }
    assert!(item(&plan, "Pokemon Y.cia").moves_in());

    // Another file under that name: never overwritten, ` (2)` and Review.
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_flat(&pile);
    put(
        &lib,
        &format!("3DS/Pokemon Y (World)/Updates/{name}"),
        b"someone else's file",
        1,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let update = item(&plan, "0004000E00055D00.cia");
    assert_eq!(update.action, OrganizeAction::Review, "{update:?}");
    assert_eq!(
        below_system(update),
        p(&[
            "Pokemon Y (World)",
            "Updates",
            "Pokemon Y (World) (v1.1.0) (2).cia"
        ])
    );
}

// --- Demos, orphans and the library lookup (Task 11) ---

use coverly_core::organize::ItemNote;

/// A PS3 PKG head with `content_id`.
fn ps3_pkg(content_id: &str, tail: u8) -> Vec<u8> {
    with_tail(&ct::pkg(1, content_id), tail)
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
        now: SystemTime::now(),
    };
    plan_organize(pile, opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap()
}

fn has_note(it: &OrganizeItem, note: &ItemNote) -> bool {
    it.notes.contains(note)
}

#[test]
fn a_demo_is_its_own_game_linked_to_its_base_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Resistance (USA).pkg",
        &ps3_pkg("UP9000-BCUS98107_00-RESISTANCE000001", 1),
        1,
    );
    put(
        &pile,
        "Resistance (USA) (Demo).pkg",
        &ps3_pkg("UP9000-NPUA80001_00-RESISTANCEDEMO01", 2),
        2,
    );
    put(
        &pile,
        "Resistance (USA) (Kiosk Demo).pkg",
        &ps3_pkg("UP9000-NPUA80002_00-RESISTANCEKIOSK1", 3),
        3,
    );
    // A 3DS demo by its header (`00040002`): ` (Demo)` is added, and its
    // product code's verified letter gives its region like a base's (part
    // C D12: a demo is a game of its own).
    put(
        &pile,
        "Pokemon Y Special.cia",
        &ct::cia_with_code(0x0004_0002_0005_5D00, "CTR-P-EK2A", false, false).head_bytes(),
        4,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));

    let demo = item(&plan, "Resistance (USA) (Demo).pkg");
    assert!(demo.moves_in(), "{demo:?}");
    let game = "Resistance (USA) (Demo)";
    assert_eq!(below_system(demo), p(&[game, &format!("{game}.pkg")]));
    let link = demo.game.as_ref().unwrap();
    assert_eq!(link.role, GameRole::Demo);
    assert_eq!(link.of, None);
    assert_eq!(link.demo_of.as_deref(), Some("Resistance (USA)"));
    assert!(has_note(
        demo,
        &ItemNote::DemoOf {
            of: "Resistance (USA)".into()
        }
    ));
    // Never inside its base's folder.
    let base = item(&plan, "Resistance (USA).pkg");
    assert!(base
        .target
        .as_deref()
        .is_some_and(|t| !t.starts_with(demo.game.as_ref().unwrap().folder.as_deref().unwrap())));

    let kiosk = item(&plan, "Resistance (USA) (Kiosk Demo).pkg");
    let game = "Resistance (USA) (Kiosk Demo)";
    assert_eq!(below_system(kiosk), p(&[game, &format!("{game}.pkg")]));
    assert_eq!(kiosk.game.as_ref().map(|g| g.role), Some(GameRole::Demo));

    let special = item(&plan, "Pokemon Y Special.cia");
    let game = "Pokemon Y Special (World) (Demo)";
    assert_eq!(
        target_parts(special),
        p(&["3DS", game, &format!("{game}.cia")])
    );
    assert_eq!(special.game.as_ref().map(|g| g.role), Some(GameRole::Demo));
    assert_eq!(special.game.as_ref().and_then(|g| g.demo_of.clone()), None);

    // Option off: flat as today, the note still shows, no link.
    let off = OrganizeOptions {
        game_folders: Some(false),
        ..options("coverly-library", &lib)
    };
    let plan = plan_of(&pile, &off);
    let demo = item(&plan, "Resistance (USA) (Demo).pkg");
    assert_eq!(below_system(demo), p(&["Resistance (USA) (Demo).pkg"]));
    assert_eq!(demo.game, None);
    assert!(has_note(
        demo,
        &ItemNote::DemoOf {
            of: "Resistance (USA)".into()
        }
    ));
}

#[test]
fn a_demo_finds_its_base_in_the_library_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Resistance (USA) (Demo).pkg",
        &ps3_pkg("UP9000-NPUA80001_00-RESISTANCEDEMO01", 2),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    // Where the profile puts PS3 games.
    let probe = plan_of(&pile, &options("coverly-library", &lib));
    let ps3 = item(&probe, "Resistance (USA) (Demo).pkg")
        .folder
        .clone()
        .unwrap();
    put(
        &lib,
        &format!("{ps3}/Resistance (USA)/Resistance (USA).pkg"),
        &ps3_pkg("UP9000-BCUS98107_00-RESISTANCE000001", 1),
        1,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let demo = item(&plan, "Resistance (USA) (Demo).pkg");
    assert!(has_note(
        demo,
        &ItemNote::DemoOf {
            of: "Resistance (USA)".into()
        }
    ));
    assert_eq!(
        demo.game.as_ref().and_then(|g| g.demo_of.as_deref()),
        Some("Resistance (USA)")
    );
}

#[test]
fn a_region_less_demo_links_to_the_usa_base_not_the_first_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    // "(Japan)" sorts before "(USA)": the old code took Japan.
    put(
        &pile,
        "Pokemon Y (Japan).cia",
        &ct::cia(0x0004_0000_0005_5E00, true, false).head_bytes(),
        1,
    );
    put(
        &pile,
        "Pokemon Y (USA).cia",
        &ct::cia(0x0004_0000_0005_5D00, true, false).head_bytes(),
        2,
    );
    // A 3DS demo by header: no region (a child's product code is not
    // trusted, part A D10).
    put(
        &pile,
        "Pokemon Y.cia",
        &ct::cia_with_code(0x0004_0002_0005_5D00, "CTR-P-EK2A", false, false).head_bytes(),
        3,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let demo = item(&plan, "Pokemon Y.cia");
    assert_eq!(
        demo.game.as_ref().and_then(|g| g.demo_of.as_deref()),
        Some("Pokemon Y (USA)")
    );
    assert!(has_note(
        demo,
        &ItemNote::DemoOf {
            of: "Pokemon Y (USA)".into()
        }
    ));
}

#[test]
fn a_demo_with_a_region_links_to_the_base_of_that_region() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Resistance (Japan).pkg",
        &ps3_pkg("JP9000-BCJS30001_00-RESISTANCE000001", 1),
        1,
    );
    put(
        &pile,
        "Resistance (USA).pkg",
        &ps3_pkg("UP9000-BCUS98107_00-RESISTANCE000001", 2),
        2,
    );
    put(
        &pile,
        "Resistance (Europe) (Demo).pkg",
        &ps3_pkg("EP9000-NPEA00001_00-RESISTANCEDEMO01", 3),
        3,
    );
    fs::create_dir_all(&lib).unwrap();
    let demo_of = |plan: &OrganizePlan| {
        item(plan, "Resistance (Europe) (Demo).pkg")
            .game
            .as_ref()
            .and_then(|g| g.demo_of.clone())
    };
    // No European base: the preference order (World, USA, Europe, Japan).
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    assert_eq!(demo_of(&plan).as_deref(), Some("Resistance (USA)"));

    // A European base: the demo's own region wins.
    put(
        &pile,
        "Resistance (Europe).pkg",
        &ps3_pkg("EP9000-BCES00001_00-RESISTANCE000001", 4),
        4,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    assert_eq!(demo_of(&plan).as_deref(), Some("Resistance (Europe)"));
}

#[test]
fn a_library_base_of_the_demo_region_beats_a_plan_base_of_another() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Resistance (USA).pkg",
        &ps3_pkg("UP9000-BCUS98107_00-RESISTANCE000001", 1),
        1,
    );
    put(
        &pile,
        "Resistance (Europe) (Demo).pkg",
        &ps3_pkg("EP9000-NPEA00001_00-RESISTANCEDEMO01", 2),
        2,
    );
    fs::create_dir_all(&lib).unwrap();
    let probe = plan_of(&pile, &options("coverly-library", &lib));
    let ps3 = item(&probe, "Resistance (USA).pkg").folder.clone().unwrap();
    // The European base is already in the library: the demo's own region
    // wins over the USA base in the pile (mods spec D20).
    put(
        &lib,
        &format!("{ps3}/Resistance (Europe)/Resistance (Europe).pkg"),
        &ps3_pkg("EP9000-BCES00001_00-RESISTANCE000001", 3),
        3,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let demo = item(&plan, "Resistance (Europe) (Demo).pkg");
    assert_eq!(
        demo.game.as_ref().and_then(|g| g.demo_of.as_deref()),
        Some("Resistance (Europe)")
    );
}

#[test]
fn a_library_demo_lookup_uses_the_same_order() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    put(
        &pile,
        "Pokemon Y.cia",
        &ct::cia_with_code(0x0004_0002_0005_5D00, "CTR-P-EK2A", false, false).head_bytes(),
        3,
    );
    put(
        &lib,
        "3DS/Pokemon Y (Japan)/Pokemon Y (Japan).cia",
        &ct::cia(0x0004_0000_0005_5E00, true, false).head_bytes(),
        1,
    );
    put(
        &lib,
        "3DS/Pokemon Y (USA)/Pokemon Y (USA).cia",
        &ct::cia(0x0004_0000_0005_5D00, true, false).head_bytes(),
        2,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let demo = item(&plan, "Pokemon Y.cia");
    assert_eq!(
        demo.game.as_ref().and_then(|g| g.demo_of.as_deref()),
        Some("Pokemon Y (USA)")
    );
}

/// The Pokemon Y update alone in `pile` (no base): TMD 1040, encrypted,
/// no title in its header.
fn orphan_update(pile: &Path) {
    put(
        pile,
        "0004000E00055D00.cia",
        &with_tail(&cia_update(PY_UPDATE, 1040), 1),
        1,
    );
}

#[test]
fn an_orphan_without_a_title_stays_flat_for_review() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    orphan_update(&pile);
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let update = item(&plan, "0004000E00055D00.cia");
    assert_eq!(update.action, OrganizeAction::Review, "{update:?}");
    assert_eq!(target_parts(update), p(&["3DS", "0004000E00055D00.cia"]));
    assert!(has_note(update, &ItemNote::BaseMissing));
    let link = update.game.as_ref().unwrap();
    assert_eq!(
        (link.role, link.of, &link.folder),
        (GameRole::Update, None, &None)
    );
}

#[test]
fn an_orphan_with_a_title_gets_a_folder_for_review() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    // A clear update whose header names its game (SMDH «Test Game»); a
    // 3DS child's own product code gives no region (D10).
    let mut s = ct::cia_with_code(PY_UPDATE, "CTR-P-EK2A", false, true);
    s.put(ct::CIA_TMD + 0x1DC, &1040u16.to_be_bytes());
    put(&pile, "0004000E00055D00.cia", &with_tail(&s, 1), 1);
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let update = item(&plan, "0004000E00055D00.cia");
    assert_eq!(update.action, OrganizeAction::Review, "{update:?}");
    assert!(has_note(update, &ItemNote::BaseMissing));
    let game = "Test Game";
    assert_eq!(
        target_parts(update),
        p(&["3DS", game, "Updates", "Test Game (v1.1.0).cia"])
    );
    let link = update.game.as_ref().unwrap();
    assert_eq!(parts(link.folder.as_deref().unwrap()), p(&["3DS", game]));
    assert!(!link.folder_exists);
}

#[test]
fn an_orphan_joins_its_game_folder_in_the_library() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    orphan_update(&pile);
    put(
        &lib,
        "3DS/Pokemon Y (World)/Pokemon Y (World).cia",
        &ct::cia_with_code(PY_BASE, "CTR-P-EK2A", false, false).head_bytes(),
        5,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let update = item(&plan, "0004000E00055D00.cia");
    assert_eq!(update.action, OrganizeAction::Auto, "{update:?}");
    assert!(!has_note(update, &ItemNote::BaseMissing));
    assert_eq!(
        target_parts(update),
        p(&[
            "3DS",
            "Pokemon Y (World)",
            "Updates",
            "Pokemon Y (World) (v1.1.0).cia"
        ])
    );
    let link = update.game.as_ref().unwrap();
    assert!(link.folder_exists);
    assert_eq!(link.of, None);

    // The budget spent before the base is read: never guessed, review.
    let mut tiny = options("coverly-library", &lib);
    tiny.limits.plan_head_budget = 0;
    let plan = plan_with(&pile, &tiny, &Cache::in_memory().unwrap());
    let update = item(&plan, "0004000E00055D00.cia");
    assert_eq!(update.action, OrganizeAction::Review, "{update:?}");
    assert!(has_note(update, &ItemNote::BaseMissing));
}

#[test]
fn an_orphan_beside_a_flat_base_stays_flat() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    orphan_update(&pile);
    put(
        &lib,
        "3DS/Pokemon Y.cia",
        &ct::cia_with_code(PY_BASE, "CTR-P-EK2A", false, false).head_bytes(),
        5,
    );
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let update = item(&plan, "0004000E00055D00.cia");
    assert_eq!(update.action, OrganizeAction::Auto, "{update:?}");
    assert!(!has_note(update, &ItemNote::BaseMissing));
    assert_eq!(target_parts(update), p(&["3DS", "0004000E00055D00.cia"]));
    // No folder created around a file the user did not pick (D19).
    assert!(!lib.join("3DS").join("Pokemon Y").exists());
    assert_eq!(update.game.as_ref().and_then(|g| g.folder.clone()), None);
}

#[test]
fn the_library_is_read_only_for_systems_with_an_orphan() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    orphan_update(&pile);
    put(
        &pile,
        "Resistance (USA) (Demo).pkg",
        &ps3_pkg("UP9000-NPUA80001_00-RESISTANCEDEMO01", 2),
        2,
    );
    let probe = plan_of(&pile, &options("coverly-library", &lib));
    let ps3 = item(&probe, "Resistance (USA) (Demo).pkg")
        .folder
        .clone()
        .unwrap();
    let base = put(
        &lib,
        "3DS/Pokemon Y (World)/Pokemon Y (World).cia",
        &ct::cia_with_code(PY_BASE, "CTR-P-EK2A", false, false).head_bytes(),
        5,
    );
    let many: Vec<PathBuf> = (0..2000)
        .map(|n| {
            put(
                &lib,
                &format!("{ps3}/Game {n:04}.pkg"),
                &ps3_pkg("UP9000-BCUS98107_00-RESISTANCE000001", (n % 251) as u8),
                n,
            )
        })
        .collect();
    let cache = Cache::in_memory().unwrap();
    let plan = plan_with(&pile, &options("coverly-library", &lib), &cache);
    assert_eq!(
        item(&plan, "0004000E00055D00.cia").action,
        OrganizeAction::Auto
    );
    let memoised = |path: &Path| {
        let meta = fs::metadata(path).unwrap();
        let mtime = coverly_core::cache::mtime_ns(meta.modified().unwrap());
        let real = fs::canonicalize(path).unwrap();
        [path.to_path_buf(), real].iter().any(|p| {
            cache
                .organize_facts(p, meta.len(), mtime)
                .unwrap()
                .is_some()
        })
    };
    // The 3DS base's header was read (and memoised); no PS3 file was.
    assert!(memoised(&base));
    assert!(many.iter().all(|m| !memoised(m)));
}

/// `pile/name` as a zip of `files`, mtime pinned.
fn put_zip(pile: &Path, name: &str, files: &[(&str, Vec<u8>)]) -> PathBuf {
    use std::io::Write;
    fs::create_dir_all(pile).unwrap();
    let path = pile.join(name);
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (n, bytes) in files {
        zip.start_file(*n, opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    pin_mtime(&path, 7);
    path
}

#[test]
fn a_child_whose_base_is_in_the_plan_is_no_orphan() {
    // The base is in the pile but cannot take a game folder as it is (a
    // zipped PS3 folder game to extract): its update is no orphan (D13 is
    // for a base neither in the plan nor in the library), so no
    // base-missing note, no review and no folder made for it.
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, raw) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("raw"),
    );
    ct::ps3_jb(&raw.join("Resistance"), "BLUS30001", "DG");
    let files: Vec<(String, Vec<u8>)> = walk_files(&raw)
        .into_iter()
        .map(|f| {
            let rel = parts(f.strip_prefix(&raw).unwrap()).join("/");
            (rel, fs::read(&f).unwrap())
        })
        .collect();
    let named: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    put_zip(&pile, "Resistance.zip", &named);
    ct::ps3_hdd(&pile.join("BLUS30001 Patch"), "BLUS30001", "GD");
    for f in walk_files(&pile.join("BLUS30001 Patch")) {
        pin_mtime(&f, 8);
    }
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let update = item(&plan, "BLUS30001 Patch");
    assert!(!has_note(update, &ItemNote::BaseMissing), "{update:?}");
    assert_eq!(update.action, OrganizeAction::Auto, "{update:?}");
    // Flat beside where its base goes, as before game folders.
    assert_eq!(below_system(update), p(&["BLUS30001 Patch"]), "{update:?}");
    assert_eq!(update.game.as_ref().and_then(|g| g.folder.clone()), None);
}

// --- Licences (game folders spec D17, plan Task 12) ---

use coverly_core::consoles::{start_open_log, take_open_log};
use coverly_core::organize::{apply_organize, ApplyEnv, OrganizeReport, TransferMode};
use coverly_core::write::trash::FakeTrash;
use coverly_core::write::{set_copy_fault, undo_job, CopyStage};

/// A PS3 base PKG (`BLUS30001`) and its `.rap` licence, flat in `pile`.
fn resistance_with_rap(pile: &Path, rap: &str) {
    put(
        pile,
        "Resistance.pkg",
        &ps3_pkg("UP9000-BLUS30001_00-RESISTANCE000001", 1),
        1,
    );
    put(pile, rap, &[0xA5; 16], 2);
}

fn apply_in(plan: &OrganizePlan, data: &Path) -> OrganizeReport {
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

/// No path with a licence or key name was opened.
fn assert_no_licence_opened() {
    let opened = take_open_log();
    let named: Vec<&PathBuf> = opened
        .iter()
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| coverly_core::consoles::is_key_material(&n.to_string_lossy()))
        })
        .collect();
    assert!(named.is_empty(), "licences opened: {named:?}");
}

fn licence_companions(it: &OrganizeItem) -> Vec<&coverly_core::organize::Companion> {
    it.companions.iter().filter(|c| c.licence).collect()
}

fn left_reason(it: &OrganizeItem) -> Option<&LeftReason> {
    match &it.action {
        OrganizeAction::Left { reason } => Some(reason),
        _ => None,
    }
}

#[test]
fn licence_follows_by_rename_only() {
    start_open_log();
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, data) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("data"),
    );
    resistance_with_rap(&pile, "BLUS30001.rap");
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    assert_eq!(plan.mode, TransferMode::Move);

    let base = item(&plan, "Resistance.pkg");
    assert!(base.moves_in(), "{base:?}");
    let target = base.target.clone().unwrap();
    let licences = licence_companions(base);
    assert_eq!(licences.len(), 1, "{base:?}");
    assert_eq!(parts(&licences[0].source), p(&["BLUS30001.rap"]));
    // Beside its game's file, under its own name (an id names it).
    assert_eq!(
        parts(&licences[0].target),
        parts(&target.parent().unwrap().join("BLUS30001.rap"))
    );
    let rap = item(&plan, "BLUS30001.rap");
    assert_eq!(
        left_reason(rap),
        Some(&LeftReason::LicenceOf {
            of: PathBuf::from("Resistance.pkg")
        }),
        "{rap:?}"
    );

    let report = apply_in(&plan, &data);
    let outcome = report.items.iter().find(|o| o.item == base.id).unwrap();
    assert!(outcome.licences_left.is_empty(), "{outcome:?}");
    let moved = lib.join(target.parent().unwrap()).join("BLUS30001.rap");
    assert_eq!(fs::read(&moved).unwrap(), [0xA5; 16]);
    assert!(!pile.join("BLUS30001.rap").exists());
    assert!(outcome.targets.contains(&moved), "{outcome:?}");

    undo_job(&data, report.job.as_deref().unwrap()).unwrap();
    assert_eq!(fs::read(pile.join("BLUS30001.rap")).unwrap(), [0xA5; 16]);
    assert!(pile.join("Resistance.pkg").is_file());
    assert!(!moved.exists());
    assert_no_licence_opened();
}

#[test]
fn a_licence_named_by_the_game_s_stem_takes_its_new_name() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    resistance_with_rap(&pile, "resistance.rap");
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "Resistance.pkg");
    let target = base.target.clone().unwrap();
    let stem = target.file_stem().unwrap().to_string_lossy().into_owned();
    let licences = licence_companions(base);
    assert_eq!(licences.len(), 1, "{base:?}");
    assert_eq!(
        parts(&licences[0].target),
        parts(&target.parent().unwrap().join(format!("{stem}.rap")))
    );
}

#[test]
fn licence_stays_in_copy_mode() {
    start_open_log();
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, data) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("data"),
    );
    resistance_with_rap(&pile, "BLUS30001.rap");
    fs::create_dir_all(&lib).unwrap();
    let opts = OrganizeOptions {
        mode: Some(TransferMode::Copy),
        ..options("coverly-library", &lib)
    };
    let plan = plan_of(&pile, &opts);
    let base = item(&plan, "Resistance.pkg");
    assert!(base.moves_in());
    assert!(licence_companions(base).is_empty(), "{base:?}");
    let rap = item(&plan, "BLUS30001.rap");
    assert_eq!(left_reason(rap), Some(&LeftReason::KeyMaterial));
    apply_in(&plan, &data);
    assert!(pile.join("BLUS30001.rap").is_file());
    let copied: Vec<PathBuf> = walk_files(&lib);
    assert!(
        copied
            .iter()
            .all(|f| f.extension().is_none_or(|e| e != "rap")),
        "{copied:?}"
    );
    assert_no_licence_opened();
}

#[test]
fn licence_stays_when_ambiguous() {
    start_open_log();
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    resistance_with_rap(&pile, "BLUS30001.rap");
    // A second item of the same title id in the same folder.
    put(
        &pile,
        "Resistance Extra.pkg",
        &ps3_pkg("UP9000-BLUS30001_00-RESISTANCEEXTRA1", 3),
        3,
    );
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    assert!(plan.items.iter().all(|i| licence_companions(i).is_empty()));
    let rap = item(&plan, "BLUS30001.rap");
    assert_eq!(left_reason(rap), Some(&LeftReason::KeyMaterial));
    assert_no_licence_opened();
}

#[test]
fn global_keys_and_other_folders_never_follow() {
    start_open_log();
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    resistance_with_rap(&pile, "prod.keys");
    // The right name, but another folder than its game.
    put(&pile, "licences/BLUS30001.rap", &[0x5A; 16], 3);
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "Resistance.pkg");
    assert!(licence_companions(base).is_empty(), "{base:?}");
    assert_eq!(
        left_reason(item(&plan, "prod.keys")),
        Some(&LeftReason::KeyMaterial)
    );
    assert_eq!(
        left_reason(item(&plan, "BLUS30001.rap")),
        Some(&LeftReason::KeyMaterial)
    );
    assert_no_licence_opened();
}

#[test]
fn licence_follows_only_with_game_folders() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    resistance_with_rap(&pile, "BLUS30001.rap");
    fs::create_dir_all(&lib).unwrap();
    let off = OrganizeOptions {
        game_folders: Some(false),
        ..options("coverly-library", &lib)
    };
    let plan = plan_of(&pile, &off);
    assert!(licence_companions(item(&plan, "Resistance.pkg")).is_empty());
    assert_eq!(
        left_reason(item(&plan, "BLUS30001.rap")),
        Some(&LeftReason::KeyMaterial)
    );
}

#[test]
fn a_taken_licence_name_at_plan_time_leaves_it() {
    start_open_log();
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    resistance_with_rap(&pile, "BLUS30001.rap");
    fs::create_dir_all(&lib).unwrap();
    let probe = plan_of(&pile, &options("coverly-library", &lib));
    let dir = item(&probe, "Resistance.pkg")
        .target
        .clone()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    put(&lib.join(&dir), "BLUS30001.rap", &[0x11; 16], 4);
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    assert!(licence_companions(item(&plan, "Resistance.pkg")).is_empty());
    assert_eq!(
        left_reason(item(&plan, "BLUS30001.rap")),
        Some(&LeftReason::KeyMaterial)
    );
    assert_no_licence_opened();
}

#[test]
fn licence_rename_exdev_leaves_it() {
    start_open_log();
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, data) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("data"),
    );
    resistance_with_rap(&pile, "BLUS30001.rap");
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "Resistance.pkg");
    assert_eq!(licence_companions(base).len(), 1);

    // The volume changed since the plan: the licence's rename is refused
    // as across volumes; the game itself still moves.
    set_copy_fault(Some(|stage, path| {
        (stage == CopyStage::Rename && path.extension().is_some_and(|e| e == "rap"))
            .then(|| std::io::Error::from(std::io::ErrorKind::CrossesDevices))
    }));
    let report = apply_in(&plan, &data);
    set_copy_fault(None);
    let outcome = report.items.iter().find(|o| o.item == base.id).unwrap();
    assert!(
        matches!(outcome.status, coverly_core::organize::ItemStatus::Moved),
        "{outcome:?}"
    );
    assert_eq!(outcome.licences_left, [pile.join("BLUS30001.rap")]);
    assert_eq!(fs::read(pile.join("BLUS30001.rap")).unwrap(), [0xA5; 16]);
    assert!(lib.join(base.target.as_ref().unwrap()).is_file());
    let copied = walk_files(&lib);
    assert!(
        copied
            .iter()
            .all(|f| f.extension().is_none_or(|e| e != "rap")),
        "never a copy: {copied:?}"
    );
    // Undo puts the game back and leaves the licence alone.
    undo_job(&data, report.job.as_deref().unwrap()).unwrap();
    assert!(pile.join("Resistance.pkg").is_file());
    assert_eq!(fs::read(pile.join("BLUS30001.rap")).unwrap(), [0xA5; 16]);
    assert_no_licence_opened();
}

#[test]
fn a_licence_name_taken_at_apply_leaves_it_and_the_other_file_untouched() {
    start_open_log();
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, data) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("data"),
    );
    resistance_with_rap(&pile, "BLUS30001.rap");
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "Resistance.pkg");
    let licence = licence_companions(base)[0].clone();
    // Someone puts a file of that name there between plan and apply.
    let there = lib.join(&licence.target);
    put(there.parent().unwrap(), "BLUS30001.rap", &[0x22; 8], 5);

    let report = apply_in(&plan, &data);
    let outcome = report.items.iter().find(|o| o.item == base.id).unwrap();
    assert_eq!(outcome.licences_left, [pile.join("BLUS30001.rap")]);
    assert_eq!(fs::read(&there).unwrap(), [0x22; 8], "never replaced");
    assert_eq!(fs::read(pile.join("BLUS30001.rap")).unwrap(), [0xA5; 16]);
    let dir_names: Vec<String> = fs::read_dir(there.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".rap"))
        .collect();
    assert_eq!(dir_names, ["BLUS30001.rap"], "never a numbered name");
    assert_no_licence_opened();
}

// --- Apply and undo round trip (game folders spec D4, D21, §8.3, plan Task 13) ---

use coverly_core::organize::ItemStatus;

/// One journal line of an organise job: its op kind and its target.
struct JournalEntry {
    op: String,
    target: PathBuf,
}

/// The entries of job `job`'s journal, in the order they were written.
fn journal(data: &Path, job: &str) -> Vec<JournalEntry> {
    let text = fs::read_to_string(data.join("jobs").join(format!("{job}.jsonl"))).unwrap();
    text.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["type"] == "entry")
        .map(|v| JournalEntry {
            op: v["op"]["kind"].as_str().unwrap().to_string(),
            target: PathBuf::from(v["target"].as_str().unwrap()),
        })
        .collect()
}

/// The folders job `job` journaled as created, below `lib`, by component.
fn dirs_created(data: &Path, job: &str, lib: &Path) -> Vec<Vec<String>> {
    journal(data, job)
        .iter()
        .filter(|e| e.op == "dir-created")
        .map(|e| parts(e.target.strip_prefix(lib).unwrap()))
        .collect()
}

/// Every file below `dir`, relative to it, by component, sorted.
fn files_below(dir: &Path) -> Vec<Vec<String>> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut out: Vec<Vec<String>> = walk_files(dir)
        .iter()
        .map(|f| parts(f.strip_prefix(dir).unwrap()))
        .collect();
    out.sort();
    out
}

fn sorted(mut v: Vec<Vec<String>>) -> Vec<Vec<String>> {
    v.sort();
    v
}

/// `pile`, `lib` and `data` under one temp dir, with Pokemon Y flat in
/// the pile and an empty library.
fn pokemon_world() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, data) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("data"),
    );
    pokemon_flat(&pile);
    fs::create_dir_all(&lib).unwrap();
    (tmp, pile, lib, data)
}

const GAME: &str = "Pokemon Y (World)";

/// Where Pokemon Y's four files land in the library.
fn pokemon_in_lib() -> Vec<Vec<String>> {
    sorted(vec![
        p(&["3DS", GAME, "Pokemon Y (World).cia"]),
        p(&["3DS", GAME, "Updates", "Pokemon Y (World) (v1.1.0).cia"]),
        p(&["3DS", GAME, "DLC", "Pokemon Y (World) - Extra Pack.cia"]),
        p(&[
            "3DS",
            GAME,
            "DLC",
            "Pokemon Y (World) - DLC 0004008C00055D00.cia",
        ]),
    ])
}

fn pokemon_in_pile() -> Vec<Vec<String>> {
    sorted(vec![
        p(&["Pokemon Y.cia"]),
        p(&["0004000E00055D00.cia"]),
        p(&["Pokemon Y - Extra Pack (USA).cia"]),
        p(&["Pokemon Y [DLC2].cia"]),
    ])
}

/// The position in `entries` of the first entry whose target, below
/// `lib`, is `want` (by component).
fn position(entries: &[JournalEntry], lib: &Path, op: &str, want: &[String]) -> usize {
    entries
        .iter()
        .position(|e| e.op == op && parts(e.target.strip_prefix(lib).unwrap()) == want)
        .unwrap_or_else(|| panic!("no {op} entry for {want:?}"))
}

#[test]
fn apply_and_undo_round_trip_restores_every_path() {
    let (_tmp, pile, lib, data) = pokemon_world();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    assert_eq!(plan.mode, TransferMode::Move);
    let bytes: Vec<(Vec<String>, Vec<u8>)> = pokemon_in_pile()
        .into_iter()
        .map(|f| {
            let b = fs::read(pile.join(&f[0])).unwrap();
            (f, b)
        })
        .collect();

    let report = apply_in(&plan, &data);
    assert!(report.paused.is_none(), "{report:?}");
    assert!(
        report
            .items
            .iter()
            .all(|o| matches!(o.status, ItemStatus::Moved)),
        "{report:?}"
    );
    assert_eq!(files_below(&lib), pokemon_in_lib());
    assert_eq!(files_below(&pile), Vec::<Vec<String>>::new());

    // Exactly the folders used, each journaled once, the game folder
    // before its fixed folders and before any child moves into them.
    let job = report.job.clone().unwrap();
    let created = dirs_created(&data, &job, &lib);
    assert_eq!(
        sorted(created.clone()),
        sorted(vec![
            p(&["3DS"]),
            p(&["3DS", GAME]),
            p(&["3DS", GAME, "Updates"]),
            p(&["3DS", GAME, "DLC"]),
        ])
    );
    let entries = journal(&data, &job);
    let game = position(&entries, &lib, "dir-created", &p(&["3DS", GAME]));
    let updates = position(&entries, &lib, "dir-created", &p(&["3DS", GAME, "Updates"]));
    let dlc = position(&entries, &lib, "dir-created", &p(&["3DS", GAME, "DLC"]));
    assert!(game < updates && game < dlc);
    let base_move = position(
        &entries,
        &lib,
        "moved",
        &p(&["3DS", GAME, "Pokemon Y (World).cia"]),
    );
    for (n, e) in entries.iter().enumerate() {
        let rel = parts(e.target.strip_prefix(&lib).unwrap());
        if e.op == "moved" && rel.len() > 3 {
            let fixed = if rel[2] == "Updates" { updates } else { dlc };
            assert!(fixed < n, "{rel:?} moved before its folder existed");
            assert!(base_move < n, "{rel:?} moved before its base");
        }
    }

    let undo = undo_job(&data, &job).unwrap();
    assert_eq!(files_below(&pile), pokemon_in_pile());
    for (f, b) in &bytes {
        assert_eq!(&fs::read(pile.join(&f[0])).unwrap(), b, "{f:?}");
    }
    assert_eq!(files_below(&lib), Vec::<Vec<String>>::new());
    assert_eq!(fs::read_dir(&lib).unwrap().count(), 0, "every folder gone");
    // Children back before their base, folders after what was in them.
    let restored: Vec<Vec<String>> = undo
        .restored
        .iter()
        .map(|r| parts(r.strip_prefix(&pile).unwrap()))
        .collect();
    assert_eq!(
        restored.last(),
        Some(&p(&["Pokemon Y.cia"])),
        "{restored:?}"
    );
    let removed: Vec<Vec<String>> = undo
        .dirs_removed
        .iter()
        .map(|d| parts(d.strip_prefix(&lib).unwrap()))
        .collect();
    assert_eq!(sorted(removed.clone()), sorted(created));
    let at = |want: &[&str]| removed.iter().position(|d| *d == p(want)).unwrap();
    assert!(at(&["3DS", GAME, "Updates"]) < at(&["3DS", GAME]));
    assert!(at(&["3DS", GAME, "DLC"]) < at(&["3DS", GAME]));
    assert!(at(&["3DS", GAME]) < at(&["3DS"]));
    assert!(undo.skipped_modified.is_empty() && undo.skipped_occupied.is_empty());
}

#[test]
fn no_dlc_folder_without_dlc_and_never_an_empty_fixed_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, data) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("data"),
    );
    pokemon_flat(&pile);
    for gone in ["Pokemon Y - Extra Pack (USA).cia", "Pokemon Y [DLC2].cia"] {
        fs::remove_file(pile.join(gone)).unwrap();
    }
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let report = apply_in(&plan, &data);
    let job = report.job.clone().unwrap();
    assert_eq!(
        sorted(dirs_created(&data, &job, &lib)),
        sorted(vec![
            p(&["3DS"]),
            p(&["3DS", GAME]),
            p(&["3DS", GAME, "Updates"]),
        ])
    );
    // No fixed folder exists that nothing went into.
    let game_dir = lib.join("3DS").join(GAME);
    let mut names: Vec<String> = fs::read_dir(&game_dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["Pokemon Y (World).cia", "Updates"]);
    for fixed in [
        FixedFolder::Dlc,
        FixedFolder::Mods,
        FixedFolder::Cheats,
        FixedFolder::Extras,
        FixedFolder::Saves,
    ] {
        assert!(!game_dir.join(fixed.dir_name()).exists(), "{fixed:?}");
    }
}

#[test]
fn undo_keeps_a_game_folder_that_holds_the_user_s_save() {
    let (_tmp, pile, lib, data) = pokemon_world();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let report = apply_in(&plan, &data);
    let job = report.job.clone().unwrap();
    // The user adds a save after the run.
    let game_dir = lib.join("3DS").join(GAME);
    put(&game_dir, "Saves/main.sav", &[7u8; 64], 9);

    let undo = undo_job(&data, &job).unwrap();
    assert_eq!(files_below(&pile), pokemon_in_pile());
    assert_eq!(files_below(&lib), [p(&["3DS", GAME, "Saves", "main.sav"])]);
    assert_eq!(
        fs::read(game_dir.join("Saves/main.sav")).unwrap(),
        [7u8; 64]
    );
    let removed: Vec<Vec<String>> = undo
        .dirs_removed
        .iter()
        .map(|d| parts(d.strip_prefix(&lib).unwrap()))
        .collect();
    assert_eq!(
        sorted(removed),
        sorted(vec![p(&["3DS", GAME, "Updates"]), p(&["3DS", GAME, "DLC"]),])
    );
    let kept: Vec<Vec<String>> = undo
        .dirs_kept
        .iter()
        .map(|d| parts(d.strip_prefix(&lib).unwrap()))
        .collect();
    assert_eq!(kept, [p(&["3DS", GAME]), p(&["3DS"])], "reported as kept");
}

#[test]
fn copy_mode_round_trip_leaves_the_pile_and_removes_the_copies() {
    let (_tmp, pile, lib, data) = pokemon_world();
    let opts = OrganizeOptions {
        mode: Some(TransferMode::Copy),
        ..options("coverly-library", &lib)
    };
    let plan = plan_of(&pile, &opts);
    let report = apply_in(&plan, &data);
    assert!(
        report
            .items
            .iter()
            .all(|o| matches!(o.status, ItemStatus::Copied)),
        "{report:?}"
    );
    assert_eq!(files_below(&lib), pokemon_in_lib());
    assert_eq!(files_below(&pile), pokemon_in_pile());
    let job = report.job.clone().unwrap();
    assert_eq!(dirs_created(&data, &job, &lib).len(), 4);

    let undo = undo_job(&data, &job).unwrap();
    assert_eq!(undo.removed.len(), 4, "{undo:?}");
    assert_eq!(files_below(&pile), pokemon_in_pile());
    assert_eq!(fs::read_dir(&lib).unwrap().count(), 0, "every folder gone");
}

#[cfg(unix)]
fn disk_full_on_bare_dlc(stage: CopyStage, path: &Path) -> Option<std::io::Error> {
    (stage == CopyStage::Rename
        && path
            .file_name()
            .is_some_and(|n| n == "Pokemon Y [DLC2].cia"))
    .then(|| std::io::Error::from_raw_os_error(28))
}

#[cfg(unix)]
#[test]
fn a_pause_mid_apply_is_finished_by_resume_and_undone_whole() {
    use coverly_core::organize::resume_organize;
    use coverly_core::progress::PauseKind;
    let (_tmp, pile, lib, data) = pokemon_world();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    set_copy_fault(Some(disk_full_on_bare_dlc));
    let report = apply_in(&plan, &data);
    set_copy_fault(None);
    assert_eq!(report.paused, Some(PauseKind::Volume), "{report:?}");
    let bare = item(&plan, "Pokemon Y [DLC2].cia").id;
    assert!(report.resumable_items.contains(&bare), "{report:?}");
    assert!(pile.join("Pokemon Y [DLC2].cia").is_file());
    let job = report.job.clone().unwrap();

    let resumed = resume_organize(
        &job,
        &ApplyEnv {
            data_dir: &data,
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
            trash: &FakeTrash::unavailable("no-trash"),
            tools: Some(&[]),
        },
    )
    .unwrap();
    assert!(resumed.paused.is_none(), "{resumed:?}");
    assert_eq!(resumed.job.as_deref(), Some(job.as_str()));
    assert_eq!(files_below(&lib), pokemon_in_lib());
    assert_eq!(files_below(&pile), Vec::<Vec<String>>::new());
    // One folder of each, whatever run created it.
    assert_eq!(
        sorted(dirs_created(&data, &job, &lib)),
        sorted(vec![
            p(&["3DS"]),
            p(&["3DS", GAME]),
            p(&["3DS", GAME, "Updates"]),
            p(&["3DS", GAME, "DLC"]),
        ])
    );

    undo_job(&data, &job).unwrap();
    assert_eq!(files_below(&pile), pokemon_in_pile());
    assert_eq!(fs::read_dir(&lib).unwrap().count(), 0, "every folder gone");
}

#[test]
fn apply_follows_the_plan_when_the_option_changes_after_it() {
    let (_tmp, pile, lib, data) = pokemon_world();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    // The option is switched off after planning: a new plan would be
    // flat, the one being applied keeps its game folder.
    let off = OrganizeOptions {
        game_folders: Some(false),
        ..options("coverly-library", &lib)
    };
    let flat = plan_of(&pile, &off);
    assert_ne!(
        item(&flat, "Pokemon Y.cia").target,
        item(&plan, "Pokemon Y.cia").target
    );
    let report = apply_in(&plan, &data);
    assert!(report.paused.is_none(), "{report:?}");
    assert_eq!(files_below(&lib), pokemon_in_lib());
}

#[cfg(unix)]
#[test]
fn undo_after_a_pause_reverses_only_what_moved() {
    let (_tmp, pile, lib, data) = pokemon_world();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    set_copy_fault(Some(disk_full_on_bare_dlc));
    let report = apply_in(&plan, &data);
    set_copy_fault(None);
    assert!(report.paused.is_some(), "{report:?}");
    let job = report.job.clone().unwrap();
    // The bare DLC never left the pile; the other three are in the game
    // folder.
    assert!(pile.join("Pokemon Y [DLC2].cia").is_file());
    assert_eq!(files_below(&lib).len(), 3, "{:?}", files_below(&lib));

    let undo = undo_job(&data, &job).unwrap();
    assert_eq!(undo.restored.len(), 3, "{undo:?}");
    assert_eq!(files_below(&pile), pokemon_in_pile());
    assert_eq!(fs::read_dir(&lib).unwrap().count(), 0, "every folder gone");
    assert!(undo.dirs_kept.is_empty(), "{undo:?}");
}

#[test]
fn a_folder_game_s_licence_moves_as_planned_or_its_outcome_says_it_stayed() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, data) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("data"),
    );
    ct::ps3_jb(&pile.join("BLUS30001"), "BLUS30001", "DG");
    for f in walk_files(&pile) {
        pin_mtime(&f, 7);
    }
    put(&pile, "BLUS30001.rap", &[0xA5; 16], 8);
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let base = item(&plan, "BLUS30001");
    assert!(base.moves_in(), "{base:?}");
    let planned = licence_companions(base);
    let report = apply_in(&plan, &data);
    let outcome = report.items.iter().find(|o| o.item == base.id).unwrap();
    assert_eq!(planned.len(), 1, "{base:?}");
    let moved = lib.join(&planned[0].target);
    // What the plan promised happened, or the outcome says it did not.
    assert!(
        (moved.is_file() && outcome.targets.contains(&moved))
            || outcome.licences_left == [pile.join("BLUS30001.rap")],
        "{outcome:?}"
    );
}

#[test]
fn children_of_a_base_kept_in_the_library_go_into_its_game_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_flat(&pile);
    let first = plan_of(&pile, &options("coverly-library", &lib));
    let kept = lib.join(item(&first, "Pokemon Y.cia").target.as_ref().unwrap());
    fs::create_dir_all(kept.parent().unwrap()).unwrap();
    fs::copy(pile.join("Pokemon Y.cia"), &kept).unwrap();
    for mode in [OrganizeDedupeMode::Report, OrganizeDedupeMode::SetAside] {
        let mut opts = options("coverly-library", &lib);
        opts.duplicates = mode;
        let plan = plan_of(&pile, &opts);
        let update = item(&plan, "0004000E00055D00.cia");
        assert!(update.moves_in(), "{mode:?} {update:?}");
        assert_eq!(
            below_system(update),
            p(&[
                "Pokemon Y (World)",
                "Updates",
                "Pokemon Y (World) (v1.1.0).cia"
            ]),
            "{mode:?}"
        );
        assert!(update.game.as_ref().unwrap().folder_exists, "{mode:?}");
        let dlc = item(&plan, "Pokemon Y - Extra Pack (USA).cia");
        assert!(dlc.moves_in(), "{mode:?} {dlc:?}");
        assert_eq!(
            below_system(dlc),
            p(&[
                "Pokemon Y (World)",
                "DLC",
                "Pokemon Y (World) - Extra Pack.cia"
            ]),
            "{mode:?}"
        );
    }
}

#[test]
fn children_of_a_base_kept_flat_in_the_library_are_never_left_as_in_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_flat(&pile);
    fs::create_dir_all(lib.join("3DS")).unwrap();
    fs::copy(
        pile.join("Pokemon Y.cia"),
        lib.join("3DS").join("Pokemon Y.cia"),
    )
    .unwrap();
    let mut opts = options("coverly-library", &lib);
    opts.duplicates = OrganizeDedupeMode::Report;
    let plan = plan_of(&pile, &opts);
    let base = item(&plan, "Pokemon Y.cia");
    assert!(
        matches!(
            &base.action,
            OrganizeAction::Left {
                reason: LeftReason::InLibrary { .. }
            }
        ),
        "{base:?}"
    );
    let update = item(&plan, "0004000E00055D00.cia");
    assert!(update.moves_in(), "{update:?}");
    assert_eq!(
        target_parts(update).len(),
        2,
        "flat beside it (D19): {update:?}"
    );
}

/// The whole layout in one plan: a World base with two updates (TMD
/// 1040 and 1056) and a DLC, a demo by its header, and the same game's
/// European base under its own title ID.
#[test]
fn a_full_game_folder_layout_plans_versions_demo_and_two_regions() {
    const EU_BASE: u64 = 0x0004_0000_0005_5E00;
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    let files: Vec<(&str, Vec<u8>)> = vec![
        (
            "Pokemon Y.cia",
            ct::cia_with_code(PY_BASE, "CTR-P-EK2A", false, false).head_bytes(),
        ),
        ("update-a.cia", with_tail(&cia_update(PY_UPDATE, 1040), 1)),
        ("update-b.cia", with_tail(&cia_update(PY_UPDATE, 1056), 2)),
        (
            "Pokemon Y - Extra Pack (USA).cia",
            with_tail(&ct::cia(PY_DLC, true, false), 3),
        ),
        // `Z` is only likely Europe (part C §5): the demo stays region-less.
        (
            "Pokemon Y (Demo).cia",
            ct::cia_with_code(0x0004_0002_0005_5D00, "CTR-P-EK2Z", false, false).head_bytes(),
        ),
        (
            "Pokemon Y (Europe).cia",
            ct::cia_with_code(EU_BASE, "CTR-P-EK2P", false, false).head_bytes(),
        ),
    ];
    for (n, (rel, bytes)) in files.iter().enumerate() {
        put(&pile, rel, bytes, n as u64);
    }
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));

    let world = "Pokemon Y (World)";
    let base = item(&plan, "Pokemon Y.cia");
    assert_eq!(below_system(base), p(&[world, "Pokemon Y (World).cia"]));
    let a = item(&plan, "update-a.cia");
    let b = item(&plan, "update-b.cia");
    assert_eq!(
        below_system(a),
        p(&[world, "Updates", "Pokemon Y (World) (v1.1.0).cia"])
    );
    assert_eq!(
        below_system(b),
        p(&[world, "Updates", "Pokemon Y (World) (v1.2.0).cia"])
    );
    let (la, lb) = (a.game.as_ref().unwrap(), b.game.as_ref().unwrap());
    assert_eq!((la.of, lb.of), (Some(base.id), Some(base.id)));
    assert!(lb.newest_here && !la.newest_here);
    let dlc = item(&plan, "Pokemon Y - Extra Pack (USA).cia");
    assert_eq!(below_system(dlc)[..2], p(&[world, "DLC"]));
    let demo = item(&plan, "Pokemon Y (Demo).cia");
    assert_eq!(demo.game.as_ref().map(|g| g.role), Some(GameRole::Demo));
    // A region-less demo links to the World base, not the European one
    // (mods spec D20).
    assert_eq!(
        demo.game.as_ref().and_then(|g| g.demo_of.as_deref()),
        Some("Pokemon Y")
    );
    assert!(!demo
        .target
        .as_deref()
        .unwrap()
        .components()
        .any(|c| c.as_os_str() == world));
    let eu = item(&plan, "Pokemon Y (Europe).cia");
    assert_eq!(eu.game.as_ref().map(|g| g.role), Some(GameRole::Base));
    assert_ne!(below_system(eu)[0], world);
    insta::assert_json_snapshot!("game_folders_full_layout_plan", rows(&plan));
}

/// A 3DS cartridge `name` with product code `code` (encrypted, so the
/// name comes from the file).
fn pokemon_card(pile: &Path, name: &str, code: &str, n: u64) {
    put(pile, name, &ct::ncsd(code, PY_BASE, false).head_bytes(), n);
}

/// The game folder (below the system folder) of the only item in a pile
/// holding `name` with `code`, and the item's notes.
fn card_folder(name: &str, code: &str) -> (String, Vec<ItemNote>) {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    pokemon_card(&pile, name, code, 1);
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let it = item(&plan, name);
    (below_system(it)[0].clone(), it.notes.clone())
}

#[test]
fn a_3ds_game_is_named_by_its_verified_region_letter() {
    // A country letter writes the country (part C §5, Q2).
    let (folder, notes) = card_folder("Pokemon Y.3ds", "CTR-P-EK2S");
    assert_eq!(folder, "Pokemon Y (Spain)");
    assert!(!notes
        .iter()
        .any(|n| matches!(n, ItemNote::RegionLetterUnknown { .. })));
    // A letter not attested for 3DS adds nothing and is flagged.
    let (folder, notes) = card_folder("Pokemon Y.3ds", "CTR-P-EK2T");
    assert_eq!(folder, "Pokemon Y");
    assert!(notes.contains(&ItemNote::RegionLetterUnknown { letter: 'T' }));
    // A name that already carries a region keeps it.
    let (folder, _) = card_folder("Pokemon Y (Europe).3ds", "CTR-P-EK2S");
    assert_eq!(folder, "Pokemon Y (Europe)");
    // `C` (likely China) and `U` (not attested) no longer add a region.
    let (folder, notes) = card_folder("Pokemon Y.3ds", "CTR-P-EK2C");
    assert_eq!(folder, "Pokemon Y");
    assert!(!notes
        .iter()
        .any(|n| matches!(n, ItemNote::RegionLetterUnknown { .. })));
    let (folder, notes) = card_folder("Pokemon Y.3ds", "CTR-P-EK2U");
    assert_eq!(folder, "Pokemon Y");
    assert!(notes.contains(&ItemNote::RegionLetterUnknown { letter: 'U' }));
}

// Part C D10 (d), D11 (a), D16: a child's own art follows it into the
// library, is read from the package when it has none, and never goes to a
// layout without game folders.

use coverly_core::art::children::{art_json_path, child_slot_path, ChildMedia};

const ICON: &[u8] = b"\x89PNG\r\n\x1a\nicon of the pack";
const ART_JSON: &[u8] = br#"{"icon":{"origin":"user","at":1}}"#;

/// `Pokemon Y/` as a pile game folder: its base, a DLC `Pack.cia`
/// (`with_meta`: its CIA carries a meta SMDH) and, when `art`, the DLC's
/// own icon and origin file in `DLC/.media/`.
fn pokemon_with_pack(pile: &Path, with_meta: bool, art: bool) {
    put(
        pile,
        "Pokemon Y/Pokemon Y.cia",
        &ct::cia_with_code(PY_BASE, "CTR-P-EK2A", false, false).head_bytes(),
        1,
    );
    put(
        pile,
        "Pokemon Y/DLC/Pack.cia",
        &ct::cia(PY_DLC, false, with_meta).bytes(),
        2,
    );
    if art {
        put(pile, "Pokemon Y/DLC/.media/Pack-icon.png", ICON, 3);
        put(pile, "Pokemon Y/DLC/.media/Pack.art.json", ART_JSON, 4);
    }
}

fn child_art_world(with_meta: bool, art: bool) -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib, data) = (
        tmp.path().join("pile"),
        tmp.path().join("lib"),
        tmp.path().join("data"),
    );
    pokemon_with_pack(&pile, with_meta, art);
    fs::create_dir_all(&lib).unwrap();
    (tmp, pile, lib, data)
}

#[test]
fn a_renamed_child_takes_its_art_to_its_new_stem() {
    let (_tmp, pile, lib, data) = child_art_world(false, true);
    let before = files_below(&pile);
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let dlc = item(&plan, "Pack.cia");
    assert!(dlc.moves_in(), "{dlc:?}");
    let target = dlc.target.clone().unwrap();
    assert_ne!(target.file_name().unwrap(), "Pack.cia", "renamed: {dlc:?}");
    let new_child = lib.join(&target);
    let want_icon = child_slot_path(&new_child, ChildMedia::Icon).unwrap();
    let want_json = art_json_path(&new_child);
    let mut art: Vec<(Vec<String>, Vec<String>)> = dlc
        .companions
        .iter()
        .filter(|c| c.child_art)
        .map(|c| (parts(&c.source), parts(&lib.join(&c.target))))
        .collect();
    art.sort();
    let mut want = vec![
        (
            p(&["Pokemon Y", "DLC", ".media", "Pack-icon.png"]),
            parts(&want_icon),
        ),
        (
            p(&["Pokemon Y", "DLC", ".media", "Pack.art.json"]),
            parts(&want_json),
        ),
    ];
    want.sort();
    assert_eq!(art, want, "{dlc:?}");
    assert!(!dlc
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::ChildArtStays { .. })));

    let report = apply_in(&plan, &data);
    assert!(report.paused.is_none(), "{report:?}");
    assert_eq!(fs::read(&want_icon).unwrap(), ICON);
    assert_eq!(fs::read(&want_json).unwrap(), ART_JSON);
    assert!(!pile.join("Pokemon Y/DLC/.media/Pack-icon.png").exists());
    // The same job: undo puts every file back where it was, the child's
    // art first, then the child, then its base.
    let undo = undo_job(&data, report.job.as_deref().unwrap()).unwrap();
    assert_eq!(files_below(&pile), before);
    let restored: Vec<Vec<String>> = undo
        .restored
        .iter()
        .map(|r| parts(r.strip_prefix(&pile).unwrap()))
        .collect();
    let at = |want: &[&str]| {
        restored
            .iter()
            .position(|r| *r == p(want))
            .unwrap_or_else(|| panic!("{want:?} not restored: {restored:?}"))
    };
    let icon = at(&["Pokemon Y", "DLC", ".media", "Pack-icon.png"]);
    let json = at(&["Pokemon Y", "DLC", ".media", "Pack.art.json"]);
    let child = at(&["Pokemon Y", "DLC", "Pack.cia"]);
    let base = at(&["Pokemon Y", "Pokemon Y.cia"]);
    assert!(icon < child && json < child && child < base, "{restored:?}");
    assert_eq!(files_below(&lib), Vec::<Vec<String>>::new());
}

#[test]
fn a_child_without_art_gets_the_icon_its_package_carries() {
    let (_tmp, pile, lib, data) = child_art_world(true, false);
    let before = files_below(&pile);
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let dlc = item(&plan, "Pack.cia");
    assert!(dlc.moves_in(), "{dlc:?}");
    assert!(dlc.companions.iter().all(|c| !c.child_art), "{dlc:?}");
    let new_child = lib.join(dlc.target.as_ref().unwrap());

    let report = apply_in(&plan, &data);
    assert!(report.paused.is_none(), "{report:?}");
    let icon = child_slot_path(&new_child, ChildMedia::Icon).unwrap();
    let bytes = fs::read(&icon).unwrap();
    assert_eq!(
        image::guess_format(&bytes).ok(),
        Some(image::ImageFormat::Png)
    );
    let origin = coverly_core::art::children::read_art_json(&new_child);
    assert_eq!(
        origin.icon.map(|s| s.origin),
        Some(coverly_core::art::children::ChildOrigin::Package)
    );
    // The base is a game, not a child: no slot of its own.
    let base = lib.join(item(&plan, "Pokemon Y.cia").target.as_ref().unwrap());
    assert!(!child_slot_path(&base, ChildMedia::Icon).unwrap().exists());

    undo_job(&data, report.job.as_deref().unwrap()).unwrap();
    assert_eq!(files_below(&pile), before);
    assert_eq!(files_below(&lib), Vec::<Vec<String>>::new());
    assert_eq!(fs::read_dir(&lib).unwrap().count(), 0, "every folder gone");
}

#[test]
fn a_layout_without_child_art_copies_none_and_lists_it() {
    let (_tmp, pile, lib, data) = child_art_world(true, true);
    let before = files_below(&pile);
    let plan = plan_of(&pile, &options("esde", &lib));
    let dlc = item(&plan, "Pack.cia");
    assert!(dlc.companions.iter().all(|c| !c.child_art), "{dlc:?}");
    assert!(dlc.moves_in(), "{dlc:?}");
    assert!(
        dlc.notes.contains(&ItemNote::ChildArtStays { count: 2 }),
        "{dlc:?}"
    );
    let report = apply_in(&plan, &data);
    assert!(report.paused.is_none(), "{report:?}");
    // Nothing with a child slot's name reached the device layout.
    let slots: Vec<Vec<String>> = files_below(&lib)
        .into_iter()
        .filter(|f| {
            f.last()
                .is_some_and(|n| n.ends_with("-icon.png") || n.ends_with(".art.json"))
        })
        .collect();
    assert!(slots.is_empty(), "{slots:?}");
    assert_eq!(
        fs::read(pile.join("Pokemon Y/DLC/.media/Pack-icon.png")).unwrap(),
        ICON
    );
    if let Some(job) = report.job.as_deref() {
        undo_job(&data, job).unwrap();
    }
    assert_eq!(files_below(&pile), before);
}

#[test]
fn a_taken_slot_in_the_library_keeps_the_child_art_where_it_was() {
    let (_tmp, pile, lib, data) = child_art_world(false, true);
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let target = item(&plan, "Pack.cia").target.clone().unwrap();
    // Someone's own icon already sits in the new slot: never replaced.
    let icon = child_slot_path(&lib.join(&target), ChildMedia::Icon).unwrap();
    fs::create_dir_all(icon.parent().unwrap()).unwrap();
    fs::write(&icon, b"mine").unwrap();
    pin_mtime(&icon, 9);
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let dlc = item(&plan, "Pack.cia");
    // The origin file never follows without its slot: it would vouch for
    // someone else's icon.
    assert!(dlc.companions.iter().all(|c| !c.child_art), "{dlc:?}");
    assert!(
        dlc.notes.contains(&ItemNote::ChildArtStays { count: 2 }),
        "{dlc:?}"
    );
    let report = apply_in(&plan, &data);
    assert!(report.paused.is_none(), "{report:?}");
    assert_eq!(fs::read(&icon).unwrap(), b"mine");
    assert_eq!(
        fs::read(pile.join("Pokemon Y/DLC/.media/Pack-icon.png")).unwrap(),
        ICON
    );
    assert_eq!(
        fs::read(pile.join("Pokemon Y/DLC/.media/Pack.art.json")).unwrap(),
        ART_JSON
    );
}
