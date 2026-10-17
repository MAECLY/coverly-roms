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
