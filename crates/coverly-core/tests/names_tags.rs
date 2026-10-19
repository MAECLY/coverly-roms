use coverly_core::identify::names::{
    has_modification_tag, parse_tags, ModClass, Region, ReleaseClass, Revision,
};

fn rev(major: u16, minor: u16, letter: u8) -> Revision {
    Revision {
        major,
        minor,
        letter,
    }
}

#[test]
fn revisions_normalise() {
    let cases: [(&str, Revision, &str); 6] = [
        ("X (JUE) (V1.1) [!]", rev(1, 1, 0), "V1.1"),
        ("X (World) (Rev 1)", rev(1, 0, 0), "Rev 1"),
        ("X (USA) (Rev A)", rev(0, 0, b'A'), "Rev A"),
        ("X (Europe) (v1.02)", rev(1, 2, 0), "v1.02"),
        ("X (USA) (Rev 1.1)", rev(1, 1, 0), "Rev 1.1"),
        ("X (JUE) (V1.0) [!]", rev(1, 0, 0), "V1.0"),
    ];
    for (name, want, text) in cases {
        let tags = parse_tags(name);
        assert_eq!(tags.revision, Some(want), "{name}");
        assert_eq!(tags.revision_text.as_deref(), Some(text), "{name}");
        assert_eq!(tags.title_key, "x", "{name}");
    }
    assert_eq!(
        parse_tags("Tetris (JUE) (V1.1) [!]").revision,
        Some(rev(1, 1, 0))
    );
    assert_eq!(parse_tags("Tetris (USA)").revision, None);
    assert!(rev(0, 0, b'A') < rev(1, 0, 0));
    assert!(rev(1, 0, 0) < rev(1, 1, 0));
    assert!(rev(1, 1, 0) < rev(1, 2, 0));
}

#[test]
fn tetris_v10_and_world_rev1_share_a_title_key() {
    let a = parse_tags("Tetris (Japan) (En)");
    let b = parse_tags("Tetris (World) (Rev 1)");
    assert_eq!(a.title_key, "tetris");
    assert_eq!(b.title_key, "tetris");
    assert_eq!(a.regions, vec![Region::Japan]);
    assert_eq!(b.regions, vec![Region::World]);
    assert_eq!(parse_tags("Tetris (JUE) (V1.0) [!]").title_key, "tetris");
    assert_eq!(
        parse_tags("Tetris (JUE) (V1.0) [!]").regions,
        vec![Region::Japan, Region::Usa, Region::Europe]
    );
    assert_eq!(
        parse_tags("Tetris (USA, Europe) (En,Fr,De)").title_key,
        "tetris"
    );
}

#[test]
fn disc_number_is_kept_in_the_key() {
    let a = parse_tags("Final Fantasy VII (USA) (Disc 1)");
    let b = parse_tags("Final Fantasy VII (USA) (Disc 2)");
    assert_ne!(a.title_key, b.title_key);
    assert_eq!(a.disc, Some(1));
    assert_eq!(b.disc, Some(2));
    assert_eq!(parse_tags("Final Fantasy VII (USA)").disc, None);
    assert_eq!(
        parse_tags("Panzer Dragoon Saga (Europe) (Disc 3 of 4)").disc,
        Some(3)
    );
}

#[test]
fn unknown_groups_stay_in_the_key() {
    let a = parse_tags("Hippo Teeth (Sporty Time & Fun)");
    let b = parse_tags("Hippo Teeth (Mini Time & Fun)");
    assert_ne!(a.title_key, b.title_key);
    assert_ne!(a.title_key, parse_tags("Hippo Teeth").title_key);
    assert_eq!(a.unknown_groups, vec!["Sporty Time & Fun".to_string()]);
    assert!(parse_tags("Tetris (USA)").unknown_groups.is_empty());
}

#[test]
fn known_groups_are_stripped() {
    let names = [
        "Pokemon Pinball (USA) (SGB Enhanced)",
        "Pokemon Pinball (USA) (GB Compatible)",
        "Pokemon Pinball (USA) (Rumble Version)",
        "Pokemon Pinball (USA) (Virtual Console)",
        "Pokemon Pinball (USA) (1999-04-01)",
        "Pokemon Pinball (USA) (1999)",
        "Pokemon Pinball (USA) (Beta)",
        "Pokemon Pinball (USA) (Beta 2)",
        "Pokemon Pinball (Japan) (Proto 2)",
        "Pokemon Pinball (USA) (Demo)",
        "Pokemon Pinball (USA) (Sample)",
        "Pokemon Pinball (USA) (Kiosk)",
    ];
    for name in names {
        let tags = parse_tags(name);
        assert_eq!(tags.title_key, "pokemon pinball", "{name}");
        assert!(tags.unknown_groups.is_empty(), "{name}");
    }
    assert_eq!(parse_tags("X (USA) (Beta 2)").class, ReleaseClass::Beta);
    assert_eq!(parse_tags("X (Japan) (Proto 2)").class, ReleaseClass::Proto);
    assert_eq!(parse_tags("X (USA) (Demo)").class, ReleaseClass::Demo);
    assert_eq!(parse_tags("X (USA) (Kiosk)").class, ReleaseClass::Demo);
    assert_eq!(parse_tags("X (USA) (Sample)").class, ReleaseClass::Sample);
    assert_eq!(parse_tags("X (USA)").class, ReleaseClass::Retail);
}

#[test]
fn goodtools_modification_flags_set_the_class_and_stay_in_the_key() {
    let clean = parse_tags("Super Mario Land (W) [!]");
    assert_eq!(clean.modification, ModClass::Original);
    let cases = [
        ("Super Mario Land (W) [h1]", ModClass::Hack),
        ("Super Mario Land (W) [t1]", ModClass::Trainer),
        ("Super Mario Land (W) [T+Eng]", ModClass::Translation),
        (
            "Super Mario Land (W) [T-Spa1.0_Author]",
            ModClass::Translation,
        ),
        ("Super Mario Land (W) [f1]", ModClass::Fixed),
        ("Super Mario Land (W) [p1]", ModClass::Pirate),
        ("Super Mario Land (World) (Hack)", ModClass::Hack),
        ("Super Mario Land (World) (Unl)", ModClass::Unlicensed),
        ("Super Mario Land (World) (Pirate)", ModClass::Pirate),
    ];
    for (name, class) in cases {
        let tags = parse_tags(name);
        assert_eq!(tags.modification, class, "{name}");
        assert_ne!(tags.title_key, clean.title_key, "{name}");
        assert!(tags.unknown_groups.is_empty(), "{name}");
    }
    assert_ne!(
        parse_tags("Super Mario Land (W) [h1]").title_key,
        parse_tags("Super Mario Land (W) [h2]").title_key
    );
}

#[test]
fn dump_flags() {
    let base = parse_tags("Tetris (W)").title_key;
    let v = parse_tags("Tetris (W) [!]");
    assert!(v.flags.verified && !v.flags.bad);
    let b = parse_tags("Tetris (W) [b1]");
    assert!(b.flags.bad && !b.flags.verified);
    assert!(parse_tags("Tetris (W) [b]").flags.bad);
    assert!(parse_tags("Tetris (W) [o]").flags.overdump);
    assert!(parse_tags("Tetris (W) [o2]").flags.overdump);
    assert!(parse_tags("Tetris (W) [a]").flags.alternate);
    assert!(parse_tags("Tetris (W) [a1]").flags.alternate);
    for name in [
        "Tetris (W) [!]",
        "Tetris (W) [b1]",
        "Tetris (W) [o]",
        "Tetris (W) [a1]",
    ] {
        assert_eq!(parse_tags(name).title_key, base, "{name}");
        assert_eq!(parse_tags(name).modification, ModClass::Original, "{name}");
    }
    assert_eq!(parse_tags("Tetris (W)").flags, Default::default());
}

#[test]
fn release_classes_order() {
    use ReleaseClass::*;
    let mut v = vec![Sample, Demo, Retail, Proto, Beta];
    v.sort();
    assert_eq!(v, vec![Retail, Beta, Proto, Demo, Sample]);
}

#[test]
fn translated_en_is_its_own_class() {
    let a = parse_tags("Heracles no Eikou (Japan)");
    let b = parse_tags("Heracles no Eikou (Japan) (Translated En)");
    assert_ne!(a.title_key, b.title_key);
    assert_eq!(b.modification, ModClass::Translation);
    assert_eq!(a.modification, ModClass::Original);
}

#[test]
fn modification_tag_detector() {
    for name in [
        "Chrono Trigger (U) [T+Spa]",
        "Chrono Trigger (J) [T-Eng]",
        "Super Mario World (U) [h1]",
        "Tales of Destiny II Undub",
        "Final Fantasy VII (USA) (Patched)",
        "Mother 3 Traducido",
        "Mother 3 (Japan) (Translated En)",
        "Super Mario Bros Hack",
    ] {
        assert!(has_modification_tag(name), "{name}");
    }
    for name in [
        "Tetris (USA)",
        "Tetris (JUE) (V1.1) [!]",
        "Tetris (W) [b1]",
        "Heracles no Eikou (Japan)",
        "Hackers (USA)",
    ] {
        assert!(!has_modification_tag(name), "{name}");
    }
}

#[test]
fn title_key_folds_like_compare_key() {
    assert_eq!(
        parse_tags("Legend of Zelda, The - A Link to the Past (USA)").title_key,
        coverly_core::identify::names::compare_key("The Legend of Zelda - A Link to the Past")
    );
    assert_eq!(
        parse_tags("Pokémon Edición Roja (Spain)").title_key,
        "pokemon edicion roja"
    );
}

#[test]
fn modification_tag_detector_reads_decomposed_file_names() {
    // macOS often hands back file names in NFD: "ó" as "o" + U+0301.
    assert!(has_modification_tag("Mother 3 Traduccio\u{301}n"));
    assert!(has_modification_tag("Mother 3 Traducción"));
}
