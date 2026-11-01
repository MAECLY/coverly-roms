//! Current-generation folder probes and file readers through the walk
//! and the planner (systems batch 3 §8): PS4 app folders through the one
//! `sce_sys` probe, the Vita beside them, a PSF contradiction, and the
//! files that are never opened.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::SystemTime;

use coverly_core::cache::Cache;
use coverly_core::consoles::testing::{
    cnt_pkg, exfat_image, fih_pkg, param_sfo_with, pkg, ps4_app, ps4_sfo, ps5_app, ps5_param_json,
    put_file, ufs2_image, vita_app,
};
use coverly_core::consoles::{start_open_log, take_open_log, IdSource};
use coverly_core::identify::evidence::IdLevel;
use coverly_core::organize::{
    plan_organize, ItemKind, ItemNote, MetaProbe, OrganizeAction, OrganizeEnv, OrganizeItem,
    OrganizeOptions, OrganizePlan, Transfer,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

/// The open log is process-wide: tests that read it take turns.
static SERIAL: Mutex<()> = Mutex::new(());

const BLOODBORNE: &str = "UP9000-CUSA00900_00-BLOODBORNE000000";

struct NoCloud;

impl MetaProbe for NoCloud {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }
}

struct World {
    db: SystemDb,
    profiles: Vec<Profile>,
    cache: Cache,
}

impl World {
    fn new() -> World {
        World {
            db: SystemDb::builtin(),
            profiles: builtin_profiles(),
            cache: Cache::in_memory().unwrap(),
        }
    }

    fn plan(&self, pile: &Path, target: &Path, layout: &str) -> OrganizePlan {
        let env = OrganizeEnv {
            db: &self.db,
            profiles: &self.profiles,
            cache: &self.cache,
            dats: None,
            probe: &NoCloud,
            online: None,
            now: SystemTime::now(),
        };
        let opts = OrganizeOptions {
            layout: layout.to_string(),
            target: Some(target.to_path_buf()),
            offline: true,
            ..OrganizeOptions::default()
        };
        plan_organize(pile, &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap()
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

fn only_folder(p: &OrganizePlan) -> &OrganizeItem {
    let all: Vec<&OrganizeItem> = p
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Folder)
        .collect();
    assert_eq!(all.len(), 1, "{:#?}", p.items);
    all[0]
}

/// The last component of a path, compared by component (Windows CI).
fn last(path: &Path) -> String {
    path.components()
        .next_back()
        .unwrap()
        .as_os_str()
        .to_string_lossy()
        .into_owned()
}

/// `(system folder, game folder)` of an item's target.
fn placed(item: &OrganizeItem) -> (String, String) {
    let t = item.target.as_ref().expect("a target");
    (t.parent().map(last).unwrap(), last(t))
}

#[test]
fn a_ps4_app_folder_is_one_item_in_the_ps4_folder() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    ps4_app(
        &pile.path().join("CUSA00900"),
        "CUSA00900",
        "gd",
        BLOODBORNE,
    );

    let p = world.plan(pile.path(), target.path(), "esde");
    assert_eq!(p.items.len(), 1, "{:#?}", p.items);
    let item = only_folder(&p);
    assert_eq!(item.sources, [PathBuf::from("CUSA00900")]);
    assert_eq!(item.system_id.as_deref(), Some("ps4"));
    assert_eq!(item.action, OrganizeAction::Auto);
    assert_eq!(placed(item), ("ps4".to_string(), "CUSA00900".to_string()));
    let tid = item.title_id.as_ref().expect("a title id");
    assert_eq!(
        (tid.id.as_str(), tid.source),
        ("CUSA00900", IdSource::ParamSfo)
    );
    assert_eq!(item.content_id.as_deref(), Some(BLOODBORNE));
    assert!(item.why.iter().any(|e| e.detail == "sce_sys/param.sfo"));
    // The licence inside moves with the folder: no item of its own.
    assert!(p
        .items
        .iter()
        .all(|i| i.sources[0].file_name().is_none_or(|n| n != "license.dat")));

    // Batocera wants the folder renamed to end with `.ps4` (D16).
    let p = world.plan(pile.path(), target.path(), "batocera");
    let item = only_folder(&p);
    assert_eq!(
        placed(item),
        ("ps4".to_string(), "CUSA00900.ps4".to_string())
    );
    assert!(
        item.notes.contains(&ItemNote::FolderSuffix {
            name: "CUSA00900.ps4".into(),
            layout: world.profile_name("batocera"),
        }),
        "{:?}",
        item.notes
    );
}

#[test]
fn eboot_and_license_are_never_opened() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let dir = pile.path().join("CUSA00900");
    ps4_app(&dir, "CUSA00900", "gd", BLOODBORNE);

    start_open_log();
    let p = world.plan(pile.path(), target.path(), "esde");
    let log = take_open_log();
    assert_eq!(only_folder(&p).system_id.as_deref(), Some("ps4"));
    let ours: Vec<&PathBuf> = log.iter().filter(|o| o.starts_with(&dir)).collect();
    assert_eq!(
        ours,
        [&dir.join("sce_sys").join("param.sfo")],
        "only the probe file is opened"
    );
}

#[test]
fn a_vita_app_beside_a_ps4_app_stays_vita() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    vita_app(&pile.path().join("PCSE00844"), "PCSE00844");
    ps4_app(
        &pile.path().join("CUSA00900"),
        "CUSA00900",
        "gd",
        BLOODBORNE,
    );

    let p = world.plan(pile.path(), target.path(), "esde");
    assert_eq!(p.items.len(), 2, "{:#?}", p.items);
    let by_source = |name: &str| {
        p.items
            .iter()
            .find(|i| i.sources[0] == Path::new(name))
            .unwrap()
    };
    let vita = by_source("PCSE00844");
    assert_eq!(vita.system_id.as_deref(), Some("psvita"));
    assert_eq!(
        placed(vita),
        ("psvita".to_string(), "PCSE00844".to_string())
    );
    assert_eq!(by_source("CUSA00900").system_id.as_deref(), Some("ps4"));
}

#[test]
fn a_cusa_id_with_a_vita_firmware_key_goes_to_review() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let dir = pile.path().join("CUSA00900");
    let sfo = param_sfo_with(
        &[("CATEGORY", "gd"), ("TITLE_ID", "CUSA00900")],
        &[("PSP2_SYSTEM_VER", 0x0360_0000)],
    );
    put_file(&dir, "sce_sys/param.sfo", &sfo);
    put_file(&dir, "eboot.bin", &[0u8; 64]);

    let p = world.plan(pile.path(), target.path(), "esde");
    let item = only_folder(&p);
    assert_eq!(item.action, OrganizeAction::Review);
    assert_eq!(item.system_id, None);
    let alts: Vec<&str> = item.alternatives.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(alts, ["ps4", "psvita"]);
}

#[test]
fn a_ps4_update_folder_groups_under_its_base_and_a_launcher_is_ps4() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    ps4_app(
        &pile.path().join("CUSA00900"),
        "CUSA00900",
        "gd",
        BLOODBORNE,
    );
    ps4_app(
        &pile.path().join("CUSA00900-UPDATE"),
        "CUSA00900",
        "gp",
        BLOODBORNE,
    );
    put_file(pile.path(), "CUSA07010.ps4", b"CUSA07010\n");

    let p = world.plan(pile.path(), target.path(), "esde");
    assert_eq!(p.items.len(), 3, "{:#?}", p.items);
    let pos = |name: &str| {
        p.items
            .iter()
            .position(|i| i.sources[0] == Path::new(name))
            .unwrap()
    };
    let (base, update) = (pos("CUSA00900"), pos("CUSA00900-UPDATE"));
    // D15: grouped by the title id, the update listed after its base.
    assert!(base < update, "{:#?}", p.items);
    let update = &p.items[update];
    assert_eq!(update.action, OrganizeAction::Auto);
    assert_eq!(
        placed(update),
        ("ps4".to_string(), "CUSA00900-UPDATE".to_string())
    );
    assert!(
        update.notes.contains(&ItemNote::Update {
            of: "Test Game".into()
        }),
        "{:?}",
        update.notes
    );
    // D17: the ES-DE launcher text names the PS4.
    let launcher = &p.items[pos("CUSA07010.ps4")];
    assert_eq!(
        (launcher.kind, launcher.system_id.as_deref()),
        (ItemKind::File, Some("ps4"))
    );
    assert_eq!(
        placed(launcher),
        ("ps4".to_string(), "CUSA07010.ps4".to_string())
    );
}

/// Spec D9: a PS4 PKG in a pile moves to `ps4/` as it is, catalogued
/// from its plaintext header and `param.sfo` entry, with `ExtractFirst`;
/// a PS3 PKG keeps batch 2's `InstallFirst`; a macOS installer is still
/// left as not a ROM.
#[test]
fn a_ps4_pkg_moves_as_is_with_the_extract_first_note() {
    const EU: &str = "EP0002-CUSA04447_00-SAMPLEGAME000000";
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let ps4 = cnt_pkg(
        EU,
        0x1A,
        0,
        &[(0x1000, 0, ps4_sfo("CUSA04447", "gd", EU, 1))],
    );
    let ps4_name = format!("{EU}.pkg");
    put_file(pile.path(), &ps4_name, &ps4.bytes());
    let ps3 = pkg(1, "UP0001-NPUB30024_00-0000000000000001");
    put_file(pile.path(), "NPUB30024.pkg", &ps3.bytes());
    let mut xar = b"xar!\0\x1c\0\x01".to_vec();
    xar.resize(4096, 0);
    put_file(pile.path(), "Install Thing.pkg", &xar);

    let p = world.plan(pile.path(), target.path(), "esde");
    let by = |name: &str| {
        p.items
            .iter()
            .find(|i| i.sources[0] == Path::new(name))
            .unwrap_or_else(|| panic!("{name}: {:#?}", p.items))
    };
    let item = by(&ps4_name);
    assert_eq!(item.system_id.as_deref(), Some("ps4"));
    assert_eq!(item.action, OrganizeAction::Auto);
    assert_eq!(placed(item), ("ps4".to_string(), ps4_name.clone()));
    assert_eq!(item.content_id.as_deref(), Some(EU));
    let tid = item.title_id.as_ref().expect("a title id");
    assert_eq!(
        (tid.id.as_str(), tid.source),
        ("CUSA04447", IdSource::ParamSfo)
    );
    assert!(
        item.notes.contains(&ItemNote::ExtractFirst),
        "{:?}",
        item.notes
    );
    assert!(!item
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::InstallFirst { .. })));

    let item = by("NPUB30024.pkg");
    assert_eq!(item.system_id.as_deref(), Some("ps3"));
    assert!(item.notes.contains(&ItemNote::InstallFirst {
        emulator: "RPCS3".into()
    }));
    assert!(!item.notes.contains(&ItemNote::ExtractFirst));

    assert_eq!(
        by("Install Thing.pkg").action,
        OrganizeAction::Left {
            reason: coverly_core::organize::LeftReason::NotARom {
                what: coverly_core::identify::sniff::NotARom::Installer
            }
        }
    );
}

/// The region a PKG's content id names rides on the console id (D8).
#[test]
fn a_ps4_pkg_header_gives_the_region() {
    use coverly_core::identify::names::Region;
    use coverly_core::identify::sniff::sniff;
    let mut f = cnt_pkg("EP0002-CUSA04447_00-SAMPLEGAME000000", 0x1A, 0, &[]);
    let len = f.len;
    let s = sniff(&mut f, len, "pkg");
    let con = s.console.expect("a console id");
    assert_eq!(
        (con.system.as_str(), con.region),
        ("ps4", Some(Region::Europe))
    );
}

/// Spec D6: a PS5 app folder is one item in `ps5/`, Pro-enhanced by its
/// `attribute3`; ES-DE has no PS5 folder on the device. A folder of the
/// same title id without `eboot.bin` is its DLC, listed after it.
#[test]
fn a_ps5_app_folder_and_its_dlc() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    ps5_app(
        &pile.path().join("PPSA01284"),
        &ps5_param_json("PPSA01284", "Returnal", true),
        true,
    );
    ps5_app(
        &pile.path().join("PPSA01284 DLC"),
        &ps5_param_json("PPSA01284", "Returnal Soundtrack", false),
        false,
    );

    let p = world.plan(pile.path(), target.path(), "esde");
    assert_eq!(p.items.len(), 2, "{:#?}", p.items);
    let base = &p.items[0];
    assert_eq!(base.sources, [PathBuf::from("PPSA01284")]);
    assert_eq!(base.kind, ItemKind::Folder);
    assert_eq!(base.system_id.as_deref(), Some("ps5"));
    assert_eq!(base.action, OrganizeAction::Auto);
    assert_eq!(placed(base), ("ps5".to_string(), "PPSA01284".to_string()));
    let tid = base.title_id.as_ref().expect("a title id");
    assert_eq!(
        (tid.id.as_str(), tid.source),
        ("PPSA01284", IdSource::ParamJson)
    );
    assert_eq!(
        base.content_id.as_deref(),
        Some("UP9000-PPSA01284_00-SAMPLEGAME000000")
    );
    assert!(
        base.notes.contains(&ItemNote::ProEnhanced),
        "{:?}",
        base.notes
    );
    assert!(
        base.notes
            .iter()
            .any(|n| matches!(n, ItemNote::NoFolderOnDevice { .. })),
        "{:?}",
        base.notes
    );
    assert!(!base.notes.contains(&ItemNote::NoEmulator));

    let dlc = &p.items[1];
    assert_eq!(dlc.sources, [PathBuf::from("PPSA01284 DLC")]);
    assert_eq!(dlc.system_id.as_deref(), Some("ps5"));
    assert_eq!(dlc.action, OrganizeAction::Auto);
    assert!(
        dlc.notes.contains(&ItemNote::Dlc {
            of: "Returnal".into()
        }),
        "{:?}",
        dlc.notes
    );
    assert!(!dlc.notes.contains(&ItemNote::ProEnhanced));
}

/// Spec D10: the PS5 files go to `ps5/`; the store PKG, the exFAT image
/// and the PFS images carry `NoEmulator`, a UFS2 `.ffpkg` plays.
#[test]
fn ps5_files_and_the_no_emulator_note() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let store = "UP9000-PPSA01284_00-SAMPLEGAME000000.pkg";
    put_file(
        pile.path(),
        store,
        &fih_pkg("UP9000-PPSA01284_00-SAMPLEGAME000000", 0x1A).bytes(),
    );
    put_file(pile.path(), "PPSA09519.exfat", &exfat_image().bytes());
    put_file(pile.path(), "game.ffpkg", &ufs2_image(true).bytes());
    put_file(pile.path(), "x.ffpfs", &[0xE7u8; 4096]);
    put_file(pile.path(), "y.ffpfsc", &[0xE8u8; 4096]);

    let p = world.plan(pile.path(), target.path(), "esde");
    let by = |name: &str| {
        p.items
            .iter()
            .find(|i| i.sources[0] == Path::new(name))
            .unwrap_or_else(|| panic!("{name}: {:#?}", p.items))
    };
    for (name, no_emulator) in [
        (store, true),
        ("PPSA09519.exfat", true),
        ("game.ffpkg", false),
        ("x.ffpfs", true),
        ("y.ffpfsc", true),
    ] {
        let item = by(name);
        assert_eq!(item.system_id.as_deref(), Some("ps5"), "{name}");
        assert_eq!(item.action, OrganizeAction::Auto, "{name}");
        assert_eq!(
            placed(item),
            ("ps5".to_string(), name.to_string()),
            "{name}"
        );
        assert_eq!(
            item.notes.contains(&ItemNote::NoEmulator),
            no_emulator,
            "{name}: {:?}",
            item.notes
        );
        assert!(!item.notes.contains(&ItemNote::ExtractFirst), "{name}");
    }
    let tid = by(store).title_id.as_ref().expect("a title id");
    assert_eq!(
        (tid.id.as_str(), tid.source),
        ("PPSA01284", IdSource::Header)
    );
    let tid = by("PPSA09519.exfat").title_id.as_ref().expect("a title id");
    assert_eq!(
        (tid.id.as_str(), tid.source),
        ("PPSA09519", IdSource::FileName)
    );
    assert!(by("game.ffpkg")
        .why
        .iter()
        .any(|e| e.detail == coverly_core::consoles::orbis::UFS2_DETAIL));
}

/// Writes a zip at `path` holding `files` (`(name, bytes)`).
fn zip_of(path: &Path, files: &[(&str, &[u8])]) {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for (name, bytes) in files {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    coverly_core::dedupe::testing::pin_mtime(path, 1);
}

/// Spec D13: a zip of a PS5 app folder is a `ps5` item taken as is; the
/// same shape holding a PS4 app is unchanged from batch 2 (not a PS4
/// item).
#[test]
fn a_ps5_app_archive_is_taken_as_is() {
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let param = ps5_param_json("PPSA01284", "Returnal", false);
    // Pro-enhanced: the archive item carries the note too (D6).
    let other = ps5_param_json("PPSA02000", "Other Game", true);
    zip_of(
        &pile.path().join("PPSA01284.zip"),
        &[
            ("PPSA01284/eboot.bin", &[0u8; 64]),
            ("PPSA01284/sce_sys/param.json", &param),
            ("PPSA01284/sce_sys/icon0.png", &[0x89, b'P', b'N', b'G']),
        ],
    );
    zip_of(
        &pile.path().join("Root.zip"),
        &[("eboot.bin", &[0u8; 64]), ("sce_sys/param.json", &other)],
    );
    let sfo = ps4_sfo("CUSA00900", "gd", BLOODBORNE, 1);
    zip_of(
        &pile.path().join("CUSA00900.zip"),
        &[
            ("CUSA00900/eboot.bin", &[0u8; 64]),
            ("CUSA00900/sce_sys/param.sfo", &sfo),
        ],
    );
    // Two folders deep is not an app archive.
    zip_of(
        &pile.path().join("Deep.zip"),
        &[
            ("a/PPSA01284/eboot.bin", &[0u8; 64]),
            ("a/PPSA01284/sce_sys/param.json", &param),
        ],
    );

    let p = world.plan(pile.path(), target.path(), "esde");
    let by = |name: &str| {
        p.items
            .iter()
            .find(|i| i.sources[0] == Path::new(name))
            .unwrap_or_else(|| panic!("{name}: {:#?}", p.items))
    };
    for (name, want) in [("PPSA01284.zip", "PPSA01284"), ("Root.zip", "PPSA02000")] {
        let item = by(name);
        assert_eq!(item.kind, ItemKind::Archive, "{name}");
        assert_eq!(item.system_id.as_deref(), Some("ps5"), "{name}");
        assert_eq!(item.action, OrganizeAction::Auto, "{name}");
        assert_eq!(item.transfer, Transfer::AsIs, "{name}");
        assert_eq!(
            placed(item),
            ("ps5".to_string(), name.to_string()),
            "{name}"
        );
        let tid = item.title_id.as_ref().expect("a title id");
        assert_eq!((tid.id.as_str(), tid.source), (want, IdSource::ParamJson));
        assert_eq!(
            item.notes.contains(&ItemNote::ProEnhanced),
            name == "Root.zip",
            "{name}: {:?}",
            item.notes
        );
        assert!(!item.notes.contains(&ItemNote::NoEmulator), "{name}");
    }
    for name in ["CUSA00900.zip", "Deep.zip"] {
        let item = by(name);
        // Batch 2: no ROM inside, so the archive is left unknown.
        assert_eq!(item.system_id, None, "{name}");
        assert_eq!(
            item.action,
            OrganizeAction::Left {
                reason: coverly_core::organize::LeftReason::Unknown
            },
            "{name}"
        );
    }
}

/// Spec D12: a `.zar` is filed by its tree. A PS4 tree goes to `ps4/`, a
/// PS5 tree to `ps5/`, a `default.xex` to `xbox360/`; a tree that says
/// nothing (or cannot be read) goes to review among the three, and a
/// `.zar` already inside `xbox360/` stays where it is.
#[test]
fn a_zar_is_classified_by_its_tree() {
    use coverly_core::consoles::testing::{xex2, zarchive};
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let sfo = ps4_sfo("CUSA04447", "gd", "EP0002-CUSA04447_00-GRAVITYRUSH00000", 1);
    let json = ps5_param_json("PPSA01284", "Returnal", false);
    let files: [(&str, Vec<u8>); 4] = [
        (
            "Gravity Rush.zar",
            zarchive(&[("eboot.bin", &[0u8; 64]), ("sce_sys/param.sfo", &sfo)]).bytes(),
        ),
        (
            "Returnal.zar",
            zarchive(&[("eboot.bin", &[0u8; 64]), ("sce_sys/param.json", &json)]).bytes(),
        ),
        (
            "Halo.zar",
            zarchive(&[("default.xex", &xex2(0x4D53_07E6))]).bytes(),
        ),
        (
            "Mystery.zar",
            zarchive(&[("media/a.bin", &[0u8; 8])]).bytes(),
        ),
    ];
    for (name, bytes) in &files {
        put_file(pile.path(), name, bytes);
    }
    // A footer whose tree points past the end: unreadable.
    let mut broken = zarchive(&[("eboot.bin", &[0u8; 64])]).bytes();
    let tail = broken.len() - 0x90;
    broken[tail + 0x30..tail + 0x38].copy_from_slice(&u64::MAX.to_be_bytes());
    put_file(pile.path(), "Broken.zar", &broken);
    put_file(
        &pile.path().join("xbox360"),
        "Old.zar",
        &zarchive(&[("media/a.bin", &[0u8; 8])]).bytes(),
    );

    let p = world.plan(pile.path(), target.path(), "esde");
    let by = |name: &str| {
        p.items
            .iter()
            .find(|i| i.sources[0] == Path::new(name))
            .unwrap_or_else(|| panic!("{name}: {:#?}", p.items))
    };
    for (name, system, tid) in [
        ("Gravity Rush.zar", "ps4", "CUSA04447"),
        ("Returnal.zar", "ps5", "PPSA01284"),
        ("Halo.zar", "xbox360", "4D5307E6"),
    ] {
        let item = by(name);
        assert_eq!(item.system_id.as_deref(), Some(system), "{name}");
        assert_eq!(item.action, OrganizeAction::Auto, "{name}");
        assert_eq!(
            placed(item),
            (system.to_string(), name.to_string()),
            "{name}"
        );
        assert_eq!(
            item.title_id.as_ref().map(|t| t.id.as_str()),
            Some(tid),
            "{name}"
        );
    }
    assert!(by("Gravity Rush.zar")
        .why
        .iter()
        .any(|e| e.detail == coverly_core::consoles::xbox::ZAR_PARAM_SFO_DETAIL));
    for name in ["Mystery.zar", "Broken.zar"] {
        let item = by(name);
        assert_eq!(item.action, OrganizeAction::Review, "{name}: {item:#?}");
        // The Shared extension only (0.50 each): the suggestion and its
        // alternatives are the three systems, no header evidence.
        assert!(
            item.why.iter().all(|e| e.kind == IdLevel::Extension),
            "{name}: {:?}",
            item.why
        );
        let mut alts: Vec<&str> = item
            .system_id
            .iter()
            .map(String::as_str)
            .chain(item.alternatives.iter().map(|(s, _)| s.as_str()))
            .collect();
        alts.sort_unstable();
        assert_eq!(alts, ["ps4", "ps5", "xbox360"], "{name}");
    }
    // Already sorted: no item, the folder is reported as sorted.
    assert!(p
        .items
        .iter()
        .all(|i| i.sources[0].file_name().is_none_or(|n| n != "Old.zar")));
    assert!(
        p.sorted_folders
            .iter()
            .any(|(f, s)| f == Path::new("xbox360") && s == "xbox360"),
        "{:?}",
        p.sorted_folders
    );
}

/// Spec D12 and D6: a PS4 DLC `.zar` (`param.sfo` without `eboot.bin`)
/// is a `ps4` DLC listed under its base, and a PS5 `.zar` enhanced for
/// the PS5 Pro carries `ProEnhanced` (and plays: no `NoEmulator`).
#[test]
fn a_dlc_zar_and_a_pro_zar_keep_their_kind_and_note() {
    use coverly_core::consoles::testing::zarchive;
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let base = ps4_sfo("CUSA04447", "gd", "EP0002-CUSA04447_00-GRAVITYRUSH00000", 1);
    let dlc = ps4_sfo("CUSA04447", "", "EP0002-CUSA04447_00-GRAVITYRUSHDLC01", 1);
    let pro = ps5_param_json("PPSA01284", "Returnal", true);
    put_file(
        pile.path(),
        "Gravity Rush.zar",
        &zarchive(&[("eboot.bin", &[0u8; 64]), ("sce_sys/param.sfo", &base)]).bytes(),
    );
    put_file(
        pile.path(),
        "Gravity Rush DLC.zar",
        &zarchive(&[("sce_sys/param.sfo", &dlc)]).bytes(),
    );
    put_file(
        pile.path(),
        "Returnal.zar",
        &zarchive(&[("eboot.bin", &[0u8; 64]), ("sce_sys/param.json", &pro)]).bytes(),
    );

    let p = world.plan(pile.path(), target.path(), "esde");
    let by = |name: &str| {
        p.items
            .iter()
            .find(|i| i.sources[0] == Path::new(name))
            .unwrap_or_else(|| panic!("{name}: {:#?}", p.items))
    };
    let dlc = by("Gravity Rush DLC.zar");
    assert_eq!(dlc.system_id.as_deref(), Some("ps4"));
    assert_eq!(dlc.action, OrganizeAction::Auto, "{dlc:#?}");
    assert!(
        dlc.notes.contains(&ItemNote::Dlc {
            of: "Test Game".into()
        }),
        "{:?}",
        dlc.notes
    );
    assert_eq!(
        dlc.content_id.as_deref(),
        Some("EP0002-CUSA04447_00-GRAVITYRUSHDLC01")
    );
    let base = by("Gravity Rush.zar");
    assert!(
        !base.notes.iter().any(|n| matches!(n, ItemNote::Dlc { .. })),
        "{:?}",
        base.notes
    );
    let returnal = by("Returnal.zar");
    assert_eq!(returnal.system_id.as_deref(), Some("ps5"));
    assert_eq!(returnal.action, OrganizeAction::Auto);
    assert!(
        returnal.notes.contains(&ItemNote::ProEnhanced),
        "{:?}",
        returnal.notes
    );
    assert!(!returnal.notes.contains(&ItemNote::NoEmulator));
}

#[test]
fn game_overlay_names_the_base_title() {
    // Plan P-3: the walk names the base folder; the planner names the
    // base's title when the base is an item with a header title.
    use coverly_core::organize::LeftReason;
    let world = World::new();
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let base = pile.path().join("CUSA00900");
    ps4_app(&base, "CUSA00900", "gd", BLOODBORNE);
    let sfo = param_sfo_with(
        &[
            ("APP_VER", "01.09"),
            ("CATEGORY", "gd"),
            ("CONTENT_ID", BLOODBORNE),
            ("TITLE", "Bloodborne"),
            ("TITLE_ID", "CUSA00900"),
            ("VERSION", "01.00"),
        ],
        &[("APP_TYPE", 1), ("PUBTOOLVER", 0x0289_0000)],
    );
    put_file(&base, "sce_sys/param.sfo", &sfo);
    put_file(pile.path(), "CUSA00900-mods/x.bin", &[1u8; 64]);

    let p = world.plan(pile.path(), target.path(), "esde");
    let mods = p
        .items
        .iter()
        .find(|i| i.sources[0] == Path::new("CUSA00900-mods"))
        .expect("the -mods row");
    assert_eq!(
        mods.action,
        OrganizeAction::Left {
            reason: LeftReason::GameOverlay {
                of: "Bloodborne".into()
            }
        }
    );
    assert!(
        p.items
            .iter()
            .all(|i| i.sources[0].file_name().is_none_or(|n| n != "x.bin")),
        "{:#?}",
        p.items
    );
    let base = only_folder(&p);
    assert_eq!(base.action, OrganizeAction::Auto);
}
