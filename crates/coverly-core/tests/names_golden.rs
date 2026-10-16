use coverly_core::identify::names::*;
use Region::*;

/// (stem, strip_numbering, title, regions, year, numeral, key)
type Case<'a> = (
    &'a str,
    bool,
    &'a str,
    &'a [Region],
    Option<u16>,
    u32,
    &'a str,
);

#[test]
fn parses_real_world_names() {
    let cases: &[Case] = &[
        (
            "Legend of Zelda, The - A Link to the Past (USA)",
            false,
            "The Legend of Zelda - A Link to the Past",
            &[Usa],
            None,
            1,
            "legend of zelda a link to the past",
        ),
        (
            "Super Mario World (USA, Europe) (Rev 1)",
            false,
            "Super Mario World",
            &[Usa, Europe],
            None,
            1,
            "super mario world",
        ),
        (
            "Street Fighter II' Turbo (J) [!]",
            false,
            "Street Fighter II' Turbo",
            &[Japan],
            None,
            2,
            "street fighter 2 turbo",
        ),
        (
            "Contra III - The Alien Wars (JUE)",
            false,
            "Contra III - The Alien Wars",
            &[Japan, Usa, Europe],
            None,
            3,
            "contra 3 the alien wars",
        ),
        (
            "001 Golden Sun",
            true,
            "Golden Sun",
            &[],
            None,
            1,
            "golden sun",
        ),
        (
            "Pokémon - Ruby Version (USA, Europe)",
            false,
            "Pokémon - Ruby Version",
            &[Usa, Europe],
            None,
            1,
            "pokemon ruby version",
        ),
        (
            "Adventures of Batman & Robin, The (USA)",
            false,
            "The Adventures of Batman & Robin",
            &[Usa],
            None,
            1,
            "adventures of batman and robin",
        ),
        (
            "Q-Bert's Qubes (USA) (1983) (Parker Brothers) [h]",
            false,
            "Q-Bert's Qubes",
            &[Usa],
            Some(1983),
            1,
            "q berts qubes",
        ),
        (
            "Mega_Man_X_(U)",
            false,
            "Mega Man X",
            &[Usa],
            None,
            10,
            "mega man 10",
        ),
        (
            "Beta Game (Beta) (Unl)",
            false,
            "Beta Game",
            &[],
            None,
            1,
            "beta game",
        ),
        // Lower-casing `İ` changes its UTF-8 length: byte offsets from a
        // lower-cased copy must never index the original (hostile names must not panic).
        (
            "İstanbul Racer, The (Turkey)",
            false,
            "The İstanbul Racer",
            &[],
            None,
            1,
            "istanbul racer",
        ),
    ];
    for (stem, strip, title, regions, year, numeral, key) in cases {
        let p = parse_name(stem, *strip);
        assert_eq!(p.title, *title, "{stem}");
        assert_eq!(p.regions, *regions, "{stem}");
        assert_eq!(p.year, *year, "{stem}");
        assert_eq!(p.numeral, *numeral, "{stem}");
        assert_eq!(p.key, *key, "{stem}");
    }
}

#[test]
fn short_key_drops_the_subtitle() {
    assert_eq!(
        parse_name("Golden Sun - The Lost Age (USA)", false).short_key,
        "golden sun"
    );
    assert_eq!(
        parse_name("Metroid: Zero Mission", false).short_key,
        "metroid"
    );
}

#[test]
fn guards_reject_other_sequels_and_years() {
    let p = |s| parse_name(s, false);
    assert!(!compatible(
        &p("Street Fighter II (USA)"),
        &p("Street Fighter III (USA)")
    ));
    assert!(!compatible(
        &p("Final Fantasy III (J)"),
        &p("Final Fantasy VI (USA)")
    ));
    assert!(!compatible(
        &p("Contra (USA)"),
        &p("Contra III - The Alien Wars (USA)")
    ));
    assert!(!compatible(&p("Q-Bert (1983)"), &p("Q-Bert (1999)")));
    assert!(compatible(&p("Contra (USA)"), &p("Contra Force (USA)")));
    assert!(compatible(&p("Q-Bert"), &p("Q-Bert (1999)")));
}

#[test]
fn score_counts_unicode_characters_not_bytes() {
    assert_eq!(score("super mario world", "super mario world"), 100);
    assert_eq!(score("aé", "ae"), 50); // bytes would give 33
    assert_eq!(score("ドラクエ", "ドラクエx"), 80); // bytes would give 93
    assert_eq!(score("", ""), 100);
    assert_eq!(score("abc", ""), 0);
}

#[test]
fn numbering_prefixes() {
    assert_eq!(strip_numbering_prefix("001 Golden Sun"), "Golden Sun");
    assert_eq!(strip_numbering_prefix("12) Foo"), "Foo");
    assert_eq!(strip_numbering_prefix("7. Bar"), "Bar");
    assert_eq!(strip_numbering_prefix("1942"), "1942");
    assert_eq!(strip_numbering_prefix("Zelda"), "Zelda");
    assert!(detect_numbering(["001 A", "002 B", "003 C", "Zelda"]));
    assert!(!detect_numbering(["1942", "Zelda", "Mario"]));
    assert!(!detect_numbering(["001 A", "002 B"])); // too few to tell
}

#[test]
fn libretro_sanitising_and_keys() {
    assert_eq!(
        sanitise_libretro("A&B*C/D:E`F\"G<H>I?J\\K|L"),
        "A_B_C_D_E_F_G_H_I_J_K_L"
    );
    assert_eq!(sanitise_libretro("Batman & Robin"), "Batman _ Robin");
    assert_eq!(
        libretro_key("Super Mario World (USA).PNG"),
        "super mario world (usa)"
    );
    assert_eq!(libretro_key("Cafe\u{301}.png"), "café"); // NFD input → NFC key
}

#[test]
fn region_words_for_config() {
    assert_eq!(Region::parse("usa"), Some(Usa));
    assert_eq!(Region::parse("EU"), Some(Europe));
    assert_eq!(Region::parse("jp"), Some(Japan));
    assert_eq!(Region::parse("World"), Some(World));
    assert_eq!(Region::parse("mars"), None);
}

#[test]
fn hostile_names_never_panic() {
    let hostile = [
        "",
        " ",
        "(",
        ")",
        "[[[",
        "}{",
        "((USA)",
        "Foo (USA",
        ", The",
        ", THE",
        "İİİ, The",
        "ǅ, the - x",
        "0",
        "0000",
        "00000 x",
        "1)",
        "1.",
        "1 - ",
        "../../etc/passwd",
        "a\u{0}b",
        "\u{301}\u{301}",
        "ẞ, The: ẞ",
        "ﬁ ﬂ, the",
        "Ⅻ (1983) (2099) (1969)",
        "x (JUEWABCFGHIKS)",
        "日本語, The (Japan)",
    ];
    for stem in hostile {
        for strip in [false, true] {
            let p = parse_name(stem, strip);
            let _ = compatible(&p, &p);
            let _ = score(&p.key, &p.short_key);
        }
        let _ = sanitise_libretro(stem);
        let _ = libretro_key(stem);
        let _ = without_subtitle(stem);
        let _ = strip_numbering_prefix(stem);
    }
    let _ = detect_numbering(hostile);
}
