//! The tag part of a copy's display facts (spec D2, §10).
//!
//! Display only: the keeper rules keep reading `UnitFacts.tags`. Everything
//! here goes through the existing `identify::names` parsers; there is no new
//! name parsing.

use crate::identify::names::{languages, parse_tags, DumpFlags, ModClass, ReleaseClass};

use super::{MemberFacts, TagSource};

/// The tag part of [`MemberFacts`] (spec D2, §10).
/// - regions, revision, class, languages, other: from the catalogue name when
///   level 2 knows the game, else from the file stem (`tags_from` says which);
/// - flags and modification: the union of both names, each with its own source
///   (catalogue first when both carry one).
pub fn tag_facts(catalogue: Option<&str>, stem: &str) -> MemberFacts {
    let file = parse_tags(stem);
    let cat = catalogue.map(|c| (c, parse_tags(c)));
    let (shown_name, shown, from) = match &cat {
        Some((c, t)) => (*c, t, TagSource::Catalogue),
        None => (stem, &file, TagSource::FileName),
    };

    let regions = shown.regions.clone();
    let revision = shown.revision_text.clone();
    let class = (shown.class != ReleaseClass::Retail).then_some(shown.class);
    let langs = languages(shown_name);
    // Unknown groups minus language lists ("En,Fr,De" is already in `languages`).
    // Modification groups never land in `unknown_groups` (`parse_tags` keeps them
    // apart), so a hack or translation tag is never repeated here.
    let other: Vec<String> = shown
        .unknown_groups
        .iter()
        .filter(|g| languages(&format!("({g})")).is_empty())
        .cloned()
        .collect();
    let any_tag = !(regions.is_empty()
        && revision.is_none()
        && class.is_none()
        && langs.is_empty()
        && other.is_empty());

    // Union of flags; the source is the first name that sets any flag.
    let cat_tags = cat.as_ref().map(|(_, t)| t);
    let cat_flags = cat_tags.map(|t| t.flags).unwrap_or_default();
    let flags = DumpFlags {
        verified: cat_flags.verified || file.flags.verified,
        bad: cat_flags.bad || file.flags.bad,
        overdump: cat_flags.overdump || file.flags.overdump,
        alternate: cat_flags.alternate || file.flags.alternate,
    };
    let flags_from = if !cat_flags.is_empty() {
        Some(TagSource::Catalogue)
    } else if !file.flags.is_empty() {
        Some(TagSource::FileName)
    } else {
        None
    };

    let cat_mod = cat_tags
        .map(|t| t.modification)
        .filter(|m| *m != ModClass::Original);
    let file_mod = Some(file.modification).filter(|m| *m != ModClass::Original);
    let (modification, modification_from) = match (cat_mod, file_mod) {
        (Some(m), _) => (Some(m), Some(TagSource::Catalogue)),
        (None, Some(m)) => (Some(m), Some(TagSource::FileName)),
        _ => (None, None),
    };

    MemberFacts {
        tags_from: any_tag.then_some(from),
        regions,
        languages: langs,
        revision,
        class,
        flags,
        flags_from,
        modification,
        modification_from,
        other,
        ..MemberFacts::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identify::names::Region;

    #[test]
    fn catalogue_world_rev1() {
        let f = tag_facts(Some("Tetris (World) (Rev 1)"), "Tetris (W) [b1]");
        assert_eq!(f.regions, vec![Region::World]);
        assert_eq!(f.revision.as_deref(), Some("Rev 1"));
        assert_eq!(f.tags_from, Some(TagSource::Catalogue));
        assert!(f.flags.bad);
        assert_eq!(f.flags_from, Some(TagSource::FileName));
        assert_eq!(f.class, None);
    }

    #[test]
    fn file_name_only_japan_verified() {
        let f = tag_facts(None, "Tetris (J) [!]");
        assert_eq!(f.regions, vec![Region::Japan]);
        assert!(f.flags.verified);
        assert_eq!(f.tags_from, Some(TagSource::FileName));
        assert_eq!(f.flags_from, Some(TagSource::FileName));
    }

    #[test]
    fn languages_in_order() {
        let f = tag_facts(Some("Zelda (Europe) (En,Fr,De)"), "x");
        assert_eq!(f.languages, vec!["en", "fr", "de"]);
        assert!(f.other.is_empty());
    }

    #[test]
    fn hack_and_translation_from_file_name() {
        let f = tag_facts(Some("Pokemon Red (USA)"), "Pokemon Red (U) [T+Spa]");
        assert_eq!(f.modification, Some(ModClass::Translation));
        assert_eq!(f.modification_from, Some(TagSource::FileName));
        assert!(!f.other.iter().any(|g| g.contains("T+Spa")));

        let f = tag_facts(None, "Sonic (U) [h1]");
        assert_eq!(f.modification, Some(ModClass::Hack));
        assert!(!f.other.iter().any(|g| g == "h1"));
    }

    #[test]
    fn unknown_region_word_is_other() {
        let f = tag_facts(Some("Tetris (Russia)"), "x");
        assert!(f.regions.is_empty());
        assert_eq!(f.other, vec!["Russia"]);
    }

    #[test]
    fn beta_class() {
        let f = tag_facts(Some("Star Fox 2 (Japan) (Beta)"), "x");
        assert_eq!(f.class, Some(ReleaseClass::Beta));
    }

    #[test]
    fn flags_union_with_catalogue_source_first() {
        let f = tag_facts(Some("Tetris (World) [!]"), "Tetris (W) [b1]");
        assert!(f.flags.verified);
        assert!(f.flags.bad);
        assert_eq!(f.flags_from, Some(TagSource::Catalogue));
    }

    #[test]
    fn catalogue_modification_wins_over_file_name() {
        let f = tag_facts(Some("Sonic (USA) (Hack)"), "Sonic (U) [T+Spa]");
        assert_eq!(f.modification, Some(ModClass::Hack));
        assert_eq!(f.modification_from, Some(TagSource::Catalogue));
        assert!(f.other.is_empty());
    }

    #[test]
    fn catalogue_hides_file_name_regions() {
        let f = tag_facts(Some("Tetris"), "Tetris (J) (Rev 2)");
        assert!(f.regions.is_empty());
        assert_eq!(f.revision, None);
        assert_eq!(f.tags_from, None);
    }

    #[test]
    fn flags_only_has_no_tag_source() {
        let f = tag_facts(None, "tetris [!]");
        assert!(f.flags.verified);
        assert_eq!(f.flags_from, Some(TagSource::FileName));
        assert_eq!(f.tags_from, None);
    }

    #[test]
    fn nothing_known_is_empty() {
        assert_eq!(tag_facts(None, "tetris"), MemberFacts::default());
    }
}
