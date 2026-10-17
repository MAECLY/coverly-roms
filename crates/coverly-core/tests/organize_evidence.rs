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
