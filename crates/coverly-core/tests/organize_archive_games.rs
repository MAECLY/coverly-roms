//! Folder games inside zip and 7z archives (adjustments spec D20–D23):
//! the folder probes run unchanged over the archive's listing, the roots
//! come from the markers of §8.4, and each system gets its action (Vita
//! as is, PS3 disc, PS4 and Wii U extracted, PS3 HDD to review with the
//! RPCS3 note, Xbox as is). Fixtures are synthetic, written to temporary
//! folders with pinned mtimes; paths are compared by component.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use coverly_core::archive::testing::{sevenz, SevenZCoder, SevenZShape};
use coverly_core::archive::Limits;
use coverly_core::cache::Cache;
use coverly_core::consoles::testing as t;
use coverly_core::consoles::{identify_dir_on_disk, ArtLocator, ConsoleId};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::organize::inspect::{inspect_file, inspect_file_with, ArchiveGame, Inspection};
use coverly_core::organize::{
    apply_organize, plan_organize, ApplyEnv, ArchiveAfter, ItemNote, ItemStatus, MetaProbe,
    OrganizeAction, OrganizeEnv, OrganizeItem, OrganizeOptions, OrganizePlan, Transfer,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::FakeTrash;
use tempfile::TempDir;

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

    fn plan(&self, pile: &Path, target: &Path) -> OrganizePlan {
        self.plan_with(pile, target, Limits::default())
    }

    fn plan_with(&self, pile: &Path, target: &Path, limits: Limits) -> OrganizePlan {
        self.plan_in("esde", pile, target, limits)
    }

    fn plan_in(&self, layout: &str, pile: &Path, target: &Path, limits: Limits) -> OrganizePlan {
        let opts = OrganizeOptions {
            layout: layout.into(),
            target: Some(target.to_path_buf()),
            offline: true,
            limits,
            ..OrganizeOptions::default()
        };
        plan_organize(pile, &opts, &self.env(), &|_| {}, &AtomicBool::new(false)).unwrap()
    }
}

/// Every file below `dir` as (`/`-separated path, bytes), sorted.
fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in fs::read_dir(dir).unwrap() {
            let path = e.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel: Vec<String> = path
                    .strip_prefix(root)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push((rel.join("/"), fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// The files of the fixture folder written by `make`, below `root` (a
/// `/`-separated prefix, `""` for none), plus `extra` files.
fn files_of(
    make: impl FnOnce(&Path),
    root: &str,
    extra: &[(&str, &[u8])],
) -> Vec<(String, Vec<u8>)> {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("game");
    make(&dir);
    let mut out: Vec<(String, Vec<u8>)> = tree(&dir)
        .into_iter()
        .map(|(rel, bytes)| {
            let name = if root.is_empty() {
                rel
            } else {
                format!("{root}/{rel}")
            };
            (name, bytes)
        })
        .collect();
    out.extend(extra.iter().map(|(n, b)| (n.to_string(), b.to_vec())));
    out
}

/// A zip of `files` whose names use `sep` as the separator.
fn write_zip(path: &Path, files: &[(String, Vec<u8>)], sep: char) {
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in files {
        zip.start_file(name.replace('/', &sep.to_string()), opts)
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    pin_mtime(path, 1);
}

fn write_7z(path: &Path, files: &[(String, Vec<u8>)], solid: bool) {
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let shape = SevenZShape {
        solid,
        coder: SevenZCoder::Lzma2 { dict: 1 << 20 },
        sheets_last: false,
    };
    fs::write(path, sevenz(&refs, shape)).unwrap();
    pin_mtime(path, 1);
}

fn inspect(path: &Path) -> Inspection {
    let len = fs::metadata(path).unwrap().len();
    inspect_file(path, len, &SystemDb::builtin()).unwrap()
}

/// The plan head budget cut to 64 KiB, so a small solid 7z runs past it.
fn small_budget() -> Limits {
    Limits {
        plan_head_budget: 64 * 1024,
        ..Limits::default()
    }
}

fn inspect_small(path: &Path) -> Inspection {
    let len = fs::metadata(path).unwrap().len();
    inspect_file_with(path, len, &SystemDb::builtin(), &small_budget(), None).unwrap()
}

fn console(insp: &Inspection) -> Option<&ConsoleId> {
    insp.payloads.iter().find_map(|p| p.sniff.console.as_ref())
}

type Make = fn(&Path);

fn jb(d: &Path) {
    t::ps3_jb(d, "BLUS30443", "DG");
}
fn hdd(d: &Path) {
    t::ps3_hdd(d, "NPUB30024", "HG");
}
fn vita(d: &Path) {
    t::vita_app(d, "PCSE00120");
}
fn ps4(d: &Path) {
    t::ps4_app(d, "CUSA00900", "gd", "UP9000-CUSA00900_00-BLOODBORNE000000");
}
fn loadiine(d: &Path) {
    t::wiiu_folder(d, "0005000010101C00", "WUP-P-ARKE", "Mario");
}
fn nus(d: &Path) {
    t::nus_folder(d, 0x0005_0000_1010_ED00);
}
fn xbe(d: &Path) {
    t::put_file(d, "default.xbe", &t::xbe(0x4D53_0004, "Halo"));
}
fn xex(d: &Path) {
    t::put_file(d, "default.xex", &t::xex2(0x4D53_07E6));
}

/// The fixtures every marker of spec §8.4 covers, with the system the
/// disk probe gives.
const PROBED: &[(&str, Make, &str, &str)] = &[
    ("jb", jb, "Game", "ps3"),
    // An HDD title's root is its title id folder.
    ("hdd", hdd, "NPUB30024", "ps3"),
    ("vita", vita, "Game", "psvita"),
    ("ps4", ps4, "Game", "ps4"),
    ("loadiine", loadiine, "Game", "wiiu"),
    ("nus", nus, "Game", "wiiu"),
    ("xbe", xbe, "Game", "xbox"),
    ("xex", xex, "Game", "xbox360"),
];

/// D20: each probe answers the same over the archive's listing as over
/// the folder on disk (zip with `/` and with `\`, 7z), except that art is
/// an archive entry below the root; the root's own name gives the
/// `CUSA…` fallback.
#[test]
fn probes_answer_alike_on_disk_and_inside_archives() {
    let tmp = TempDir::new().unwrap();
    for (case, make, top, system) in PROBED {
        let disk = tmp.path().join(format!("{case}-disk"));
        make(&disk);
        let names: Vec<String> = fs::read_dir(&disk)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let on_disk = identify_dir_on_disk(&disk, &names).unwrap().unwrap().id;
        assert_eq!(on_disk.system, *system, "{case}");
        let files = files_of(make, top, &[]);
        for (kind, write) in [
            (
                "zip",
                (|p: &Path, f: &[(String, Vec<u8>)]| write_zip(p, f, '/'))
                    as fn(&Path, &[(String, Vec<u8>)]),
            ),
            ("zip", |p, f| write_zip(p, f, '\\')),
            ("7z", |p, f| write_7z(p, f, false)),
        ] {
            let path = tmp.path().join(format!("{case}.{kind}"));
            write(&path, &files);
            let insp = inspect(&path);
            let Some(ArchiveGame::One {
                root, system: s, ..
            }) = &insp.folder_game
            else {
                panic!("{case} {kind}: {:?}", insp.folder_game);
            };
            assert_eq!((root.as_str(), s.as_str()), (*top, *system), "{case}");
            let id = console(&insp).unwrap();
            assert_eq!(
                (&id.system, &id.title_id, &id.title, id.kind),
                (
                    &on_disk.system,
                    &on_disk.title_id,
                    &on_disk.title,
                    on_disk.kind
                ),
                "{case} {kind}"
            );
            for art in &id.art {
                match art {
                    ArtLocator::ZipEntry { name, .. } => {
                        assert!(name.starts_with(&format!("{top}/")), "{case}: {name}")
                    }
                    other => panic!("{case}: {other:?}"),
                }
            }
            fs::remove_file(&path).unwrap();
        }
    }
}

/// D20: a split Switch file has no FAT archive bit inside an archive,
/// and the markers of §8.4 do not name it: no folder game.
#[test]
fn split_files_are_no_folder_game_inside_an_archive() {
    let tmp = TempDir::new().unwrap();
    let nsp = t::nsp(&[("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55; 64])]);
    let files = vec![
        ("Mario.nsp/00".to_string(), nsp.bytes()),
        ("Mario.nsp/01".to_string(), vec![0u8; 16]),
    ];
    let path = tmp.path().join("split.zip");
    write_zip(&path, &files, '/');
    assert_eq!(inspect(&path).folder_game, None);
}

/// The PS4 `CUSA…` fallback reads the root folder's name.
#[test]
fn the_cusa_fallback_reads_the_root_name() {
    let tmp = TempDir::new().unwrap();
    let sfo = t::param_sfo_with(&[("CATEGORY", "gd"), ("TITLE", "T")], &[]);
    let files = vec![
        ("CUSA00900/eboot.bin".to_string(), vec![0u8; 64]),
        ("CUSA00900/sce_sys/param.sfo".to_string(), sfo),
    ];
    let path = tmp.path().join("x.zip");
    write_zip(&path, &files, '/');
    let insp = inspect(&path);
    let id = console(&insp).unwrap();
    assert_eq!(
        (id.system.as_str(), id.title_id.as_deref()),
        ("ps4", Some("CUSA00900"))
    );
}

fn only(p: &OrganizePlan) -> &OrganizeItem {
    assert_eq!(p.items.len(), 1, "{:#?}", p.items);
    &p.items[0]
}

fn last(path: &Path) -> String {
    path.components()
        .next_back()
        .unwrap()
        .as_os_str()
        .to_string_lossy()
        .into_owned()
}

/// A plan of one archive `name` holding `files`.
fn plan_one(
    world: &World,
    name: &str,
    files: &[(String, Vec<u8>)],
) -> (TempDir, TempDir, OrganizePlan) {
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let path = pile.path().join(name);
    if name.ends_with(".7z") {
        write_7z(&path, files, false);
    } else {
        write_zip(&path, files, '/');
    }
    let p = world.plan(pile.path(), target.path());
    (pile, target, p)
}

/// D23: a PS3 disc (JB) zip extracts its root's subtree into a folder of
/// `ps3/` named like a disk folder game (`.ps3` suffix in ES-DE); an
/// `.nfo` outside the root is a leftover, so the archive is kept.
#[test]
fn a_ps3_disc_zip_extracts_its_root() {
    let world = World::new();
    let files = files_of(jb, "Game [BLUS30443]", &[("Game.nfo", b"scene")]);
    let (_pile, _target, p) = plan_one(&world, "Game.zip", &files);
    let item = only(&p);
    assert_eq!(item.action, OrganizeAction::Auto, "{item:#?}");
    assert_eq!(item.system_id.as_deref(), Some("ps3"));
    let Transfer::Extract {
        entries,
        archive_after,
        ..
    } = &item.transfer
    else {
        panic!("{item:#?}");
    };
    assert_eq!(*archive_after, ArchiveAfter::Keep);
    assert!(item
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::ArchiveKeptLeftovers { count: 1, .. })));
    let target = item.target.as_ref().unwrap();
    assert_eq!(last(target), "Game [BLUS30443].ps3");
    assert_eq!(target.parent().map(last).as_deref(), Some("ps3"));
    assert_eq!(entries.len(), 4);
    let sfo = entries
        .iter()
        .find(|e| e.name.ends_with("PARAM.SFO"))
        .unwrap();
    assert_eq!(sfo.target, target.join("PS3_GAME").join("PARAM.SFO"));
    assert!(entries.iter().all(|e| e.target.starts_with(target)));
}

/// D23: the extraction is applied: the subtree lands below the folder.
#[test]
fn a_ps3_disc_zip_is_extracted_on_apply() {
    let world = World::new();
    let files = files_of(jb, "", &[]);
    let (_pile, target, p) = plan_one(&world, "Demon's Souls.zip", &files);
    let item = only(&p);
    assert!(
        matches!(item.transfer, Transfer::Extract { .. }),
        "{item:#?}"
    );
    let data = TempDir::new().unwrap();
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
    let outcome = &report.items[0];
    assert!(
        matches!(
            outcome.status,
            ItemStatus::Extracted | ItemStatus::ExtractedKept
        ),
        "{report:#?}"
    );
    let folder = target.path().join(item.target.as_ref().unwrap());
    assert_eq!(last(&folder), "Demon's Souls.ps3");
    for rel in [
        "PS3_GAME/PARAM.SFO",
        "PS3_GAME/USRDIR/EBOOT.BIN",
        "PS3_DISC.SFB",
    ] {
        let path = rel.split('/').fold(folder.clone(), |p, c| p.join(c));
        assert!(path.is_file(), "{}", path.display());
    }
}

/// D23: a PS3 HDD/PSN title goes to review with the RPCS3 install note.
#[test]
fn a_ps3_hdd_zip_goes_to_review_with_the_install_note() {
    let world = World::new();
    let files = files_of(hdd, "NPUB30024", &[]);
    let (_pile, _target, p) = plan_one(&world, "Flower.zip", &files);
    let item = only(&p);
    assert_eq!(item.action, OrganizeAction::Review, "{item:#?}");
    assert_eq!(item.system_id.as_deref(), Some("ps3"));
    assert!(item.notes.contains(&ItemNote::InstallFirst {
        emulator: "RPCS3".into()
    }));
    assert_eq!(item.transfer, Transfer::AsIs);
}

/// D23: a PS4 zip extracts, a Wii U loadiine or NUS zip too.
#[test]
fn ps4_and_wiiu_zips_extract() {
    let world = World::new();
    for (make, root, system) in [
        (ps4 as Make, "CUSA00900", "ps4"),
        (loadiine, "Mario Kart 8", "wiiu"),
        (nus, "", "wiiu"),
    ] {
        let files = files_of(make, root, &[]);
        let (_pile, _target, p) = plan_one(&world, "Game.zip", &files);
        let item = only(&p);
        assert_eq!(item.system_id.as_deref(), Some(system), "{item:#?}");
        let Transfer::Extract { entries, .. } = &item.transfer else {
            panic!("{system}: {item:#?}");
        };
        assert_eq!(entries.len(), files.len(), "{system}");
        let want = if root.is_empty() { "Game" } else { root };
        assert_eq!(last(item.target.as_ref().unwrap()), want, "{system}");
    }
}

/// D23 with batch 3 D16: a zipped PS4 DLC extracts into the frontend's
/// DLC folder like a DLC folder on disk, without the folder game suffix
/// (only the base game is listed).
#[test]
fn a_zipped_ps4_dlc_extracts_into_the_dlc_folder_without_the_suffix() {
    let world = World::new();
    let sfo = t::ps4_sfo("CUSA00900", "ac", "UP9000-CUSA00900_00-OLDHUNTERS000000", 1);
    let files = vec![
        ("The Old Hunters/sce_sys/param.sfo".to_string(), sfo),
        ("The Old Hunters/content.dat".to_string(), vec![7u8; 64]),
    ];
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    write_zip(&pile.path().join("Old Hunters.zip"), &files, '/');
    let p = world.plan_in("batocera", pile.path(), target.path(), Limits::default());
    let item = only(&p);
    assert_eq!(item.system_id.as_deref(), Some("ps4"), "{item:#?}");
    assert!(
        matches!(item.transfer, Transfer::Extract { .. }),
        "{item:#?}"
    );
    let parts: Vec<String> = item
        .target
        .as_ref()
        .unwrap()
        .components()
        .rev()
        .take(4)
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    assert_eq!(parts, ["The Old Hunters", "CUSA00900", "DLC", "ps4"]);
    assert!(
        !item
            .notes
            .iter()
            .any(|n| matches!(n, ItemNote::FolderSuffix { .. })),
        "{:?}",
        item.notes
    );
}

/// D23: a Vita game moves as it is; in a 7z it is noted that Vita3K only
/// installs zip, vpk or vci.
#[test]
fn a_vita_archive_moves_as_is() {
    let world = World::new();
    let files = files_of(vita, "PCSE00120", &[]);
    for (name, noted) in [("Persona.zip", false), ("Persona.7z", true)] {
        let (_pile, _target, p) = plan_one(&world, name, &files);
        let item = only(&p);
        assert_eq!(item.system_id.as_deref(), Some("psvita"), "{item:#?}");
        assert_eq!(item.transfer, Transfer::AsIs, "{name}");
        assert_eq!(item.notes.contains(&ItemNote::Vita7z), noted, "{name}");
        assert_eq!(last(item.target.as_ref().unwrap()), name);
    }
}

/// §8.4: a Vita game with the module Vita3K aborts on goes to review.
#[test]
fn a_vita_archive_with_steroid_goes_to_review() {
    let world = World::new();
    let files = files_of(vita, "", &[("sce_module/steroid.suprx", &[0u8; 32])]);
    let (_pile, _target, p) = plan_one(&world, "Persona.zip", &files);
    let item = only(&p);
    assert_eq!(item.action, OrganizeAction::Review, "{item:#?}");
    assert!(item.notes.contains(&ItemNote::NoEmulator));
}

/// Q16: an Xbox game is recognised and moved as it is.
#[test]
fn an_xbox_archive_moves_as_is() {
    let world = World::new();
    let files = files_of(xbe, "Halo", &[]);
    let (_pile, _target, p) = plan_one(&world, "Halo.zip", &files);
    let item = only(&p);
    assert_eq!(item.system_id.as_deref(), Some("xbox"), "{item:#?}");
    assert_eq!(item.transfer, Transfer::AsIs);
}

/// D21: two roots → review «El comprimido contiene 2 juegos».
#[test]
fn two_roots_go_to_review() {
    let world = World::new();
    let mut files = files_of(jb, "A", &[]);
    files.extend(files_of(jb, "B/C", &[]));
    let (_pile, _target, p) = plan_one(&world, "Two.zip", &files);
    let item = only(&p);
    assert_eq!(item.action, OrganizeAction::Review, "{item:#?}");
    assert!(item.notes.contains(&ItemNote::ArchiveGames { count: 2 }));
    assert_eq!(item.transfer, Transfer::AsIs);
}

/// D21: a root deeper than three folders is ignored.
#[test]
fn a_root_four_levels_deep_is_ignored() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("deep.zip");
    write_zip(&path, &files_of(jb, "a/b/c/d", &[]), '/');
    assert_eq!(inspect(&path).folder_game, None);
    let path = tmp.path().join("three.zip");
    write_zip(&path, &files_of(jb, "a/b/c", &[]), '/');
    assert!(matches!(
        inspect(&path).folder_game,
        Some(ArchiveGame::One { .. })
    ));
}

/// D20: `PS3_GAME/` and `ps3_game/` collide on macOS and Windows.
#[test]
fn names_that_differ_only_in_case_go_to_review() {
    let world = World::new();
    let files = files_of(jb, "", &[("ps3_game/ICON1.PNG", &[1, 2, 3])]);
    let (_pile, _target, p) = plan_one(&world, "Case.zip", &files);
    let item = only(&p);
    assert_eq!(item.action, OrganizeAction::Review, "{item:#?}");
    assert!(item.notes.contains(&ItemNote::ArchiveCaseCollision));
    assert_eq!(item.transfer, Transfer::AsIs);
}

/// D22: a solid 7z whose `param.sfo` lies past the plan budget is told
/// by its structure only: PS4 or Vita, to review with both.
#[test]
fn a_solid_7z_beyond_the_budget_is_ps4_or_vita() {
    let world = World::new();
    let files = vec![
        ("CUSA00900/eboot.bin".to_string(), vec![0x5Au8; 200 * 1024]),
        (
            "CUSA00900/sce_sys/param.sfo".to_string(),
            t::ps4_sfo("CUSA00900", "gd", "UP9000-CUSA00900_00-BLOODBORNE000000", 1),
        ),
    ];
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    let path = pile.path().join("Bloodborne.7z");
    write_7z(&path, &files, true);
    let insp = inspect_small(&path);
    assert_eq!(
        insp.folder_game,
        Some(ArchiveGame::Ps4OrVita {
            root: "CUSA00900".into()
        })
    );
    let p = world.plan_with(pile.path(), target.path(), small_budget());
    let item = only(&p);
    assert_eq!(item.action, OrganizeAction::Review, "{item:#?}");
    assert!(item.notes.contains(&ItemNote::Ps4OrVita));
    let alts: Vec<&str> = item.alternatives.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(alts, ["ps4", "psvita"]);
}

/// D22: PS3 and Wii U still decide from names when the metadata lies
/// past the budget.
#[test]
fn ps3_and_wiiu_decide_from_the_structure_past_the_budget() {
    for (make, system) in [(jb as Make, "ps3"), (loadiine, "wiiu")] {
        let tmp = TempDir::new().unwrap();
        let mut files = vec![("Game/aaa.bin".to_string(), vec![0x5Au8; 200 * 1024])];
        files.extend(files_of(make, "Game", &[]));
        let path = tmp.path().join("g.7z");
        write_7z(&path, &files, true);
        let insp = inspect_small(&path);
        let Some(ArchiveGame::One {
            system: s,
            structure_only,
            ..
        }) = &insp.folder_game
        else {
            panic!("{system}: {:?}", insp.folder_game);
        };
        assert_eq!((s.as_str(), *structure_only), (system, true));
    }
}

/// Plain cartridge zips are untouched by the marker pass.
#[test]
fn a_cartridge_zip_is_no_folder_game() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("Tetris.zip");
    let files = vec![("Tetris (World).gb".to_string(), vec![0u8; 32 * 1024])];
    write_zip(&path, &files, '/');
    let insp = inspect(&path);
    assert_eq!(insp.folder_game, None);
    assert_eq!(insp.payloads.len(), 1);
}

/// D23 with batch 3 D11: a zipped PS4 update follows its zipped base's
/// final name in Batocera (`CUSA00900.ps4-UPDATE`, the sibling shadPS4
/// reads), and every extracted entry lands below that folder.
#[test]
fn a_zipped_ps4_update_follows_its_base_name() {
    let world = World::new();
    let base = files_of(ps4, "CUSA00900", &[]);
    let upd_sfo = t::ps4_sfo("CUSA00900", "gp", "UP9000-CUSA00900_00-BLOODBORNE000000", 1);
    let upd = vec![
        ("CUSA00900-UPDATE/sce_sys/param.sfo".to_string(), upd_sfo),
        ("CUSA00900-UPDATE/eboot.bin".to_string(), vec![1u8; 64]),
    ];
    let pile = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    write_zip(&pile.path().join("CUSA00900.zip"), &base, '/');
    write_zip(&pile.path().join("CUSA00900-UPDATE.zip"), &upd, '/');
    let p = world.plan_in("batocera", pile.path(), target.path(), Limits::default());
    let by = |name: &str| {
        p.items
            .iter()
            .find(|i| last(&i.sources[0]) == name)
            .unwrap_or_else(|| panic!("{name}: {:#?}", p.items))
    };
    let base = by("CUSA00900.zip");
    assert_eq!(last(base.target.as_ref().unwrap()), "CUSA00900.ps4");
    let update = by("CUSA00900-UPDATE.zip");
    assert_eq!(update.action, OrganizeAction::Auto, "{update:#?}");
    let folder = update.target.as_ref().unwrap();
    assert_eq!(last(folder), "CUSA00900.ps4-UPDATE");
    assert_eq!(folder.parent(), base.target.as_ref().unwrap().parent());
    let Transfer::Extract { entries, .. } = &update.transfer else {
        panic!("{update:#?}");
    };
    assert_eq!(entries.len(), 2);
    let mut targets: Vec<&Path> = entries.iter().map(|e| e.target.as_path()).collect();
    targets.sort();
    assert_eq!(
        targets,
        [
            folder.join("eboot.bin").as_path(),
            folder.join("sce_sys").join("param.sfo").as_path()
        ]
    );
    assert!(
        !update
            .notes
            .iter()
            .any(|n| matches!(n, ItemNote::FolderSuffix { .. })),
        "{:?}",
        update.notes
    );
}
