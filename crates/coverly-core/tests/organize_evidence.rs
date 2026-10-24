mod common;

use coverly_core::identify::evidence::{Evidence, IdLevel::*};
use coverly_core::identify::sniff::{fixtures as fx, sniff, SliceAt};
use coverly_core::organize::evidence::{
    combine, extension_evidence, CombineInput, Thresholds, Verdict,
};
use coverly_core::systems::SystemDb;

fn ev(kind: coverly_core::identify::evidence::IdLevel, system: &str, w: f32) -> Evidence {
    Evidence::new(kind, system, w, "")
}

fn decide(evidence: &[Evidence], excluded: &[&str]) -> (Verdict, Option<String>) {
    let excluded: Vec<String> = excluded.iter().map(|s| s.to_string()).collect();
    let c = combine(&CombineInput {
        evidence,
        excluded: &excluded,
        content_read: true,
        broad_root: false,
        thresholds: Thresholds::default(),
    });
    (c.verdict, c.system)
}

#[test]
fn a_dat_hit_wins() {
    assert_eq!(
        decide(
            &[
                ev(Hash, "gb", 1.0),
                ev(Extension, "gb", 0.5),
                ev(Extension, "gbc", 0.5)
            ],
            &[]
        ),
        (Verdict::Auto, Some("gb".into()))
    );
}

#[test]
fn gb_or_gbc_by_the_cgb_flag() {
    assert_eq!(
        decide(
            &[
                ev(Header, "gb", 0.95),
                ev(Extension, "gb", 0.5),
                ev(Extension, "gbc", 0.5)
            ],
            &[]
        ),
        (Verdict::Auto, Some("gb".into()))
    );
    assert_eq!(
        decide(
            &[
                ev(Header, "gbc", 0.85),
                ev(Extension, "gb", 0.5),
                ev(Extension, "gbc", 0.5)
            ],
            &[]
        ),
        (Verdict::Auto, Some("gbc".into())),
        "dual-mode → GBC, GB as alternative"
    );
}

#[test]
fn gbc_only_header_never_goes_to_gb() {
    let (v, s) = decide(&[ev(Header, "gbc", 0.95), ev(Hash, "gb", 1.0)], &["gb"]);
    assert_eq!((v, s.as_deref()), (Verdict::Auto, Some("gbc")));
}

#[test]
fn a_shared_extension_without_header_needs_review() {
    assert_eq!(
        decide(&[ev(Extension, "gb", 0.5), ev(Extension, "gbc", 0.5)], &[]).0,
        Verdict::Review
    );
}

#[test]
fn extension_and_name_alone_never_reach_auto() {
    // A unique extension of a system with a signature, no header hit: not content support.
    let c = combine(&CombineInput {
        evidence: &[ev(Extension, "gba", 0.85), ev(Name, "gba", 0.80)],
        excluded: &[],
        content_read: false,
        broad_root: false,
        thresholds: Thresholds::default(),
    });
    assert_eq!(c.verdict, Verdict::Review);
}

#[test]
fn a_unique_extension_of_a_system_without_signature_is_enough() {
    let (v, s) = decide(&[ev(Extension, "pcengine", 0.85)], &[]);
    assert_eq!((v, s.as_deref()), (Verdict::Auto, Some("pcengine")));
}

#[test]
fn a_contradicting_sniff_downgrades_a_unique_extension() {
    let db = SystemDb::builtin();
    let bytes = fx::zeros(64 * 1024); // no GBA header
    let s = sniff(&mut SliceAt(&bytes), bytes.len() as u64, "gba");
    let e = extension_evidence(&db, "gba", &s);
    assert_eq!(e.len(), 1);
    assert!((e[0].weight - 0.60).abs() < 1e-6);
    assert_eq!(decide(&e, &[]).0, Verdict::Review);
}

#[test]
fn the_margin_rule() {
    assert_eq!(
        decide(
            &[
                ev(Header, "mastersystem", 0.90),
                ev(Header, "gamegear", 0.85)
            ],
            &[]
        )
        .0,
        Verdict::Review
    );
}

#[test]
fn two_catalogues_for_one_crc_need_review() {
    assert_eq!(
        decide(&[ev(Hash, "gb", 1.0), ev(Hash, "gbc", 1.0)], &[]).0,
        Verdict::Review
    );
}

#[test]
fn nothing_above_half_is_unknown() {
    assert_eq!(decide(&[ev(Name, "gba", 0.40)], &[]).0, Verdict::Unknown);
    assert_eq!(decide(&[], &[]).0, Verdict::Unknown);
}

#[test]
fn broad_roots_drop_reviews_without_content_evidence() {
    let c = combine(&CombineInput {
        evidence: &[ev(Extension, "gb", 0.5), ev(Extension, "gbc", 0.5)],
        excluded: &[],
        content_read: true,
        broad_root: true,
        thresholds: Thresholds::default(),
    });
    assert_eq!(c.verdict, Verdict::Unknown);
}

#[test]
fn agreement_adds_a_small_bonus_and_why_is_strongest_first() {
    let c = combine(&CombineInput {
        evidence: &[
            ev(Extension, "gba", 0.85),
            ev(Header, "gba", 0.95),
            ev(Hash, "gba", 1.0),
        ],
        excluded: &[],
        content_read: true,
        broad_root: false,
        thresholds: Thresholds::default(),
    });
    assert_eq!(c.confidence, 1.0);
    assert_eq!(c.method, Some(Hash));
    assert_eq!(c.why[0].kind, Hash);
}

#[test]
fn an_exact_margin_of_a_tenth_is_enough() {
    // `.gbc` (unique to gbc) on a DMG cart: 0.95 - 0.85 is 0.0999… in f32.
    let (v, s) = decide(&[ev(Header, "gb", 0.95), ev(Extension, "gbc", 0.85)], &[]);
    assert_eq!((v, s.as_deref()), (Verdict::Auto, Some("gb")));
}

#[test]
fn alternatives_keep_other_candidates_from_four_tenths() {
    let c = combine(&CombineInput {
        evidence: &[
            ev(Header, "gbc", 0.85),
            ev(Extension, "gb", 0.5),
            ev(Name, "gba", 0.30),
        ],
        excluded: &[],
        content_read: true,
        broad_root: false,
        thresholds: Thresholds::default(),
    });
    assert_eq!(c.alternatives, vec![("gb".to_string(), 0.5)]);
    assert_eq!(c.why.len(), 1);
}

#[test]
fn a_remembered_choice_settles_a_tie_and_is_the_method() {
    // Two catalogues held the CRC; the user picked gb once. Re-planning must
    // not ask again (spec §6.6: method = user, confidence 1.0).
    let c = combine(&CombineInput {
        evidence: &[
            ev(Hash, "gb", 1.0),
            ev(Hash, "gbc", 1.0),
            ev(User, "gb", 1.0),
        ],
        excluded: &[],
        content_read: true,
        broad_root: false,
        thresholds: Thresholds::default(),
    });
    assert_eq!(c.verdict, Verdict::Auto);
    assert_eq!(c.system.as_deref(), Some("gb"));
    assert_eq!(c.method, Some(User));
    assert_eq!(c.confidence, 1.0);
    assert_eq!(c.why[0].kind, User);
    assert_eq!(c.alternatives, vec![("gbc".to_string(), 1.0)]);
}

#[test]
fn a_remembered_choice_never_overrides_a_hard_veto() {
    let (v, s) = decide(&[ev(Header, "gbc", 0.95), ev(User, "gb", 1.0)], &["gb"]);
    assert_eq!((v, s.as_deref()), (Verdict::Auto, Some("gbc")));
}

#[test]
fn an_unread_unique_extension_is_not_content_support() {
    // A `.pce` inside a 7z (or cloud-only): no signature to check, but the
    // bytes were never read either.
    let c = combine(&CombineInput {
        evidence: &[ev(Extension, "pcengine", 0.85)],
        excluded: &[],
        content_read: false,
        broad_root: false,
        thresholds: Thresholds::default(),
    });
    assert_eq!(c.verdict, Verdict::Review);
}

// --- Catalogue over cached DATs (Task 13) ---

use coverly_core::cache::{Cache, DatRow};
use coverly_core::identify::dat::{dat_sources, DatStore};
use coverly_core::net::testing::MockServer;
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::organize::evidence::{Catalogue, DatCatalogue, MAX_DAT_DOWNLOADS};

fn row(game: &str, rom: &str, size: u64, crc: &str) -> DatRow {
    DatRow {
        game: game.into(),
        rom_name: rom.into(),
        size: Some(size),
        crc32: Some(crc.into()),
        md5: None,
        sha1: None,
        serial: None,
    }
}

fn http() -> PoliteHttp<UreqHttp> {
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    PoliteHttp::new(UreqHttp::new(&cfg), &cfg)
}

fn first_dat(db: &SystemDb, id: &str) -> String {
    dat_sources(db.get(id).unwrap()).remove(0)
}

#[test]
fn offline_uses_cached_dats_only_and_reports_the_rest() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    cache
        .replace_dat(
            &first_dat(&db, "gb"),
            &[row(
                "Tetris (World)",
                "Tetris (World).gb",
                32768,
                "46DF91AD",
            )],
        )
        .unwrap();
    let cat = DatCatalogue::new(&db, &cache, None, MAX_DAT_DOWNLOADS);
    let hits = cat.hits(&["gb".into(), "gbc".into()], "46df91ad", 32768);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].system, "gb");
    assert_eq!(hits[0].game, "Tetris (World)");
    assert_eq!(hits[0].rom_name, "Tetris (World).gb");
    assert_eq!(cat.unconsulted(), ["gbc"]);
}

#[test]
fn a_crc_hit_with_another_size_is_not_a_hit() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    cache
        .replace_dat(
            &first_dat(&db, "gb"),
            &[row(
                "Tetris (World)",
                "Tetris (World).gb",
                32768,
                "46DF91AD",
            )],
        )
        .unwrap();
    let cat = DatCatalogue::new(&db, &cache, None, 0);
    assert!(cat.hits(&["gb".into()], "46DF91AD", 65536).is_empty());
    assert!(cat.any_hits("46DF91AD", 65536).is_empty());
}

#[test]
fn missing_dats_are_downloaded_up_to_the_cap() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let server = MockServer::start();
    for id in ["gb", "gbc", "gba", "snes", "nes"] {
        for dat in dat_sources(db.get(id).unwrap()) {
            server.serve_dat(
                &dat,
                "game (\n name \"X\"\n rom ( name \"x\" size 1 crc 00000001 )\n)\n",
            );
        }
    }
    let http = http();
    let store = DatStore::new(&http, &cache, server.base());
    let cat = DatCatalogue::new(&db, &cache, Some(&store), 2);
    let systems: Vec<String> = ["gb", "gbc", "gba", "snes", "nes"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let hits = cat.hits(&systems, "00000001", 1);
    assert_eq!(hits.len(), 2, "the two downloaded DATs answer");
    assert_eq!(cat.unconsulted().len(), 3, "only two downloads per plan");
    // Asking again never downloads past the cap.
    let _ = cat.hits(&systems, "00000001", 1);
    assert_eq!(server.requests().len(), 2);
}

#[test]
fn any_hits_names_the_system_of_each_cached_dat() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    cache
        .replace_dat(
            &first_dat(&db, "gba"),
            &[row(
                "Sonic Advance 3 (USA)",
                "Sonic Advance 3 (USA).gba",
                16,
                "ABCDEF01",
            )],
        )
        .unwrap();
    let cat = DatCatalogue::new(&db, &cache, None, 0);
    let hits = cat.any_hits("abcdef01", 16);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].system, "gba");
    assert_eq!(hits[0].game, "Sonic Advance 3 (USA)");
}

#[test]
fn stems_come_from_ready_dats_and_stem_systems_from_the_cache() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    cache
        .replace_dat(
            &first_dat(&db, "gb"),
            &[row(
                "Tetris (World)",
                "Tetris (World).gb",
                32768,
                "46DF91AD",
            )],
        )
        .unwrap();
    let cat = DatCatalogue::new(&db, &cache, None, 0);
    assert_eq!(
        cat.stems("gb"),
        [("tetris (world)".to_string(), "Tetris (World)".to_string())]
    );
    assert!(cat.stems("gbc").is_empty());
    assert_eq!(cat.stem_systems(), ["gb"]);
}

#[test]
fn arcade_sets_by_name_with_their_roms() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    cache
        .replace_dat(
            &first_dat(&db, "fbneo"),
            &[
                row("sf2", "sf2-1.bin", 4, "11111111"),
                row("sf2", "sf2-2.bin", 4, "22222222"),
            ],
        )
        .unwrap();
    let cat = DatCatalogue::new(&db, &cache, None, 0);
    let (game, roms) = cat.set_roms("fbneo", "SF2").unwrap();
    assert_eq!(game, "sf2");
    assert_eq!(
        roms,
        [("11111111".to_string(), 4), ("22222222".to_string(), 4)]
    );
    assert!(cat.set_roms("fbneo", "nope").is_none());
}

#[test]
fn arcade_sets_listed_by_zip_name_resolve_to_their_description() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    cache
        .replace_dat(
            &first_dat(&db, "fbneo"),
            &[row(
                "Street Fighter II (World 910522)",
                "sf2.zip",
                4,
                "33333333",
            )],
        )
        .unwrap();
    let cat = DatCatalogue::new(&db, &cache, None, 0);
    let (game, roms) = cat.set_roms("fbneo", "sf2").unwrap();
    assert_eq!(game, "Street Fighter II (World 910522)");
    assert_eq!(roms, [("33333333".to_string(), 4)]);
}

#[test]
fn a_full_dat_set_is_found_by_its_own_name_before_a_clone_sharing_a_rom() {
    // MAME-style full DAT: the clone `mspacman` ships the parent's
    // `pacman.6e`, whose stem equals the set name being asked for.
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    cache
        .replace_dat(
            &first_dat(&db, "mame"),
            &[
                row("mspacman", "pacman.6e", 4096, "C1E6AB10"),
                row("mspacman", "boot1", 4096, "D16B31B7"),
                row("pacman", "pacman.6e", 4096, "C1E6AB10"),
                row("pacman", "pacman.6f", 4096, "1A6FB2D4"),
            ],
        )
        .unwrap();
    let cat = DatCatalogue::new(&db, &cache, None, 0);
    let (game, roms) = cat.set_roms("mame", "pacman").unwrap();
    assert_eq!(game, "pacman");
    assert_eq!(
        roms,
        [
            ("C1E6AB10".to_string(), 4096),
            ("1A6FB2D4".to_string(), 4096)
        ]
    );
}

struct OfflineHttp(std::sync::atomic::AtomicUsize);

impl coverly_core::net::Http for OfflineHttp {
    fn get(
        &self,
        _req: &coverly_core::net::HttpRequest,
    ) -> Result<coverly_core::net::HttpResponse, coverly_core::net::NetError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(coverly_core::net::NetError::Offline("unreachable".into()))
    }
}

#[test]
fn once_the_network_is_down_no_more_downloads_are_tried() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let http = OfflineHttp(Default::default());
    let store = DatStore::new(&http, &cache, "https://example.invalid");
    let cat = DatCatalogue::new(&db, &cache, Some(&store), MAX_DAT_DOWNLOADS);
    let systems: Vec<String> = ["gb", "gbc", "gba", "snes", "nes"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert!(cat.hits(&systems, "00000001", 1).is_empty());
    assert_eq!(
        http.0.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "an offline error stops the on-demand downloads"
    );
    assert_eq!(cat.unconsulted().len(), 5);
}

// --- The identification ladder per file (Task 14) ---

use std::path::Path;

use common::roms::zip_file;
use coverly_core::identify::sniff::NotARom;
use coverly_core::organize::evidence::{
    identify_candidate, scene_group_hint, CatalogueHit, Facts, Identification, LadderEnv,
    OnlineLookup,
};
use coverly_core::organize::inspect::inspect_file;
use coverly_core::organize::plan::ItemNote;
use coverly_core::organize::LeftReason;

struct FakeCatalogue {
    rows: Vec<(&'static str, String, u64, &'static str)>,
    stems: Vec<(&'static str, &'static str)>,
}

impl FakeCatalogue {
    fn empty() -> FakeCatalogue {
        FakeCatalogue {
            rows: vec![],
            stems: vec![],
        }
    }

    fn hit(row: &(&'static str, String, u64, &'static str)) -> CatalogueHit {
        CatalogueHit {
            system: row.0.to_string(),
            game: row.3.to_string(),
            rom_name: row.3.to_string(),
        }
    }
}

impl Catalogue for FakeCatalogue {
    fn hits(&self, systems: &[String], crc32: &str, size: u64) -> Vec<CatalogueHit> {
        self.rows
            .iter()
            .filter(|r| systems.iter().any(|s| s == r.0))
            .filter(|r| r.1.eq_ignore_ascii_case(crc32) && r.2 == size)
            .map(Self::hit)
            .collect()
    }
    fn any_hits(&self, crc32: &str, size: u64) -> Vec<CatalogueHit> {
        self.rows
            .iter()
            .filter(|r| r.1.eq_ignore_ascii_case(crc32) && r.2 == size)
            .map(Self::hit)
            .collect()
    }
    fn stems(&self, system: &str) -> Vec<(String, String)> {
        self.stems
            .iter()
            .filter(|s| s.0 == system)
            .map(|s| (s.1.to_lowercase(), s.1.to_string()))
            .collect()
    }
    fn stem_systems(&self) -> Vec<String> {
        let mut out: Vec<String> = self.stems.iter().map(|s| s.0.to_string()).collect();
        out.sort();
        out.dedup();
        out
    }
    fn set_roms(&self, _system: &str, _set: &str) -> Option<(String, Vec<(String, u64)>)> {
        None
    }
    fn unconsulted(&self) -> Vec<String> {
        Vec::new()
    }
}

fn identify_with(
    path: &Path,
    cat: &dyn Catalogue,
    cache: &Cache,
    online: Option<&dyn OnlineLookup>,
) -> Identification {
    let db = SystemDb::builtin();
    let len = std::fs::metadata(path).unwrap().len();
    let mut facts = Facts {
        inspection: inspect_file(path, len, &db).unwrap(),
        raw_crcs: vec![],
        hashed: false,
    };
    let env = LadderEnv {
        db: &db,
        catalogue: cat,
        cache,
        thresholds: Thresholds::default(),
        broad_root: false,
        need_hash: false,
        arcade_order: &["fbneo".into(), "mame".into()],
        online,
    };
    identify_candidate(
        path,
        path.file_name().unwrap().to_str().unwrap(),
        len,
        &mut facts,
        false,
        &env,
    )
}

fn identify(path: &Path, cat: &FakeCatalogue) -> Identification {
    identify_with(path, cat, &Cache::in_memory().unwrap(), None)
}

#[test]
fn scene_named_gba_zip_is_decided_by_crc_and_header() {
    let tmp = tempfile::tempdir().unwrap();
    let rom = fx::gba();
    let crc = format!("{:08X}", crc32fast::hash(&rom));
    let p = tmp.path().join("1490 - Sonic Advance 3 (U)(Venom).zip");
    zip_file(&p, &[("1490 - Sonic Advance 3 (U)(Venom).gba", &rom, true)]);
    let id = identify(
        &p,
        &FakeCatalogue {
            rows: vec![(
                "gba",
                crc.clone(),
                rom.len() as u64,
                "Sonic Advance 3 (USA)",
            )],
            stems: vec![],
        },
    );
    assert_eq!(id.combined.verdict, Verdict::Auto);
    assert_eq!(id.combined.system.as_deref(), Some("gba"));
    assert_eq!(id.combined.method, Some(Hash));
    assert_eq!(id.catalogue_name.as_deref(), Some("Sonic Advance 3 (USA)"));
    assert_eq!(id.content_key, Some((crc, rom.len() as u64)));
    assert_eq!(id.rom_count, 1);
}

#[test]
fn without_any_dat_the_header_still_decides() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Tetris DX.zip");
    zip_file(&p, &[("Tetris DX.gbc", &fx::gb(0xC0), true)]);
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(
        (id.combined.verdict, id.combined.system.as_deref()),
        (Verdict::Auto, Some("gbc"))
    );
    assert_eq!(id.combined.method, Some(Header));
}

#[test]
fn same_title_different_systems() {
    let tmp = tempfile::tempdir().unwrap();
    let snes = tmp.path().join("Tetris 2.zip");
    zip_file(
        &snes,
        &[("Tetris 2.sfc", &fx::snes(fx::SnesMap::Lo, false), true)],
    );
    let gb = tmp.path().join("Tetris 2 (1).zip");
    zip_file(&gb, &[("Tetris 2.gb", &fx::gb(0x00), true)]);
    let none = FakeCatalogue::empty();
    assert_eq!(
        identify(&snes, &none).combined.system.as_deref(),
        Some("snes")
    );
    assert_eq!(identify(&gb, &none).combined.system.as_deref(), Some("gb"));
}

#[test]
fn a_web_page_and_a_collection_are_left() {
    let tmp = tempfile::tempdir().unwrap();
    let html = tmp.path().join("3-D Tetris (USA).zip");
    std::fs::write(&html, fx::html_page()).unwrap();
    let id = identify(&html, &FakeCatalogue::empty());
    assert!(matches!(
        id.left,
        Some(LeftReason::NotARom {
            what: NotARom::WebPage
        })
    ));
    assert_eq!(id.combined.verdict, Verdict::Unknown);
    let mixed = tmp.path().join("mixed.zip");
    zip_file(
        &mixed,
        &[("a.gb", &fx::gb(0), false), ("b.gba", &fx::gba(), false)],
    );
    assert!(matches!(
        identify(&mixed, &FakeCatalogue::empty()).left,
        Some(LeftReason::MultiGameArchive { count: 2 })
    ));
}

#[test]
fn an_unsupported_system_is_left_with_its_name() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Pico Game.zip");
    zip_file(&p, &[("Pico Game.md", &fx::pico(), true)]);
    assert!(matches!(
        identify(&p, &FakeCatalogue::empty()).left,
        Some(LeftReason::UnsupportedSystem { .. })
    ));
}

#[test]
fn two_versions_of_one_game_in_one_zip_are_allowed_with_a_note() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Tetris (JUE).zip");
    zip_file(
        &p,
        &[
            ("Tetris (V1.0).gb", &fx::gb(0), false),
            ("Tetris (V1.1).gb", &fx::gb(0), false),
        ],
    );
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(id.left, None);
    assert_eq!(id.combined.system.as_deref(), Some("gb"));
    assert!(id.notes.contains(&ItemNote::MultiRomZip { count: 2 }));
    assert_eq!(id.rom_count, 2);
}

/// A real 7z holding one entry that carries no header evidence.
fn opaque_7z(path: &Path) {
    use coverly_core::archive::testing::{sevenz, SevenZShape};
    let bytes = sevenz(&[("data.xyz", &[0u8; 64])], SevenZShape::non_solid_lzma());
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn a_seven_zip_without_header_evidence_is_named_only_and_never_auto() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Tetris (World).7z");
    opaque_7z(&p);
    let cat = FakeCatalogue {
        rows: vec![],
        stems: vec![("gb", "Tetris (World)")],
    };
    let id = identify(&p, &cat);
    assert_eq!(id.combined.verdict, Verdict::Review);
    assert_eq!(id.combined.system.as_deref(), Some("gb"));
    assert_eq!(id.combined.method, Some(Name));
}

#[test]
fn raw_files_are_hashed_only_when_needed() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.gba");
    std::fs::write(&p, fx::gba()).unwrap();
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(id.combined.verdict, Verdict::Auto, "header decides");
    assert_eq!(id.content_key, None, "no hashing for an Auto raw file");
}

#[test]
fn a_raw_file_below_auto_is_hashed_and_found_in_a_dat() {
    // A `.gb` with no header: extension alone is shared (Review), so the
    // file is hashed and the DAT decides.
    let tmp = tempfile::tempdir().unwrap();
    let rom = fx::zeros(32 * 1024);
    let crc = format!("{:08X}", crc32fast::hash(&rom));
    let p = tmp.path().join("Homebrew.gb");
    std::fs::write(&p, &rom).unwrap();
    let cat = FakeCatalogue {
        rows: vec![("gb", crc.clone(), rom.len() as u64, "Homebrew (World)")],
        stems: vec![],
    };
    let id = identify(&p, &cat);
    assert_eq!(
        (id.combined.verdict, id.combined.system.as_deref()),
        (Verdict::Auto, Some("gb"))
    );
    assert_eq!(id.combined.method, Some(Hash));
    assert_eq!(id.content_key, Some((crc, rom.len() as u64)));
    assert_eq!(id.catalogue_name.as_deref(), Some("Homebrew (World)"));
}

#[test]
fn a_crc_outside_the_candidates_is_found_in_any_cached_dat() {
    let tmp = tempfile::tempdir().unwrap();
    let rom = fx::zeros(4096);
    let crc = format!("{:08X}", crc32fast::hash(&rom));
    let p = tmp.path().join("Mystery.zip");
    zip_file(&p, &[("Mystery.bin", &rom, true)]);
    let cat = FakeCatalogue {
        rows: vec![("pcengine", crc, rom.len() as u64, "Mystery (Japan)")],
        stems: vec![],
    };
    let id = identify(&p, &cat);
    assert_eq!(id.combined.system.as_deref(), Some("pcengine"));
    assert_eq!(id.combined.method, Some(Hash));
    assert!((id.combined.why[0].weight - 0.97).abs() < 1e-6);
}

#[test]
fn several_dats_hit_and_the_header_keeps_its_own() {
    let tmp = tempfile::tempdir().unwrap();
    let rom = fx::gb(0x00);
    let crc = format!("{:08X}", crc32fast::hash(&rom));
    let p = tmp.path().join("Tetris.zip");
    zip_file(&p, &[("Tetris.gb", &rom, true)]);
    let size = rom.len() as u64;
    let cat = FakeCatalogue {
        rows: vec![
            ("gb", crc.clone(), size, "Tetris (World)"),
            ("gbc", crc, size, "Tetris (World) (GBC)"),
        ],
        stems: vec![],
    };
    let id = identify(&p, &cat);
    assert_eq!(
        (id.combined.verdict, id.combined.system.as_deref()),
        (Verdict::Auto, Some("gb"))
    );
    assert_eq!(id.catalogue_name.as_deref(), Some("Tetris (World)"));
}

#[test]
fn a_contradicted_extension_is_noted() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Hack.zip");
    zip_file(&p, &[("Hack.gba", &fx::zeros(64 * 1024), true)]);
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(id.combined.verdict, Verdict::Review);
    assert!(id
        .notes
        .contains(&ItemNote::ExtensionContradicted { ext: "gba".into() }));
}

#[test]
fn scene_groups_hint_gba() {
    assert_eq!(
        scene_group_hint("0319 - Crash Bandicoot XS (E)(Paracox)"),
        Some("gba")
    );
    assert_eq!(
        scene_group_hint("Pokemon Verde Hoja (S)(Rising Sun)"),
        Some("gba")
    );
    assert_eq!(
        scene_group_hint("1490 - Sonic Advance 3 (U)(venom)"),
        Some("gba")
    );
    assert_eq!(scene_group_hint("Tetris (World)"), None);
    assert_eq!(scene_group_hint("Venom - Lethal Protector (USA)"), None);
}

#[test]
fn tags_hint_the_game_boy_family() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Pokemon Pinball (USA) (GB Compatible).7z");
    opaque_7z(&p);
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(id.combined.verdict, Verdict::Review);
    assert_eq!(id.combined.system.as_deref(), Some("gbc"));
}

#[test]
fn a_remembered_choice_settles_the_same_content() {
    let tmp = tempfile::tempdir().unwrap();
    let rom = fx::zeros(32 * 1024);
    let crc = format!("{:08X}", crc32fast::hash(&rom));
    let p = tmp.path().join("Tetris.zip");
    zip_file(&p, &[("Tetris.gb", &rom, true)]);
    let cache = Cache::in_memory().unwrap();
    let before = identify_with(&p, &FakeCatalogue::empty(), &cache, None);
    assert_eq!(before.combined.verdict, Verdict::Review);
    cache
        .put_organize_choice(&crc.to_lowercase(), rom.len() as u64, "gbc")
        .unwrap();
    let id = identify_with(&p, &FakeCatalogue::empty(), &cache, None);
    assert_eq!(id.combined.verdict, Verdict::Auto);
    assert_eq!(id.combined.system.as_deref(), Some("gbc"));
    assert_eq!(id.combined.method, Some(User));
}

struct FakeOnline;

impl OnlineLookup for FakeOnline {
    fn lookup(
        &self,
        systems: &[String],
        _file_name: &str,
        _crc32: &str,
        _size: u64,
    ) -> Vec<(String, String)> {
        assert!(systems.iter().any(|s| s == "gb"));
        vec![("gb".into(), "Tetris (World)".into())]
    }
}

#[test]
fn online_lookup_adds_name_level_evidence_only() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Tetris.zip");
    zip_file(&p, &[("Tetris.gb", &fx::zeros(32 * 1024), true)]);
    let id = identify_with(
        &p,
        &FakeCatalogue::empty(),
        &Cache::in_memory().unwrap(),
        Some(&FakeOnline),
    );
    assert_eq!(id.combined.verdict, Verdict::Review);
    assert_eq!(id.combined.system.as_deref(), Some("gb"));
    assert_eq!(id.combined.method, Some(Extension));
    assert!(id.combined.why.iter().any(|e| e.kind == Online));
}

struct ArcadeCatalogue;

impl Catalogue for ArcadeCatalogue {
    fn hits(&self, _: &[String], _: &str, _: u64) -> Vec<CatalogueHit> {
        Vec::new()
    }
    fn any_hits(&self, _: &str, _: u64) -> Vec<CatalogueHit> {
        Vec::new()
    }
    fn stems(&self, _: &str) -> Vec<(String, String)> {
        Vec::new()
    }
    fn stem_systems(&self) -> Vec<String> {
        Vec::new()
    }
    fn set_roms(&self, system: &str, set: &str) -> Option<(String, Vec<(String, u64)>)> {
        (set == "sf2" && (system == "fbneo" || system == "mame")).then(|| {
            (
                "Street Fighter II".to_string(),
                vec![
                    (format!("{:08X}", crc32fast::hash(b"one")), 3),
                    (format!("{:08X}", crc32fast::hash(b"two")), 3),
                ],
            )
        })
    }
    fn unconsulted(&self) -> Vec<String> {
        vec!["fbneo".into()]
    }
}

#[test]
fn an_arcade_set_confirmed_by_contents_follows_the_arcade_order() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("sf2.zip");
    zip_file(
        &p,
        &[("sf2-1.bin", b"one", true), ("sf2-2.bin", b"two", true)],
    );
    let id = identify_with(&p, &ArcadeCatalogue, &Cache::in_memory().unwrap(), None);
    assert_eq!(
        (id.combined.verdict, id.combined.system.as_deref()),
        (Verdict::Auto, Some("fbneo"))
    );
    assert_eq!(id.combined.alternatives, vec![("mame".to_string(), 0.99)]);
    assert_eq!(id.catalogue_name.as_deref(), Some("Street Fighter II"));
    assert!(id.notes.contains(&ItemNote::CatalogueMissing {
        system: "fbneo".into()
    }));

    let named = tmp.path().join("sf2b.zip");
    zip_file(&named, &[("x.bin", b"zz", true), ("y.bin", b"yy", true)]);
    let p2 = tmp.path().join("sf2.zip");
    std::fs::rename(&named, &p2).unwrap();
    let id = identify_with(&p2, &ArcadeCatalogue, &Cache::in_memory().unwrap(), None);
    assert_eq!(id.combined.verdict, Verdict::Review, "name only");
}

#[test]
fn a_cloud_only_file_is_never_auto_even_with_remembered_facts() {
    // Facts memoised while the file was local still name the system, but a
    // placeholder is identified by name/extension only (spec §7.1).
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.gba");
    std::fs::write(&p, fx::gba()).unwrap();
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let cat = FakeCatalogue::empty();
    let len = std::fs::metadata(&p).unwrap().len();
    let mut facts = Facts {
        inspection: inspect_file(&p, len, &db).unwrap(),
        raw_crcs: vec![],
        hashed: false,
    };
    let env = LadderEnv {
        db: &db,
        catalogue: &cat,
        cache: &cache,
        thresholds: Thresholds::default(),
        broad_root: false,
        need_hash: true,
        arcade_order: &[],
        online: None,
    };
    let id = identify_candidate(&p, "Game.gba", len, &mut facts, true, &env);
    assert_eq!(id.combined.verdict, Verdict::Review);
    assert_eq!(id.combined.system.as_deref(), Some("gba"));
    assert!(!facts.hashed, "a placeholder is never hashed");
}

/// Reports a system as not consulted only once something asked for it,
/// like [`DatCatalogue`] does.
struct AskedCatalogue(std::sync::Mutex<std::collections::BTreeSet<String>>);

impl Catalogue for AskedCatalogue {
    fn hits(&self, systems: &[String], _: &str, _: u64) -> Vec<CatalogueHit> {
        self.0.lock().unwrap().extend(systems.iter().cloned());
        Vec::new()
    }
    fn any_hits(&self, _: &str, _: u64) -> Vec<CatalogueHit> {
        Vec::new()
    }
    fn stems(&self, system: &str) -> Vec<(String, String)> {
        self.0.lock().unwrap().insert(system.to_string());
        Vec::new()
    }
    fn stem_systems(&self) -> Vec<String> {
        Vec::new()
    }
    fn set_roms(&self, system: &str, _: &str) -> Option<(String, Vec<(String, u64)>)> {
        self.0.lock().unwrap().insert(system.to_string());
        None
    }
    fn unconsulted(&self) -> Vec<String> {
        self.0.lock().unwrap().iter().cloned().collect()
    }
}

#[test]
fn the_missing_catalogue_note_does_not_depend_on_the_order_of_files() {
    let tmp = tempfile::tempdir().unwrap();
    let raw = tmp.path().join("Game.gba");
    std::fs::write(&raw, fx::gba()).unwrap();
    let zip = tmp.path().join("Other.zip");
    zip_file(&zip, &[("Other.gba", &fx::gba(), true)]);
    let cache = Cache::in_memory().unwrap();
    let cat = AskedCatalogue(std::sync::Mutex::default());
    let raw_first = identify_with(&raw, &cat, &cache, None);
    let zipped = identify_with(&zip, &cat, &cache, None);
    let raw_after = identify_with(&raw, &cat, &cache, None);
    assert!(zipped.notes.contains(&ItemNote::CatalogueMissing {
        system: "gba".into()
    }));
    assert_eq!(raw_first.notes, raw_after.notes);
    assert!(
        raw_after.notes.is_empty(),
        "the raw file never needed the DAT"
    );
}
