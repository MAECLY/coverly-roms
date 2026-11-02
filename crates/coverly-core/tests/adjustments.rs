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
use coverly_core::catalog::{CatalogTitle, TitleNames};
use coverly_core::consoles::testing::{self as ct, SparseAt};
use coverly_core::consoles::{IdSource, PartKind, TitleIdRef, TitleKind, TitleSource};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::identify::evidence::{Evidence, IdLevel};
use coverly_core::identify::sniff::{sniff_named, NotARom};
use coverly_core::organize::evidence::Verdict;
use coverly_core::organize::inspect::inspect_file;
use coverly_core::organize::{
    plan_from_survey, survey, Choice, Decision, ItemNote, LeftReason, MetaProbe, OrganizeAction,
    OrganizeEnv, OrganizeOptions, OrganizePlan, Transfer,
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
    // Switch homebrew with a NACP (§8.2).
    (
        "Checkpoint.nro",
        checkpoint_nro,
        "switch",
        TitleKind::Homebrew,
        None,
    ),
    // A game's ExeFS executable.
    (
        "main.nso",
        ct::nso,
        "switch",
        TitleKind::Unknown,
        Some(PartKind::ExeFs),
    ),
    // A loose content archive, by its name only (D6).
    (
        "0123456789abcdef0123456789abcdef.nca",
        empty_nca,
        "switch",
        TitleKind::Unknown,
        Some(PartKind::Nca),
    ),
    // 3DS homebrew (§8.2): read by `ctr.rs`, never shadowed.
    ("FTPD.3dsx", ftpd_3dsx, "3ds", TitleKind::Homebrew, None),
    // A loose Cafe OS executable is homebrew; a library is a part (§8.2).
    ("ftpiiu.rpx", cafe_elf, "wiiu", TitleKind::Homebrew, None),
    (
        "coreinit.rpl",
        cafe_elf,
        "wiiu",
        TitleKind::Unknown,
        Some(PartKind::Library),
    ),
    // A loose TMD is title metadata of the system its title id names.
    (
        "title.tmd",
        tmd_3ds_update,
        "3ds",
        TitleKind::Update,
        Some(PartKind::TitleMeta),
    ),
    // XBE homebrew: no title id, or only hard-disk media.
    (
        "evox.xbe",
        xbe_no_title_id,
        "xbox",
        TitleKind::Homebrew,
        None,
    ),
    ("xbmc.xbe", xbe_hard_disk, "xbox", TitleKind::Homebrew, None),
    // A retail XBE alone in its folder is the game.
    ("Burnout 3.xbe", xbe_retail, "xbox", TitleKind::Base, None),
    // XEX module flags: a patch, a library, a title.
    (
        "Halo 3 TU.xex",
        xex_patch,
        "xbox360",
        TitleKind::Update,
        None,
    ),
    (
        "xam.xex",
        xex_dll,
        "xbox360",
        TitleKind::Unknown,
        Some(PartKind::Module),
    ),
    (
        "Halo 3 (USA).xex",
        xex_title,
        "xbox360",
        TitleKind::Base,
        None,
    ),
    // A development title id is not guessed.
    (
        "homebrew.xex",
        xex_dev_id,
        "xbox360",
        TitleKind::Unknown,
        None,
    ),
];

/// (title id, system, kind): a `.tmd` by its full title-id high word (D14).
const TMD_KINDS: &[(u64, &str, TitleKind)] = &[
    (0x0004_0000_0003_0800, "3ds", TitleKind::Base),
    (0x0004_0001_0003_0800, "3ds", TitleKind::Base),
    (0x0004_0002_0003_0800, "3ds", TitleKind::Demo),
    // A 3DS patch is an update, never DLC (a bit test would say DLC).
    (0x0004_000E_0003_0800, "3ds", TitleKind::Update),
    (0x0004_008C_0003_0800, "3ds", TitleKind::Dlc),
    (0x0001_0000_5241_4145, "wii", TitleKind::Base),
    (0x0001_0001_5741_4145, "wii", TitleKind::Base),
    (0x0001_0004_5241_4145, "wii", TitleKind::Base),
    (0x0001_0005_5241_4145, "wii", TitleKind::Dlc),
    (0x0005_0000_1010_ED00, "wiiu", TitleKind::Base),
    (0x0005_0002_1010_ED00, "wiiu", TitleKind::Demo),
    // Wii U patch.
    (0x0005_000E_1010_ED00, "wiiu", TitleKind::Update),
    (0x0005_000C_1010_ED00, "wiiu", TitleKind::Dlc),
    // DSiWare.
    (0x0003_0004_4B41_4145, "nds", TitleKind::Base),
];

/// Title ids whose TMD is a console's own title (D14): never a ROM.
const TMD_SYSTEM: &[u64] = &[
    0x0004_0010_0002_0000,
    0x0004_001B_0001_0002,
    0x0004_0030_0000_2000,
    0x0004_009B_0001_0002,
    0x0004_00DB_0001_0002,
    0x0004_0130_0000_1002,
    0x0004_0138_0000_0002,
    0x0001_0002_4841_4141,
    0x0001_0008_4845_4141,
    0x0000_0001_0000_0002,
    0x0005_0010_1000_4000,
    0x0005_001B_1004_7000,
    0x0005_004E_1004_9000,
    0x0007_0002_0000_0002,
    0x0007_0008_0000_0002,
    0x0000_0007_0000_0050,
    0x0003_0015_484E_4241,
    0x0003_0017_484E_4141,
];

/// High words no table names: no verdict, no guess (D14).
const TMD_UNKNOWN: &[u64] = &[
    0x0004_0004_0003_0800,
    0x0004_0006_0003_0800,
    0x0005_0001_1010_ED00,
    0x0006_0000_0000_0000,
    0x0100_0000_0001_0000,
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

fn ftpd_3dsx() -> SparseAt {
    ct::dsx_with_smdh("FTPD")
}

fn checkpoint_nro() -> SparseAt {
    ct::nro("Checkpoint", "3.8.0", &[])
}

fn bytes(b: Vec<u8>) -> SparseAt {
    let mut f = SparseAt::new(b.len() as u64);
    f.put(0, &b);
    f
}

fn cafe_elf() -> SparseAt {
    bytes(ct::rpx(0xFE))
}

fn tmd_3ds_update() -> SparseAt {
    bytes(ct::tmd(0x0004_000E_0003_0800))
}

fn xbe_no_title_id() -> SparseAt {
    bytes(ct::xbe_loose(0, ct::XBE_MEDIA_DVD_X2, 0))
}

fn xbe_hard_disk() -> SparseAt {
    bytes(ct::xbe_loose(
        0x4D53_0004,
        ct::XBE_MEDIA_HARD_DISK | ct::XBE_MEDIA_NONSECURE_HARD_DISK,
        0,
    ))
}

fn xbe_retail() -> SparseAt {
    bytes(ct::xbe_loose(
        0x4541_000D,
        ct::XBE_MEDIA_HARD_DISK | ct::XBE_MEDIA_DVD_X2,
        0,
    ))
}

fn xex_patch() -> SparseAt {
    bytes(ct::xex_loose(
        ct::XEX_MODULE_TITLE | ct::XEX_PATCH_DELTA,
        0x4D53_07E6,
        1,
        1,
    ))
}

fn xex_dll() -> SparseAt {
    bytes(ct::xex_loose(ct::XEX_MODULE_DLL, 0x4D53_07E6, 1, 1))
}

fn xex_title() -> SparseAt {
    bytes(ct::xex_loose(ct::XEX_MODULE_TITLE, 0x4D53_07E6, 1, 1))
}

fn xex_dev_id() -> SparseAt {
    bytes(ct::xex_loose(ct::XEX_MODULE_TITLE, 0xFFFE_07D1, 1, 1))
}

/// Any bytes: a loose NCA is never read.
fn empty_nca() -> SparseAt {
    let mut f = SparseAt::new(0x400);
    f.put(0x200, b"NCA3");
    f
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

fn sniff_tmd(b: Vec<u8>) -> coverly_core::identify::sniff::Sniff {
    let mut f = bytes(b);
    let len = f.len;
    sniff_named(&mut f, len, "tmd", "title.tmd")
}

#[test]
fn title_metadata_kinds() {
    for (tid, system, kind) in TMD_KINDS {
        let s = sniff_tmd(ct::tmd(*tid));
        assert_eq!(best(&s.hits), Some(*system), "{tid:016X}");
        let c = s
            .console
            .unwrap_or_else(|| panic!("{tid:016X}: no console id"));
        assert_eq!((c.system.as_str(), c.kind), (*system, *kind), "{tid:016X}");
        assert_eq!(c.part, Some(PartKind::TitleMeta), "{tid:016X}");
        assert_eq!(c.id_source, Some(IdSource::Tmd), "{tid:016X}");
        assert_eq!(c.title_id, Some(format!("{tid:016X}")));
        assert_eq!(c.version.as_deref(), Some("32"), "{tid:016X}");
    }
    for tid in TMD_SYSTEM {
        let s = sniff_tmd(ct::tmd(*tid));
        assert_eq!(s.not_a_rom, Some(NotARom::ConsoleSystem), "{tid:016X}");
        assert!(s.hits.is_empty() && s.console.is_none(), "{tid:016X}");
    }
    for tid in TMD_UNKNOWN {
        let s = sniff_tmd(ct::tmd(*tid));
        assert!(s.console.is_none() && s.not_a_rom.is_none(), "{tid:016X}");
    }
}

/// The TMD header follows the signature, whose length the signature type
/// gives (spec §12): ECDSA, RSA-4096 and HMAC heads read at their own
/// offsets; an unknown type is not identified.
#[test]
fn title_metadata_header_follows_the_signature_type() {
    let tid = 0x0004_008C_0003_0800;
    for sig in [
        0x0001_0000,
        0x0001_0001,
        0x0001_0002,
        0x0001_0003,
        0x0001_0004,
        0x0001_0005,
        0x0001_0006,
    ] {
        let c = sniff_tmd(ct::tmd_signed(sig, tid, 1040))
            .console
            .unwrap_or_else(|| panic!("{sig:08X}: no console id"));
        assert_eq!(
            (c.system.as_str(), c.kind, c.version.as_deref()),
            ("3ds", TitleKind::Dlc, Some("1040")),
            "{sig:08X}"
        );
    }
    let s = sniff_tmd(ct::tmd_signed(0x0002_0004, tid, 1040));
    assert!(s.console.is_none(), "{:?}", s.console);
}

/// XEX disc numbers (spec D24): disc 2 of 2 carries both; XBE
/// `DiscNumber` counts from 0.
#[test]
fn loose_executables_carry_their_disc_numbers() {
    let mut f = bytes(ct::xex_loose(ct::XEX_MODULE_TITLE, 0x4D53_07E6, 2, 2));
    let len = f.len;
    let c = sniff_named(&mut f, len, "xex", "Game (Disc 2).xex")
        .console
        .unwrap();
    assert_eq!((c.disc_no, c.disc_count), (Some(2), Some(2)));
    assert_eq!(c.id_source, Some(IdSource::Xex));
    let mut f = bytes(ct::xbe_loose(0x4541_000D, ct::XBE_MEDIA_DVD_X2, 1));
    let len = f.len;
    let c = sniff_named(&mut f, len, "xbe", "Game (Disc 2).xbe")
        .console
        .unwrap();
    assert_eq!((c.disc_no, c.disc_count), (Some(2), None));
    assert_eq!(c.id_source, Some(IdSource::Xbe));
}

/// A retail XBE beside other files is a game folder no probe matched
/// (§8.2): its executable is asked about, never moved alone. Alone, it
/// moves; homebrew moves too.
#[test]
fn a_retail_xbe_with_siblings_is_reviewed() {
    let tmp = tempfile::tempdir().unwrap();
    let game = tmp.path().join("Burnout 3");
    put(&game.join("Burnout 3.xbe"), &xbe_retail().bytes(), 1);
    put(&game.join("media").join("intro.xmv"), &[7u8; 64], 2);
    let lone = ct::xbe_loose(
        0x4D53_0004,
        ct::XBE_MEDIA_HARD_DISK | ct::XBE_MEDIA_DVD_X2,
        0,
    );
    put(&tmp.path().join("Lone").join("Lone.xbe"), &lone, 3);
    put(
        &tmp.path().join("Apps").join("evox.xbe"),
        &xbe_no_title_id().bytes(),
        4,
    );
    put(&tmp.path().join("Apps").join("evox.ini"), b"[Menu]", 5);
    let p = plan(tmp.path(), &[], &World::new());
    let burnout = item_named(&p, "Burnout 3.xbe");
    assert_eq!(burnout.action, OrganizeAction::Review, "{burnout:?}");
    for name in ["Lone.xbe", "evox.xbe"] {
        let item = item_named(&p, name);
        assert_eq!(item.system_id.as_deref(), Some("xbox"), "{name}");
        assert_eq!(item.action, OrganizeAction::Auto, "{name}: {item:?}");
    }
}

/// A retail XBE the user already filed (a remembered choice) is left as
/// the user decided: no part and no review, even beside other files. A
/// second copy elsewhere makes the survey hash both, so the choice is
/// found by content.
#[test]
fn a_remembered_xbe_with_siblings_keeps_the_users_choice() {
    let tmp = tempfile::tempdir().unwrap();
    let game = tmp.path().join("Burnout 3");
    let xbe = xbe_retail().bytes();
    put(&game.join("Burnout 3.xbe"), &xbe, 1);
    put(&game.join("media").join("intro.xmv"), &[7u8; 64], 2);
    put(&tmp.path().join("Copy").join("Burnout 3 copy.xbe"), &xbe, 3);
    let world = World::new();
    let crc = format!("{:08X}", crc32fast::hash(&xbe));
    world
        .cache
        .put_organize_choice(&crc, xbe.len() as u64, "xbox")
        .unwrap();
    let env = world.env();
    let s = survey(
        tmp.path(),
        &options(),
        &env,
        &|_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    let entry = s
        .entries
        .iter()
        .find(|e| {
            e.sources.first().and_then(|p| p.file_name())
                == Some(std::ffi::OsStr::new("Burnout 3.xbe"))
        })
        .expect("the XBE is surveyed");
    let id = entry.identification.as_ref().unwrap();
    assert_eq!(id.combined.method, Some(IdLevel::User), "{id:?}");
    assert_eq!(id.combined.verdict, Verdict::Auto, "{id:?}");
    assert_eq!(entry.console.as_ref().unwrap().part, None);
}

/// `code/*.rpx` beside `meta/` stays the loadiine probe's (§8.2): the
/// folder is one Wii U game, its executable is never a loose homebrew.
#[test]
fn a_loadiine_rpx_stays_in_its_folder_game() {
    let tmp = tempfile::tempdir().unwrap();
    let game = tmp.path().join("New Super Mario Bros U");
    ct::wiiu_folder(
        &game,
        "000500001014B700",
        "WUP-P-ARPE",
        "New SUPER MARIO BROS. U",
    );
    put(&game.join("code").join("red-pro2.rpx"), &ct::rpx(0xFE), 9);
    let p = plan(tmp.path(), &[], &World::new());
    assert_eq!(p.items.len(), 1, "{:?}", p.items);
    assert_eq!(p.items[0].system_id.as_deref(), Some("wiiu"));
    assert_eq!(p.items[0].sources.len(), 1);
    assert_eq!(p.items[0].sources[0].file_name(), game.file_name());
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

    fn env_with<'a>(&'a self, titles: &'a dyn TitleNames) -> OrganizeEnv<'a> {
        OrganizeEnv {
            titles: Some(titles),
            ..self.env()
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

/// One Switch game (adjustments D6, D7): its meta NCA names it.
struct Odyssey;

const ODYSSEY: u64 = 0x0100_0000_0001_0000;
const ODYSSEY_META_NCA: &str = "fedcba9876543210fedcba9876543210";

impl TitleNames for Odyssey {
    fn title(&self, tid: u64) -> Option<CatalogTitle> {
        (tid == ODYSSEY).then(|| CatalogTitle {
            name: "Super Mario Odyssey".into(),
            publisher: None,
            release: None,
            region: None,
            source: TitleSource::Titledb,
        })
    }

    fn by_nca(&self, nca: &[u8; 16]) -> Option<(u64, u8)> {
        (nca[0] == 0xFE).then_some((ODYSSEY, 1))
    }
}

fn switch_pile(dir: &Path) {
    put(
        &dir.join("Odyssey Update [0100000000010800][v65536].nsp"),
        &ct::nsp_pfs0(&["a.nca"]).bytes(),
        1,
    );
    let meta = format!("{ODYSSEY_META_NCA}.cnmt.nca");
    put(
        &dir.join("Cartridge.xci"),
        &ct::xci_with_secure(&[&meta]).bytes(),
        2,
    );
}

fn item_named<'a>(plan: &'a OrganizePlan, name: &str) -> &'a coverly_core::organize::OrganizeItem {
    plan.items
        .iter()
        .find(|i| i.sources[0].file_name().is_some_and(|n| n == name))
        .unwrap_or_else(|| panic!("{name} not in the plan"))
}

/// D7 through the planner: the catalogue names an update whose base is
/// not in the pile, and gives an NCA-only cartridge its title id and
/// catalogue name; without the catalogue the same survey keeps the file
/// names and ids.
#[test]
fn switch_catalogue_names_reach_the_plan() {
    let tmp = tempfile::tempdir().unwrap();
    switch_pile(tmp.path());
    let world = World::new();
    let plain = world.env();
    let s = survey(
        tmp.path(),
        &options(),
        &plain,
        &|_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(s.has_switch());
    assert!(s.wants_ncas());
    assert_eq!(s.switch_title_ids(), vec![0x0100_0000_0001_0800]);

    let without = plan_from_survey(&s, &options(), &[], &plain).unwrap();
    let update = item_named(&without, "Odyssey Update [0100000000010800][v65536].nsp");
    assert!(update.notes.contains(&ItemNote::Update {
        of: "0100000000010000".into()
    }));
    let cart = item_named(&without, "Cartridge.xci");
    assert_eq!((&cart.title_id, &cart.catalogue_name), (&None, &None));

    let names = Odyssey;
    let with = plan_from_survey(&s, &options(), &[], &world.env_with(&names)).unwrap();
    let update = item_named(&with, "Odyssey Update [0100000000010800][v65536].nsp");
    assert!(
        update.notes.contains(&ItemNote::Update {
            of: "Super Mario Odyssey".into()
        }),
        "{:?}",
        update.notes
    );
    assert_eq!(update.catalogue_name, None);
    let cart = item_named(&with, "Cartridge.xci");
    assert_eq!(
        cart.title_id,
        Some(TitleIdRef {
            id: "0100000000010000".into(),
            source: IdSource::NcaCatalog,
        })
    );
    assert_eq!(cart.catalogue_name.as_deref(), Some("Super Mario Odyssey"));
}

fn encrypted_name(item: &coverly_core::organize::OrganizeItem) -> bool {
    item.notes.contains(&ItemNote::EncryptedName)
}

fn encrypted_content(item: &coverly_core::organize::OrganizeItem) -> bool {
    item.notes
        .iter()
        .any(|n| matches!(n, ItemNote::EncryptedContent { .. }))
}

/// D11 on Switch: a package whose title nothing resolves (no header
/// title, no catalogue, a file name that is only its ids) is labelled
/// «Cifrado»; the catalogue or a file name with a title keep it off.
#[test]
fn unresolved_switch_titles_are_labelled_encrypted() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let nsp = ct::nsp_pfs0(&["a.nca"]).bytes();
    put(&dir.join("[0100000000010000][v0].nsp"), &nsp, 1);
    put(&dir.join("0100000000010000.nsp"), &nsp, 2);
    put(
        &dir.join("Super Mario Odyssey [0100000000010000].nsp"),
        &nsp,
        3,
    );
    put(
        &dir.join("Cartridge.xci"),
        &ct::xci_with_secure(&[&format!("{ODYSSEY_META_NCA}.cnmt.nca")]).bytes(),
        4,
    );
    put(
        &dir.join("Untitled.nro"),
        &ct::nro("", "1.0.0", &[]).bytes(),
        5,
    );
    let world = World::new();
    let plain = world.env();
    let s = survey(dir, &options(), &plain, &|_| {}, &AtomicBool::new(false)).unwrap();

    let without = plan_from_survey(&s, &options(), &[], &plain).unwrap();
    for name in ["[0100000000010000][v0].nsp", "0100000000010000.nsp"] {
        let item = item_named(&without, name);
        assert!(encrypted_name(item), "{name}: {:?}", item.notes);
        assert!(!encrypted_content(item), "{name}");
    }
    for name in [
        "Super Mario Odyssey [0100000000010000].nsp",
        "Cartridge.xci",
    ] {
        let item = item_named(&without, name);
        assert!(!encrypted_name(item), "{name}: {:?}", item.notes);
    }

    // The catalogue names every package with a title id; a bare id
    // without brackets is not a file-name title id, so that one stays.
    let names = Odyssey;
    let with = plan_from_survey(&s, &options(), &[], &world.env_with(&names)).unwrap();
    for item in &with.items {
        let bare = item.sources[0]
            .file_name()
            .is_some_and(|n| n == "0100000000010000.nsp");
        assert_eq!(
            encrypted_name(item),
            bare,
            "{:?}: {:?}",
            item.sources,
            item.notes
        );
    }
}

/// D11 on 3DS: an encrypted card without a readable title shows only the
/// «Cifrado» label; an encrypted CIA whose Meta section names it keeps the
/// emulator note alone; a NoCrypto card and homebrew carry neither.
#[test]
fn unresolved_3ds_titles_are_labelled_encrypted_once() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut card = ct::ncsd("CTR-P-AMKP", 0x0004_0000_0003_0800, false);
    card.len = 0x10_0000;
    put(&dir.join("Mario Kart 7 (Europe).3ds"), &card.bytes(), 1);
    put(
        &dir.join("Pokemon Y (Europe).cia"),
        &ct::cia(0x0004_0000_0005_5D00, true, true).bytes(),
        2,
    );
    let mut clear = ct::ncsd("CTR-P-AMKP", 0x0004_0000_0003_0800, true);
    clear.len = 0x10_0000;
    put(
        &dir.join("Mario Kart 7 (Europe) (Decrypted).3ds"),
        &clear.bytes(),
        3,
    );
    put(&dir.join("FTPD.3dsx"), &ftpd_3dsx().bytes(), 4);
    let p = plan(dir, &[], &World::new());

    let card = item_named(&p, "Mario Kart 7 (Europe).3ds");
    assert_eq!(card.system_id.as_deref(), Some("3ds"));
    assert!(encrypted_name(card), "{:?}", card.notes);
    assert!(!encrypted_content(card), "{:?}", card.notes);

    let cia = item_named(&p, "Pokemon Y (Europe).cia");
    assert!(encrypted_content(cia), "{:?}", cia.notes);
    assert!(!encrypted_name(cia), "{:?}", cia.notes);

    for name in ["Mario Kart 7 (Europe) (Decrypted).3ds", "FTPD.3dsx"] {
        let item = item_named(&p, name);
        assert!(!encrypted_name(item) && !encrypted_content(item), "{name}");
    }
}
