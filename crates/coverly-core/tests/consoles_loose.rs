//! Loose files read by `consoles/loose.rs` and the existing readers
//! through the walk, the inspection and the planner (adjustments spec
//! §8.2, D12, D13): one positive and one near miss per format, game
//! parts left with `GamePart`, homebrew moved with its note, the Wii
//! homebrew `apps/<name>/` folder as one item, a Vita backup never read,
//! and the D24 budgets on the largest fixture. All bytes are synthetic.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use coverly_core::cache::Cache;
use coverly_core::consoles::testing::{self as ct, SparseAt};
use coverly_core::consoles::{
    identify, Budgeted, PartKind, TitleKind, MAX_DECODED, MAX_READ, MAX_SEEKS,
};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::organize::inspect::inspect_file;
use coverly_core::organize::{
    plan_from_survey, survey, ItemKind, ItemNote, LeftReason, MetaProbe, OrganizeAction,
    OrganizeEnv, OrganizeItem, OrganizeOptions, OrganizePlan,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::SystemDb;

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

    fn env(&self) -> OrganizeEnv<'_> {
        OrganizeEnv {
            db: &self.db,
            profiles: &self.profiles,
            cache: &self.cache,
            dats: None,
            probe: &NoCloud,
            online: None,
            titles: None,
            now: SystemTime::now(),
        }
    }
}

fn plan(dir: &Path) -> OrganizePlan {
    let world = World::new();
    let env = world.env();
    let opts = OrganizeOptions {
        layout: "esde".into(),
        ..OrganizeOptions::default()
    };
    let s = survey(dir, &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
    plan_from_survey(&s, &opts, &[], &env).unwrap()
}

fn put(path: &Path, bytes: &[u8], n: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    pin_mtime(path, n);
}

fn item_named<'a>(plan: &'a OrganizePlan, name: &str) -> &'a OrganizeItem {
    plan.items
        .iter()
        .find(|i| i.sources[0].file_name().is_some_and(|n| n == name))
        .unwrap_or_else(|| panic!("{name} not in the plan: {:?}", plan.items))
}

fn part(item: &OrganizeItem) -> Option<(PartKind, &str, Option<&str>)> {
    match &item.action {
        OrganizeAction::Left {
            reason:
                LeftReason::GamePart {
                    part,
                    system,
                    title_id,
                },
        } => Some((*part, system.as_str(), title_id.as_deref())),
        _ => None,
    }
}

/// (file name, bytes, verdict): `Some((system, kind, part))` for a
/// positive, `None` for a near miss (no console verdict).
type Row = (
    &'static str,
    fn() -> Vec<u8>,
    Option<(&'static str, TitleKind, Option<PartKind>)>,
);

const ROWS: &[Row] = &[
    (
        "main.npdm",
        || ct::npdm(0x0100_0000_0001_0000),
        Some(("switch", TitleKind::Unknown, Some(PartKind::ExeFs))),
    ),
    ("main.npdm", || ct::npdm(1)[4..].to_vec(), None),
    (
        "0123456789abcdef0123456789abcdef.nca",
        || vec![0u8; 0x400],
        Some(("switch", TitleKind::Unknown, Some(PartKind::Nca))),
    ),
    // 31 hex digits: not an NCA name.
    (
        "0123456789abcdef0123456789abcde.nca",
        || vec![0u8; 0x400],
        None,
    ),
    (
        "swiss.dol",
        || ct::dol(1, false),
        Some(("gamecube", TitleKind::Homebrew, None)),
    ),
    ("data.dol", dol_outside_ram, None),
    (
        "multiman.self",
        || ct::self_header([0, 0, 0, 2], 4, 0x1010_0000_0100_0003),
        Some(("ps3", TitleKind::Homebrew, None)),
    ),
    ("odd.self", || ct::self_header([0, 0, 0, 9], 4, 0), None),
    (
        "EBOOT.BIN",
        || ct::self_header([3, 0, 0, 0], 8, 0x2100_0000_0000_0001),
        Some(("psvita", TitleKind::Unknown, Some(PartKind::ExeFs))),
    ),
    (
        "EBOOT.PBP",
        || ct::pbp("ME", "SLUS00594", b"PSISOIMG0000"),
        Some(("psx", TitleKind::Base, None)),
    ),
    (
        "EBOOT.PBP",
        || ct::pbp("ME", "SLUS00594", b"")[2..].to_vec(),
        None,
    ),
    (
        "cxmb.prx",
        || ct::prx("cxmb"),
        Some(("psp", TitleKind::Unknown, Some(PartKind::Module))),
    ),
    (
        "plain.prx",
        || vec![0x7F, b'E', b'L', b'F', 0, 0, 0, 0],
        None,
    ),
    (
        "game.psvimg",
        || vec![0xA5; 0x400],
        Some(("psvita", TitleKind::Unknown, Some(PartKind::Backup))),
    ),
];

/// A DOL whose first text section loads below main RAM.
fn dol_outside_ram() -> Vec<u8> {
    let mut d = ct::dol(1, false);
    d[0x48..0x4C].copy_from_slice(&0x0000_3100u32.to_be_bytes());
    d
}

#[test]
fn every_format_has_a_positive_and_a_near_miss() {
    let db = SystemDb::builtin();
    let tmp = tempfile::tempdir().unwrap();
    for (i, (name, build, want)) in ROWS.iter().enumerate() {
        let dir = tmp.path().join(i.to_string());
        let path = dir.join(name);
        put(&path, &build(), i as u64);
        let insp = inspect_file(&path, fs::metadata(&path).unwrap().len(), &db).unwrap();
        let got = insp.payloads[0].sniff.console.as_ref();
        match want {
            Some((system, kind, part)) => {
                let c = got.unwrap_or_else(|| panic!("{i} {name}: no verdict"));
                assert_eq!(
                    (c.system.as_str(), c.kind, c.part),
                    (*system, *kind, *part),
                    "{i} {name}"
                );
            }
            None => assert!(got.is_none(), "{i} {name}: {got:?}"),
        }
    }
}

/// A Vita backup is told by its name: the inspection reads none of it.
#[test]
fn a_backup_is_left_with_the_title_id_of_its_path() {
    let tmp = tempfile::tempdir().unwrap();
    // `PS Vita/PGAME/<account>/PCSE00123/game/` as the user pointed
    // Coverly at the account folder (the walk goes four levels down).
    let game = tmp.path().join("PCSE00123").join("game");
    put(&game.join("game.psvimg"), &[0xA5; 0x400], 1);
    put(&game.join("game.psvmd"), &[0xA5; 0x100], 2);
    let db = SystemDb::builtin();
    let insp = inspect_file(&game.join("game.psvimg"), 0x400, &db).unwrap();
    assert_eq!(insp.payloads[0].sniff.read, 0);
    let p = plan(tmp.path());
    for name in ["game.psvimg", "game.psvmd"] {
        assert_eq!(
            part(item_named(&p, name)),
            Some((PartKind::Backup, "psvita", Some("PCSE00123"))),
            "{name}"
        );
    }
}

/// D13: a part is never moved on its own; the NPDM's program id is the
/// game's title id.
#[test]
fn game_parts_are_left_where_they_are() {
    let tmp = tempfile::tempdir().unwrap();
    let exefs = tmp.path().join("exefs");
    put(
        &exefs.join("main.npdm"),
        &ct::npdm(0x0100_0000_0001_0000),
        1,
    );
    put(&exefs.join("main.nso"), &ct::nso().bytes(), 2);
    put(
        &tmp.path()
            .join("nca")
            .join("0123456789abcdef0123456789abcdef.nca"),
        &[0u8; 0x400],
        3,
    );
    put(
        &tmp.path().join("plugins").join("cxmb.prx"),
        &ct::prx("cxmb"),
        4,
    );
    put(
        &tmp.path().join("dump").join("EBOOT.BIN"),
        &ct::self_header([0, 0, 0, 2], 4, 0x1010_0000_0100_0003),
        5,
    );
    let p = plan(tmp.path());
    assert_eq!(
        part(item_named(&p, "main.npdm")),
        Some((PartKind::ExeFs, "switch", Some("0100000000010000")))
    );
    assert_eq!(
        part(item_named(&p, "main.nso")),
        Some((PartKind::ExeFs, "switch", None))
    );
    assert_eq!(
        part(item_named(&p, "0123456789abcdef0123456789abcdef.nca")),
        Some((PartKind::Nca, "switch", None))
    );
    assert_eq!(
        part(item_named(&p, "cxmb.prx")),
        Some((PartKind::Module, "psp", None))
    );
    assert_eq!(
        part(item_named(&p, "EBOOT.BIN")),
        Some((PartKind::ExeFs, "ps3", None))
    );
    for item in &p.items {
        assert!(item.target.is_none(), "{item:?}");
    }
}

/// Homebrew goes to its system folder with the homebrew note (D13).
#[test]
fn homebrew_moves_with_its_note() {
    let tmp = tempfile::tempdir().unwrap();
    put(&tmp.path().join("swiss.dol"), &ct::dol(1, false), 1);
    put(
        &tmp.path().join("Checkpoint.nro"),
        &ct::nro("Checkpoint", "3.8.0", &[]).bytes(),
        2,
    );
    put(
        &tmp.path().join("FTPD.3dsx"),
        &ct::dsx_with_smdh("FTPD").bytes(),
        3,
    );
    put(
        &tmp.path().join("hbl.pbp"),
        &ct::pbp("MG", "000000001", b""),
        4,
    );
    let p = plan(tmp.path());
    for (name, system) in [
        ("swiss.dol", "gamecube"),
        ("Checkpoint.nro", "switch"),
        ("FTPD.3dsx", "3ds"),
        ("hbl.pbp", "psp"),
    ] {
        let item = item_named(&p, name);
        assert_eq!(item.system_id.as_deref(), Some(system), "{name}: {item:?}");
        assert_eq!(item.action, OrganizeAction::Auto, "{name}: {item:?}");
        assert!(
            item.notes.contains(&ItemNote::Homebrew),
            "{name}: {:?}",
            item.notes
        );
    }
}

/// A PS1 classic in an `EBOOT.PBP` goes to `psx`, a PSP game to `psp`.
#[test]
fn pbps_go_to_their_system() {
    let tmp = tempfile::tempdir().unwrap();
    put(
        &tmp.path().join("Final Fantasy VII (USA).pbp"),
        &ct::pbp("ME", "SLUS00594", b"PSISOIMG0000"),
        1,
    );
    put(
        &tmp.path().join("Lumines (USA).pbp"),
        &ct::pbp("UG", "ULUS10041", b""),
        2,
    );
    let p = plan(tmp.path());
    let ff7 = item_named(&p, "Final Fantasy VII (USA).pbp");
    assert_eq!(ff7.system_id.as_deref(), Some("psx"), "{ff7:?}");
    let lumines = item_named(&p, "Lumines (USA).pbp");
    assert_eq!(lumines.system_id.as_deref(), Some("psp"), "{lumines:?}");
    for item in [ff7, lumines] {
        assert!(!item.notes.contains(&ItemNote::Homebrew));
    }
}

/// `apps/<name>/boot.dol` is a Homebrew Channel app: the folder is one
/// item, moved whole with the homebrew note (D13).
#[test]
fn a_homebrew_channel_app_folder_is_one_item() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("apps").join("usbloader_gx");
    put(&app.join("boot.dol"), &ct::dol(2, true), 1);
    put(
        &app.join("meta.xml"),
        b"<app><name>USB Loader GX</name></app>",
        2,
    );
    put(&app.join("icon.png"), &ct::png_8x8(), 3);
    let p = plan(tmp.path());
    assert_eq!(p.items.len(), 1, "{:?}", p.items);
    let item = &p.items[0];
    assert_eq!(item.kind, ItemKind::Folder);
    assert_eq!(item.sources[0].file_name(), app.file_name());
    assert_eq!(item.system_id.as_deref(), Some("wii"));
    assert!(item.notes.contains(&ItemNote::Homebrew), "{:?}", item.notes);
    // A `boot.dol` outside `apps/` is a loose file.
    let other = tempfile::tempdir().unwrap();
    put(
        &other.path().join("tools").join("boot.dol"),
        &ct::dol(1, true),
        4,
    );
    put(&other.path().join("tools").join("readme.txt"), b"hi", 5);
    let p = plan(other.path());
    let item = item_named(&p, "boot.dol");
    assert_eq!(item.kind, ItemKind::File);
}

/// The largest fixture (seven 1 MiB text sections, the Wii word past the
/// first MiB) stays within D24: at most 1 MiB of text searched, the read
/// and seek caps kept; the word is not reached, so it is a GameCube DOL.
#[test]
fn a_huge_dol_stays_within_the_budgets() {
    const MIB: u32 = 1 << 20;
    let mut h = vec![0u8; 0x100];
    for i in 0..7u32 {
        let at = 0x100 + i * MIB;
        let o = i as usize * 4;
        h[o..o + 4].copy_from_slice(&at.to_be_bytes());
        h[0x48 + o..0x4C + o].copy_from_slice(&(0x8000_3100 + i * 0x10_0000).to_be_bytes());
        h[0x90 + o..0x94 + o].copy_from_slice(&MIB.to_be_bytes());
    }
    h[0xE0..0xE4].copy_from_slice(&0x8000_3100u32.to_be_bytes());
    let mut f = SparseAt::new(0x100 + 7 * u64::from(MIB));
    f.put(0, &h);
    f.put(0x100 + u64::from(MIB), &0x7C73_FBA6u32.to_be_bytes());
    let len = f.len;
    let mut b = Budgeted::new(&mut f, len);
    let c = identify(&mut b, "dol", "big.dol").unwrap();
    assert_eq!(c.system, "gamecube");
    let used = b.used();
    // The other readers' heads (a few KiB) plus at most 1 MiB of text.
    assert!(used.read <= MAX_DECODED + 0x4000, "{used:?}");
    assert!(used.read <= MAX_READ && used.decoded <= MAX_DECODED);
    assert!(used.seeks <= MAX_SEEKS, "{used:?}");
}

/// A disc a tool extracted (Dolphin «Extract Entire Disc»: `sys/` holds
/// `main.dol` beside `boot.bin`, `bi2.bin` and `apploader.img`) is a game
/// folder: its DOL is the game's executable, left as a part, never moved
/// as homebrew (D13, research §11).
#[test]
fn an_extracted_disc_dol_is_a_game_part() {
    let tmp = tempfile::tempdir().unwrap();
    let sys = tmp.path().join("GALE01").join("sys");
    put(&sys.join("main.dol"), &ct::dol(1, false), 1);
    put(&sys.join("boot.bin"), &[0u8; 0x440], 2);
    put(&sys.join("bi2.bin"), &[0u8; 0x2000], 3);
    put(&sys.join("apploader.img"), &[0u8; 0x100], 4);
    let p = plan(tmp.path());
    let item = item_named(&p, "main.dol");
    assert_eq!(
        part(item),
        Some((PartKind::ExeFs, "gamecube", None)),
        "{item:?}"
    );
    assert!(
        !item.notes.contains(&ItemNote::Homebrew),
        "{:?}",
        item.notes
    );
    assert!(item.target.is_none(), "{item:?}");
}
