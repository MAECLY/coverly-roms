mod common;

use coverly_core::identify::evidence::{Evidence, IdLevel::*};
use coverly_core::identify::sniff::{fixtures as fx, sniff, SliceAt, Sniff};
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
fn a_zipped_pico_cart_is_no_longer_left_unsupported() {
    // Sega Pico was the last cart `Unsupported` verdict (spec §8.1): its
    // zip is now organised as Sega Pico.
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Pico Game.zip");
    zip_file(&p, &[("Pico Game.md", &fx::pico(), true)]);
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(id.left, None);
    assert_eq!(
        (id.combined.system.as_deref(), id.combined.verdict),
        (Some("pico"), Verdict::Auto)
    );
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
    assert!(id.notes.contains(&ItemNote::ExtensionContradicted {
        ext: "gba".into(),
        system: Some("gba".into()),
    }));
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

#[test]
fn weak_extension_alone_is_never_evidence() {
    let db = SystemDb::builtin()
        .with_overrides("[[system]]\nid = \"zz\"\nname = \"ZZ\"\nweak_extensions = [\"zzq\"]\n")
        .unwrap();
    let ev = extension_evidence(
        &db,
        "zzq",
        &Sniff {
            read: 4096,
            ..Sniff::default()
        },
    );
    assert!(ev.is_empty());
}

#[test]
fn weak_owner_header_contradicts_the_strong_extension() {
    // D24: `.st` is atarist's; a ≥ 0.95 header for a system that lists
    // `st` as weak drops atarist's claim to 0.60 with detail `st!`.
    let db = SystemDb::builtin()
        .with_overrides("[[system]]\nid = \"zz\"\nname = \"ZZ\"\nweak_extensions = [\"st\"]\n")
        .unwrap();
    let mut sniff = Sniff {
        read: 4096,
        ..Sniff::default()
    };
    sniff
        .hits
        .push(Evidence::new(Header, "zz", 0.95, "ZZ MAGIC"));
    let ev = extension_evidence(&db, "st", &sniff);
    let st = ev.iter().find(|e| e.system == "atarist").unwrap();
    assert_eq!((st.weight, st.detail.as_str()), (0.60, "st!"));
    assert!(
        ev.iter().all(|e| e.system != "zz"),
        "the weak owner adds nothing here"
    );

    // A weaker header, or one for a system that does not list `st`, does not.
    let mut weak = sniff.clone();
    weak.hits[0].weight = 0.90;
    let st = extension_evidence(&db, "st", &weak);
    assert_eq!((st[0].weight, st[0].detail.as_str()), (0.85, "st"));
    let mut other = sniff.clone();
    other.hits[0].system = "gba".into();
    let st = extension_evidence(&db, "st", &other);
    assert_eq!((st[0].weight, st[0].detail.as_str()), (0.85, "st"));
}

#[test]
fn weak_extension_supports_a_header() {
    use coverly_core::organize::evidence::weak_extension_support;
    let db = SystemDb::builtin()
        .with_overrides("[[system]]\nid = \"zz\"\nname = \"ZZ\"\nweak_extensions = [\"zzq\"]\n")
        .unwrap();
    // Nothing names `zz`: the weak extension adds nothing.
    let mut alone = vec![ev(Name, "zz", 0.80)];
    weak_extension_support(&db, "zzq", &mut alone);
    assert_eq!(alone.len(), 1);
    // A header names it: `zzq~` at 0.50, once.
    let mut with_header = vec![Evidence::new(Header, "zz", 0.95, "ZZ MAGIC")];
    weak_extension_support(&db, "ZZQ", &mut with_header);
    weak_extension_support(&db, "zzq", &mut with_header);
    assert_eq!(with_header.len(), 2);
    let e = &with_header[1];
    assert_eq!(
        (e.kind, e.system.as_str(), e.weight, e.detail.as_str()),
        (Extension, "zz", 0.50, "zzq~")
    );
    // A strong extension is never weak support.
    let mut gba = vec![Evidence::new(Header, "gba", 0.95, "logo")];
    weak_extension_support(&db, "gba", &mut gba);
    assert_eq!(gba.len(), 1);
}

#[test]
fn ps2_cso_contradicts_psp() {
    // Spec §12: a PS2 volume read through the CISO reader proves `ps2`,
    // which lists `cso` as weak, so psp's strong `.cso` is contradicted.
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.cso");
    let bytes = coverly_core::disc::testing::cso_of(&coverly_core::disc::testing::ps2_iso());
    std::fs::write(&p, &bytes).unwrap();
    let s = sniff(&mut SliceAt(&bytes), bytes.len() as u64, "cso");
    let ev = extension_evidence(&SystemDb::builtin(), "cso", &s);
    let psp = ev.iter().find(|e| e.system == "psp").unwrap();
    assert_eq!((psp.weight, psp.detail.as_str()), (0.60, "cso!"));
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(id.combined.system.as_deref(), Some("ps2"));
    assert_eq!(id.combined.verdict, Verdict::Auto);
}

#[test]
fn loose_bin_of_an_early_console_is_never_auto() {
    // Task 5: `.bin` and `.rom` are Generic, so a loose cart with no header
    // never moves on its own; `.col` is ColecoVision's alone.
    let tmp = tempfile::tempdir().unwrap();
    let mut seed = 0x1234_5678u32;
    let body: Vec<u8> = (0..16 * 1024)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 24) as u8 | 1
        })
        .collect();
    for name in ["Game.bin", "Game.rom", "Game.col"] {
        std::fs::write(tmp.path().join(name), &body).unwrap();
    }
    let bin = identify(&tmp.path().join("Game.bin"), &FakeCatalogue::empty());
    assert_ne!(bin.combined.verdict, Verdict::Auto, "{:?}", bin.combined);
    let rom = identify(&tmp.path().join("Game.rom"), &FakeCatalogue::empty());
    assert_eq!(rom.combined.system, None, "{:?}", rom.combined);
    let col = identify(&tmp.path().join("Game.col"), &FakeCatalogue::empty());
    assert_eq!(
        (col.combined.system.as_deref(), col.combined.verdict),
        (Some("colecovision"), Verdict::Auto)
    );
}

#[test]
fn a_vec_without_the_gce_line_is_not_auto() {
    // Vectrex is DECISIVE: a `.vec` whose bytes lack `g GCE` is
    // contradicted (0.60, review); with the line it moves on its own.
    let tmp = tempfile::tempdir().unwrap();
    let good = coverly_core::identify::sniff::fixtures::vectrex();
    let mut bare = good.clone();
    bare[..5].copy_from_slice(b"xxxxx");
    std::fs::write(tmp.path().join("Good.vec"), &good).unwrap();
    std::fs::write(tmp.path().join("Bare.vec"), &bare).unwrap();
    let g = identify(&tmp.path().join("Good.vec"), &FakeCatalogue::empty());
    assert_eq!(
        (g.combined.system.as_deref(), g.combined.verdict),
        (Some("vectrex"), Verdict::Auto)
    );
    let b = identify(&tmp.path().join("Bare.vec"), &FakeCatalogue::empty());
    assert_ne!(b.combined.verdict, Verdict::Auto, "{:?}", b.combined);
}

#[test]
fn a_coleco_bin_with_test_bytes_is_only_a_candidate() {
    // `AA 55` is 16 bits (0.60): a loose `.bin` carrying it is never Auto,
    // but a `.col` with it still is.
    let tmp = tempfile::tempdir().unwrap();
    let cart = coverly_core::identify::sniff::fixtures::colecovision(false);
    std::fs::write(tmp.path().join("Game.bin"), &cart).unwrap();
    std::fs::write(tmp.path().join("Game.col"), &cart).unwrap();
    let bin = identify(&tmp.path().join("Game.bin"), &FakeCatalogue::empty());
    assert_ne!(bin.combined.verdict, Verdict::Auto, "{:?}", bin.combined);
    let col = identify(&tmp.path().join("Game.col"), &FakeCatalogue::empty());
    assert_eq!(
        (col.combined.system.as_deref(), col.combined.verdict),
        (Some("colecovision"), Verdict::Auto)
    );
}

/// Pseudo-random bytes with no header any rule reads.
fn opaque(len: usize) -> Vec<u8> {
    let mut seed = 0x0bad_5eedu32;
    (0..len)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 24) as u8 | 1
        })
        .collect()
}

#[test]
fn pico_md_beats_megadrive() {
    // D24: `SEGA PICO` proves pico, which lists `md` as weak, so the Mega
    // Drive's strong `.md` is contradicted.
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.md");
    let bytes = fx::pico();
    std::fs::write(&p, &bytes).unwrap();
    let s = sniff(&mut SliceAt(&bytes), bytes.len() as u64, "md");
    let ev = extension_evidence(&SystemDb::builtin(), "md", &s);
    let md = ev.iter().find(|e| e.system == "megadrive").unwrap();
    assert_eq!((md.weight, md.detail.as_str()), (0.60, "md!"));
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(
        (id.combined.system.as_deref(), id.combined.verdict),
        (Some("pico"), Verdict::Auto)
    );
}

#[test]
fn sufami_st_beats_atarist() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Sailor Moon (Japan).st");
    let bytes = fx::sufami(false);
    std::fs::write(&p, &bytes).unwrap();
    let s = sniff(&mut SliceAt(&bytes), bytes.len() as u64, "st");
    let ev = extension_evidence(&SystemDb::builtin(), "st", &s);
    let st = ev.iter().find(|e| e.system == "atarist").unwrap();
    assert_eq!((st.weight, st.detail.as_str()), (0.60, "st!"));
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(
        (id.combined.system.as_deref(), id.combined.verdict),
        (Some("sufami"), Verdict::Auto)
    );
    // The note names the extension's owner, not the proven system (D24).
    assert_eq!(
        id.notes,
        vec![ItemNote::ExtensionContradicted {
            ext: "st".into(),
            system: Some("atarist".into()),
        }]
    );
}

#[test]
fn plain_st_stays_atarist() {
    // Without the magic a `.st` is an Atari ST disk, as before the batch.
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Prince of Persia.st");
    std::fs::write(&p, opaque(720 * 1024)).unwrap();
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(
        (id.combined.system.as_deref(), id.combined.verdict),
        (Some("atarist"), Verdict::Auto)
    );
}

#[test]
fn sgx_now_supergrafx() {
    // D3: `.sgx` is SuperGrafx's own format; PC Engine keeps it as weak.
    let empty = Sniff::default();
    let ev = extension_evidence(&SystemDb::builtin(), "sgx", &empty);
    assert_eq!(ev.len(), 1, "{ev:?}");
    assert_eq!(
        (ev[0].system.as_str(), ev[0].weight, ev[0].detail.as_str()),
        ("supergrafx", 0.85, "sgx")
    );
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Daimakaimura (Japan).sgx");
    std::fs::write(&p, opaque(512 * 1024)).unwrap();
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(
        (id.combined.system.as_deref(), id.combined.verdict),
        (Some("supergrafx"), Verdict::Auto)
    );
}

#[test]
fn mega_duck_bin_needs_its_folder() {
    // Task 7: Mega Duck has no header and only `.bin`, which is Generic, so
    // a loose cart is never assigned; inside `MEGADUCK/` (a sorted folder,
    // never walked) the scan lists it under megaduck.
    use coverly_core::scan::{scan_library, ScanOptions};
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Suleiman Treasure (Unknown).bin");
    std::fs::write(&p, opaque(32 * 1024)).unwrap();
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(id.combined.system, None, "{:?}", id.combined);
    assert_ne!(id.combined.verdict, Verdict::Auto);

    let db = SystemDb::builtin();
    assert_eq!(
        db.resolve_folder("MEGADUCK").map(|s| s.id.as_str()),
        Some("megaduck")
    );
    let card = tmp.path().join("card");
    let rom = card
        .join("Roms")
        .join("MEGADUCK")
        .join("Suleiman Treasure (Unknown).bin");
    std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
    std::fs::write(&rom, opaque(32 * 1024)).unwrap();
    let scan = scan_library(
        &card,
        &common::profile("onion"),
        &db,
        &ScanOptions::default(),
    );
    let duck = scan
        .systems
        .iter()
        .find(|s| s.folder_name == "MEGADUCK")
        .unwrap();
    assert_eq!(duck.system_id.as_deref(), Some("megaduck"));
    let stems: Vec<&str> = duck.roms.iter().map(|r| r.stem.as_str()).collect();
    assert_eq!(stems, ["Suleiman Treasure (Unknown)"]);
}

#[test]
fn supervision_and_gamecom_unique_extensions_are_enough() {
    // Task 7: `.sv` and `.tgc` are Unique and neither system has a header,
    // so the extension alone decides (same rule as other headerless
    // systems with a unique extension).
    let tmp = tempfile::tempdir().unwrap();
    for (name, want) in [
        ("Crystball (Europe).sv", "supervision"),
        ("Lights Out (USA).tgc", "gamecom"),
    ] {
        let p = tmp.path().join(name);
        std::fs::write(&p, opaque(64 * 1024)).unwrap();
        let id = identify(&p, &FakeCatalogue::empty());
        assert_eq!(
            (id.combined.verdict, id.combined.system.as_deref()),
            (Verdict::Auto, Some(want)),
            "{name}: {:?}",
            id.combined
        );
    }
}

#[test]
fn a_loose_handheld_bin_is_found_by_its_dat_hash() {
    // Task 7: a Supervision or Game.com cart dumped as `.bin` (Generic, no
    // header) is assigned only by its no-intro DAT hash.
    let tmp = tempfile::tempdir().unwrap();
    for (id_want, stem, seed) in [
        ("supervision", "Crystball (Europe)", 0x11u8),
        ("gamecom", "Lights Out (USA)", 0x22u8),
    ] {
        let rom: Vec<u8> = opaque(32 * 1024).iter().map(|b| b ^ seed).collect();
        let crc = format!("{:08X}", crc32fast::hash(&rom));
        let p = tmp.path().join(format!("{stem}.bin"));
        std::fs::write(&p, &rom).unwrap();
        let none = identify(&p, &FakeCatalogue::empty());
        assert_ne!(none.combined.verdict, Verdict::Auto, "{id_want}");
        let cat = FakeCatalogue {
            rows: vec![(id_want, crc, rom.len() as u64, stem)],
            stems: vec![],
        };
        let id = identify(&p, &cat);
        assert_eq!(
            (id.combined.verdict, id.combined.system.as_deref()),
            (Verdict::Auto, Some(id_want)),
            "{:?}",
            id.combined
        );
        assert_eq!(id.combined.method, Some(Hash));
    }
}

#[test]
fn loose_dsk_goes_to_review() {
    // Task 8 (D3): `.dsk` is Shared. With the CPCEMU magic the likely list
    // is Amstrad CPC, then ZX Spectrum (+3); without it every `.dsk` owner
    // is a candidate. Nothing is Auto, and size is never proof.
    let tmp = tempfile::tempdir().unwrap();
    let cpc = tmp.path().join("Arkanoid (1987).dsk");
    std::fs::write(&cpc, fx::cpc_dsk(true)).unwrap();
    let id = identify(&cpc, &FakeCatalogue::empty());
    assert_eq!(id.combined.verdict, Verdict::Review, "{:?}", id.combined);
    assert_eq!(id.combined.system.as_deref(), Some("amstradcpc"));
    assert_eq!(
        id.combined.alternatives.first().map(|a| a.0.as_str()),
        Some("zxspectrum"),
        "{:?}",
        id.combined.alternatives
    );

    let bare = tmp.path().join("Unknown Disk.dsk");
    std::fs::write(&bare, opaque(180 * 1024)).unwrap();
    let id = identify(&bare, &FakeCatalogue::empty());
    assert_ne!(id.combined.verdict, Verdict::Auto, "{:?}", id.combined);
    let db = SystemDb::builtin();
    let owners: Vec<&str> = db
        .systems_for_extension("dsk")
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    let mut listed: Vec<&str> = id.combined.system.iter().map(String::as_str).collect();
    listed.extend(id.combined.alternatives.iter().map(|a| a.0.as_str()));
    listed.sort_unstable();
    let mut want = owners.clone();
    want.sort_unstable();
    assert_eq!(listed, want);
    for id in ["msx", "zxspectrum", "amstradcpc", "dragon", "coco"] {
        assert!(owners.contains(&id), "{id} owns .dsk");
    }
}

#[test]
fn loose_tap_stays_review() {
    // `.tap` was already Shared (c64, vic20) and still is; the Spectrum
    // joins its owners.
    let db = SystemDb::builtin();
    assert_eq!(
        db.extension_class("tap"),
        coverly_core::systems::ExtClass::Shared
    );
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Manic Miner.tap");
    std::fs::write(&p, opaque(48 * 1024)).unwrap();
    let id = identify(&p, &FakeCatalogue::empty());
    assert_ne!(id.combined.verdict, Verdict::Auto, "{:?}", id.combined);
    let mut listed: Vec<&str> = id.combined.system.iter().map(String::as_str).collect();
    listed.extend(id.combined.alternatives.iter().map(|a| a.0.as_str()));
    for want in ["c64", "vic20", "zxspectrum"] {
        assert!(listed.contains(&want), "{want}: {listed:?}");
    }
}

#[test]
fn tzx_and_uef_with_their_magic_are_auto() {
    // Task 8: a header ≥ 0.85 plus an extension the system owns (`.sna`
    // is Shared: the CPCEMU snapshot magic decides it).
    let tmp = tempfile::tempdir().unwrap();
    for (name, bytes, want) in [
        ("Manic Miner (1983)(Bug-Byte).tzx", fx::tzx(), "zxspectrum"),
        ("Roland in Time (1985)(Amsoft).cdt", fx::tzx(), "amstradcpc"),
        ("Elite (1984)(Acornsoft).uef", fx::uef(), "bbcmicro"),
        ("Gryzor (1987)(Ocean).sna", fx::cpc_sna(), "amstradcpc"),
    ] {
        let p = tmp.path().join(name);
        std::fs::write(&p, &bytes).unwrap();
        let id = identify(&p, &FakeCatalogue::empty());
        assert_eq!(
            (id.combined.system.as_deref(), id.combined.verdict),
            (Some(want), Verdict::Auto),
            "{name}"
        );
    }
}

#[test]
fn zx81_p_and_thomson_fd_are_weak() {
    // D4: `.p` and `.fd` alone never assign a system.
    let tmp = tempfile::tempdir().unwrap();
    for name in ["3D Monster Maze.p", "Game.fd"] {
        let p = tmp.path().join(name);
        std::fs::write(&p, opaque(16 * 1024)).unwrap();
        let id = identify(&p, &FakeCatalogue::empty());
        assert_eq!(id.combined.system, None, "{name}: {:?}", id.combined);
    }
}

#[test]
fn loose_sna_without_magic_stays_review() {
    // A Spectrum snapshot has no magic: `.sna` (zxspectrum, amstradcpc)
    // alone never assigns a system.
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Jet Set Willy.sna");
    std::fs::write(&p, opaque(49_179)).unwrap();
    let id = identify(&p, &FakeCatalogue::empty());
    assert_ne!(id.combined.verdict, Verdict::Auto, "{:?}", id.combined);
    let mut listed: Vec<&str> = id.combined.system.iter().map(String::as_str).collect();
    listed.extend(id.combined.alternatives.iter().map(|a| a.0.as_str()));
    listed.sort_unstable();
    assert_eq!(listed, ["amstradcpc", "zxspectrum"]);
}

#[test]
fn loose_amiga_and_st_disks_stay_automatic() {
    // P-2 / D3: `.hdf`, `.fdi` and `.dim` turn Shared when x68000 and pc98
    // list them; the bytes that name Amiga or Atari ST keep those loose
    // disks Auto. Without a recognised header they go to review.
    let tmp = tempfile::tempdir().unwrap();
    for (name, bytes, want) in [
        ("Workbench.hdf", fx::amiga_rdb(), "amiga"),
        ("Game.hdf", fx::amiga_dos(), "amiga"),
        ("Partitioned.hdf", fx::amiga_rdb_at(1), "amiga"),
        ("Game.fdi", fx::fdi2(), "amiga"),
        ("Game.dim", fx::st_dim(), "atarist"),
        ("Human68k.hdf", fx::x68k_scsi(), "x68000"),
    ] {
        let p = tmp.path().join(name);
        std::fs::write(&p, &bytes).unwrap();
        let id = identify(&p, &FakeCatalogue::empty());
        assert_eq!(
            (id.combined.system.as_deref(), id.combined.verdict),
            (Some(want), Verdict::Auto),
            "{name}: {:?}",
            id.combined
        );
    }
    for (ext, owners) in [
        ("hdf", ["amiga", "x68000"]),
        ("fdi", ["amiga", "pc98"]),
        ("dim", ["atarist", "x68000"]),
    ] {
        let p = tmp.path().join(format!("Unknown Disk.{ext}"));
        std::fs::write(&p, opaque(720 * 1024)).unwrap();
        let id = identify(&p, &FakeCatalogue::empty());
        assert_eq!(
            id.combined.verdict,
            Verdict::Review,
            "{ext}: {:?}",
            id.combined
        );
        let mut listed: Vec<&str> = id.combined.system.iter().map(String::as_str).collect();
        listed.extend(id.combined.alternatives.iter().map(|a| a.0.as_str()));
        listed.sort_unstable();
        assert_eq!(listed, owners, "{ext}");
    }
}

#[test]
fn a_prg_load_address_never_lifts_a_weak_owner() {
    // Spec §8: the `.prg` load address is a 0.40 tie-break inside a shared
    // folder only. Loose, a Plus/4 `$1001` is also an unexpanded VIC-20
    // and a PET `$0401` a VIC-20 with 3K: the strong owners (c64, vic20)
    // keep their tie and the weak owner gains no `prg~` support (D24: a
    // weak owner wins only with a ≥ 0.95 header).
    let tmp = tempfile::tempdir().unwrap();
    for (name, load, weak) in [
        ("Plus.prg", [0x01, 0x10], "plus4"),
        ("Pet.prg", [0x01, 0x04], "pet"),
        ("C128.prg", [0x01, 0x1C], "c128"),
    ] {
        let p = tmp.path().join(name);
        std::fs::write(&p, fx::prg(load)).unwrap();
        let id = identify(&p, &FakeCatalogue::empty());
        let c = &id.combined;
        assert_ne!(c.system.as_deref(), Some(weak), "{name}: {c:?}");
        assert_ne!(c.verdict, Verdict::Auto, "{name}: {c:?}");
        // The weak owner stays under the review line: a candidate only.
        assert!(
            c.alternatives.iter().all(|(s, w)| s != weak || *w < 0.50),
            "{name}: {c:?}"
        );
    }
}

#[test]
fn loose_wasm_hex_lua_are_unknown() {
    // D4: `.wasm`, `.hex` and `.lua` are everyday developer files. On a
    // desktop pile they never name a system; inside `WASM4/` the scan
    // lists a `.wasm` under WASM-4.
    let tmp = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        ("game.wasm", fx::wasm()),
        ("firmware.hex", b":00000001FF\r\n".to_vec()),
        ("main.lua", b"function love.draw() end\n".to_vec()),
    ] {
        let p = tmp.path().join(name);
        std::fs::write(&p, &bytes).unwrap();
        let id = identify(&p, &FakeCatalogue::empty());
        assert_eq!(id.combined.system, None, "{name}: {:?}", id.combined);
        assert!(id.combined.alternatives.is_empty(), "{name}");
    }
    let lib = tempfile::tempdir().unwrap();
    common::write(lib.path(), "roms/WASM4/game.wasm", "\0asm");
    common::write(lib.path(), "roms/WASM4/notes.lua", "-- notes");
    let scan = coverly_core::scan::scan_library(
        lib.path(),
        &common::profile("esde"),
        &SystemDb::builtin(),
        &coverly_core::scan::ScanOptions::default(),
    );
    let s = scan
        .systems
        .iter()
        .find(|s| s.folder_name == "WASM4")
        .unwrap();
    assert_eq!(s.system_id.as_deref(), Some("wasm4"));
    let stems: Vec<&str> = s.roms.iter().map(|r| r.stem.as_str()).collect();
    assert_eq!(stems, ["game"]);
}

#[test]
fn fantasy_console_carts_move_only_with_their_signature() {
    // Uzebox and Vircon32 are DECISIVE: a `.uze` without `UZEBOX` or a
    // `.v32` without `V32-CART` is contradicted and never moves on its
    // own; with the marker it does. `.tic` and `.nx` (LowRes NX source,
    // text, D26) have no header and move on their unique extension.
    let tmp = tempfile::tempdir().unwrap();
    let mut bare_uze = fx::uzebox();
    bare_uze[..6].copy_from_slice(b"xxxxxx");
    let mut bare_v32 = fx::vircon32();
    bare_v32[..8].copy_from_slice(b"xxxxxxxx");
    let mut tic = vec![0u8; 1024];
    tic[0] = 0x05; // a TIC-80 code chunk
    let nx = b"PRINT \"HELLO\"\nGOTO 1\n".repeat(32);
    for (name, bytes, want) in [
        ("Game.uze", fx::uzebox(), Some("uzebox")),
        ("Bare.uze", bare_uze, None),
        ("Game.v32", fx::vircon32(), Some("vircon32")),
        ("Bare.v32", bare_v32, None),
        ("Game.tic", tic, Some("tic80")),
        ("Game.nx", nx, Some("lowresnx")),
    ] {
        let p = tmp.path().join(name);
        std::fs::write(&p, &bytes).unwrap();
        let id = identify(&p, &FakeCatalogue::empty());
        match want {
            Some(sys) => assert_eq!(
                (id.combined.system.as_deref(), id.combined.verdict),
                (Some(sys), Verdict::Auto),
                "{name}: {:?}",
                id.combined
            ),
            None => assert_ne!(
                id.combined.verdict,
                Verdict::Auto,
                "{name}: {:?}",
                id.combined
            ),
        }
    }
}

#[test]
fn wad_without_magic_is_not_auto() {
    // D15: doom is DECISIVE, so a `.wad` without `IWAD`/`PWAD` (a WiiWare
    // WAD one day) is contradicted (0.60, `wad!`) and never moves alone;
    // with the magic it does.
    let tmp = tempfile::tempdir().unwrap();
    let mut bare = fx::wad(b"IWAD");
    bare[..4].copy_from_slice(b"\0\0\0\x20");
    let s = sniff(&mut SliceAt(&bare), bare.len() as u64, "wad");
    let ev = extension_evidence(&SystemDb::builtin(), "wad", &s);
    let doom = ev.iter().find(|e| e.system == "doom").unwrap();
    assert_eq!((doom.weight, doom.detail.as_str()), (0.60, "wad!"));
    let p = tmp.path().join("Bare.wad");
    std::fs::write(&p, &bare).unwrap();
    let id = identify(&p, &FakeCatalogue::empty());
    assert_ne!(id.combined.verdict, Verdict::Auto, "{:?}", id.combined);

    let p = tmp.path().join("DOOM.WAD");
    std::fs::write(&p, fx::wad(b"IWAD")).unwrap();
    let id = identify(&p, &FakeCatalogue::empty());
    assert_eq!(
        (id.combined.system.as_deref(), id.combined.verdict),
        (Some("doom"), Verdict::Auto),
        "{:?}",
        id.combined
    );
}
