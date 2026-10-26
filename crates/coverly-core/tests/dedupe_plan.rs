//! Duplicates: the planner as a whole (spec §5–§10). Every library here is
//! synthetic and lives in a temporary folder; a plan is a dry run and
//! never writes to it.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use coverly_core::archive::Limits;
use coverly_core::cache::{Cache, DatRow};
use coverly_core::dedupe::archive::ArchiveEntryListing;
use coverly_core::dedupe::keys::FileBytes;
use coverly_core::dedupe::testing::{
    self, disc_id, fingerprint, nes_header, CountingBytes, CountingListing, FakeDiscProbe,
};
use coverly_core::dedupe::{
    default_quarantine, plan_dedupe, regroup, set_keeper, skip_group, DedupeEnv, DedupeError,
    DedupeOptions, DedupePhase, DedupePlan, DedupeProgress, GroupAction, KeepReason, Relation,
    ReportWhy,
};
use coverly_core::identify::dat::{DatGame, DatStore};
use coverly_core::net::{Http, HttpRequest, HttpResponse, NetError};
use coverly_core::profiles::Profile;
use coverly_core::scan::{scan_library, LibraryScan, ScanOptions};
use coverly_core::settings::{DedupeLevel, Settings};
use coverly_core::systems::SystemDb;
use sha1::{Digest, Sha1};
use tempfile::TempDir;

// --- Builders ---

/// Synthetic ROM bytes: binary (NUL bytes), distinct per `seed`.
fn rom_bytes(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| if i % 3 == 0 { 0 } else { (i as u8) ^ seed })
        .collect()
}

/// Writes `bytes` at `root/rel`, creating parent folders.
fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    pin_mtime(&p);
    p
}

/// Gives each fixture file its own modified time, one second after the
/// previous one, so the oldest-first tie-break follows the write order on
/// every OS (Windows clocks can stamp quick writes with one time).
fn pin_mtime(p: &Path) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    testing::pin_mtime(p, n);
}

/// A zip at `root/rel` holding one stored entry.
fn put_zip(root: &Path, rel: &str, inner: &str, bytes: &[u8]) -> PathBuf {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(&p).unwrap());
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file(inner, opts).unwrap();
    zip.write_all(bytes).unwrap();
    zip.finish().unwrap();
    pin_mtime(&p);
    p
}

fn crc_hex(bytes: &[u8]) -> String {
    format!("{:08X}", crc32fast::hash(bytes))
}

fn profile() -> Profile {
    common::profile("anbernic-stock")
}

fn scan(root: &Path) -> LibraryScan {
    scan_library(
        root,
        &profile(),
        &SystemDb::builtin(),
        &ScanOptions::default(),
    )
}

/// What a plan runs with; the readers count what they read.
struct Fx {
    cache: Cache,
    db: SystemDb,
    profile: Profile,
    settings: Settings,
    listing: CountingListing<ArchiveEntryListing>,
    discs: FakeDiscProbe,
    bytes: CountingBytes,
    cancel: AtomicBool,
}

impl Fx {
    fn new() -> Fx {
        Fx::with_discs(FakeDiscProbe::new())
    }

    fn with_discs(discs: FakeDiscProbe) -> Fx {
        Fx {
            cache: Cache::in_memory().unwrap(),
            db: SystemDb::builtin(),
            profile: profile(),
            settings: Settings::default(),
            listing: CountingListing::new(ArchiveEntryListing::new(Limits::default())),
            discs,
            bytes: CountingBytes::new(),
            cancel: AtomicBool::new(false),
        }
    }

    fn opts(&self) -> DedupeOptions {
        DedupeOptions::from_settings(&self.settings.dedupe)
    }

    fn plan(&self, scan: &LibraryScan, opts: &DedupeOptions) -> DedupePlan {
        self.run(scan, opts, &self.bytes, None, &|_| {}).unwrap()
    }

    fn run(
        &self,
        scan: &LibraryScan,
        opts: &DedupeOptions,
        bytes: &dyn FileBytes,
        dats: Option<&DatStore>,
        progress: &dyn Fn(DedupeProgress),
    ) -> Result<DedupePlan, DedupeError> {
        let env = DedupeEnv {
            cache: &self.cache,
            db: &self.db,
            profile: &self.profile,
            settings: &self.settings,
            listing: &self.listing,
            discs: &self.discs,
            dats,
            bytes,
            progress,
            cancel: &self.cancel,
        };
        plan_dedupe(scan, opts, &env)
    }
}

/// Caches the catalogue of `system` with one ROM per `(game, bytes)`.
fn cache_dat(cache: &Cache, dat: &str, games: &[(&str, &[u8])]) {
    let games: Vec<DatGame> = games
        .iter()
        .map(|(name, bytes)| DatGame {
            name: name.to_string(),
            roms: vec![DatRow {
                game: name.to_string(),
                rom_name: format!("{name}.gba"),
                size: Some(bytes.len() as u64),
                crc32: Some(crc_hex(bytes)),
                md5: None,
                sha1: None,
                serial: None,
            }],
            cloneof: None,
            region: None,
            is_bios: false,
        })
        .collect();
    cache.replace_dat_games(dat, &games).unwrap();
}

const GBA_DAT: &str = "metadat/no-intro/Nintendo - Game Boy Advance.dat";

/// Two versions of one GBA game (different bytes and sizes) and their
/// catalogue entries.
fn gba_versions(root: &Path, cache: Option<&Cache>) {
    let usa = rom_bytes(7, 4096);
    let eur = rom_bytes(8, 4100);
    put(root, "ROMS/GBA/Advance Wars (USA).gba", &usa);
    put(root, "ROMS/GBA/Advance Wars (Europe).gba", &eur);
    if let Some(cache) = cache {
        cache_dat(
            cache,
            GBA_DAT,
            &[
                ("Advance Wars (USA)", &usa),
                ("Advance Wars (Europe)", &eur),
            ],
        );
    }
}

/// The snapshot library: six systems, every relation.
fn six_systems(root: &Path, cache: &Cache) -> FakeDiscProbe {
    // GB: three exact copies, one zipped; a fourth file of the same size
    // that is another game. The middle copy has a cover.
    let tetris = rom_bytes(1, 32 * 1024);
    put(root, "ROMS/GB/Tetris.gb", &tetris);
    put(root, "ROMS/GB/Tetris (1).gb", &tetris);
    put_zip(root, "ROMS/GB/Tetris.zip", "Tetris.gb", &tetris);
    put(root, "ROMS/GB/Imgs/Tetris (1).png", b"\x89PNG\r\n\x1a\nart");
    put(root, "ROMS/GB/Dr. Mario.gb", &rom_bytes(2, 32 * 1024));

    // SFC: a headerless dump and the same game with a copier header.
    let mario = rom_bytes(3, 64 * 1024);
    let mut headered = vec![0u8; 512];
    headered.extend_from_slice(&mario);
    put(root, "ROMS/SFC/Mario.sfc", &mario);
    put(root, "ROMS/SFC/Mario (h).smc", &headered);

    // FC: the same PRG under two different iNES headers.
    let prg = rom_bytes(4, 16 * 1024);
    let mut a = nes_header(1).to_vec();
    a.extend_from_slice(&prg);
    let mut b = nes_header(4).to_vec();
    b.extend_from_slice(&prg);
    put(root, "ROMS/FC/Zelda.nes", &a);
    put(root, "ROMS/FC/Zelda (alt).nes", &b);

    // PS: one disc as cue/bin and as PBP, same serial and volume.
    put(
        root,
        "ROMS/PS/Crash Bash.cue",
        b"FILE \"Crash Bash.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
    );
    put(root, "ROMS/PS/Crash Bash.bin", &rom_bytes(5, 2352 * 4));
    let mut pbp = b"\0PBP".to_vec();
    pbp.extend_from_slice(&rom_bytes(6, 4096));
    put(root, "ROMS/PS/Crash Bash.pbp", &pbp);

    // GBA: two versions of one game in the catalogue.
    gba_versions(root, Some(cache));

    // MD: two exact copies whose real saves differ: a conflict.
    let sonic = rom_bytes(9, 8 * 1024);
    put(root, "ROMS/MD/Sonic.md", &sonic);
    put(root, "ROMS/MD/Sonic (1).md", &sonic);
    put(root, "ROMS/MD/Sonic.srm", &rom_bytes(10, 8192));
    put(root, "ROMS/MD/Sonic (1).srm", &rom_bytes(11, 8192));

    let id = disc_id(&["SCUS-94570"], None, Some(fingerprint(1)));
    FakeDiscProbe::new()
        .with(root.join("ROMS/PS/Crash Bash.cue"), id.clone())
        .with(root.join("ROMS/PS/Crash Bash.pbp"), id)
}

/// Every file under `root`: path, size, mtime and content hash.
fn tree(root: &Path) -> BTreeMap<PathBuf, (u64, i64, String)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            let meta = fs::symlink_metadata(&p).unwrap();
            if meta.is_dir() {
                out.insert(p.clone(), (0, 0, String::new()));
                stack.push(p);
            } else {
                let mtime = filetime::FileTime::from_last_modification_time(&meta);
                let sha: String = Sha1::digest(fs::read(&p).unwrap())
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                out.insert(
                    p,
                    (
                        meta.len(),
                        mtime.unix_seconds() * 1_000_000_000 + mtime.nanoseconds() as i64,
                        sha,
                    ),
                );
            }
        }
    }
    out
}

/// The plan as JSON with the temporary root and the times taken out.
fn normalised(plan: &DedupePlan, root: &Path) -> serde_json::Value {
    let mut roots = vec![root.display().to_string()];
    if let Ok(real) = root.canonicalize() {
        roots.insert(0, real.display().to_string());
    }
    let mut v = serde_json::to_value(plan).unwrap();
    // Paths are compared as text: the temporary root becomes `[root]` and
    // Windows separators become `/`, so one snapshot serves every OS.
    fn walk(v: &mut serde_json::Value, roots: &[String]) {
        match v {
            serde_json::Value::String(s) => {
                let mut out = s.clone();
                for r in roots {
                    out = out.replace(r.as_str(), "[root]");
                }
                *s = out.replace('\\', "/");
            }
            serde_json::Value::Object(map) => {
                for (k, x) in map.iter_mut() {
                    if k == "mtime_ns" {
                        *x = serde_json::Value::from(0);
                    } else if k == "mtimes_ns" {
                        // Only whether a set is timed file by file.
                        if let serde_json::Value::Array(list) = x {
                            list.iter_mut()
                                .for_each(|t| *t = serde_json::Value::from(0));
                        }
                    } else {
                        walk(x, roots);
                    }
                }
            }
            // f32 fields widen to f64 in a Value (0.9 → 0.8999999761…).
            serde_json::Value::Number(n) if n.is_f64() => {
                let x = n.as_f64().unwrap_or_default();
                *v = serde_json::json!((x * 1e6).round() / 1e6);
            }
            serde_json::Value::Array(list) => list.iter_mut().for_each(|x| walk(x, roots)),
            _ => {}
        }
    }
    walk(&mut v, &roots);
    v
}

fn group_of<'a>(plan: &'a DedupePlan, system: &str) -> &'a coverly_core::dedupe::DupGroup {
    plan.groups
        .iter()
        .find(|g| g.system == system)
        .unwrap_or_else(|| panic!("no {system} group in {:#?}", plan.groups))
}

fn member_at<'a>(
    group: &'a coverly_core::dedupe::DupGroup,
    path: &Path,
) -> &'a coverly_core::dedupe::Member {
    group
        .members
        .iter()
        .find(|m| m.unit.primary == path)
        .unwrap_or_else(|| panic!("no member at {}", path.display()))
}

/// An HTTP client that counts requests and finds nothing.
#[derive(Default)]
struct NotFound {
    calls: AtomicUsize,
}

impl Http for NotFound {
    fn get(&self, _req: &HttpRequest) -> Result<HttpResponse, NetError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(HttpResponse {
            status: 404,
            etag: None,
            last_modified: None,
            content_type: None,
            retry_after: None,
            body: Vec::new(),
        })
    }
}

/// Counts opens and raises `cancel` once `after` files were opened.
struct CancelAfter<'a> {
    inner: CountingBytes,
    after: usize,
    cancel: &'a AtomicBool,
}

impl FileBytes for CancelAfter<'_> {
    fn open(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        if self.inner.opens() + 1 >= self.after {
            self.cancel.store(true, Ordering::SeqCst);
        }
        self.inner.open(path)
    }
}

// --- Tests ---

#[test]
fn library_plan_snapshot() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let cache = Cache::in_memory().unwrap();
    let discs = six_systems(root, &cache);
    let fx = Fx {
        cache,
        ..Fx::with_discs(discs)
    };
    let plan = fx.plan(&scan(root), &fx.opts());

    let systems: Vec<&str> = plan.groups.iter().map(|g| g.system.as_str()).collect();
    assert_eq!(
        systems,
        ["gb", "gba", "megadrive", "nes", "psx", "snes"],
        "{:#?}",
        plan.groups
    );
    let gb = group_of(&plan, "gb");
    assert_eq!(gb.members.len(), 3, "Dr. Mario is another game");
    assert_eq!(gb.action, GroupAction::Act);
    assert!(matches!(
        group_of(&plan, "nes").action,
        GroupAction::ReportOnly {
            why: ReportWhy::HeaderDiffers
        }
    ));
    assert!(matches!(
        group_of(&plan, "gba").action,
        GroupAction::ReportOnly {
            why: ReportWhy::LevelTwoOff
        }
    ));
    assert!(matches!(
        group_of(&plan, "megadrive").action,
        GroupAction::Conflict { .. }
    ));
    let ps = group_of(&plan, "psx");
    assert_eq!(ps.action, GroupAction::Act);
    assert!(ps.members.iter().any(|m| m.relation == Relation::Format));
    let sfc = group_of(&plan, "snes");
    assert_eq!(
        sfc.members[sfc.keeper].unit.primary,
        root.join("ROMS/SFC/Mario.sfc")
    );

    insta::assert_json_snapshot!("library_plan", normalised(&plan, root));
}

#[test]
fn dry_plan_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let cache = Cache::in_memory().unwrap();
    let discs = six_systems(root, &cache);
    let fx = Fx {
        cache,
        ..Fx::with_discs(discs)
    };
    let before = tree(root);
    let plan = fx.plan(&scan(root), &fx.opts());
    assert!(!plan.groups.is_empty());
    assert_eq!(tree(root), before);
    assert!(!plan.quarantine.exists(), "the quarantine is not created");
}

#[test]
fn default_quarantine_is_inside_the_library() {
    // A card with a ROMs folder: beside it, inside the library root.
    let spot = default_quarantine(Path::new("/Volumes/SD"), "ROMS", "_Duplicados");
    assert_eq!(spot.dir, PathBuf::from("/Volumes/SD/_Duplicados"));
    assert_eq!(spot.writer_root, PathBuf::from("/Volumes/SD"));
    assert_eq!(spot.roms_dir, PathBuf::from("/Volumes/SD/ROMS"));
    assert!(!spot.fallback);

    // A folder opened as system folders: inside it, never beside it
    // (it was `<parent>/ROMS _Duplicados`).
    let dir = TempDir::new().unwrap();
    let roms = dir.path().join("ROMS");
    fs::create_dir_all(&roms).unwrap();
    let spot = default_quarantine(&roms, "", "_Duplicados");
    assert_eq!(spot.dir, roms.join("_Duplicados"));
    assert_eq!(spot.writer_root, roms);
    assert_eq!(spot.roms_dir, roms);
    assert!(!spot.fallback);
    assert!(
        !dir.path().join("ROMS _Duplicados").exists(),
        "nothing beside the library"
    );

    // A volume opened as system folders: the same rule.
    let spot = default_quarantine(Path::new("/Volumes/SD"), "", "_Duplicates");
    assert_eq!(spot.dir, PathBuf::from("/Volumes/SD/_Duplicates"));
    assert_eq!(spot.writer_root, PathBuf::from("/Volumes/SD"));
    assert!(!spot.fallback);

    // Slashes around the ROMs folder name do not matter.
    let spot = default_quarantine(Path::new("/Volumes/SD"), "/ROMS/", "_Duplicados");
    assert_eq!(spot.dir, PathBuf::from("/Volumes/SD/_Duplicados"));
    assert_eq!(spot.roms_dir, PathBuf::from("/Volumes/SD/ROMS"));
}

#[test]
fn custom_quarantine_inside_a_system_folder_is_refused() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let tetris = rom_bytes(1, 32 * 1024);
    put(root, "ROMS/GB/Tetris.gb", &tetris);
    put(root, "ROMS/GB/Tetris (1).gb", &tetris);
    let fx = Fx::new();
    let scan = scan(root);

    // Inside a system folder, or around one (the ROMs folder, the root).
    let around = [root.join("ROMS"), root.to_path_buf()];
    for inside in [root.join("ROMS/GB"), root.join("ROMS/GB/_Duplicados")]
        .into_iter()
        .chain(around)
    {
        let mut opts = fx.opts();
        opts.quarantine = coverly_core::dedupe::QuarantineChoice::Folder {
            path: inside.clone(),
        };
        let err = fx
            .run(&scan, &opts, &fx.bytes, None, &|_| {})
            .expect_err("a quarantine inside a system folder");
        assert_eq!(err, DedupeError::QuarantineInSystemFolder(inside));
    }

    // Anywhere else is fine: beside the ROMs folder, or outside the
    // library.
    for fine in [root.join("Dups"), dir.path().join("x")] {
        let mut opts = fx.opts();
        opts.quarantine = coverly_core::dedupe::QuarantineChoice::Folder { path: fine.clone() };
        let plan = fx.plan(&scan, &opts);
        assert_eq!(plan.quarantine, fine);
    }
    assert!(!root.join("Dups").exists(), "a dry run creates nothing");
}

#[test]
fn destinations_mirror_rel_dir() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let tetris = rom_bytes(1, 32 * 1024);
    let keeper = put(root, "ROMS/GB/Tetris.gb", &tetris);
    put(root, "ROMS/GB/Tetris.sav", &rom_bytes(20, 8192));
    let extra = put(root, "ROMS/GB/old/Tetris (1).gb", &tetris);
    let fx = Fx::new();
    let plan = fx.plan(&scan(root), &fx.opts());

    assert_eq!(plan.quarantine, root.join("_Duplicados"));
    assert_eq!(plan.roms_dir, root.join("ROMS"));
    let g = group_of(&plan, "gb");
    assert_eq!(g.members[g.keeper].unit.primary, keeper);
    assert!(matches!(g.reason, KeepReason::HasSave { .. }));
    assert_eq!(member_at(g, &keeper).dest, None, "the keeper stays");
    assert_eq!(
        member_at(g, &extra).dest,
        Some(root.join("_Duplicados/GB/old/Tetris (1).gb"))
    );

    // A folder of your choice holds the same shape.
    let mut opts = fx.opts();
    let custom = root.join("elsewhere");
    opts.quarantine = coverly_core::dedupe::QuarantineChoice::Folder {
        path: custom.clone(),
    };
    let plan = fx.plan(&scan(root), &opts);
    let g = group_of(&plan, "gb");
    assert_eq!(plan.quarantine, custom);
    assert_eq!(
        member_at(g, &extra).dest,
        Some(custom.join("GB/old/Tetris (1).gb"))
    );
}

#[test]
fn totals_say_bytes_leaving_roms() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let tetris = rom_bytes(1, 32 * 1024);
    put(root, "ROMS/GB/Tetris.gb", &tetris);
    put(root, "ROMS/GB/Tetris (1).gb", &tetris);
    put(root, "ROMS/GB/Tetris (2).gb", &tetris);
    let mario = rom_bytes(3, 64 * 1024);
    put(root, "ROMS/SFC/Mario.sfc", &mario);
    put(root, "ROMS/SFC/Mario (1).sfc", &mario);
    // Report-only: other headers, not counted.
    let prg = rom_bytes(4, 16 * 1024);
    let mut a = nes_header(1).to_vec();
    a.extend_from_slice(&prg);
    let mut b = nes_header(4).to_vec();
    b.extend_from_slice(&prg);
    put(root, "ROMS/FC/Zelda.nes", &a);
    put(root, "ROMS/FC/Zelda (alt).nes", &b);

    let fx = Fx::new();
    let plan = fx.plan(&scan(root), &fx.opts());
    let t = &plan.totals;
    assert_eq!(t.groups, 3);
    assert_eq!(t.extras, 3);
    assert_eq!(t.bytes, 2 * 32 * 1024 + 64 * 1024);
    assert_eq!(t.conflicts, 0);
    assert_eq!(
        t.by_system,
        BTreeMap::from([
            ("gb".to_string(), (1, 2, 2 * 32 * 1024)),
            ("snes".to_string(), (1, 1, 64 * 1024)),
        ])
    );
}

#[test]
fn conflict_groups_are_not_counted() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let sonic = rom_bytes(9, 8 * 1024);
    put(root, "ROMS/MD/Sonic.md", &sonic);
    put(root, "ROMS/MD/Sonic (1).md", &sonic);
    put(root, "ROMS/MD/Sonic.srm", &rom_bytes(10, 8192));
    put(root, "ROMS/MD/Sonic (1).srm", &rom_bytes(11, 8192));
    let tetris = rom_bytes(1, 32 * 1024);
    put(root, "ROMS/GB/Tetris.gb", &tetris);
    put(root, "ROMS/GB/Tetris (1).gb", &tetris);

    let fx = Fx::new();
    let plan = fx.plan(&scan(root), &fx.opts());
    let md = group_of(&plan, "megadrive");
    let GroupAction::Conflict { saves } = &md.action else {
        panic!("not a conflict: {:?}", md.action);
    };
    assert_eq!(saves.len(), 2);
    assert!(md.members.iter().all(|m| m.dest.is_none()));
    let t = &plan.totals;
    assert_eq!(
        (t.groups, t.extras, t.bytes, t.conflicts),
        (2, 1, 32 * 1024, 1)
    );
    assert!(!t.by_system.contains_key("megadrive"));
}

#[test]
fn offline_skips_level_two_and_lists_systems_without_catalogue() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    gba_versions(root, None);
    let tetris = rom_bytes(1, 32 * 1024);
    put(root, "ROMS/GB/Tetris.gb", &tetris);
    put(root, "ROMS/GB/Tetris (1).gb", &tetris);
    let fx = Fx::new();
    let http = NotFound::default();
    let store = DatStore::new(&http, &fx.cache, "https://dats.invalid");

    let mut opts = fx.opts();
    opts.offline = true;
    let plan = fx
        .run(&scan(root), &opts, &fx.bytes, Some(&store), &|_| {})
        .unwrap();
    assert_eq!(http.calls.load(Ordering::SeqCst), 0, "offline: no download");
    assert!(plan.groups.iter().all(|g| g.system != "gba"));
    assert_eq!(plan.no_catalogue, ["gb", "gba"]);
    assert_eq!(plan.groups.len(), 1, "level 1 still runs");

    // Online, downloads stop at the cap.
    let mut fx = fx;
    fx.settings.dedupe.max_dat_downloads = 1;
    let store = DatStore::new(&http, &fx.cache, "https://dats.invalid");
    let plan = fx
        .run(&scan(root), &fx.opts(), &fx.bytes, Some(&store), &|_| {})
        .unwrap();
    assert_eq!(http.calls.load(Ordering::SeqCst), 1);
    assert_eq!(plan.no_catalogue, ["gb", "gba"]);
}

#[test]
fn warm_run_hashes_nothing() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let tetris = rom_bytes(1, 32 * 1024);
    put(root, "ROMS/GB/Tetris.gb", &tetris);
    put(root, "ROMS/GB/Tetris (1).gb", &tetris);
    put_zip(root, "ROMS/GB/Tetris.zip", "Tetris.gb", &tetris);
    put(root, "ROMS/GB/Dr. Mario.gb", &rom_bytes(2, 32 * 1024));
    let fx = Fx::new();
    let library = scan(root);
    let first = fx.plan(&library, &fx.opts());
    assert!(fx.bytes.opens() > 0);
    assert!(fx.listing.calls() > 0);

    let warm = CountingBytes::new();
    let listing_before = fx.listing.calls();
    let second = fx.run(&library, &fx.opts(), &warm, None, &|_| {}).unwrap();
    assert_eq!(warm.opens(), 0, "a warm run reads no file");
    // Units still list each archive once to know its kind (served by the
    // archive cache); the keys list and decode nothing.
    assert_eq!(
        fx.listing.calls(),
        listing_before + 1,
        "only the units list the zip"
    );
    assert_eq!(second.groups, first.groups);
}

#[test]
fn cancel_between_units_keeps_the_cache() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    // Twenty files in one size bucket, all different: each is read once.
    for i in 0..20u8 {
        put(root, &format!("ROMS/GB/Game {i}.gb"), &rom_bytes(i, 4096));
    }
    let library = scan(root);

    let fx = Fx::new();
    let cancelling = CancelAfter {
        inner: CountingBytes::new(),
        after: 3,
        cancel: &fx.cancel,
    };
    let phases = std::sync::Mutex::new(Vec::new());
    let result = fx.run(&library, &fx.opts(), &cancelling, None, &|p| {
        phases.lock().unwrap().push(p.phase)
    });
    assert_eq!(result.unwrap_err(), DedupeError::Cancelled);
    assert!(phases.lock().unwrap().contains(&DedupePhase::Fingerprints));
    let read_before_cancel = cancelling.inner.opens();
    assert!(
        read_before_cancel < 20,
        "stopped early: {read_before_cancel}"
    );

    // What was read before the cancel is not read again.
    fx.cancel.store(false, Ordering::SeqCst);
    let again = CountingBytes::new();
    let plan = fx.run(&library, &fx.opts(), &again, None, &|_| {}).unwrap();
    assert!(plan.groups.is_empty());
    assert_eq!(again.opens(), 20 - read_before_cancel);
}

#[test]
fn set_keeper_persists_and_survives_a_rename() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let tetris = rom_bytes(1, 32 * 1024);
    let a = put(root, "ROMS/GB/Tetris.gb", &tetris);
    let b = put(root, "ROMS/GB/Tetris (Copy).gb", &tetris);
    let fx = Fx::new();
    let mut plan = fx.plan(&scan(root), &fx.opts());
    let g = group_of(&plan, "gb").clone();
    let auto = g.members[g.keeper].unit.primary.clone();
    let pick = if auto == a { b.clone() } else { a.clone() };
    let pick_id = member_at(&g, &pick).id;

    set_keeper(&mut plan, g.id, pick_id, &fx.cache).unwrap();
    let g = group_of(&plan, "gb");
    assert_eq!(g.members[g.keeper].unit.primary, pick);
    assert_eq!(g.reason, KeepReason::UserChoice);
    assert_eq!(member_at(g, &pick).dest, None);
    assert!(member_at(g, &auto).dest.is_some());
    let id = g.id;

    // Unknown groups and members are errors, and change nothing.
    let before = plan.clone();
    assert_eq!(
        set_keeper(&mut plan, 1, 0, &fx.cache).unwrap_err(),
        DedupeError::NoGroup(1)
    );
    assert!(matches!(
        set_keeper(&mut plan, id, 9, &fx.cache).unwrap_err(),
        DedupeError::NoMember { .. }
    ));
    assert_eq!(plan, before);

    // A later run remembers the pick. On Unix it follows the file through a
    // rename (by inode); Windows has no stable file id in std, so there the
    // pick is remembered by path and the copy keeps its name.
    #[cfg(unix)]
    let renamed = {
        let renamed = root.join("ROMS/GB/Tetris (Kept).gb");
        fs::rename(&pick, &renamed).unwrap();
        renamed
    };
    #[cfg(not(unix))]
    let renamed = pick.clone();
    let plan = fx.plan(&scan(root), &fx.opts());
    let g = group_of(&plan, "gb");
    assert_eq!(g.members[g.keeper].unit.primary, renamed);
    assert_eq!(g.reason, KeepReason::UserChoice);

    // Unless the choices are reset.
    let mut opts = fx.opts();
    opts.reset_choices = true;
    let plan = fx.plan(&scan(root), &opts);
    assert_ne!(group_of(&plan, "gb").reason, KeepReason::UserChoice);
}

#[test]
fn the_latest_pick_is_the_one_remembered() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let tetris = rom_bytes(1, 32 * 1024);
    let a = put(root, "ROMS/GB/Tetris.gb", &tetris);
    let b = put(root, "ROMS/GB/Tetris (Copy).gb", &tetris);
    let fx = Fx::new();
    let library = scan(root);

    // Each pick is made in a later run than the one before it, and the
    // next run keeps that one, whichever copy is listed first.
    for pick in [&a, &b, &a, &b] {
        let mut plan = fx.plan(&library, &fx.opts());
        let g = group_of(&plan, "gb").clone();
        set_keeper(&mut plan, g.id, member_at(&g, pick).id, &fx.cache).unwrap();

        let plan = fx.plan(&library, &fx.opts());
        let g = group_of(&plan, "gb");
        assert_eq!(&g.members[g.keeper].unit.primary, pick);
        assert_eq!(g.reason, KeepReason::UserChoice);
    }
}

#[test]
fn a_plan_never_groups_across_systems_or_by_name() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    // The same bytes in two systems' folders.
    let tetris = rom_bytes(1, 32 * 1024);
    put(root, "ROMS/GB/Tetris.gb", &tetris);
    put(root, "ROMS/GBC/Tetris.gbc", &tetris);
    // The same name, other bytes, in one system.
    put(root, "ROMS/GB/Zelda.gb", &rom_bytes(2, 32 * 1024));
    put(root, "ROMS/GB/old/Zelda.gb", &rom_bytes(3, 32 * 1024));
    put(root, "ROMS/GB/Zelda (USA).gb", &rom_bytes(4, 32 * 1024));
    let fx = Fx::new();
    let mut opts = fx.opts();
    opts.levels = vec![
        DedupeLevel::Exact,
        DedupeLevel::Version,
        DedupeLevel::Format,
    ];
    opts.one_game_one_rom = true;
    let plan = fx.plan(&scan(root), &opts);
    assert!(plan.groups.is_empty(), "{:#?}", plan.groups);
}

#[test]
fn skip_group_reports_only_and_comes_back() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let tetris = rom_bytes(1, 32 * 1024);
    put(root, "ROMS/GB/Tetris.gb", &tetris);
    put(root, "ROMS/GB/Tetris (1).gb", &tetris);
    let fx = Fx::new();
    let mut plan = fx.plan(&scan(root), &fx.opts());
    let id = plan.groups[0].id;

    skip_group(&mut plan, id, true);
    assert_eq!(
        plan.groups[0].action,
        GroupAction::ReportOnly {
            why: ReportWhy::UserSkipped
        }
    );
    assert!(plan.groups[0].members.iter().all(|m| m.dest.is_none()));
    assert_eq!((plan.totals.extras, plan.totals.bytes), (0, 0));

    skip_group(&mut plan, id, false);
    assert_eq!(plan.groups[0].action, GroupAction::Act);
    assert_eq!(plan.totals.extras, 1);
    assert_eq!(
        plan.groups[0]
            .members
            .iter()
            .filter(|m| m.dest.is_some())
            .count(),
        1
    );
}

#[test]
fn regroup_refuses_a_quarantine_inside_a_system_folder() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let tetris = rom_bytes(1, 32 * 1024);
    put(root, "ROMS/GB/Tetris.gb", &tetris);
    put(root, "ROMS/GB/Tetris (1).gb", &tetris);
    let fx = Fx::new();
    let opts = fx.opts();
    let plan = fx.plan(&scan(root), &opts);

    for inside in [root.join("ROMS/GB/_Duplicados"), root.join("ROMS")] {
        let mut moved = opts.clone();
        moved.quarantine = coverly_core::dedupe::QuarantineChoice::Folder {
            path: inside.clone(),
        };
        let err = regroup(&plan, &moved).expect_err("a quarantine inside a system folder");
        assert_eq!(err, DedupeError::QuarantineInSystemFolder(inside));
    }

    let mut fine = opts.clone();
    fine.quarantine = coverly_core::dedupe::QuarantineChoice::Folder {
        path: root.join("Dups"),
    };
    let regrouped = regroup(&plan, &fine).expect("beside the ROMs folder");
    assert_eq!(regrouped.quarantine, root.join("Dups"));
}

#[test]
fn regroup_with_1g1r_adds_version_groups_without_reading() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let fx = Fx::new();
    gba_versions(root, Some(&fx.cache));
    let mut opts = fx.opts();
    opts.levels = vec![DedupeLevel::Exact, DedupeLevel::Format];
    let plan = fx.plan(&scan(root), &opts);
    assert!(plan.groups.is_empty(), "other versions are not looked for");
    let reads = (fx.bytes.opens(), fx.listing.calls());

    let mut one = opts.clone();
    one.one_game_one_rom = true;
    let regrouped = regroup(&plan, &one).expect("regroup");
    assert_eq!(
        (fx.bytes.opens(), fx.listing.calls()),
        reads,
        "nothing read"
    );
    let g = group_of(&regrouped, "gba");
    assert_eq!(g.action, GroupAction::Act);
    assert_eq!(
        g.members[g.keeper].unit.primary,
        root.join("ROMS/GBA/Advance Wars (USA).gba")
    );
    let extra = member_at(g, &root.join("ROMS/GBA/Advance Wars (Europe).gba"));
    assert_eq!(extra.relation, Relation::Version);
    assert!(extra.dest.is_some());
    assert_eq!(
        extra.catalogue_name.as_deref(),
        Some("Advance Wars (Europe)")
    );
    assert_eq!(g.title, "Advance Wars (USA)");
    assert_eq!(regrouped.totals.extras, 1);

    // Back to the first options: the version group is gone again.
    assert!(regroup(&regrouped, &opts)
        .expect("regroup")
        .groups
        .is_empty());
}

#[test]
fn huge_group_is_collapsed_and_report_only() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let bytes = rom_bytes(1, 1024);
    for i in 0..51 {
        put(root, &format!("ROMS/GB/old/Tetris {i}.gb"), &bytes);
    }
    let fx = Fx::new();
    let plan = fx.plan(&scan(root), &fx.opts());
    let g = group_of(&plan, "gb");
    assert_eq!(
        g.action,
        GroupAction::ReportOnly {
            why: ReportWhy::BigGroup { members: 51 }
        }
    );
    assert!(g.members.iter().all(|m| m.dest.is_none()));
    assert_eq!(plan.totals.extras, 0);
}

#[test]
#[ignore = "performance gate: cargo test --release -- --ignored"]
fn ten_thousand_files_hash_only_colliders() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    // Every game has its own size; 2 % have an exact copy.
    let n = 10_000usize;
    let dups = n / 50;
    for i in 0..n {
        let len = 1024 + i;
        let bytes = rom_bytes((i % 251) as u8, len);
        put(root, &format!("ROMS/GB/Game {i}.gb"), &bytes);
        if i < dups {
            put(root, &format!("ROMS/GB/Game {i} (1).gb"), &bytes);
        }
    }
    let library = scan(root);
    let fx = Fx::new();
    let started = Instant::now();
    let plan = fx.plan(&library, &fx.opts());
    let took = started.elapsed();
    assert_eq!(plan.groups.len(), dups);
    assert!(fx.bytes.opens() <= 2 * dups, "{} opens", fx.bytes.opens());
    assert!(took.as_secs() < 10, "took {took:?}");
}
