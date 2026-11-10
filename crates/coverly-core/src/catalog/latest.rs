//! The newest version of a game: the 3.1 seam and the in-library facts
//! (spec D5, D6, §10; plan P-3).
//!
//! «Newest» has one home. Every comparison goes through
//! [`crate::dedupe::keeper::version_order`] (hex for `xbox360`), after
//! stripping a leading `v`/`V`, so the panel, the filters and Duplicados
//! never disagree. Two versions that are equal by that order (`1.05` and
//! `1.5`), or a version that does not parse, make no version «the newest»:
//! nothing is claimed and no warning is shown.
//!
//! [`LatestVersions`] is the seam for an offline title list that knows the
//! newest version of a title (3.1). Part C ships only [`NoTitleList`], so
//! «newest available» falls back to the in-library comparison
//! ([`newer_elsewhere`]): an orphan update of the same game that is newer
//! than what the game's own folder holds.

use serde::Serialize;

use crate::dedupe::keeper::version_order;

/// Which offline title list answered [`LatestVersions::latest`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LatestSource {
    TitleDb,
    GameTdb,
}

/// The newest version a title list knows for a title.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LatestKnown {
    pub version: String,
    pub source: LatestSource,
}

/// The 3.1 seam (spec §10): an offline title list that knows the newest
/// version of a title. Implementations never touch the network.
pub trait LatestVersions: Send + Sync {
    fn latest(&self, system: &str, base_title_id: &str) -> Option<LatestKnown>;
}

/// The only implementation in part C: nothing is known.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoTitleList;

impl LatestVersions for NoTitleList {
    fn latest(&self, _system: &str, _base_title_id: &str) -> Option<LatestKnown> {
        None
    }
}

/// The version a name carries: `(v1.05)` or `[v131072]` anywhere (the last
/// one wins), or a bare `v1.2.0` as the last word. Pass a file stem, not a
/// file name with its extension. Revisions (`(Rev 1)`) are not versions.
pub fn version_in_name(name: &str) -> Option<String> {
    let bracketed = name
        .char_indices()
        .filter(|(_, c)| matches!(c, '(' | '['))
        .filter_map(|(i, open)| {
            let close = if open == '(' { ')' } else { ']' };
            let rest = &name[i + 1..];
            let end = rest.find(close)?;
            version_word(&rest[..end])
        })
        .next_back();
    if bracketed.is_some() {
        return bracketed;
    }
    name.split_whitespace().next_back().and_then(version_word)
}

/// `v1.2.0` / `V131072` → `1.2.0` / `131072`: a `v` followed by a digit,
/// then only ASCII letters, digits and inner dots.
fn version_word(word: &str) -> Option<String> {
    let rest = word.trim().strip_prefix(['v', 'V'])?;
    let ok = rest.starts_with(|c: char| c.is_ascii_digit())
        && !rest.ends_with('.')
        && !rest.contains("..")
        && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
    ok.then(|| rest.to_string())
}

/// The order of one version, after stripping a leading `v`/`V`.
fn order(system: &str, version: &str) -> Option<Vec<u64>> {
    version_order(system, version.trim().trim_start_matches(['v', 'V']))
}

/// The strict newest of `versions` by `version_order`. `None` when the list
/// is empty, when any version does not parse (it cannot be placed, so no
/// other can be called the newest), or when the top is a tie.
pub fn newest<'a>(system: &str, versions: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    newest_by(system, versions.into_iter().map(|v| (v, v))).map(|(v, _)| v)
}

/// [`newest`] over items carrying their version; returns the item.
fn newest_by<'a, T, I>(system: &str, items: I) -> Option<(T, Vec<u64>)>
where
    I: IntoIterator<Item = (T, &'a str)>,
{
    let mut best: Option<(T, Vec<u64>)> = None;
    let mut tie = false;
    for (item, version) in items {
        let key = order(system, version)?;
        match &best {
            Some((_, top)) if key < *top => {}
            Some((_, top)) if key == *top => tie = true,
            _ => {
                best = Some((item, key));
                tie = false;
            }
        }
    }
    if tie {
        None
    } else {
        best
    }
}

/// D5: the version installed for a game is the newest one in the library:
/// the highest update version, else the base's own version, else none.
/// With updates present but no strict newest among them (a tie or an
/// unparseable version), nothing is claimed.
pub fn installed_version(system: &str, base: Option<&str>, updates: &[&str]) -> Option<String> {
    if updates.is_empty() {
        return base.map(str::to_string);
    }
    newest(system, updates.iter().copied()).map(str::to_string)
}

/// D6 in the library: the newest orphan update linked to this game (its
/// index, its version) when it is strictly newer than `installed`.
/// `installed: None` means the library holds no version of the game at
/// all; a game in doubt must not reach here (see [`version_facts`]). An
/// unparseable version on either side, or a tie at the top of the orphans,
/// claims nothing.
pub fn newer_elsewhere<'a>(
    system: &str,
    installed: Option<&str>,
    orphans: &[(usize, &'a str)],
) -> Option<(usize, &'a str)> {
    let ((index, version), top) = newest_by(system, orphans.iter().map(|&(i, v)| ((i, v), v)))?;
    match installed {
        None => Some((index, version)),
        Some(have) => (top > order(system, have)?).then_some((index, version)),
    }
}

/// D5 and D6 for one game, computed together. Use this, not
/// [`installed_version`] then [`newer_elsewhere`]: `installed_version`
/// answers `None` both when the library holds no version of the game and
/// when it is in doubt (a tie or an unparseable update), and only the first
/// may compare against orphans. `updates` carries each own update's version
/// as read from its name; an update without one is doubt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionFacts<'a> {
    pub installed: Option<String>,
    pub newer_elsewhere: Option<(usize, &'a str)>,
}

pub fn version_facts<'a>(
    system: &str,
    base: Option<&str>,
    updates: &[Option<&str>],
    orphans: &[(usize, &'a str)],
) -> VersionFacts<'a> {
    let Some(known) = updates.iter().copied().collect::<Option<Vec<&str>>>() else {
        return VersionFacts {
            installed: None,
            newer_elsewhere: None,
        };
    };
    let installed = installed_version(system, base, &known);
    let in_doubt = installed.is_none() && !known.is_empty();
    let newer_elsewhere = if in_doubt {
        None
    } else {
        newer_elsewhere(system, installed.as_deref(), orphans)
    };
    VersionFacts {
        installed,
        newer_elsewhere,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_in_names() {
        assert_eq!(
            version_in_name("Tidewalker (v1.0.2)").as_deref(),
            Some("1.0.2")
        );
        assert_eq!(
            version_in_name("Neon Rhythm [v131072]").as_deref(),
            Some("131072")
        );
        assert_eq!(version_in_name("Game v1.05").as_deref(), Some("1.05"));
        assert_eq!(version_in_name("Game (Europe)"), None);
        assert_eq!(version_in_name("Game (Rev 1)"), None); // revisions are not versions
    }

    #[test]
    fn versions_in_names_edge_cases() {
        assert_eq!(
            version_in_name("Tidewalker (Europe) (v1.3.0) (Update)").as_deref(),
            Some("1.3.0")
        );
        assert_eq!(
            version_in_name("Game [0100ABCD] [V2]").as_deref(),
            Some("2")
        );
        assert_eq!(
            version_in_name("Game (v1.0) (v1.1)").as_deref(),
            Some("1.1")
        );
        assert_eq!(version_in_name("Vampire"), None);
        assert_eq!(version_in_name("Game (v)"), None);
        assert_eq!(version_in_name("Game (v1.)"), None);
        assert_eq!(version_in_name("Game v1.05 (Europe)"), None);
        assert_eq!(version_in_name(""), None);
    }

    #[test]
    fn installed_is_the_newest_by_version_order() {
        assert_eq!(
            installed_version("switch", None, &["1.0.2", "1.0.10"]).as_deref(),
            Some("1.0.10")
        );
        assert_eq!(
            installed_version("ps4", Some("1.00"), &[]).as_deref(),
            Some("1.00")
        );
        assert_eq!(installed_version("ps4", None, &[]), None);
        // hex on Xbox 360
        assert_eq!(
            installed_version("xbox360", None, &["9", "A"]).as_deref(),
            Some("A")
        );
        // 1.05 == 1.5 → no newest, so nothing is claimed
        assert_eq!(installed_version("ps4", None, &["1.05", "1.5"]), None);
        // an unparseable one among the top → none
        assert_eq!(installed_version("ps4", None, &["1.2", "beta"]), None);
    }

    #[test]
    fn installed_strips_a_leading_v_and_keeps_the_text() {
        assert_eq!(
            installed_version("switch", Some("1.0"), &["v1.2", "1.10"]).as_deref(),
            Some("1.10")
        );
        assert_eq!(newest("ps3", ["v01.02", "1.1"]), Some("v01.02"));
        assert_eq!(newest("ps3", std::iter::empty()), None);
    }

    #[test]
    fn newer_elsewhere_only_when_strictly_newer() {
        assert_eq!(
            newer_elsewhere("switch", Some("1.0.2"), &[(7, "1.3.0")]),
            Some((7, "1.3.0"))
        );
        assert_eq!(
            newer_elsewhere("switch", Some("1.3.0"), &[(7, "1.3.0")]),
            None
        );
        assert_eq!(
            newer_elsewhere("switch", None, &[(7, "1.3.0")]),
            Some((7, "1.3.0"))
        );
        assert_eq!(newer_elsewhere("switch", Some("1.0"), &[(7, "x")]), None);
    }

    #[test]
    fn newer_elsewhere_picks_the_newest_orphan_and_refuses_doubt() {
        assert_eq!(
            newer_elsewhere("switch", Some("1.0"), &[(3, "1.1"), (9, "1.2"), (4, "1.0")]),
            Some((9, "1.2"))
        );
        assert_eq!(
            newer_elsewhere("switch", Some("1.0"), &[(3, "1.2"), (9, "1.2")]),
            None
        );
        assert_eq!(newer_elsewhere("switch", Some("beta"), &[(3, "1.2")]), None);
        assert_eq!(
            newer_elsewhere("xbox360", Some("9"), &[(1, "A")]),
            Some((1, "A"))
        );
        assert_eq!(newer_elsewhere("ps4", Some("1.5"), &[(1, "1.05")]), None);
        assert_eq!(newer_elsewhere("switch", None, &[]), None);
    }

    #[test]
    fn version_facts_never_warn_in_doubt() {
        // A tie among the game's own updates: nothing installed is claimed,
        // and an orphan must not be called «newer» either (D5: no warning).
        let facts = version_facts("ps4", None, &[Some("1.05"), Some("1.5")], &[(7, "1.3")]);
        assert_eq!(
            facts,
            VersionFacts {
                installed: None,
                newer_elsewhere: None
            }
        );
        // An update whose name carries no version is doubt too.
        let facts = version_facts("switch", Some("1.0"), &[None], &[(7, "1.3.0")]);
        assert_eq!(
            facts,
            VersionFacts {
                installed: None,
                newer_elsewhere: None
            }
        );
        let facts = version_facts("switch", None, &[Some("1.2"), Some("beta")], &[(7, "1.3")]);
        assert_eq!(facts.newer_elsewhere, None);
    }

    #[test]
    fn version_facts_compare_when_the_library_is_clear() {
        let facts = version_facts(
            "switch",
            None,
            &[Some("1.0.2"), Some("1.0.10")],
            &[(7, "1.3.0")],
        );
        assert_eq!(facts.installed.as_deref(), Some("1.0.10"));
        assert_eq!(facts.newer_elsewhere, Some((7, "1.3.0")));
        // Nothing installed at all (no updates, no base version): any orphan is newer.
        let facts = version_facts("switch", None, &[], &[(7, "1.3.0")]);
        assert_eq!(
            facts,
            VersionFacts {
                installed: None,
                newer_elsewhere: Some((7, "1.3.0"))
            }
        );
        // The base's own version stands when there are no updates.
        let facts = version_facts("ps4", Some("1.00"), &[], &[(2, "1.00")]);
        assert_eq!(
            facts,
            VersionFacts {
                installed: Some("1.00".into()),
                newer_elsewhere: None
            }
        );
        // An unparseable base version is doubt: no claim against orphans.
        let facts = version_facts("ps4", Some("beta"), &[], &[(2, "1.10")]);
        assert_eq!(facts.newer_elsewhere, None);
    }

    #[test]
    fn no_title_list_knows_nothing() {
        assert_eq!(NoTitleList.latest("switch", "0100000000010000"), None);
        let seam: Box<dyn LatestVersions> = Box::new(NoTitleList);
        assert_eq!(seam.latest("3ds", "0004000000030000"), None);
    }

    #[test]
    fn latest_known_serialises_its_source_in_kebab_case() {
        let known = LatestKnown {
            version: "1.3.0".into(),
            source: LatestSource::GameTdb,
        };
        assert_eq!(
            serde_json::to_string(&known).unwrap(),
            r#"{"version":"1.3.0","source":"game-tdb"}"#
        );
    }
}
