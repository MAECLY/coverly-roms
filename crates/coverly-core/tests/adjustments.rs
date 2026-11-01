//! Table-driven checks for the adjustments batch (plan P-3): loose files
//! classified by their plaintext headers (spec §8.2), title metadata kinds
//! (D14), folder games inside archives (§8.4) and loose multi-disc sets
//! (D24). Each task appends rows; every check runs over every row. All
//! bytes are synthetic (`consoles::testing`); names may be real.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use coverly_core::cache::Cache;
use coverly_core::consoles::testing::{self as ct, SparseAt};
use coverly_core::consoles::{PartKind, TitleKind};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::identify::evidence::Evidence;
use coverly_core::identify::sniff::sniff_named;
use coverly_core::organize::inspect::inspect_file;
use coverly_core::organize::{
    plan_from_survey, survey, Choice, Decision, LeftReason, MetaProbe, OrganizeAction, OrganizeEnv,
    OrganizeOptions, OrganizePlan, Transfer,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::SystemDb;

/// Builds a synthetic file in memory.
type Build = fn() -> SparseAt;

/// (file name, bytes, system, kind, part): one loose file read by its own
/// header (spec §8.2, D12).
type Loose = (
    &'static str,
    Build,
    &'static str,
    TitleKind,
    Option<PartKind>,
);
const LOOSE: &[Loose] = &[
    // Regression (spec §8.1): a NoCrypto NCCH card stays a 3DS game.
    (
        "Mario Kart 7 (Europe).3ds",
        mario_kart_7,
        "3ds",
        TitleKind::Base,
        None,
    ),
];

/// (title id, system, kind): a `.tmd` by its full title-id high word (D14).
const TMD_KINDS: &[(u64, &str, TitleKind)] = &[
    // Wii U patch.
    (0x0005_000E_1010_ED00, "wiiu", TitleKind::Update),
];

/// Writes an archive at the given path.
type WriteArchive = fn(&Path);
/// (archive name, writer, system): a folder game inside a zip or 7z
/// (spec §8.4, D20–D23).
const ARCHIVE_GAMES: &[(&str, WriteArchive, &str)] = &[
    // Today's VPK reader (`vpk_console`), the template of D20.
    ("Persona 4 Golden.vpk", persona_vpk, "psvita"),
];

/// Writes a pile's files into a folder.
type WritePile = fn(&Path);
/// (case, pile, items, playlists): loose discs and whether they form a set
/// (spec D24). `items` is the plan's item count, `playlists` the number of
/// `.m3u` the plan writes.
const DISC_SETS: &[(&str, WritePile, usize, usize)] = &[
    // Two GameCube discs whose headers name two different games: the
    // headers contradict the names, never a set.
    ("different games", two_games, 2, 0),
];

fn mario_kart_7() -> SparseAt {
    ct::ncsd("CTR-P-AMKP", 0x0004_0000_0003_0800, true)
}

fn persona_vpk(path: &Path) {
    ct::vpk(path, "PCSE00120");
}

fn put(path: &Path, bytes: &[u8], n: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    pin_mtime(path, n);
}

/// A GameCube disc head in an RVZ: `id6` and the disc byte `number`.
fn gc_rvz(id6: &str, number: u8) -> Vec<u8> {
    let mut h = ct::gc_header(id6, false);
    h[6] = number;
    ct::wia(&h, true, 1).head_bytes()
}

fn two_games(dir: &Path) {
    put(
        &dir.join("Resident Evil 4 (USA) (Disc 1).rvz"),
        &gc_rvz("G4BE08", 0),
        1,
    );
    put(
        &dir.join("Resident Evil 4 (USA) (Disc 2).rvz"),
        &gc_rvz("GM8E01", 1),
        2,
    );
}

fn best(hits: &[Evidence]) -> Option<&str> {
    hits.iter()
        .max_by(|a, b| a.weight.total_cmp(&b.weight))
        .map(|e| e.system.as_str())
}

fn ext_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

#[test]
fn loose_files_by_header() {
    for (name, build, system, kind, part) in LOOSE {
        let mut f = build();
        let len = f.len;
        let s = sniff_named(&mut f, len, &ext_of(name), name);
        assert_eq!(best(&s.hits), Some(*system), "{name}");
        let c = s.console.unwrap_or_else(|| panic!("{name}: no console id"));
        assert_eq!((c.kind, c.part), (*kind, *part), "{name}");
    }
}

#[test]
fn title_metadata_kinds() {
    for (tid, system, kind) in TMD_KINDS {
        let mut f = SparseAt::new(0);
        let bytes = ct::tmd(*tid);
        f.len = bytes.len() as u64;
        f.put(0, &bytes);
        let len = f.len;
        let s = sniff_named(&mut f, len, "tmd", "title.tmd");
        let c = s
            .console
            .unwrap_or_else(|| panic!("{tid:016X}: no console id"));
        assert_eq!((c.system.as_str(), c.kind), (*system, *kind), "{tid:016X}");
    }
}

#[test]
fn folder_games_in_archives() {
    let db = SystemDb::builtin();
    let tmp = tempfile::tempdir().unwrap();
    for (i, (name, write, system)) in ARCHIVE_GAMES.iter().enumerate() {
        let path = tmp.path().join(name);
        write(&path);
        pin_mtime(&path, i as u64);
        let len = fs::metadata(&path).unwrap().len();
        let insp = inspect_file(&path, len, &db).unwrap();
        let hits: Vec<Evidence> = insp
            .payloads
            .iter()
            .flat_map(|p| p.sniff.hits.iter().cloned())
            .collect();
        assert_eq!(best(&hits), Some(*system), "{name}");
    }
}

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

fn options() -> OrganizeOptions {
    OrganizeOptions {
        layout: "esde".into(),
        ..OrganizeOptions::default()
    }
}

fn plan(dir: &Path, decisions: &[Decision], world: &World) -> OrganizePlan {
    let env = world.env();
    let s = survey(dir, &options(), &env, &|_| {}, &AtomicBool::new(false)).unwrap();
    plan_from_survey(&s, &options(), decisions, &env).unwrap()
}

fn playlists(plan: &OrganizePlan) -> usize {
    plan.items
        .iter()
        .filter(|i| matches!(&i.transfer, Transfer::Extract { m3u: Some(_), .. }))
        .count()
}

#[test]
fn loose_disc_sets() {
    for (case, write, items, m3u) in DISC_SETS {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path());
        let p = plan(tmp.path(), &[], &World::new());
        assert_eq!((p.items.len(), playlists(&p)), (*items, *m3u), "{case}");
    }
}

/// «No» to a huge item (spec D16): the item stays with `NotConfirmed`, and
/// the answer is never remembered by content (the next plan proposes it
/// again).
#[test]
fn a_declined_item_is_left_not_confirmed_and_not_remembered() {
    let tmp = tempfile::tempdir().unwrap();
    put(
        &tmp.path().join("Mario Kart 7 (Europe).3ds"),
        &mario_kart_7().head_bytes(),
        1,
    );
    let world = World::new();
    let first = plan(tmp.path(), &[], &world);
    let item = &first.items[0];
    let before = item.action.clone();
    assert_ne!(
        before,
        OrganizeAction::Left {
            reason: LeftReason::NotConfirmed
        }
    );
    let declined = plan(
        tmp.path(),
        &[Decision {
            item: item.id,
            choice: Choice::Decline,
        }],
        &world,
    );
    let left = &declined.items[0];
    assert_eq!(
        left.action,
        OrganizeAction::Left {
            reason: LeftReason::NotConfirmed
        }
    );
    assert_eq!(left.target, None);
    let again = plan(tmp.path(), &[], &world);
    assert_eq!(again.items[0].action, before);
}
