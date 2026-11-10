//! Links a system's demos and orphan add-ons to their game (part C D6,
//! D8, plan P-2). Pure: it reads only the names the scan already has and
//! never looks across systems.
//!
//! A *game* here is a version group (part A D15): the revisions of one
//! title in one region are one candidate, so `X (USA)` and
//! `X (USA) (Rev 1)` never make a link ambiguous. The link points at one
//! member of that group (the first by name bytes); the webview folds the
//! group into its game.

use std::collections::HashMap;
use std::path::Component;

use super::RomEntry;
use crate::game_folder::FixedFolder;
use crate::identify::names::{parse_tags, Region};
use crate::organize::library_bases::{demo_free_key, has_demo_tag};

/// Sets `is_demo`, `demo_of` and `orphan_of` on one system's entries.
/// Indexes point into the same slice. Never across systems.
///
/// - A demo is a non-orphan entry whose name has a demo tag; it links to
///   the one game whose title key is its [`demo_free_key`]. When several
///   games share that key (several regions), it links only to the one
///   whose region tags are exactly the demo's, else to none (spec §11).
/// - An orphan update or DLC links by the title key of its game folder
///   name (the folder holding its `Updates/` or `DLC/`; its own stem when
///   there is none) to exactly one game; zero or several give none.
/// - Demos and orphans are never link targets.
pub(crate) fn link_titles(roms: &mut [RomEntry]) {
    for rom in roms.iter_mut() {
        rom.is_demo = rom.role.is_none() && has_demo_tag(rom.title_stem());
        rom.demo_of = None;
        rom.orphan_of = None;
    }
    // Title key → the full games (non-demo, non-orphan) that carry it.
    let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, rom) in roms.iter().enumerate() {
        if rom.role.is_some() || rom.is_demo {
            continue;
        }
        let key = parse_tags(rom.title_stem()).title_key;
        if !key.is_empty() {
            by_key.entry(key).or_default().push(i);
        }
    }
    let mut links: Vec<(usize, Link)> = Vec::new();
    for (i, rom) in roms.iter().enumerate() {
        if rom.is_demo {
            let cands = candidates(&by_key, &demo_free_key(rom.title_stem()));
            if let Some(t) = demo_target(roms, cands, &parse_tags(rom.title_stem()).regions) {
                links.push((i, Link::Demo(t)));
            }
        } else if rom.role.is_some() {
            let key = parse_tags(game_folder_name(rom)).title_key;
            if let Some(t) = sole_game(roms, candidates(&by_key, &key)) {
                links.push((i, Link::Orphan(t)));
            }
        }
    }
    for (i, link) in links {
        match link {
            Link::Demo(t) => roms[i].demo_of = Some(t),
            Link::Orphan(t) => roms[i].orphan_of = Some(t),
        }
    }
}

enum Link {
    Demo(usize),
    Orphan(usize),
}

fn candidates<'a>(by_key: &'a HashMap<String, Vec<usize>>, key: &str) -> &'a [usize] {
    if key.is_empty() {
        return &[];
    }
    by_key.get(key).map(Vec::as_slice).unwrap_or(&[])
}

/// The game of a demo among `cands`: the only game, else the only game
/// tagged with exactly the demo's (non-empty) regions.
fn demo_target(roms: &[RomEntry], cands: &[usize], regions: &[Region]) -> Option<usize> {
    if let Some(t) = sole_game(roms, cands) {
        return Some(t);
    }
    if regions.is_empty() {
        return None;
    }
    let same: Vec<usize> = cands
        .iter()
        .copied()
        .filter(|&c| parse_tags(roms[c].title_stem()).regions == regions)
        .collect();
    sole_game(roms, &same)
}

/// The member to link to when every candidate is one game (one version
/// group); `None` for no candidate or several games.
fn sole_game(roms: &[RomEntry], cands: &[usize]) -> Option<usize> {
    let first = *cands.first()?;
    let group = |i: usize| roms[i].version_group.as_deref();
    let one_game = match group(first) {
        Some(g) => cands.iter().all(|&c| group(c) == Some(g)),
        None => cands.len() == 1,
    };
    if !one_game {
        return None;
    }
    cands.iter().copied().min_by(|&a, &b| {
        (roms[a].title_stem().as_bytes(), &roms[a].path)
            .cmp(&(roms[b].title_stem().as_bytes(), &roms[b].path))
    })
}

/// The name of the game folder an orphan sits in: the folder holding its
/// nearest `Updates/` or `DLC/` (compared by component, any case); its own
/// stem when it is not inside one with a parent.
fn game_folder_name(rom: &RomEntry) -> &str {
    let parts: Vec<&str> = rom
        .rel_dir
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    let fixed = parts
        .iter()
        .rposition(|p| FixedFolder::from_dir_name(p).is_some_and(|f| f.child_role().is_some()));
    match fixed {
        Some(at) if at > 0 => parts[at - 1],
        _ => rom.title_stem(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use super::*;
    use crate::game_folder::ChildRole;
    use crate::scan::{set_version_group, ChildEntry};

    const SYSTEM: &str = "switch";

    /// A synthetic entry of the `switch` folder: `rel_dir` as components,
    /// mtime pinned, path built with `Path::join`.
    fn rom(stem: &str, rel_dir: &[&str], role: Option<ChildRole>) -> RomEntry {
        let rel: PathBuf = rel_dir.iter().collect();
        let path = PathBuf::from("ROMS")
            .join(SYSTEM)
            .join(&rel)
            .join(format!("{stem}.nsp"));
        let mut rom = RomEntry {
            path,
            rel_dir: rel,
            stem: stem.to_string(),
            ext: "nsp".to_string(),
            size: 1,
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            companions: Vec::new(),
            is_dir: false,
            children: Vec::new(),
            role,
            version_group: None,
            revision: None,
            fixed_counts: None,
            is_demo: false,
            demo_of: None,
            orphan_of: None,
        };
        if role.is_none() {
            set_version_group(&mut rom, SYSTEM);
        }
        rom
    }

    fn linked(mut roms: Vec<RomEntry>) -> Vec<RomEntry> {
        link_titles(&mut roms);
        roms
    }

    #[test]
    fn a_demo_links_to_its_full_game() {
        let roms = linked(vec![
            rom("Lantern Harbor (Europe)", &[], None),
            rom("Lantern Harbor (Europe) (Demo)", &[], None),
        ]);
        assert!(!roms[0].is_demo);
        assert_eq!(roms[0].demo_of, None);
        assert!(roms[1].is_demo);
        assert_eq!(roms[1].demo_of, Some(0));
    }

    #[test]
    fn trial_and_kiosk_demo_tags_are_demos() {
        let roms = linked(vec![
            rom("Neon Rhythm (Trial)", &[], None),
            rom("Neon Rhythm (Kiosk Demo)", &[], None),
            rom("Neon Rhythm", &[], None),
        ]);
        assert!(roms[0].is_demo);
        assert!(roms[1].is_demo);
        assert!(!roms[2].is_demo);
        assert_eq!(roms[0].demo_of, Some(2));
        assert_eq!(roms[1].demo_of, Some(2));
        let roms = linked(vec![rom("Neon Rhythm (Kiosk)", &[], None)]);
        assert!(!roms[0].is_demo, "(Kiosk) alone is not a demo tag");
    }

    #[test]
    fn a_demo_without_its_game_has_no_link() {
        let roms = linked(vec![rom("Paper Skies (Demo)", &[], None)]);
        assert!(roms[0].is_demo);
        assert_eq!(roms[0].demo_of, None);
    }

    #[test]
    fn two_regions_pick_the_same_region_or_none() {
        let roms = linked(vec![
            rom("X (USA)", &[], None),
            rom("X (Japan)", &[], None),
            rom("X (Japan) (Demo)", &[], None),
        ]);
        assert_eq!(roms[2].demo_of, Some(1));

        let roms = linked(vec![
            rom("X (USA)", &[], None),
            rom("X (Japan)", &[], None),
            rom("X (Demo)", &[], None),
        ]);
        assert!(roms[2].is_demo);
        assert_eq!(roms[2].demo_of, None);
    }

    #[test]
    fn one_region_game_is_linked_whatever_the_demo_region() {
        let roms = linked(vec![
            rom("X (USA)", &[], None),
            rom("X (Europe) (Demo)", &[], None),
        ]);
        assert_eq!(roms[1].demo_of, Some(0));
    }

    #[test]
    fn revisions_of_one_game_are_one_candidate() {
        let roms = linked(vec![
            rom("X (USA)", &[], None),
            rom("X (USA) (Rev 1)", &[], None),
            rom("X (Demo)", &[], None),
        ]);
        let target = roms[2].demo_of.expect("one game, two revisions");
        assert!(target < 2);
        assert_eq!(roms[target].version_group, roms[0].version_group);
    }

    #[test]
    fn an_orphan_update_links_by_its_game_folder_name() {
        let mut base = rom("Tidewalker", &["Tidewalker"], None);
        base.children.push(ChildEntry {
            path: PathBuf::from("ROMS")
                .join(SYSTEM)
                .join("Tidewalker")
                .join("Updates")
                .join("Tidewalker (v1.0.2).nsp"),
            role: ChildRole::Update,
            size: 1,
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        });
        let roms = linked(vec![
            base,
            rom(
                "Tidewalker (v1.3.0)",
                &["Tidewalker Old", "Updates"],
                Some(ChildRole::Update),
            ),
            rom(
                "Tidewalker (v1.3.0)",
                &["Downloads", "Tidewalker", "Updates"],
                Some(ChildRole::Update),
            ),
        ]);
        assert_eq!(roms[1].orphan_of, None, "folder key `tidewalker old`");
        assert_eq!(roms[2].orphan_of, Some(0));
        assert!(!roms[1].is_demo && !roms[2].is_demo);
        assert_eq!(roms[0].orphan_of, None);
    }

    #[test]
    fn an_orphan_dlc_links_by_its_folder_and_not_its_own_name() {
        let roms = linked(vec![
            rom("Tidewalker (Europe)", &[], None),
            rom(
                "Coral Reef Pack",
                &["Tidewalker", "DLC"],
                Some(ChildRole::Dlc),
            ),
        ]);
        assert_eq!(roms[1].orphan_of, Some(0));
    }

    #[test]
    fn a_loose_orphan_links_by_its_own_stem() {
        let roms = linked(vec![
            rom("Tidewalker", &[], None),
            rom("Tidewalker", &["Updates"], Some(ChildRole::Update)),
        ]);
        assert_eq!(roms[1].orphan_of, Some(0));
    }

    #[test]
    fn two_candidate_games_link_nothing() {
        let roms = linked(vec![
            rom("Tidewalker (USA)", &[], None),
            rom("Tidewalker (Europe)", &[], None),
            rom(
                "Tidewalker (v1.3.0)",
                &["Tidewalker", "Updates"],
                Some(ChildRole::Update),
            ),
        ]);
        assert_eq!(roms[2].orphan_of, None);
    }

    #[test]
    fn a_demo_is_never_an_orphan_target() {
        let roms = linked(vec![
            rom("Paper Skies (Demo)", &[], None),
            rom("Pack", &["Paper Skies", "DLC"], Some(ChildRole::Dlc)),
        ]);
        assert_eq!(roms[1].orphan_of, None);
        assert!(!roms[1].is_demo);
    }

    #[test]
    fn an_orphan_is_never_a_demo_nor_a_demo_target() {
        let roms = linked(vec![
            rom("Glow (Demo)", &["Glow", "DLC"], Some(ChildRole::Dlc)),
            rom("Glow", &["Glow", "DLC"], Some(ChildRole::Dlc)),
            rom("Glow (Demo)", &[], None),
        ]);
        assert!(!roms[0].is_demo);
        assert_eq!(roms[0].demo_of, None);
        assert_eq!(roms[2].demo_of, None, "an orphan is not a full game");
    }

    #[test]
    fn a_rescan_clears_stale_links() {
        let mut roms = vec![rom("Paper Skies (Demo)", &[], None)];
        roms[0].demo_of = Some(7);
        roms[0].orphan_of = Some(7);
        link_titles(&mut roms);
        assert_eq!(roms[0].demo_of, None);
        assert_eq!(roms[0].orphan_of, None);
    }
}
