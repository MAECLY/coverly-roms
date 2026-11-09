//! Linking an addon to exactly one game (mods spec D5, D6, D8). Pure: the
//! caller gathers the candidate games (plan items first, then library
//! bases) with the digests and ids it already has, and this module only
//! compares.

use std::path::PathBuf;

use unicode_normalization::UnicodeNormalization;

use crate::disc::serial::normalise_serial;
use crate::identify::names::{parse_tags, Region};

use super::{AddonKey, HeaderFit, LinkVia};

/// A game an addon may belong to: a plan item or a library base.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GameRef {
    Item(usize),
    Library(PathBuf),
}

impl Default for GameRef {
    fn default() -> Self {
        GameRef::Item(0)
    }
}

/// One game an addon may link to, with what is already known about it.
#[derive(Clone, Debug, Default)]
pub struct Candidate {
    pub game: GameRef,
    pub system: String,
    pub file_name: String,
    pub stem: String,
    pub title_key: String,
    pub regions: Vec<Region>,
    pub catalogue_name: Option<String>,
    pub size: u64,
    pub crc_full: Option<u32>,
    pub crc_headerless: Option<u32>,
    pub has_header: bool,
    pub serials: Vec<String>,
    pub game_id: Option<String>,
    pub title_id: Option<String>,
    pub n64_id: Option<String>,
    pub n64_name: Option<String>,
    pub keeper: bool,
}

/// Every candidate, plan items first and library bases after.
pub struct Candidates<'a> {
    pub list: &'a [Candidate],
    /// DAT games of a CRC32: `(game name, size)`.
    pub dat_by_crc: &'a dyn Fn(u32) -> Vec<(String, u64)>,
    /// The lookup budget ran out before every candidate was known.
    pub budget_out: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    One {
        game: GameRef,
        via: LinkVia,
        header: Option<HeaderFit>,
    },
    Several(usize),
    None,
    Budget,
}

/// Links `key` to exactly one game (D5, D6, D8).
///
/// Tiers are tried strongest proof first and the first tier with any hit
/// decides; inside a tier, plan items come before library bases. One game
/// → [`Resolution::One`]; several → the dedupe keeper when they are dumps
/// of one game (same system, title key and regions), else
/// [`Resolution::Several`]. `system`, when known, filters the candidates.
///
/// `own_stem` is only used for a keyless addon ([`AddonKey::None`]): the
/// name fallback, a patch stem equal to a ROM stem. A plain code-text
/// cheat ([`super::AddonFormat::CodeText`]) links only that way, so its
/// caller passes [`AddonKey::None`] with the cheat's stem. `Many` and a
/// build id without a title id never link.
pub fn resolve(
    key: &AddonKey,
    system: Option<&str>,
    own_stem: &str,
    c: &Candidates<'_>,
) -> Resolution {
    let tiers = tiers_for(key, own_stem, c);
    if tiers.is_empty() {
        return Resolution::None;
    }
    let in_system = |cand: &Candidate| system.is_none_or(|s| cand.system == s);
    for (via, test) in &tiers {
        let hits = hits_of(
            c.list,
            |cand| {
                if in_system(cand) {
                    test(cand)
                } else {
                    None
                }
            },
        );
        if !hits.is_empty() {
            return decide(&hits, *via);
        }
    }
    if c.budget_out {
        Resolution::Budget
    } else {
        Resolution::None
    }
}

/// Readme fallback for keyless patches (D5): exactly one candidate whose
/// file name or catalogue (No-Intro) name appears in `readme`, or whose
/// CRC32 is written as 8 hex digits next to «CRC». Only the first 64 KiB
/// are searched.
pub fn resolve_by_readme(readme: &str, c: &Candidates<'_>) -> Resolution {
    let text = cap(readme, README_CAP);
    let lower = fold(text);
    let crcs = crcs_in(text);
    let named = |name: &str| {
        let n = fold(name);
        !n.trim().is_empty() && stands_alone(&lower, &n)
    };
    let hits = hits_of(c.list, |cand| {
        let by_name = named(&cand.file_name)
            || cand.catalogue_name.as_deref().is_some_and(named)
            || [cand.crc_full, cand.crc_headerless]
                .into_iter()
                .flatten()
                .any(|crc| crcs.contains(&crc));
        by_name.then_some(None)
    });
    if hits.is_empty() {
        Resolution::None
    } else {
        decide(&hits, LinkVia::Readme)
    }
}

/// Source sizes worth hashing for a `SourceCrc` key: the size itself, plus
/// an iNES header (16 bytes), plus a SNES/SMS copier header (512 bytes).
pub fn wanted_sizes(source_size: u64) -> [u64; 3] {
    [
        source_size,
        source_size.saturating_add(16),
        source_size.saturating_add(512),
    ]
}

const README_CAP: usize = 64 * 1024;

/// A tier's test: `Some(header fit)` when the candidate is a hit.
type Test<'a> = Box<dyn Fn(&Candidate) -> Option<Option<HeaderFit>> + 'a>;

fn tiers_for<'a>(
    key: &'a AddonKey,
    own_stem: &'a str,
    c: &'a Candidates<'_>,
) -> Vec<(LinkVia, Test<'a>)> {
    let mut tiers: Vec<(LinkVia, Test<'a>)> = Vec::new();
    match key {
        AddonKey::SourceCrc { crc32, size } => {
            let (crc, size) = (*crc32, *size);
            let sizes = wanted_sizes(size);
            tiers.push((
                LinkVia::SourceCrc,
                Box::new(move |cand| {
                    if cand.crc_full == Some(crc) && cand.size == size {
                        Some(Some(if cand.has_header {
                            HeaderFit::NeedsHeadered
                        } else {
                            HeaderFit::Matches
                        }))
                    } else if cand.crc_headerless == Some(crc) && sizes.contains(&cand.size) {
                        Some(Some(if cand.has_header {
                            HeaderFit::NeedsHeaderless
                        } else {
                            HeaderFit::Matches
                        }))
                    } else {
                        None
                    }
                }),
            ));
            let dat_names: Vec<String> = (c.dat_by_crc)(crc)
                .into_iter()
                .filter(|(_, s)| *s == size)
                .map(|(name, _)| fold(&name))
                .collect();
            if !dat_names.is_empty() {
                tiers.push((
                    LinkVia::Dat,
                    Box::new(move |cand| {
                        // Only a ROM whose digests were not computed: one
                        // that was hashed and did not match is another dump.
                        let unhashed = cand.crc_full.is_none() && cand.crc_headerless.is_none();
                        let named = cand
                            .catalogue_name
                            .as_deref()
                            .is_some_and(|n| dat_names.contains(&fold(n)));
                        (unhashed && named && sizes.contains(&cand.size)).then_some(Some(
                            if cand.has_header {
                                HeaderFit::NeedsHeaderless
                            } else {
                                HeaderFit::Matches
                            },
                        ))
                    }),
                ));
            }
        }
        AddonKey::SourceName { name } => {
            tiers.push((
                LinkVia::Name,
                name_test(move |cand| eq_name(&cand.file_name, name)),
            ));
        }
        AddonKey::RomName { name } => {
            tiers.push((
                LinkVia::Name,
                name_test(move |cand| eq_name(&cand.stem, name)),
            ));
            let tags = parse_tags(name);
            if !tags.title_key.is_empty() {
                let regions = region_set(&tags.regions);
                tiers.push((
                    LinkVia::Name,
                    name_test(move |cand| {
                        cand.title_key == tags.title_key && region_set(&cand.regions) == regions
                    }),
                ));
            }
            tiers.push((
                LinkVia::Name,
                name_test(move |cand| {
                    cand.catalogue_name
                        .as_deref()
                        .is_some_and(|n| eq_name(n, name))
                }),
            ));
        }
        AddonKey::Serial { serial } | AddonKey::SerialVersion { serial, .. } => {
            if let Some(want) = serial_key(serial) {
                tiers.push((LinkVia::Key, name_test(move |cand| has_id(cand, &want))));
            }
        }
        AddonKey::TitleId { id }
        | AddonKey::BuildId {
            title_id: Some(id), ..
        } => {
            if let Some(want) = serial_key(id) {
                tiers.push((LinkVia::Key, name_test(move |cand| has_id(cand, &want))));
            }
        }
        AddonKey::TitleIds { ids } => {
            let wants: Vec<String> = ids.iter().filter_map(|id| serial_key(id)).collect();
            if !wants.is_empty() {
                tiers.push((
                    LinkVia::Key,
                    name_test(move |cand| wants.iter().any(|w| has_id(cand, w))),
                ));
            }
        }
        AddonKey::GameId { id } => {
            let want = compact(id);
            if !want.is_empty() {
                tiers.push((
                    LinkVia::Key,
                    name_test(move |cand| {
                        let gid = cand.game_id.as_deref().map(compact);
                        match want.len() {
                            3 => gid.is_some_and(|g| g.len() >= 3 && g.starts_with(&want)),
                            _ => gid.as_deref() == Some(want.as_str()) || has_id(cand, &want),
                        }
                    }),
                ));
            }
        }
        AddonKey::N64Id { id } => {
            let want = compact(id);
            if !want.is_empty() {
                tiers.push((
                    LinkVia::Key,
                    name_test(move |cand| {
                        // An APS10 key is the cart id and the country
                        // (header 0x3C-0x3E): the game code without its
                        // media letter (`SME` of `NSME`).
                        cand.n64_id.as_deref().map(compact).is_some_and(|n| {
                            n == want || (want.len() == 3 && n.len() == 4 && n[1..] == want)
                        })
                    }),
                ));
            }
        }
        AddonKey::N64Name { name } => {
            let want = name.trim();
            if !want.is_empty() {
                tiers.push((
                    LinkVia::Key,
                    name_test(move |cand| {
                        cand.n64_name
                            .as_deref()
                            .is_some_and(|n| eq_name(n.trim(), want))
                    }),
                ));
            }
        }
        AddonKey::None => {
            if !own_stem.trim().is_empty() {
                tiers.push((
                    LinkVia::Name,
                    name_test(move |cand| eq_name(&cand.stem, own_stem)),
                ));
            }
        }
        AddonKey::BuildId { title_id: None, .. } | AddonKey::Many { .. } => {}
    }
    tiers
}

fn name_test<'a>(f: impl Fn(&Candidate) -> bool + 'a) -> Test<'a> {
    Box::new(move |cand| f(cand).then_some(None))
}

/// Hits of one tier: plan items first; library bases only when no plan
/// item is a hit.
fn hits_of(
    list: &[Candidate],
    test: impl Fn(&Candidate) -> Option<Option<HeaderFit>>,
) -> Vec<(&Candidate, Option<HeaderFit>)> {
    let in_plan = |cand: &&Candidate| matches!(cand.game, GameRef::Item(_));
    let plan: Vec<_> = list
        .iter()
        .filter(in_plan)
        .filter_map(|cand| test(cand).map(|h| (cand, h)))
        .collect();
    if !plan.is_empty() {
        return plan;
    }
    list.iter()
        .filter(|cand| !in_plan(cand))
        .filter_map(|cand| test(cand).map(|h| (cand, h)))
        .collect()
}

/// D6: one game, or the keeper of dumps of one game, or several.
fn decide(hits: &[(&Candidate, Option<HeaderFit>)], via: LinkVia) -> Resolution {
    let mut games: Vec<(&Candidate, Option<HeaderFit>)> = Vec::new();
    for (cand, header) in hits {
        if !games.iter().any(|(g, _)| g.game == cand.game) {
            games.push((cand, *header));
        }
    }
    let pick = |(cand, header): (&Candidate, Option<HeaderFit>)| Resolution::One {
        game: cand.game.clone(),
        via,
        header,
    };
    match games.as_slice() {
        [] => Resolution::None,
        [only] => pick(*only),
        [first, rest @ ..] => {
            let same_game = rest.iter().all(|(g, _)| {
                g.system == first.0.system
                    && !g.title_key.is_empty()
                    && g.title_key == first.0.title_key
                    && region_set(&g.regions) == region_set(&first.0.regions)
                    && same_ids(g, first.0)
            });
            let keepers: Vec<_> = games.iter().filter(|(g, _)| g.keeper).collect();
            match (same_game, keepers.as_slice()) {
                (true, [keeper]) => pick(**keeper),
                _ => Resolution::Several(games.len()),
            }
        }
    }
}

/// Two dumps of one game never carry different ids: a region-less name
/// with `RMCE01` and `RMCP01` is two regions, not two dumps.
fn same_ids(a: &Candidate, b: &Candidate) -> bool {
    fn agree(x: Option<String>, y: Option<String>) -> bool {
        match (x, y) {
            (Some(x), Some(y)) => x == y,
            _ => true,
        }
    }
    let serials = |c: &Candidate| -> Vec<String> {
        let mut v: Vec<String> = c.serials.iter().filter_map(|s| serial_key(s)).collect();
        v.sort();
        v.dedup();
        v
    };
    let (sa, sb) = (serials(a), serials(b));
    agree(
        a.game_id.as_deref().map(compact),
        b.game_id.as_deref().map(compact),
    ) && agree(
        a.title_id.as_deref().and_then(serial_key),
        b.title_id.as_deref().and_then(serial_key),
    ) && agree(
        a.n64_id.as_deref().map(compact),
        b.n64_id.as_deref().map(compact),
    ) && (sa.is_empty() || sb.is_empty() || sa == sb)
}

fn region_set(regions: &[Region]) -> Vec<Region> {
    let mut r = regions.to_vec();
    r.sort();
    r.dedup();
    r
}

/// NFC and lower case, for case-insensitive name comparisons.
fn fold(s: &str) -> String {
    s.nfc().flat_map(char::to_lowercase).collect()
}

fn eq_name(a: &str, b: &str) -> bool {
    !a.is_empty() && fold(a) == fold(b)
}

/// Letters and digits only, upper case: `SLUS-00594`, `SLUS_005.94` and
/// `slus00594` compare equal.
fn compact(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// A serial or title id in its compared form, after `normalise_serial`.
fn serial_key(s: &str) -> Option<String> {
    let k = compact(&normalise_serial(s).unwrap_or_else(|| s.to_string()));
    (!k.is_empty()).then_some(k)
}

/// Whether one of the candidate's serials or its title id is `want`.
fn has_id(cand: &Candidate, want: &str) -> bool {
    cand.serials
        .iter()
        .chain(cand.title_id.iter())
        .filter_map(|s| serial_key(s))
        .any(|s| s == want)
}

/// Whether `name` appears in `text` as a whole name: no letter or digit
/// right before or after it, and no further ` (tag)` / ` [tag]` after it
/// (`Contra (USA) (Rev 1)` names another dump than `Contra (USA)`).
fn stands_alone(text: &str, name: &str) -> bool {
    let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let rest = &text[at + name.len()..];
        let after = rest.chars().next();
        let more_tags = rest.trim_start_matches([' ', '\t']).starts_with(['(', '[']);
        !word(before) && !word(after) && !more_tags
    })
}

fn cap(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// CRC32s written as 8 hex digits right after «CRC» or «CRC32» (with a
/// few separators between).
fn crcs_in(text: &str) -> Vec<u32> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 <= b.len() {
        if !b[i..i + 3].eq_ignore_ascii_case(b"crc") {
            i += 1;
            continue;
        }
        let after = i + 3;
        let mut starts = vec![after];
        if b.get(after..after + 2) == Some(b"32") {
            starts.push(after + 2);
        }
        for start in starts {
            let mut j = start;
            while j < b.len() && j - start < 4 && b" \t:=#-(".contains(&b[j]) {
                j += 1;
            }
            let run = b[j..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
            if run == 8 {
                if let Some(crc) = std::str::from_utf8(&b[j..j + 8])
                    .ok()
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                {
                    if !out.contains(&crc) {
                        out.push(crc);
                    }
                }
            }
        }
        i = after;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_dat(_: u32) -> Vec<(String, u64)> {
        Vec::new()
    }

    fn rom(i: usize, system: &str, file: &str) -> Candidate {
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s).to_string();
        let tags = crate::identify::names::parse_tags(&stem);
        Candidate {
            game: GameRef::Item(i),
            system: system.into(),
            file_name: file.into(),
            stem,
            title_key: tags.title_key,
            regions: tags.regions,
            ..Candidate::default()
        }
    }

    fn run(key: &AddonKey, system: Option<&str>, list: &[Candidate]) -> Resolution {
        resolve(
            key,
            system,
            "",
            &Candidates {
                list,
                dat_by_crc: &no_dat,
                budget_out: false,
            },
        )
    }

    fn one(i: usize, via: LinkVia, header: Option<HeaderFit>) -> Resolution {
        Resolution::One {
            game: GameRef::Item(i),
            via,
            header,
        }
    }

    const CRC: u32 = 0x2D20_6BF7;
    const SIZE: u64 = 1_048_576;

    fn crc_key() -> AddonKey {
        AddonKey::SourceCrc {
            crc32: CRC,
            size: SIZE,
        }
    }

    #[test]
    fn wanted_sizes_cover_ines_and_copier_headers() {
        assert_eq!(wanted_sizes(1_048_576), [1_048_576, 1_048_592, 1_049_088]);
    }

    #[test]
    fn source_crc_full_match_fits() {
        let mut a = rom(0, "snes", "Chrono Trigger (USA).sfc");
        a.size = SIZE;
        a.crc_full = Some(CRC);
        a.crc_headerless = Some(CRC);
        let mut b = rom(1, "snes", "Other (USA).sfc");
        b.size = SIZE;
        b.crc_full = Some(0x1234_5678);
        assert_eq!(
            run(&crc_key(), Some("snes"), &[b, a]),
            one(0, LinkVia::SourceCrc, Some(HeaderFit::Matches))
        );
    }

    #[test]
    fn headerless_match_on_a_headered_rom_needs_headerless() {
        let mut a = rom(0, "nes", "Contra (USA).nes");
        a.size = SIZE + 16;
        a.crc_full = Some(0xDEAD_BEEF);
        a.crc_headerless = Some(CRC);
        a.has_header = true;
        assert_eq!(
            run(&crc_key(), Some("nes"), &[a]),
            one(0, LinkVia::SourceCrc, Some(HeaderFit::NeedsHeaderless))
        );
    }

    #[test]
    fn full_match_on_a_headered_rom_needs_headered() {
        let mut a = rom(0, "snes", "Game (USA).smc");
        a.size = SIZE;
        a.crc_full = Some(CRC);
        a.crc_headerless = Some(0x0BAD_F00D);
        a.has_header = true;
        assert_eq!(
            run(&crc_key(), Some("snes"), &[a]),
            one(0, LinkVia::SourceCrc, Some(HeaderFit::NeedsHeadered))
        );
    }

    #[test]
    fn a_crc_with_the_wrong_size_does_not_link() {
        let mut a = rom(0, "snes", "Game (USA).sfc");
        a.size = SIZE * 2;
        a.crc_full = Some(CRC);
        assert_eq!(run(&crc_key(), Some("snes"), &[a]), Resolution::None);
    }

    #[test]
    fn dat_only_links_by_catalogue_name_and_size() {
        let mut a = rom(0, "snes", "ct.sfc");
        a.size = SIZE;
        a.catalogue_name = Some("Chrono Trigger (USA)".into());
        let mut b = rom(1, "snes", "other.sfc");
        b.size = SIZE;
        b.catalogue_name = Some("Other (USA)".into());
        let dat = |crc: u32| {
            if crc == CRC {
                vec![("Chrono Trigger (USA)".to_string(), SIZE)]
            } else {
                Vec::new()
            }
        };
        let list = [b, a];
        let c = Candidates {
            list: &list,
            dat_by_crc: &dat,
            budget_out: false,
        };
        assert_eq!(
            resolve(&crc_key(), Some("snes"), "", &c),
            one(0, LinkVia::Dat, Some(HeaderFit::Matches))
        );
        // A DAT game of another size is not the source.
        let wrong = |_: u32| vec![("Chrono Trigger (USA)".to_string(), SIZE + 1)];
        let c = Candidates {
            list: &list,
            dat_by_crc: &wrong,
            budget_out: false,
        };
        assert_eq!(resolve(&crc_key(), Some("snes"), "", &c), Resolution::None);
    }

    #[test]
    fn identical_dumps_link_to_the_keeper_or_stay() {
        let mut a = rom(0, "snes", "Game (USA).sfc");
        a.size = SIZE;
        a.crc_full = Some(CRC);
        let mut b = a.clone();
        b.game = GameRef::Item(1);
        b.file_name = "Game (USA) copy.sfc".into();
        b.keeper = true;
        assert_eq!(
            run(&crc_key(), Some("snes"), &[a.clone(), b.clone()]),
            one(1, LinkVia::SourceCrc, Some(HeaderFit::Matches))
        );
        b.keeper = false;
        assert_eq!(
            run(&crc_key(), Some("snes"), &[a, b]),
            Resolution::Several(2)
        );
    }

    #[test]
    fn plan_items_win_over_the_library_in_one_tier() {
        let mut a = rom(0, "snes", "Game (USA).sfc");
        a.size = SIZE;
        a.crc_full = Some(CRC);
        let mut lib = a.clone();
        lib.game = GameRef::Library(PathBuf::from("lib").join("Game (USA).sfc"));
        assert_eq!(
            run(&crc_key(), Some("snes"), &[a, lib]),
            one(0, LinkVia::SourceCrc, Some(HeaderFit::Matches))
        );
    }

    #[test]
    fn source_name_matches_the_file_name_case_insensitively() {
        let key = AddonKey::SourceName {
            name: "game (usa).iso".into(),
        };
        let list = [
            rom(0, "ps2", "Other (USA).iso"),
            rom(1, "ps2", "Game (USA).iso"),
        ];
        assert_eq!(run(&key, None, &list), one(1, LinkVia::Name, None));
    }

    #[test]
    fn rom_name_needs_the_stem_or_the_same_regions() {
        let key = AddonKey::RomName {
            name: "Chrono Trigger (USA)".into(),
        };
        assert_eq!(
            run(
                &key,
                Some("snes"),
                &[rom(0, "snes", "Chrono Trigger (USA).sfc")]
            ),
            one(0, LinkVia::Name, None)
        );
        assert_eq!(
            run(
                &key,
                Some("snes"),
                &[rom(0, "snes", "Chrono Trigger (Japan).sfc")]
            ),
            Resolution::None
        );
        // Same title and regions, other tags: still the game.
        assert_eq!(
            run(
                &key,
                Some("snes"),
                &[rom(3, "snes", "Chrono Trigger (USA) [!].sfc")]
            ),
            one(3, LinkVia::Name, None)
        );
        let mut cat = rom(4, "snes", "ct.sfc");
        cat.catalogue_name = Some("Chrono Trigger (USA)".into());
        assert_eq!(run(&key, Some("snes"), &[cat]), one(4, LinkVia::Name, None));
    }

    #[test]
    fn another_system_never_matches() {
        let key = AddonKey::RomName {
            name: "Tetris (World)".into(),
        };
        assert_eq!(
            run(&key, Some("gb"), &[rom(0, "nes", "Tetris (World).nes")]),
            Resolution::None
        );
        assert_eq!(
            run(&key, Some("gb"), &[rom(1, "gb", "Tetris (World).gb")]),
            one(1, LinkVia::Name, None)
        );
    }

    #[test]
    fn serials_compare_in_their_normal_form() {
        let key = AddonKey::Serial {
            serial: "SLUS-00594".into(),
        };
        let mut a = rom(0, "psx", "Game (USA).chd");
        a.serials = vec!["SLUS_005.94".into()];
        let b = rom(1, "psx", "Other (USA).chd");
        assert_eq!(
            run(&key, Some("psx"), &[b, a.clone()]),
            one(0, LinkVia::Key, None)
        );
        let key = AddonKey::SerialVersion {
            serial: "CUSA07023".into(),
            version: "01.03".into(),
        };
        let mut p = rom(2, "ps4", "Game.pkg");
        p.title_id = Some("CUSA07023".into());
        assert_eq!(run(&key, Some("ps4"), &[p]), one(2, LinkVia::Key, None));
    }

    #[test]
    fn a_three_character_game_id_with_two_regions_is_several() {
        let mut us = rom(0, "wii", "Mario Kart Wii (USA).rvz");
        us.game_id = Some("RMCE01".into());
        us.keeper = true;
        let mut eu = rom(1, "wii", "Mario Kart Wii (Europe).rvz");
        eu.game_id = Some("RMCP01".into());
        eu.keeper = true;
        let three = AddonKey::GameId { id: "RMC".into() };
        assert_eq!(
            run(&three, Some("wii"), &[us.clone(), eu.clone()]),
            Resolution::Several(2)
        );
        // A keeper does not choose between two regions.
        eu.keeper = false;
        assert_eq!(
            run(&three, Some("wii"), &[us.clone(), eu.clone()]),
            Resolution::Several(2)
        );
        let six = AddonKey::GameId {
            id: "RMCP01".into(),
        };
        assert_eq!(
            run(&six, Some("wii"), &[us.clone(), eu]),
            one(1, LinkVia::Key, None)
        );
        assert_eq!(run(&three, Some("wii"), &[us]), one(0, LinkVia::Key, None));
    }

    #[test]
    fn a_keeper_never_chooses_between_two_ids_of_one_name() {
        // Region-less names: only the game ids tell the regions apart.
        let mut us = rom(0, "wii", "Mario Kart Wii.rvz");
        us.game_id = Some("RMCE01".into());
        us.keeper = true;
        let mut eu = rom(1, "wii", "Mario Kart Wii.wbfs");
        eu.game_id = Some("RMCP01".into());
        let three = AddonKey::GameId { id: "RMC".into() };
        assert_eq!(run(&three, Some("wii"), &[us, eu]), Resolution::Several(2));
        let key = AddonKey::TitleIds {
            ids: vec!["00050000101C9400".into(), "00050000101C9500".into()],
        };
        let mut a = rom(2, "wiiu", "Zelda.wua");
        a.title_id = Some("00050000101C9400".into());
        a.keeper = true;
        let mut b = rom(3, "wiiu", "Zelda.wux");
        b.title_id = Some("00050000101C9500".into());
        assert_eq!(
            run(&key, Some("wiiu"), &[a.clone(), b.clone()]),
            Resolution::Several(2)
        );
        // Two dumps with the same id are one game: the keeper.
        b.title_id = a.title_id.clone();
        assert_eq!(run(&key, Some("wiiu"), &[a, b]), one(2, LinkVia::Key, None));
    }

    #[test]
    fn a_readme_name_must_stand_alone() {
        let mut a = rom(0, "nes", "Contra (USA).nes");
        a.catalogue_name = Some("Contra (USA)".into());
        let list = [a, rom(1, "nes", "Game.nes")];
        let c = Candidates {
            list: &list,
            dat_by_crc: &no_dat,
            budget_out: false,
        };
        // Another dump of the same title is not this one.
        assert_eq!(
            resolve_by_readme("Patch Contra (USA) (Rev 1) only.\n", &c),
            Resolution::None
        );
        assert_eq!(
            resolve_by_readme("Apply to MyGame.nes or Game.nes2\n", &c),
            Resolution::None
        );
        assert_eq!(
            resolve_by_readme("Apply to \"Game.nes\".\n", &c),
            one(1, LinkVia::Readme, None)
        );
        assert_eq!(
            resolve_by_readme("Use Contra (USA), headerless.\n", &c),
            one(0, LinkVia::Readme, None)
        );
    }

    #[test]
    fn title_ids_link_when_one_listed_base_is_present() {
        let key = AddonKey::TitleIds {
            ids: vec!["00050000101C9400".into(), "00050000101C9500".into()],
        };
        let mut us = rom(0, "wiiu", "Zelda (USA).wua");
        us.title_id = Some("00050000101c9400".into());
        let other = rom(1, "wiiu", "Other (USA).wua");
        assert_eq!(
            run(&key, Some("wiiu"), &[other, us.clone()]),
            one(0, LinkVia::Key, None)
        );
        let mut eu = rom(2, "wiiu", "Zelda (Europe).wua");
        eu.title_id = Some("00050000101C9500".into());
        assert_eq!(run(&key, Some("wiiu"), &[us, eu]), Resolution::Several(2));
        let key = AddonKey::TitleId {
            id: "0004000000055D00".into(),
        };
        let mut t = rom(5, "3ds", "Game (USA).3ds");
        t.title_id = Some("0004000000055D00".into());
        assert_eq!(run(&key, Some("3ds"), &[t]), one(5, LinkVia::Key, None));
    }

    #[test]
    fn build_ids_link_only_through_their_title_id() {
        let mut s = rom(0, "switch", "Game.nsp");
        s.title_id = Some("0100000000010000".into());
        let list = [s];
        let lone = AddonKey::BuildId {
            build_id: "A1B2C3D4E5F60718".into(),
            title_id: None,
        };
        assert_eq!(run(&lone, Some("switch"), &list), Resolution::None);
        let with = AddonKey::BuildId {
            build_id: "A1B2C3D4E5F60718".into(),
            title_id: Some("0100000000010000".into()),
        };
        assert_eq!(
            run(&with, Some("switch"), &list),
            one(0, LinkVia::Key, None)
        );
    }

    #[test]
    fn n64_ids_and_names() {
        let mut a = rom(0, "n64", "Super Mario 64 (USA).z64");
        a.n64_id = Some("NSME".into());
        a.n64_name = Some("SUPER MARIO 64".into());
        let list = [a];
        // An APS10 header keeps the cart id and the country (0x3C-0x3E):
        // `SME` of the game code `NSME`, not its media letter.
        assert_eq!(
            run(&AddonKey::N64Id { id: "SME".into() }, Some("n64"), &list),
            one(0, LinkVia::Key, None)
        );
        assert_eq!(
            run(&AddonKey::N64Id { id: "NSM".into() }, Some("n64"), &list),
            Resolution::None
        );
        assert_eq!(
            run(&AddonKey::N64Id { id: "NSME".into() }, Some("n64"), &list),
            one(0, LinkVia::Key, None)
        );
        assert_eq!(
            run(
                &AddonKey::N64Name {
                    name: "SUPER MARIO 64".into()
                },
                Some("n64"),
                &list
            ),
            one(0, LinkVia::Key, None)
        );
    }

    #[test]
    fn many_and_keyless_never_link_by_key() {
        let list = [rom(0, "ps3", "Game.iso")];
        assert_eq!(
            run(&AddonKey::Many { count: 400 }, None, &list),
            Resolution::None
        );
        assert_eq!(run(&AddonKey::None, None, &list), Resolution::None);
    }

    #[test]
    fn a_keyless_patch_links_by_its_own_stem() {
        let list = [
            rom(0, "snes", "Game (USA).sfc"),
            rom(1, "snes", "Game (Japan).sfc"),
        ];
        let c = Candidates {
            list: &list,
            dat_by_crc: &no_dat,
            budget_out: false,
        };
        assert_eq!(
            resolve(&AddonKey::None, Some("snes"), "Game (Japan)", &c),
            one(1, LinkVia::Name, None)
        );
        assert_eq!(
            resolve(&AddonKey::None, Some("snes"), "Game", &c),
            Resolution::None
        );
    }

    #[test]
    fn budget_out_with_no_match_is_budget() {
        let list = [rom(0, "snes", "Other (USA).sfc")];
        let c = Candidates {
            list: &list,
            dat_by_crc: &no_dat,
            budget_out: true,
        };
        assert_eq!(
            resolve(&crc_key(), Some("snes"), "", &c),
            Resolution::Budget
        );
        // A hit is still a hit.
        let key = AddonKey::RomName {
            name: "Other (USA)".into(),
        };
        assert_eq!(
            resolve(&key, Some("snes"), "", &c),
            one(0, LinkVia::Name, None)
        );
    }

    #[test]
    fn readme_names_one_game_or_several() {
        let mut a = rom(0, "nes", "Contra (USA).nes");
        a.catalogue_name = Some("Contra (USA)".into());
        let mut b = rom(1, "nes", "Gradius (USA).nes");
        b.crc_headerless = Some(0x1A2B_3C4D);
        let list = [a, b];
        let c = Candidates {
            list: &list,
            dat_by_crc: &no_dat,
            budget_out: false,
        };
        assert_eq!(
            resolve_by_readme("Apply this patch to Contra (USA).nes\n", &c),
            one(0, LinkVia::Readme, None)
        );
        assert_eq!(
            resolve_by_readme("Use the No-Intro dump contra (usa)\n", &c),
            one(0, LinkVia::Readme, None)
        );
        assert_eq!(
            resolve_by_readme("ROM: headerless, CRC32: 1a2b3c4d\n", &c),
            one(1, LinkVia::Readme, None)
        );
        assert_eq!(
            resolve_by_readme("Contra (USA) and Gradius (USA).nes\n", &c),
            Resolution::Several(2)
        );
        assert_eq!(
            resolve_by_readme("A translation by someone.\n", &c),
            Resolution::None
        );
        // Eight hex digits not next to «CRC» are not a checksum.
        assert_eq!(
            resolve_by_readme("Version 1A2B3C4D\n", &c),
            Resolution::None
        );
    }
}
