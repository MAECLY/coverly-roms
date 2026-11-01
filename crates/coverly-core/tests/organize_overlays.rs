//! shadPS4 sibling overlays follow their base (systems batch 3 D11):
//! derived names, collisions, the base's action after the user's
//! decisions; and the Batocera `ps4/DLC/<TITLE_ID>/` path (D16).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use coverly_core::cache::Cache;
use coverly_core::consoles::testing::{cnt_pkg, param_sfo_with, ps4_app, ps4_sfo, put_file};
use coverly_core::organize::{
    plan_from_survey, survey, Choice, Decision, ItemNote, LeftReason, MetaProbe, OrganizeAction,
    OrganizeEnv, OrganizeItem, OrganizeOptions, OrganizePlan, RenameReason,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

const BLOODBORNE: &str = "UP9000-CUSA00900_00-BLOODBORNE000000";
const OLD_HUNTERS: &str = "UP9000-CUSA00900_00-OLDHUNTERS000000";

struct NoCloud;

impl MetaProbe for NoCloud {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }
}

/// The plan's clock: well after every pinned fixture mtime.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000 + 100_000)
}

struct World {
    db: SystemDb,
    profiles: Vec<Profile>,
    cache: Cache,
}

impl World {
    fn new() -> World {
        let mut profiles = builtin_profiles();
        // A custom profile saved from Batocera before batch 3: its stored
        // maps have no `dlc_folder` (D16).
        let mut old = profiles
            .iter()
            .find(|p| p.id == "batocera")
            .unwrap()
            .clone();
        old.id = "my-batocera".into();
        old.name = "My Batocera".into();
        old.dlc_folder.clear();
        profiles.push(old);
        World {
            db: SystemDb::builtin(),
            profiles,
            cache: Cache::in_memory().unwrap(),
        }
    }

    fn plan_with(
        &self,
        pile: &Path,
        target: &Path,
        layout: &str,
        decisions: &dyn Fn(&OrganizePlan) -> Vec<Decision>,
    ) -> OrganizePlan {
        let env = OrganizeEnv {
            db: &self.db,
            profiles: &self.profiles,
            cache: &self.cache,
            dats: None,
            probe: &NoCloud,
            online: None,
            titles: None,
            now: now(),
        };
        let opts = OrganizeOptions {
            layout: layout.to_string(),
            target: Some(target.to_path_buf()),
            offline: true,
            ..OrganizeOptions::default()
        };
        let s = survey(pile, &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
        let first = plan_from_survey(&s, &opts, &[], &env).unwrap();
        let decisions = decisions(&first);
        if decisions.is_empty() {
            return first;
        }
        plan_from_survey(&s, &opts, &decisions, &env).unwrap()
    }

    fn plan(&self, pile: &Path, target: &Path, layout: &str) -> OrganizePlan {
        self.plan_with(pile, target, layout, &|_| Vec::new())
    }

    fn profile_name(&self, id: &str) -> String {
        self.profiles
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .name
            .clone()
    }
}

fn by<'a>(p: &'a OrganizePlan, name: &str) -> &'a OrganizeItem {
    p.items
        .iter()
        .find(|i| i.sources[0] == Path::new(name))
        .unwrap_or_else(|| panic!("{name}: {:#?}", p.items))
}

/// The components of an item's target below the target root's ROMs
/// folder, compared by component (Windows CI): `["ps4", "CUSA00900"]`.
fn placed(p: &OrganizePlan, item: &OrganizeItem) -> Vec<String> {
    let target = item
        .target
        .as_ref()
        .unwrap_or_else(|| panic!("a target: {item:#?}"));
    let abs = p.target_root.join(target);
    abs.strip_prefix(&p.roms_dir)
        .unwrap()
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect()
}

/// A PS4 `param.sfo` titled `title`.
fn sfo(title_id: &str, category: &str, content_id: &str, title: &str) -> Vec<u8> {
    param_sfo_with(
        &[
            ("APP_VER", "01.09"),
            ("CATEGORY", category),
            ("CONTENT_ID", content_id),
            ("TITLE", title),
            ("TITLE_ID", title_id),
            ("VERSION", "01.00"),
        ],
        &[("APP_TYPE", 1), ("PUBTOOLVER", 0x0289_0000)],
    )
}

/// `CUSA00900` (Bloodborne) and `CUSA00900<suffix>` (its update) in `pile`.
fn base_and_update(pile: &Path, suffix: &str) {
    let base = pile.join("CUSA00900");
    ps4_app(&base, "CUSA00900", "gd", BLOODBORNE);
    put_file(
        &base,
        "sce_sys/param.sfo",
        &sfo("CUSA00900", "gd", BLOODBORNE, "Bloodborne"),
    );
    ps4_app(
        &pile.join(format!("CUSA00900{suffix}")),
        "CUSA00900",
        "gp",
        BLOODBORNE,
    );
}

/// A DLC label folder of Bloodborne (`CATEGORY ac`).
fn dlc_folder(pile: &Path, name: &str) {
    let dir = pile.join(name);
    put_file(
        &dir,
        "sce_sys/param.sfo",
        &sfo("CUSA00900", "ac", OLD_HUNTERS, "The Old Hunters"),
    );
    put_file(&dir, "content.dat", &[7u8; 64]);
}

fn overlay(of: &str) -> OrganizeAction {
    OrganizeAction::Left {
        reason: LeftReason::GameOverlay { of: of.into() },
    }
}

#[test]
fn batocera_derives_the_update_name_and_places_dlc_in_its_folder() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    base_and_update(pile.path(), "-UPDATE");
    dlc_folder(pile.path(), "The Old Hunters");

    let p = world.plan(pile.path(), target.path(), "batocera");
    let base = by(&p, "CUSA00900");
    let update = by(&p, "CUSA00900-UPDATE");
    let dlc = by(&p, "The Old Hunters");
    assert_eq!(placed(&p, base), ["ps4", "CUSA00900.ps4"]);
    assert_eq!(placed(&p, update), ["ps4", "CUSA00900.ps4-UPDATE"]);
    assert_eq!(update.action, OrganizeAction::Auto);
    // Only the base is listed: the sibling gets no folder suffix note.
    assert!(
        !update
            .notes
            .iter()
            .any(|n| matches!(n, ItemNote::FolderSuffix { .. })),
        "{:?}",
        update.notes
    );
    assert!(update.notes.contains(&ItemNote::Update {
        of: "Bloodborne".into()
    }));
    assert_eq!(dlc.system_id.as_deref(), Some("ps4"));
    assert_eq!(
        placed(&p, dlc),
        ["ps4", "DLC", "CUSA00900", "The Old Hunters"]
    );
    assert_eq!(dlc.folder.as_deref(), Some("ps4"));
    assert!(
        dlc.notes.contains(&ItemNote::DlcFolder {
            layout: world.profile_name("batocera"),
        }),
        "{:?}",
        dlc.notes
    );
    assert!(
        !dlc.notes
            .iter()
            .any(|n| matches!(n, ItemNote::FolderSuffix { .. })),
        "{:?}",
        dlc.notes
    );
    assert_eq!(
        world.profile_name("batocera"),
        "Batocera / Knulli / EmulationStation"
    );
}

#[test]
fn esde_keeps_plain_names_and_dlc_beside_the_game() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    base_and_update(pile.path(), "-UPDATE");
    dlc_folder(pile.path(), "The Old Hunters");

    let p = world.plan(pile.path(), target.path(), "esde");
    assert_eq!(placed(&p, by(&p, "CUSA00900")), ["ps4", "CUSA00900"]);
    assert_eq!(
        placed(&p, by(&p, "CUSA00900-UPDATE")),
        ["ps4", "CUSA00900-UPDATE"]
    );
    let dlc = by(&p, "The Old Hunters");
    assert_eq!(placed(&p, dlc), ["ps4", "The Old Hunters"]);
    assert!(dlc.notes.contains(&ItemNote::Dlc {
        of: "Bloodborne".into()
    }));
    assert!(!dlc
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::DlcFolder { .. })));
}

#[test]
fn a_collision_carries_over_and_a_taken_sibling_name_keeps_the_group() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    base_and_update(pile.path(), "-UPDATE");

    let p = world.plan(pile.path(), target.path(), "esde");
    let ps4 = p.roms_dir.join("ps4");
    fs::create_dir_all(ps4.join("CUSA00900")).unwrap();
    let p = world.plan(pile.path(), target.path(), "esde");
    let base = by(&p, "CUSA00900");
    let update = by(&p, "CUSA00900-UPDATE");
    assert_eq!(placed(&p, base), ["ps4", "CUSA00900 (2)"]);
    assert_eq!(placed(&p, update), ["ps4", "CUSA00900 (2)-UPDATE"]);
    assert_eq!(
        update.rename,
        Some(RenameReason::Collision {
            name: "CUSA00900-UPDATE".into()
        })
    );

    fs::create_dir_all(ps4.join("CUSA00900 (2)-UPDATE")).unwrap();
    let p = world.plan(pile.path(), target.path(), "esde");
    for name in ["CUSA00900", "CUSA00900-UPDATE"] {
        let item = by(&p, name);
        assert_eq!(
            item.action,
            OrganizeAction::Left {
                reason: LeftReason::CollisionSet
            },
            "{name}"
        );
        assert_eq!(item.target, None);
    }
}

#[test]
fn a_base_in_review_holds_its_sibling_until_decided() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let base = pile.path().join("CUSA00900");
    let contradicted = param_sfo_with(
        &[("CATEGORY", "gd"), ("TITLE_ID", "CUSA00900")],
        &[("PSP2_SYSTEM_VER", 0x0360_0000)],
    );
    put_file(&base, "sce_sys/param.sfo", &contradicted);
    put_file(&base, "eboot.bin", &[0u8; 64]);
    ps4_app(
        &pile.path().join("CUSA00900-UPDATE"),
        "CUSA00900",
        "gp",
        BLOODBORNE,
    );

    let p = world.plan(pile.path(), target.path(), "esde");
    let base = by(&p, "CUSA00900");
    let update = by(&p, "CUSA00900-UPDATE");
    assert_eq!(base.action, OrganizeAction::Review);
    assert_eq!(update.action, OrganizeAction::Review);
    assert_eq!(update.system_id, base.system_id);
    assert_eq!(update.alternatives, base.alternatives);
    assert_eq!(update.target, None);

    // The user's choice on the base applies to its sibling.
    let assign = |p: &OrganizePlan| {
        vec![Decision {
            item: by(p, "CUSA00900").id,
            choice: Choice::Assign {
                system: "ps4".into(),
            },
        }]
    };
    let p = world.plan_with(pile.path(), target.path(), "esde", &assign);
    let update = by(&p, "CUSA00900-UPDATE");
    assert_eq!(by(&p, "CUSA00900").action, OrganizeAction::Auto);
    assert_eq!(update.action, OrganizeAction::Auto);
    assert_eq!(placed(&p, update), ["ps4", "CUSA00900-UPDATE"]);

    let leave = |p: &OrganizePlan| {
        vec![Decision {
            item: by(p, "CUSA00900").id,
            choice: Choice::Leave,
        }]
    };
    let p = world.plan_with(pile.path(), target.path(), "esde", &leave);
    assert!(by(&p, "CUSA00900").is_left());
    assert_eq!(by(&p, "CUSA00900-UPDATE").action, overlay("CUSA00900"));
}

#[test]
fn a_base_left_by_a_gate_or_by_the_user_holds_its_sibling() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    base_and_update(pile.path(), "-UPDATE");

    let leave = |p: &OrganizePlan| {
        vec![Decision {
            item: by(p, "CUSA00900").id,
            choice: Choice::Leave,
        }]
    };
    let p = world.plan_with(pile.path(), target.path(), "esde", &leave);
    assert!(by(&p, "CUSA00900").is_left());
    let update = by(&p, "CUSA00900-UPDATE");
    assert_eq!(update.action, overlay("Bloodborne"));
    assert_eq!(update.target, None);

    // A file of the base written just now: the base may still be copied.
    let recent = pile.path().join("CUSA00900").join("eboot.bin");
    fs::OpenOptions::new()
        .write(true)
        .open(&recent)
        .unwrap()
        .set_modified(now())
        .unwrap();
    let p = world.plan(pile.path(), target.path(), "esde");
    assert_eq!(
        by(&p, "CUSA00900").action,
        OrganizeAction::Left {
            reason: LeftReason::RecentlyModified
        }
    );
    assert_eq!(by(&p, "CUSA00900-UPDATE").action, overlay("Bloodborne"));
}

#[test]
fn a_lower_case_suffix_keeps_its_spelling() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    base_and_update(pile.path(), "-update");

    let p = world.plan(pile.path(), target.path(), "batocera");
    assert_eq!(
        placed(&p, by(&p, "CUSA00900-update")),
        ["ps4", "CUSA00900.ps4-update"]
    );
}

#[test]
fn an_update_and_a_patch_both_follow() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    base_and_update(pile.path(), "-UPDATE");
    ps4_app(
        &pile.path().join("CUSA00900-patch"),
        "CUSA00900",
        "gp",
        BLOODBORNE,
    );

    let p = world.plan(pile.path(), target.path(), "batocera");
    assert_eq!(
        placed(&p, by(&p, "CUSA00900-UPDATE")),
        ["ps4", "CUSA00900.ps4-UPDATE"]
    );
    assert_eq!(
        placed(&p, by(&p, "CUSA00900-patch")),
        ["ps4", "CUSA00900.ps4-patch"]
    );
}

#[test]
fn a_sibling_without_its_base_keeps_its_own_name() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    ps4_app(
        &pile.path().join("CUSA00900-UPDATE"),
        "CUSA00900",
        "gp",
        BLOODBORNE,
    );

    let p = world.plan(pile.path(), target.path(), "esde");
    let update = by(&p, "CUSA00900-UPDATE");
    assert_eq!(update.action, OrganizeAction::Auto);
    assert_eq!(placed(&p, update), ["ps4", "CUSA00900-UPDATE"]);
    assert!(
        update.notes.contains(&ItemNote::Update {
            of: "CUSA00900".into()
        }),
        "{:?}",
        update.notes
    );
}

#[test]
fn a_custom_profile_without_a_dlc_folder_places_dlc_beside_the_game() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    base_and_update(pile.path(), "-UPDATE");
    dlc_folder(pile.path(), "The Old Hunters");

    let p = world.plan(pile.path(), target.path(), "my-batocera");
    let dlc = by(&p, "The Old Hunters");
    assert_eq!(placed(&p, dlc).first().map(String::as_str), Some("ps4"));
    assert_eq!(placed(&p, dlc).len(), 2, "{:?}", dlc.target);
    assert!(!dlc
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::DlcFolder { .. })));
}

#[test]
fn a_batocera_dlc_pkg_stays_in_the_system_folder() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let pkg = cnt_pkg(
        OLD_HUNTERS,
        0x1B,
        0,
        &[(0x1000, 0, ps4_sfo("CUSA00900", "ac", OLD_HUNTERS, 1))],
    );
    let name = format!("{OLD_HUNTERS}.pkg");
    put_file(pile.path(), &name, &pkg.bytes());

    let p = world.plan(pile.path(), target.path(), "batocera");
    let item = by(&p, &name);
    assert_eq!(item.system_id.as_deref(), Some("ps4"));
    assert_eq!(placed(&p, item), ["ps4".to_string(), name.clone()]);
    assert!(item.notes.contains(&ItemNote::ExtractFirst));
    assert!(!item
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::DlcFolder { .. })));
}

#[test]
fn a_batocera_dlc_zar_goes_to_the_dlc_folder() {
    use coverly_core::consoles::testing::zarchive;
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let dlc = ps4_sfo("CUSA00900", "ac", OLD_HUNTERS, 1);
    put_file(
        pile.path(),
        "The Old Hunters.zar",
        &zarchive(&[("sce_sys/param.sfo", &dlc)]).bytes(),
    );

    let p = world.plan(pile.path(), target.path(), "batocera");
    let item = by(&p, "The Old Hunters.zar");
    assert_eq!(item.system_id.as_deref(), Some("ps4"));
    assert_eq!(
        placed(&p, item),
        ["ps4", "DLC", "CUSA00900", "The Old Hunters.zar"]
    );
    assert!(item.notes.contains(&ItemNote::DlcFolder {
        layout: world.profile_name("batocera"),
    }));
}

#[test]
fn derived_paths_are_relative_to_the_target_root() {
    // The sibling's target is relative like every other target.
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    base_and_update(pile.path(), "-UPDATE");
    let p = world.plan(pile.path(), target.path(), "batocera");
    let t: &PathBuf = by(&p, "CUSA00900-UPDATE").target.as_ref().unwrap();
    assert!(t.is_relative(), "{t:?}");
}

#[test]
fn applying_a_batocera_plan_writes_the_sibling_and_dlc_paths() {
    use coverly_core::organize::{apply_organize, ApplyEnv, ItemStatus};
    use coverly_core::write::trash::FakeTrash;
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    base_and_update(pile.path(), "-UPDATE");
    dlc_folder(pile.path(), "The Old Hunters");

    let p = world.plan(pile.path(), target.path(), "batocera");
    let report = apply_organize(
        &p,
        &ApplyEnv {
            data_dir: data.path(),
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
            trash: &FakeTrash::unavailable("no-trash"),
            tools: Some(&[]),
        },
    )
    .unwrap();
    for name in ["CUSA00900", "CUSA00900-UPDATE", "The Old Hunters"] {
        let id = by(&p, name).id;
        let outcome = report.items.iter().find(|o| o.item == id).unwrap();
        assert!(
            matches!(outcome.status, ItemStatus::Moved | ItemStatus::Copied),
            "{name}: {:?}",
            outcome.status
        );
    }
    let ps4 = p.roms_dir.join("ps4");
    assert!(ps4.join("CUSA00900.ps4").join("eboot.bin").is_file());
    assert!(ps4
        .join("CUSA00900.ps4-UPDATE")
        .join("sce_sys")
        .join("param.sfo")
        .is_file());
    assert!(ps4
        .join("DLC")
        .join("CUSA00900")
        .join("The Old Hunters")
        .join("content.dat")
        .is_file());
}
