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
                ("Mods".into(), FixedFolder::Mods, "Pokemon Y".into()),
                ("Saves".into(), FixedFolder::Saves, "Pokemon Y".into()),
            ],
            "{layout}"
        );
        for folder in ["Mods", "Saves", "Extras"] {
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
