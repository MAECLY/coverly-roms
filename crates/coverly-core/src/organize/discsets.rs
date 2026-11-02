//! Loose multi-disc games (adjustments spec D24): discs of one game that
//! sit as separate files (or in sibling «one folder per disc» folders)
//! are grouped into one set, confirmed by what their headers or the DAT
//! say, never by the name alone.
//!
//! Candidates share a system, a folder (or sibling folders whose names
//! are equal once the disc tag is removed) and a name key: the title with
//! its disc tag `(Disc|Disk|CD|GD) (N|A)( of M)` removed, every other tag
//! (region, Rev, languages) kept. A group is confirmed when:
//!
//! - GameCube/Wii: the same 6-character id and distinct disc numbers;
//! - Saturn/Dreamcast: the same `m` of the `n/m` device field;
//! - Xbox/Xbox 360: the same title id with disc numbers;
//! - a DAT hash names every disc, and the names are equal without their
//!   disc tags.
//!
//! A group with none of these is proposed for review by name only
//! ([`SetEvidence::NameOnly`]). A group is rejected (each disc stays its
//! own item) when two discs share a number (the duplicates step handles
//! that), when their regions differ, or when a header or a DAT name
//! contradicts the name (another game, another disc number, another
//! count). DuckStation's
//! `discsets.yaml` is not used (CC BY-NC-ND).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::identify::names::Region;

/// How a set was confirmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetEvidence {
    /// GameCube/Wii: the same ID6, distinct disc bytes.
    NintendoId,
    /// Saturn/Dreamcast: the same `m` of `CD-n/m` or `GD-ROMn/m`.
    SegaCount,
    /// XBE `DiscNumber` / XEX `disc_number` with the same title id.
    XboxDisc,
    /// DAT game names (by hash) equal once the disc tag is removed.
    DatHash,
    /// Only the names agree: review, never automatic.
    NameOnly,
}

/// What a disc's own header says about its place in a set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeaderDisc {
    /// GameCube/Wii ID6 and the 1-based disc number.
    Nintendo { id: String, n: u8 },
    /// Sega device field `n/m`.
    Sega { n: u8, m: u8 },
    /// Xbox/Xbox 360 title id, disc number and (XEX) disc count.
    Xbox { id: String, n: u8, m: Option<u8> },
}

impl HeaderDisc {
    fn n(&self) -> u8 {
        match self {
            HeaderDisc::Nintendo { n, .. }
            | HeaderDisc::Sega { n, .. }
            | HeaderDisc::Xbox { n, .. } => *n,
        }
    }

    fn count(&self) -> Option<u8> {
        match self {
            HeaderDisc::Nintendo { .. } => None,
            HeaderDisc::Sega { m, .. } => Some(*m),
            HeaderDisc::Xbox { m, .. } => *m,
        }
    }

    /// The game this header names: discs of one game share it.
    fn game(&self) -> (u8, String) {
        match self {
            HeaderDisc::Nintendo { id, .. } => (0, id.clone()),
            HeaderDisc::Sega { m, .. } => (1, m.to_string()),
            HeaderDisc::Xbox { id, .. } => (2, id.clone()),
        }
    }
}

/// One disc that may belong to a set.
#[derive(Clone, Debug)]
pub struct Candidate<'a> {
    /// The disc's primary file: its sheet (`.cue`, `.gdi`) or its image.
    pub path: &'a Path,
    /// The system the disc was identified as.
    pub system: &'a str,
    pub header: Option<HeaderDisc>,
    /// The DAT game name, only when a hash decided it.
    pub dat_name: Option<&'a str>,
    /// The region a header read, when it says.
    pub region: Option<Region>,
}

/// A confirmed (or name-only) set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscSet {
    /// Indices into the candidates, in disc order (`Disc 0` first).
    pub members: Vec<usize>,
    /// The game's name without its disc tag, as the first disc spells it
    /// (`Panzer Dragoon Saga (USA)`): the folder and playlist name.
    pub title: String,
    pub evidence: SetEvidence,
    /// Disc numbers of the set that are not in the pile.
    pub missing: Vec<u8>,
}

/// A disc tag found in a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscTag {
    /// The name without the tag, spaces collapsed.
    pub title: String,
    /// The disc number (`A` = 1); 0 for an opening `Disc 0`.
    pub n: u8,
    /// The `M` of `Disc N of M`.
    pub of: Option<u8>,
}

/// The disc tag of `stem` (a file stem or folder name):
/// `(Disc|Disk|CD|GD) (N|A)( of M)`, case-insensitive. `None` without one;
/// with several, the first counts and every tag is removed from the title.
pub fn disc_tag(stem: &str) -> Option<DiscTag> {
    let mut title = String::with_capacity(stem.len());
    let mut found: Option<(u8, Option<u8>)> = None;
    let mut rest = stem;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let group = &rest[open + 1..open + close];
        title.push_str(&rest[..open]);
        match tag_group(group) {
            Some(tag) => found = found.or(Some(tag)),
            None => title.push_str(&rest[open..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    title.push_str(rest);
    let (n, of) = found?;
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then_some(DiscTag { title, n, of })
}

/// `(n, of)` of one parenthesised group, or `None` when it is no disc tag.
fn tag_group(group: &str) -> Option<(u8, Option<u8>)> {
    let mut words = group.split_whitespace();
    let word = words.next()?;
    if !["disc", "disk", "cd", "gd"]
        .iter()
        .any(|w| word.eq_ignore_ascii_case(w))
    {
        return None;
    }
    let n = disc_number(words.next()?)?;
    match (words.next(), words.next(), words.next()) {
        (None, None, None) => Some((n, None)),
        (Some(of), Some(m), None) if of.eq_ignore_ascii_case("of") => {
            let m = m.parse::<u8>().ok().filter(|&m| m >= 1)?;
            (n <= m).then_some((n, Some(m)))
        }
        _ => None,
    }
}

/// `3` or `C` (= 3); at most 99.
fn disc_number(word: &str) -> Option<u8> {
    if let Ok(n) = word.parse::<u8>() {
        return (n < 100).then_some(n);
    }
    match word.as_bytes() {
        [c] if c.is_ascii_alphabetic() => Some(c.to_ascii_uppercase() - b'A' + 1),
        _ => None,
    }
}

/// Folds a title for comparison.
fn key(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Where a disc sits for grouping, its title and its tag.
fn placed(path: &Path) -> Option<((PathBuf, String), DiscTag)> {
    let stem = path.file_stem()?.to_string_lossy();
    let parent = path.parent().unwrap_or(Path::new(""));
    let folder = parent
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .and_then(|f| disc_tag(&f));
    let own = disc_tag(&stem);
    match folder {
        // «One folder per disc»: sibling folders equal once untagged.
        Some(folder) => {
            let place = (
                parent.parent().unwrap_or(Path::new("")).to_path_buf(),
                key(&folder.title),
            );
            Some((place, own.unwrap_or(folder)))
        }
        None => Some(((parent.to_path_buf(), String::new()), own?)),
    }
}

/// System, folder, sibling-folder title and name key of a disc.
type GroupKey = (String, PathBuf, String, String);

/// Groups `candidates` into sets (spec D24). A candidate is in at most
/// one set; sets come in the order of their first member.
pub fn group(candidates: &[Candidate]) -> Vec<DiscSet> {
    let mut groups: BTreeMap<GroupKey, Vec<(usize, DiscTag)>> = BTreeMap::new();
    for (i, c) in candidates.iter().enumerate() {
        let Some(((dir, folder), tag)) = placed(c.path) else {
            continue;
        };
        groups
            .entry((c.system.to_string(), dir, folder, key(&tag.title)))
            .or_default()
            .push((i, tag));
    }
    let mut sets: Vec<DiscSet> = groups
        .into_values()
        .filter(|g| g.len() > 1)
        .filter_map(|g| confirm(candidates, g))
        .collect();
    sets.sort_by_key(|s| s.members.iter().copied().min());
    sets
}

/// The set a group of tagged discs forms, or `None` when it is rejected.
fn confirm(candidates: &[Candidate], mut discs: Vec<(usize, DiscTag)>) -> Option<DiscSet> {
    discs.sort_by_key(|(i, tag)| (tag.n, *i));
    // Two discs with one number are copies: dedupe's business.
    if discs.windows(2).any(|w| w[0].1.n == w[1].1.n) {
        return None;
    }
    // Regions a header read must agree.
    let mut regions = discs.iter().filter_map(|(i, _)| candidates[*i].region);
    if let Some(first) = regions.next() {
        if regions.any(|r| r != first) {
            return None;
        }
    }
    // Every header names the same game, and its number is the name's.
    let headers: Vec<(&DiscTag, &HeaderDisc)> = discs
        .iter()
        .filter_map(|(i, tag)| Some((tag, candidates[*i].header.as_ref()?)))
        .collect();
    if let Some((_, first)) = headers.first() {
        if headers.iter().any(|(_, h)| h.game() != first.game()) {
            return None;
        }
    }
    if headers.iter().any(|(tag, h)| tag.n >= 1 && h.n() != tag.n) {
        return None;
    }
    // A DAT name a hash gave says which disc the file is, like a header.
    let dat_contradicts = discs.iter().any(|(i, tag)| {
        candidates[*i]
            .dat_name
            .and_then(disc_tag)
            .is_some_and(|dat| dat.n != tag.n)
    });
    if dat_contradicts {
        return None;
    }
    let numbered: Vec<&(usize, DiscTag)> = discs.iter().filter(|(_, t)| t.n >= 1).collect();
    let all_headers = numbered
        .iter()
        .all(|(i, _)| candidates[*i].header.is_some());
    let evidence = if all_headers && !numbered.is_empty() {
        match candidates[numbered[0].0].header.as_ref() {
            Some(HeaderDisc::Nintendo { .. }) => SetEvidence::NintendoId,
            Some(HeaderDisc::Sega { .. }) => SetEvidence::SegaCount,
            Some(HeaderDisc::Xbox { .. }) => SetEvidence::XboxDisc,
            None => SetEvidence::NameOnly,
        }
    } else {
        let names: Vec<Option<&str>> = numbered
            .iter()
            .map(|(i, _)| candidates[*i].dat_name)
            .collect();
        let titles: Vec<String> = names
            .iter()
            .flatten()
            .map(|n| disc_tag(n).map_or_else(|| key(n), |t| key(&t.title)))
            .collect();
        // DAT names of one set must agree; names of two games never group.
        if titles.windows(2).any(|w| w[0] != w[1]) {
            return None;
        }
        if !names.is_empty() && names.iter().all(Option::is_some) {
            SetEvidence::DatHash
        } else {
            SetEvidence::NameOnly
        }
    };
    // The count a header or a name gives; else the highest number.
    let counts: Vec<u8> = headers
        .iter()
        .filter_map(|(_, h)| h.count())
        .chain(discs.iter().filter_map(|(_, t)| t.of))
        .collect();
    if counts.windows(2).any(|w| w[0] != w[1]) {
        return None;
    }
    let highest = discs.iter().map(|(_, t)| t.n).max().unwrap_or(0);
    let count = counts.first().copied().unwrap_or(highest);
    if highest > count {
        return None;
    }
    let present: Vec<u8> = discs.iter().map(|(_, t)| t.n).collect();
    let missing = (1..=count).filter(|n| !present.contains(n)).collect();
    let title = discs
        .iter()
        .find(|(_, t)| t.n >= 1)
        .or(discs.first())
        .map(|(_, t)| t.title.clone())?;
    Some(DiscSet {
        members: discs.iter().map(|(i, _)| *i).collect(),
        title,
        evidence,
        missing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_tags() {
        let t = disc_tag("Panzer Dragoon Saga (USA) (Disc 2)").unwrap();
        assert_eq!(
            (t.title.as_str(), t.n, t.of),
            ("Panzer Dragoon Saga (USA)", 2, None)
        );
        let t = disc_tag("Final Fantasy VII (USA) (Disc 3 of 3)").unwrap();
        assert_eq!((t.n, t.of), (3, Some(3)));
        let t = disc_tag("Shenmue (Europe) (GD 1) (Rev 1)").unwrap();
        assert_eq!((t.title.as_str(), t.n), ("Shenmue (Europe) (Rev 1)", 1));
        assert_eq!(disc_tag("Game (Disk B)").unwrap().n, 2);
        assert_eq!(disc_tag("Game (cd 0)").unwrap().n, 0);
        assert_eq!(disc_tag("Game (USA)"), None);
        assert_eq!(disc_tag("Game (Disc 4 of 3)"), None);
        assert_eq!(disc_tag("Game (Disc two)"), None);
        assert_eq!(disc_tag("(Disc 1)"), None);
    }

    fn cand<'a>(path: &'a str, system: &'a str, header: Option<HeaderDisc>) -> Candidate<'a> {
        Candidate {
            path: Path::new(path),
            system,
            header,
            dat_name: None,
            region: None,
        }
    }

    #[test]
    fn untagged_and_lonely_discs_never_group() {
        let c = [
            cand("p/Game (USA).iso", "gamecube", None),
            cand("p/Other (USA) (Disc 1).iso", "gamecube", None),
        ];
        assert!(group(&c).is_empty());
    }

    #[test]
    fn regions_and_dat_names_must_agree() {
        let mut c = [
            cand("p/Game (Disc 1).iso", "psx", None),
            cand("p/Game (Disc 2).iso", "psx", None),
        ];
        c[0].region = Some(Region::Usa);
        c[1].region = Some(Region::Europe);
        assert!(group(&c).is_empty());
        c[1].region = Some(Region::Usa);
        assert_eq!(group(&c)[0].evidence, SetEvidence::NameOnly);
        c[0].dat_name = Some("Game (USA) (Disc 1)");
        c[1].dat_name = Some("Other (USA) (Disc 2)");
        assert!(group(&c).is_empty());
        c[1].dat_name = Some("Game (USA) (Disc 2)");
        assert_eq!(group(&c)[0].evidence, SetEvidence::DatHash);
    }

    #[test]
    fn dat_disc_numbers_must_be_the_names() {
        let mut c = [
            cand("p/Game (Disc 1).bin", "psx", None),
            cand("p/Game (Disc 2).bin", "psx", None),
        ];
        // Swapped names: the hashes say which disc each file is.
        c[0].dat_name = Some("Game (USA) (Disc 2)");
        c[1].dat_name = Some("Game (USA) (Disc 1)");
        assert!(group(&c).is_empty());
        // Two copies of disc 1 under two disc names.
        c[1].dat_name = Some("Game (USA) (Disc 2)");
        c[0].dat_name = Some("Game (USA) (Disc 2)");
        assert!(group(&c).is_empty());
    }

    #[test]
    fn headers_name_one_game_and_its_numbers() {
        let nintendo = |id: &str, n| Some(HeaderDisc::Nintendo { id: id.into(), n });
        let set = |a, b| {
            group(&[
                cand("p/Game (Disc 1).rvz", "gamecube", a),
                cand("p/Game (Disc 2).rvz", "gamecube", b),
            ])
        };
        assert_eq!(
            set(nintendo("GAME01", 1), nintendo("GAME01", 2))[0].evidence,
            SetEvidence::NintendoId
        );
        assert!(set(nintendo("GAME01", 1), nintendo("OTHR01", 2)).is_empty());
        assert!(set(nintendo("GAME01", 1), nintendo("GAME01", 1)).is_empty());
        // One header only: the names alone decide, for review.
        assert_eq!(
            set(nintendo("GAME01", 1), None)[0].evidence,
            SetEvidence::NameOnly
        );
        let sega = |n, m| Some(HeaderDisc::Sega { n, m });
        let s = group(&[
            cand("p/Game (Disc 1).cue", "saturn", sega(1, 3)),
            cand("p/Game (Disc 3).cue", "saturn", sega(3, 3)),
        ]);
        assert_eq!(
            (s[0].evidence, s[0].missing.as_slice()),
            (SetEvidence::SegaCount, &[2u8][..])
        );
    }

    #[test]
    fn other_systems_and_folders_never_group() {
        let c = [
            cand("p/Game (Disc 1).cue", "saturn", None),
            cand("p/Game (Disc 2).cue", "segacd", None),
            cand("q/Game (Disc 2).cue", "saturn", None),
        ];
        assert!(group(&c).is_empty());
    }
}
