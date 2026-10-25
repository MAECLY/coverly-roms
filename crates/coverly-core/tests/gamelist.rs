//! The gamelist.xml merge (D31): only the art tags change, nothing else.

use coverly_core::gamelist::{merge, GameArt, MergeError, MAX_GAMELIST_BYTES};
use coverly_core::profiles::Media;

fn cover(rom: &str, art: &str) -> GameArt {
    GameArt {
        rom: rom.into(),
        name: rom
            .trim_start_matches("./")
            .rsplit_once('.')
            .map_or(rom, |(s, _)| s)
            .into(),
        art: vec![(Media::Cover, art.into())],
    }
}

fn yes(_: &str) -> bool {
    true
}

fn no(_: &str) -> bool {
    false
}

#[test]
fn creates_a_gamelist_when_none_exists() {
    let out = merge(
        None,
        &[cover("./Zelda.sfc", "./images/Zelda-image.png")],
        false,
        &no,
    )
    .unwrap()
    .unwrap();
    assert!(
        out.starts_with("<?xml version=\"1.0\"?>\n<gameList>"),
        "{out}"
    );
    assert!(out.contains("<path>./Zelda.sfc</path>"));
    assert!(out.contains("<name>Zelda</name>"));
    assert!(out.contains("<thumbnail>./images/Zelda-image.png</thumbnail>"));
    assert!(out.contains("<image>./images/Zelda-image.png</image>"));
    // The result parses and merging again changes nothing.
    assert_eq!(
        merge(
            Some(&out),
            &[cover("./Zelda.sfc", "./images/Zelda-image.png")],
            false,
            &yes
        )
        .unwrap(),
        None
    );
}

const EXISTING: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n\
<!-- scraped by Skraper -->\r\n\
<gameList>\r\n\
\t<provider><System>snes</System></provider>\r\n\
\t<game id=\"1234\" source=\"ScreenScraper.fr\">\r\n\
\t\t<path>./Tom &amp; Jerry.sfc</path>\r\n\
\t\t<name>Tom &amp; Jerry</name>\r\n\
\t\t<rating>0.7</rating>\r\n\
\t\t<marquee>./images/Tom &amp; Jerry-marquee.png</marquee>\r\n\
\t</game>\r\n\
\t<game>\r\n\
\t\t<path>./Other.sfc</path>\r\n\
\t\t<image>./images/Other-image.png</image>\r\n\
\t</game>\r\n\
</gameList>\r\n";

#[test]
fn adds_tags_to_an_existing_game_and_changes_nothing_else() {
    let out = merge(
        Some(EXISTING),
        &[cover("./Tom & Jerry.sfc", "./images/Tom & Jerry-image.png")],
        false,
        &no,
    )
    .unwrap()
    .unwrap();
    // Inserted with the file's own line ending and indentation.
    let added = "\r\n\t\t<thumbnail>./images/Tom &amp; Jerry-image.png</thumbnail>\r\n\t\t<image>./images/Tom &amp; Jerry-image.png</image>";
    assert!(out.contains(added), "{out}");
    assert_eq!(
        out.replacen(added, "", 1),
        EXISTING,
        "every other byte is kept"
    );
}

#[test]
fn a_usable_image_is_kept_and_an_unusable_one_is_replaced() {
    let game = || cover("./Other.sfc", "./images/Other-cover.png");
    let kept = merge(Some(EXISTING), &[game()], false, &yes)
        .unwrap()
        .unwrap();
    assert!(kept.contains("<image>./images/Other-image.png</image>"));
    assert!(kept.contains("<thumbnail>./images/Other-cover.png</thumbnail>"));
    let replaced = merge(Some(EXISTING), &[game()], false, &no)
        .unwrap()
        .unwrap();
    assert!(replaced.contains("<image>./images/Other-cover.png</image>"));
    assert!(!replaced.contains("Other-image.png"));
}

#[test]
fn replace_mode_replaces_set_tags_but_never_a_usable_image_with_a_cover() {
    let existing = "<gameList><game><path>./A.sfc</path><thumbnail>./images/old.png</thumbnail><image>./images/shot.png</image></game></gameList>";
    let out = merge(
        Some(existing),
        &[cover("./A.sfc", "./images/A-image.png")],
        true,
        &yes,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        out,
        "<gameList><game><path>./A.sfc</path><thumbnail>./images/A-image.png</thumbnail><image>./images/shot.png</image></game></gameList>"
    );
    let screenshot = GameArt {
        rom: "./A.sfc".into(),
        name: "A".into(),
        art: vec![
            (Media::Screenshot, "./images/A-screenshot.png".into()),
            (Media::Title, "./images/A-titleshot.png".into()),
        ],
    };
    let out = merge(Some(existing), &[screenshot], true, &yes)
        .unwrap()
        .unwrap();
    assert!(out.contains("<image>./images/A-screenshot.png</image>"));
    assert!(out.contains("<titleshot>./images/A-titleshot.png</titleshot>"));
    assert!(out.contains("<thumbnail>./images/old.png</thumbnail>"));
}

#[test]
fn nothing_to_change_is_none() {
    let existing = "<gameList><game><path>./A.sfc</path><thumbnail>./images/A.png</thumbnail><image>./images/A.png</image></game></gameList>";
    assert_eq!(
        merge(
            Some(existing),
            &[cover("./A.sfc", "./images/A.png")],
            false,
            &yes
        )
        .unwrap(),
        None
    );
    assert_eq!(merge(Some(existing), &[], false, &yes).unwrap(), None);
    assert_eq!(merge(None, &[], false, &yes).unwrap(), None);
}

#[test]
fn hostile_and_broken_files_are_refused() {
    let bomb = "<?xml version=\"1.0\"?><!DOCTYPE gameList [<!ENTITY a \"aaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;\">]><gameList>&b;</gameList>";
    let g = [cover("./A.sfc", "./images/A.png")];
    assert_eq!(merge(Some(bomb), &g, false, &no), Err(MergeError::Doctype));
    assert!(matches!(
        merge(Some("<gameList><game></gameList>"), &g, false, &no),
        Err(MergeError::Malformed(_))
    ));
    assert!(matches!(
        merge(Some("<gameList>"), &g, false, &no),
        Err(MergeError::Malformed(_))
    ));
    assert_eq!(
        merge(Some("<foo/>"), &g, false, &no),
        Err(MergeError::NoRoot)
    );
    assert_eq!(merge(Some(""), &g, false, &no), Err(MergeError::NoRoot));
    assert_eq!(
        merge(Some("<gameList/><gameList/>"), &g, false, &no)
            .map(|_| ())
            .unwrap_err(),
        MergeError::Malformed("two root elements".into())
    );
    let deep = format!(
        "<gameList>{}{}</gameList>",
        "<a>".repeat(100),
        "</a>".repeat(100)
    );
    assert!(matches!(
        merge(Some(&deep), &g, false, &no),
        Err(MergeError::Malformed(_))
    ));
    let big = " ".repeat(MAX_GAMELIST_BYTES + 1);
    assert_eq!(merge(Some(&big), &g, false, &no), Err(MergeError::TooLarge));
}

#[test]
fn paths_match_with_entities_without_dot_slash_and_ignoring_case() {
    let existing = "<gameList><game><path>Zelda (USA).SFC</path></game><game><path>./B&#38;C.sfc</path></game></gameList>";
    let out = merge(
        Some(existing),
        &[
            cover("./zelda (usa).sfc", "./images/z.png"),
            cover("./B&C.sfc", "./images/bc.png"),
        ],
        false,
        &no,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        out.matches("<game>").count(),
        2,
        "both matched, none appended: {out}"
    );
    assert!(out.contains("<thumbnail>./images/z.png</thumbnail>"));
    assert!(out.contains("<thumbnail>./images/bc.png</thumbnail>"));
}

#[test]
fn self_closing_root_and_empty_tags_are_expanded() {
    let out = merge(
        Some("<gameList/>"),
        &[cover("./A.sfc", "./images/A.png")],
        false,
        &no,
    )
    .unwrap()
    .unwrap();
    assert!(
        out.starts_with("<gameList>") && out.trim_end().ends_with("</gameList>"),
        "{out}"
    );
    let out = merge(
        Some(
            "<gameList><game><path>./A.sfc</path><image/><thumbnail></thumbnail></game></gameList>",
        ),
        &[cover("./A.sfc", "./images/A.png")],
        false,
        &yes,
    )
    .unwrap()
    .unwrap();
    assert!(
        out.contains("<image>./images/A.png</image>")
            && out.contains("<thumbnail>./images/A.png</thumbnail>"),
        "{out}"
    );
}

#[test]
fn only_the_first_of_duplicate_games_is_updated() {
    let existing =
        "<gameList><game><path>./A.sfc</path></game><game><path>./A.sfc</path></game></gameList>";
    let out = merge(
        Some(existing),
        &[cover("./A.sfc", "./images/A.png")],
        false,
        &no,
    )
    .unwrap()
    .unwrap();
    assert_eq!(out.matches("<thumbnail>").count(), 1);
    let first = out.find("<thumbnail>").unwrap();
    let second_game = out.rfind("<game>").unwrap();
    assert!(first < second_game);
}

#[test]
fn values_are_escaped_and_hostile_names_stay_text() {
    let g = GameArt {
        rom: "./<img src=x onerror=alert(1)> & \"q\".sfc".into(),
        name: "<img src=x onerror=alert(1)> & \"q\"".into(),
        art: vec![(Media::Cover, "./images/<img>-image.png".into())],
    };
    let out = merge(None, std::slice::from_ref(&g), false, &no)
        .unwrap()
        .unwrap();
    assert!(!out.contains("<img"), "{out}");
    assert!(out.contains("&lt;img src=x onerror=alert(1)&gt; &amp; &quot;q&quot;"));
    assert_eq!(
        merge(Some(&out), &[g], false, &yes).unwrap(),
        None,
        "the escaped path matches again"
    );
}

#[test]
fn a_bom_and_comments_inside_games_are_kept() {
    let existing = "\u{feff}<gameList><game><!-- note --><path>./A.sfc</path></game></gameList>";
    let out = merge(
        Some(existing),
        &[cover("./A.sfc", "./images/A.png")],
        false,
        &no,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        out,
        "\u{feff}<gameList><game><!-- note --><path>./A.sfc</path><thumbnail>./images/A.png</thumbnail><image>./images/A.png</image></game></gameList>"
    );
}

#[test]
fn roms_differing_only_by_case_never_edit_the_same_game_twice() {
    let existing = "<gameList><game><path>./A.sfc</path><thumbnail></thumbnail></game></gameList>";
    let out = merge(
        Some(existing),
        &[
            cover("./a.sfc", "./images/a-é.png"),
            cover("./A.sfc", "./images/A-image.png"),
        ],
        false,
        &no,
    )
    .unwrap()
    .unwrap();
    // The exact spelling claims the existing game; the other ROM gets its own.
    assert!(
        out.starts_with(
            "<gameList><game><path>./A.sfc</path><thumbnail>./images/A-image.png</thumbnail>"
        ),
        "{out}"
    );
    assert!(
        out.contains("<path>./a.sfc</path>")
            && out.contains("<thumbnail>./images/a-é.png</thumbnail>"),
        "{out}"
    );
    assert_eq!(out.matches("<game>").count(), 2, "{out}");
    // The same ROM listed twice is merged once.
    let twice = [
        cover("./B.sfc", "./images/B.png"),
        cover("./B.sfc", "./images/B.png"),
    ];
    let out = merge(Some(existing), &twice, false, &no).unwrap().unwrap();
    assert_eq!(out.matches("<path>./B.sfc</path>").count(), 1, "{out}");
}

#[test]
fn control_characters_in_a_rom_path_do_not_duplicate_its_game() {
    let g = [cover("./Bad\u{7}Name.sfc", "./images/Bad\u{7}Name.png")];
    let out = merge(None, &g, false, &no).unwrap().unwrap();
    assert!(!out.contains('\u{7}'));
    assert_eq!(merge(Some(&out), &g, false, &yes).unwrap(), None, "{out}");
}

#[test]
fn a_bom_never_shifts_the_edits() {
    let g = |rom: &str| cover(rom, &format!("./images/{}.png", &rom[2..3]));
    // Matched game, compact file.
    let out = merge(
        Some("\u{feff}<gameList><game><path>./A.sfc</path></game></gameList>"),
        &[g("./A.sfc")],
        false,
        &no,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        out,
        "\u{feff}<gameList><game><path>./A.sfc</path><thumbnail>./images/A.png</thumbnail><image>./images/A.png</image></game></gameList>"
    );
    // Declaration, comment, one game matched and one appended.
    let existing = "\u{feff}<?xml version=\"1.0\"?>\n<!-- c -->\n<gameList>\n\t<game>\n\t\t<path>./A.sfc</path>\n\t</game>\n</gameList>\n";
    let out = merge(Some(existing), &[g("./A.sfc"), g("./B.sfc")], false, &no)
        .unwrap()
        .unwrap();
    assert_eq!(
        out,
        "\u{feff}<?xml version=\"1.0\"?>\n<!-- c -->\n<gameList>\n\t<game>\n\t\t<path>./A.sfc</path>\
         \n\t\t<thumbnail>./images/A.png</thumbnail>\n\t\t<image>./images/A.png</image>\n\t</game>\
         \n\t<game>\n\t\t<path>./B.sfc</path>\n\t\t<name>B</name>\
         \n\t\t<thumbnail>./images/B.png</thumbnail>\n\t\t<image>./images/B.png</image>\n\t</game>\
         \n</gameList>\n"
    );
    // Whitespace before a self-closing root.
    let out = merge(Some("\u{feff}  <gameList/>"), &[g("./A.sfc")], false, &no)
        .unwrap()
        .unwrap();
    assert!(out.starts_with("\u{feff}  <gameList>\n\t<game>"), "{out:?}");
    assert!(out.ends_with("</game>\n</gameList>"), "{out:?}");
}

#[test]
fn a_bom_and_four_byte_characters_next_to_tags_never_panic() {
    let existing =
        "\u{feff}<gameList><game><path>./A😀</path><desc>😀</desc></game><game>😀</game></gameList>";
    let out = merge(
        Some(existing),
        &[cover("./A😀", "./images/A.png")],
        false,
        &no,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        out,
        "\u{feff}<gameList><game><path>./A😀</path><desc>😀</desc><thumbnail>./images/A.png</thumbnail><image>./images/A.png</image></game><game>😀</game></gameList>"
    );
    for hostile in [
        "\u{feff}😀<gameList/>",
        "\u{feff}<gameList>😀<game>😀<path>😀</path>😀</game>😀</gameList>",
        "\u{feff}<!--😀--><gameList><game><path>./😀</path><image>😀</image></game></gameList>",
        "\u{feff}<?xml version=\"1.0\"?><gameList><game><path>😀😀</path></game></gameList>😀",
    ] {
        for rom in ["./😀", "./😀😀", "./A.sfc"] {
            // Merged or refused, never a panic; a merged file parses again.
            if let Ok(Some(out)) = merge(Some(hostile), &[cover(rom, "./i/😀.png")], false, &no) {
                assert!(
                    merge(Some(&out), &[cover(rom, "./i/😀.png")], false, &yes).is_ok(),
                    "{out:?}"
                );
            }
        }
    }
}

#[test]
fn text_outside_the_root_is_refused() {
    let g = [cover("./A.sfc", "./images/A.png")];
    for bad in [
        "garbage<gameList/>",
        "<gameList/>junk",
        "<gameList/>&amp;",
        "<![CDATA[x]]><gameList/>",
    ] {
        assert!(
            matches!(
                merge(Some(bad), &g, false, &no),
                Err(MergeError::Malformed(_))
            ),
            "{bad}"
        );
    }
    assert!(merge(Some(" \r\n\t<gameList/>\n "), &g, false, &no).is_ok());
}

#[test]
fn remove_games_takes_the_entry_and_its_line_only() {
    let xml = "<?xml version=\"1.0\"?>\n<!-- mine -->\n<gameList>\n\t<game>\n\t\t<path>./Tetris.gb</path>\n\t\t<name>Tetris</name>\n\t</game>\n\t<game>\n\t\t<path>./old/Tetris (1).gb</path>\n\t</game>\n\t<folder><path>./old</path></folder>\n</gameList>\n";
    let out = coverly_core::gamelist::remove_games(xml, &["./old/Tetris (1).gb".to_string()])
        .unwrap()
        .unwrap();
    assert_eq!(
        out,
        "<?xml version=\"1.0\"?>\n<!-- mine -->\n<gameList>\n\t<game>\n\t\t<path>./Tetris.gb</path>\n\t\t<name>Tetris</name>\n\t</game>\n\t<folder><path>./old</path></folder>\n</gameList>\n"
    );
    // Matched ignoring case, as ES does; CRLF lines go whole.
    let crlf = xml.replace('\n', "\r\n");
    let out = coverly_core::gamelist::remove_games(&crlf, &["./TETRIS.GB".to_string()])
        .unwrap()
        .unwrap();
    assert!(!out.contains("<name>Tetris</name>"), "{out}");
    assert!(
        out.contains("<gameList>\r\n\t<game>\r\n\t\t<path>./old/"),
        "{out}"
    );
    // Nothing matched: no change.
    assert_eq!(
        coverly_core::gamelist::remove_games(xml, &["./Zelda.gb".to_string()]).unwrap(),
        None
    );
    // Two entries on one line: only the bytes of the one removed.
    let inline =
        "<gameList><game><path>./a.gb</path></game><game><path>./b.gb</path></game></gameList>";
    assert_eq!(
        coverly_core::gamelist::remove_games(inline, &["./a.gb".to_string()])
            .unwrap()
            .unwrap(),
        "<gameList><game><path>./b.gb</path></game></gameList>"
    );
}
