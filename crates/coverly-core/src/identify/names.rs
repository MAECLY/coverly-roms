//! ROM and art name parsing, normalisation and fuzzy scoring
//! (spec §16.4 and §16.7, after Skyscraper's getCompareTitle /
//! getUrlQueryName, but on Unicode characters).

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Region {
    World,
    Usa,
    Europe,
    Japan,
    Asia,
    Australia,
    Brazil,
    Canada,
    China,
    France,
    Germany,
    HongKong,
    Italy,
    Korea,
    Netherlands,
    Spain,
    Sweden,
    Taiwan,
    UnitedKingdom,
}

impl Region {
    pub fn from_no_intro(word: &str) -> Option<Region> {
        use Region::*;
        Some(match word.trim().to_lowercase().as_str() {
            "world" => World,
            "usa" => Usa,
            "europe" => Europe,
            "japan" => Japan,
            "asia" => Asia,
            "australia" => Australia,
            "brazil" => Brazil,
            "canada" => Canada,
            "china" => China,
            "france" => France,
            "germany" => Germany,
            "hong kong" => HongKong,
            "italy" => Italy,
            "korea" => Korea,
            "netherlands" => Netherlands,
            "spain" => Spain,
            "sweden" => Sweden,
            "taiwan" => Taiwan,
            "uk" | "united kingdom" => UnitedKingdom,
            _ => return None,
        })
    }

    pub fn from_goodtools(code: char) -> Option<Region> {
        use Region::*;
        Some(match code {
            'W' => World,
            'U' => Usa,
            'E' => Europe,
            'J' => Japan,
            'A' => Australia,
            'B' => Brazil,
            'C' => China,
            'F' => France,
            'G' => Germany,
            'H' => Netherlands,
            'I' => Italy,
            'K' => Korea,
            'S' => Spain,
            _ => return None,
        })
    }

    pub fn parse(s: &str) -> Option<Region> {
        match s.trim().to_lowercase().as_str() {
            "us" => Some(Region::Usa),
            "eu" => Some(Region::Europe),
            "jp" | "jpn" => Some(Region::Japan),
            other => Region::from_no_intro(other),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedName {
    pub title: String,
    pub regions: Vec<Region>,
    pub year: Option<u16>,
    pub numeral: u32,
    pub key: String,
    pub short_key: String,
}

pub fn parse_name(stem: &str, strip_numbering: bool) -> ParsedName {
    let nfc: String = stem.nfc().collect();
    let base = if strip_numbering {
        strip_numbering_prefix(&nfc)
    } else {
        nfc.as_str()
    };
    let (outside, groups) = split_groups(&base.replace('_', " "));
    let mut regions: Vec<Region> = Vec::new();
    let mut year = None;
    for group in &groups {
        if let Some(found) = region_group(group) {
            for r in found {
                if !regions.contains(&r) {
                    regions.push(r);
                }
            }
        } else if let Some(y) = year_group(group) {
            year.get_or_insert(y);
        }
    }
    let title = tidy_title(&outside);
    let key = compare_key(&title);
    let short_key = compare_key(&without_subtitle(&title));
    let numeral = numeral_of(&key);
    ParsedName {
        title,
        regions,
        year,
        numeral,
        key,
        short_key,
    }
}

/// Text outside `()`, `[]` and `{}`, and the contents of top-level `()`
/// groups. An unclosed bracket is kept as text.
fn split_groups(s: &str) -> (String, Vec<String>) {
    let (outside, groups) = split_all_groups(s);
    let groups = groups
        .into_iter()
        .filter(|(open, _)| *open == '(')
        .map(|(_, g)| g)
        .collect();
    (outside, groups)
}

/// Text outside `()`, `[]` and `{}`, and every top-level group with its
/// opening bracket, in order. An unclosed bracket is kept as text.
fn split_all_groups(s: &str) -> (String, Vec<(char, String)>) {
    let mut outside = String::new();
    let mut groups = Vec::new();
    let mut stack: Vec<char> = Vec::new();
    let mut current = String::new();
    let mut pending = String::new(); // raw text of the open group, for unclosed brackets
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => {
                stack.push(c);
                pending.push(c);
                if stack.len() > 1 {
                    current.push(c);
                }
            }
            ')' | ']' | '}' if !stack.is_empty() => {
                let open = stack.pop().unwrap();
                pending.push(c);
                if stack.is_empty() {
                    groups.push((open, current.trim().to_string()));
                    current.clear();
                    pending.clear();
                } else {
                    current.push(c);
                }
            }
            _ if !stack.is_empty() => {
                current.push(c);
                pending.push(c);
            }
            _ => outside.push(c),
        }
    }
    outside.push_str(&pending);
    (outside, groups)
}

/// No-Intro language codes (as they appear: `En`, `Fr`…).
const LANGUAGE_CODES: [&str; 26] = [
    "En", "Fr", "De", "Es", "It", "Nl", "Pt", "Sv", "No", "Da", "Fi", "Ja", "Zh", "Ko", "Pl", "Ru",
    "Ca", "Eu", "Gd", "Cs", "Hu", "El", "Tr", "Ar", "He", "Hr",
];

/// The languages of a No-Intro name's `(En,Fr,De)` group(s), lowercase, in
/// order. A group counts only when every comma-separated token is a code.
pub fn languages(name: &str) -> Vec<String> {
    let (_, groups) = split_groups(name);
    let mut out: Vec<String> = Vec::new();
    for group in &groups {
        let tokens: Vec<&str> = group.split(',').map(str::trim).collect();
        if !tokens.is_empty() && tokens.iter().all(|t| LANGUAGE_CODES.contains(t)) {
            for t in tokens {
                let code = t.to_ascii_lowercase();
                if !out.contains(&code) {
                    out.push(code);
                }
            }
        }
    }
    out
}

fn region_group(group: &str) -> Option<Vec<Region>> {
    let parts: Vec<&str> = group.split(',').map(str::trim).collect();
    if let Some(all) = parts
        .iter()
        .map(|p| Region::from_no_intro(p))
        .collect::<Option<Vec<_>>>()
    {
        return Some(all);
    }
    let g = group.trim();
    if (1..=4).contains(&g.len()) && g.chars().all(|c| c.is_ascii_uppercase()) {
        return g.chars().map(Region::from_goodtools).collect();
    }
    None
}

fn year_group(group: &str) -> Option<u16> {
    let g = group.trim();
    (g.len() == 4 && g.chars().all(|c| c.is_ascii_digit()))
        .then(|| g.parse::<u16>().ok())
        .flatten()
        .filter(|y| (1970..=2099).contains(y))
}

/// Collapses spaces, trims separators and moves ", The" to the front.
fn tidy_title(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title = collapsed
        .trim_matches(|c: char| c == '-' || c == ' ' || c == ',')
        .to_string();
    // ASCII lower-casing keeps byte offsets identical, so `i` indexes `title` safely.
    let lower = title.to_ascii_lowercase();
    if let Some(i) = lower.find(", the") {
        let after = &title[i + 5..];
        if after.is_empty() || after.starts_with(" -") || after.starts_with(':') {
            title = format!("The {}{}", &title[..i], after);
        }
    }
    title
}

/// Case-, accent- and punctuation-insensitive key with roman numerals as
/// arabic numbers and a leading "the" dropped.
pub fn compare_key(title: &str) -> String {
    let folded: String = title
        .nfd()
        .filter(|c| !('\u{0300}'..='\u{036F}').contains(c))
        .nfc()
        .collect::<String>()
        .to_lowercase()
        .replace('&', " and ");
    let mut cleaned = String::with_capacity(folded.len());
    for c in folded.chars() {
        if c.is_alphanumeric() {
            cleaned.push(c);
        } else if c != '\'' && c != '\u{2019}' {
            cleaned.push(' ');
        }
    }
    let mut tokens: Vec<String> = cleaned.split_whitespace().map(str::to_string).collect();
    if tokens.len() > 1 && tokens[0] == "the" {
        tokens.remove(0);
    }
    let last = tokens.len().saturating_sub(1);
    for (i, tok) in tokens.iter_mut().enumerate() {
        if let Some(n) = roman(tok, i == last && i > 0) {
            *tok = n.to_string();
        }
    }
    tokens.join(" ")
}

/// II..XX anywhere; single-letter V and X only as the last word.
fn roman(tok: &str, single_ok: bool) -> Option<u32> {
    const R: [(&str, u32); 19] = [
        ("ii", 2),
        ("iii", 3),
        ("iv", 4),
        ("v", 5),
        ("vi", 6),
        ("vii", 7),
        ("viii", 8),
        ("ix", 9),
        ("x", 10),
        ("xi", 11),
        ("xii", 12),
        ("xiii", 13),
        ("xiv", 14),
        ("xv", 15),
        ("xvi", 16),
        ("xvii", 17),
        ("xviii", 18),
        ("xix", 19),
        ("xx", 20),
    ];
    let n = R.iter().find(|(r, _)| *r == tok)?.1;
    (tok.len() > 1 || single_ok).then_some(n)
}

/// The last 1–2 digit number after the first word; 1 when none.
fn numeral_of(key: &str) -> u32 {
    key.split(' ')
        .skip(1)
        .filter(|t| (1..=2).contains(&t.len()) && t.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|t| t.parse().ok())
        .last()
        .unwrap_or(1)
}

pub fn without_subtitle(title: &str) -> String {
    let cut = [": ", " - ", " ~ "]
        .iter()
        .filter_map(|sep| title.find(sep))
        .min();
    match cut {
        Some(i) if title[..i].trim().chars().count() > 3 => title[..i].trim().to_string(),
        _ => title.to_string(),
    }
}

pub fn score(a: &str, b: &str) -> u8 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let l = a.len().max(b.len());
    if l == 0 {
        return 100;
    }
    (100 * (l - levenshtein(&a, &b)) / l) as u8
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j + 1] + 1)
                .min(cur[j] + 1)
                .min(prev[j] + usize::from(ca != cb));
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

pub fn compatible(rom: &ParsedName, candidate: &ParsedName) -> bool {
    rom.numeral == candidate.numeral
        && match (rom.year, candidate.year) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
}

/// Strips `NNN `, `NN) `, `N. ` or `N - ` (1–4 digits) when text follows.
pub fn strip_numbering_prefix(stem: &str) -> &str {
    let digits = stem.chars().take_while(|c| c.is_ascii_digit()).count();
    if !(1..=4).contains(&digits) {
        return stem;
    }
    let rest = &stem[digits..];
    let rest = rest
        .strip_prefix(')')
        .or_else(|| rest.strip_prefix('.'))
        .or_else(|| rest.trim_start().strip_prefix("- "))
        .unwrap_or(rest);
    if !rest.starts_with(' ') && rest.len() == stem.len() - digits {
        return stem; // "1942", "10yard"
    }
    let rest = rest.trim_start();
    if rest.is_empty() {
        stem
    } else {
        rest
    }
}

/// Whether a folder numbers its ROMs: at least 3 stems and 60 % of them.
pub fn detect_numbering<'a>(stems: impl IntoIterator<Item = &'a str>) -> bool {
    let (mut total, mut numbered) = (0usize, 0usize);
    for s in stems {
        total += 1;
        if strip_numbering_prefix(s) != s {
            numbered += 1;
        }
    }
    numbered >= 3 && numbered * 10 >= total * 6
}

pub fn sanitise_libretro(name: &str) -> String {
    name.chars()
        .map(|c| if "&*/:`\"<>?\\|".contains(c) { '_' } else { c })
        .collect()
}

pub fn libretro_key(file_name: &str) -> String {
    let lower = file_name.nfc().collect::<String>().to_lowercase();
    lower
        .strip_suffix(".png")
        .map(str::to_string)
        .unwrap_or(lower)
}

// ---------------------------------------------------------------------------
// Catalogue name tags (duplicates spec §5.3, §12.2). Used on DAT game names
// for level 2 grouping and keeper rules; `ParsedName` is unaffected.

/// A comparable revision: GoodTools `V1.1` → (1,1,0), No-Intro `Rev 1` →
/// (1,0,0), `Rev A` → (0,0,b'A'), `v1.02` → (1,2,0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Revision {
    pub major: u16,
    pub minor: u16,
    pub letter: u8,
}

/// Release class; the derived order is the preference order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseClass {
    Retail,
    Beta,
    Proto,
    Demo,
    Sample,
}

/// GoodTools / No-Intro dump-quality flags: `[!] [b] [o] [a]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpFlags {
    pub verified: bool,
    pub bad: bool,
    pub overdump: bool,
    pub alternate: bool,
}

/// Whether the program itself was modified. Not a dump quality: a hack or
/// translation is a different product and stays in the title key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModClass {
    Original,
    Hack,
    Trainer,
    Translation,
    Fixed,
    Pirate,
    Unlicensed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameTags {
    /// `compare_key` of the title plus the disc number, unknown groups and
    /// modification tags; known groups are stripped.
    pub title_key: String,
    pub revision: Option<Revision>,
    /// The revision as written (`V1.1`, `Rev 1`), for `KeepReason::Revision`.
    pub revision_text: Option<String>,
    pub class: ReleaseClass,
    pub flags: DumpFlags,
    pub modification: ModClass,
    pub disc: Option<u8>,
    pub regions: Vec<Region>,
    pub unknown_groups: Vec<String>,
}

/// Parses the tags of a catalogue name (`Tetris (World) (Rev 1)`).
pub fn parse_tags(name: &str) -> NameTags {
    let nfc: String = name.nfc().collect();
    let (outside, groups) = split_all_groups(&nfc.replace('_', " "));
    let mut tags = NameTags {
        title_key: String::new(),
        revision: None,
        revision_text: None,
        class: ReleaseClass::Retail,
        flags: DumpFlags::default(),
        modification: ModClass::Original,
        disc: None,
        regions: Vec::new(),
        unknown_groups: Vec::new(),
    };
    let mut kept: Vec<String> = Vec::new();
    let set_mod = |tags: &mut NameTags, class: ModClass| {
        if tags.modification == ModClass::Original {
            tags.modification = class;
        }
    };
    for (open, group) in &groups {
        let g = group.trim();
        if g.is_empty() {
            continue;
        }
        if *open == '[' {
            if let Some(flag) = dump_flag(g) {
                match flag {
                    'v' => tags.flags.verified = true,
                    'b' => tags.flags.bad = true,
                    'o' => tags.flags.overdump = true,
                    _ => tags.flags.alternate = true,
                }
                continue;
            }
            if let Some(class) = goodtools_modification(g) {
                set_mod(&mut tags, class);
                kept.push(g.to_string());
                continue;
            }
        }
        if *open == '(' {
            if let Some(found) = region_group(g) {
                for r in found {
                    if !tags.regions.contains(&r) {
                        tags.regions.push(r);
                    }
                }
                continue;
            }
            if is_language_group(g) || is_date_group(g) || is_known_edition(g) {
                continue;
            }
            if let Some(class) = release_class(g) {
                if tags.class == ReleaseClass::Retail {
                    tags.class = class;
                }
                continue;
            }
            if let Some(r) = revision_group(g) {
                if tags.revision.is_none() {
                    tags.revision = Some(r);
                    tags.revision_text = Some(g.to_string());
                }
                continue;
            }
            if let Some(n) = disc_group(g) {
                tags.disc.get_or_insert(n);
                kept.push(format!("disc {n}"));
                continue;
            }
            if let Some(class) = word_modification(g) {
                set_mod(&mut tags, class);
                kept.push(g.to_string());
                continue;
            }
        }
        tags.unknown_groups.push(g.to_string());
        kept.push(g.to_string());
    }
    let mut parts = vec![compare_key(&tidy_title(&outside))];
    parts.extend(
        kept.iter()
            .map(|g| compare_key(g))
            .filter(|k| !k.is_empty()),
    );
    tags.title_key = parts.join(" ").trim().to_string();
    tags
}

/// Whether a file or catalogue name marks a patched program: GoodTools
/// `[T+…]`, `[T-…]`, `[h…]`, `[t…]`, `[f…]`, or the words Hack, Translated,
/// Traducido, Undub, Patched. Names may only veto a group, never form one.
pub fn has_modification_tag(name: &str) -> bool {
    let (_, groups) = split_all_groups(name);
    let bracket = groups.iter().any(|(open, g)| {
        *open == '['
            && matches!(
                goodtools_modification(g.trim()),
                Some(ModClass::Hack | ModClass::Trainer | ModClass::Translation | ModClass::Fixed)
            )
    });
    bracket
        || name
            .split(|c: char| !c.is_alphanumeric())
            .map(str::to_lowercase)
            .any(|w| {
                matches!(
                    w.as_str(),
                    "hack"
                        | "hacked"
                        | "translated"
                        | "translation"
                        | "traducido"
                        | "traduccion"
                        | "traducción"
                        | "undub"
                        | "undubbed"
                        | "patched"
                )
            })
}

/// `!` → 'v'; `b`, `b1`, `o`, `o2`, `a`, `a1` → that letter.
fn dump_flag(g: &str) -> Option<char> {
    if g == "!" {
        return Some('v');
    }
    let mut chars = g.chars();
    let first = chars.next()?;
    let rest = chars.as_str();
    (matches!(first, 'b' | 'o' | 'a') && rest.chars().all(|c| c.is_ascii_digit())).then_some(first)
}

/// GoodTools modification flags: `[h1]`, `[h2C]`, `[t1]`, `[T+Eng]`,
/// `[T-Spa]`, `[f1]`, `[p1]`.
fn goodtools_modification(g: &str) -> Option<ModClass> {
    if g.starts_with("T+") || g.starts_with("T-") {
        return Some(ModClass::Translation);
    }
    let mut chars = g.chars();
    let first = chars.next()?;
    let rest = chars.as_str();
    let numbered = rest.is_empty() || rest.starts_with(|c: char| c.is_ascii_digit());
    match first {
        'h' if numbered || rest.starts_with(|c: char| c.is_ascii_uppercase()) || rest == "ack" => {
            Some(ModClass::Hack)
        }
        't' if numbered => Some(ModClass::Trainer),
        'f' if numbered || rest.starts_with('_') => Some(ModClass::Fixed),
        'p' if numbered => Some(ModClass::Pirate),
        _ => None,
    }
}

/// `(Hack)`, `(Translated En)`, `(Unl)`, `(Pirate)`.
fn word_modification(g: &str) -> Option<ModClass> {
    let lower = g.to_lowercase();
    let first = lower.split_whitespace().next()?;
    match first {
        "hack" | "hacked" => Some(ModClass::Hack),
        "translated" | "translation" | "traducido" | "trad" => Some(ModClass::Translation),
        "unl" | "unlicensed" => Some(ModClass::Unlicensed),
        "pirate" => Some(ModClass::Pirate),
        "patched" | "fixed" => Some(ModClass::Fixed),
        _ => None,
    }
}

fn is_language_group(g: &str) -> bool {
    let tokens: Vec<&str> = g.split(',').map(str::trim).collect();
    !tokens.is_empty() && tokens.iter().all(|t| LANGUAGE_CODES.contains(t))
}

/// `1999`, `1999-04`, `1999-04-01`.
fn is_date_group(g: &str) -> bool {
    let parts: Vec<&str> = g.split('-').collect();
    year_group(parts[0]).is_some()
        && parts.len() <= 3
        && parts[1..]
            .iter()
            .all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_digit()))
}

fn is_known_edition(g: &str) -> bool {
    matches!(
        g.to_lowercase().as_str(),
        "sgb enhanced" | "gb compatible" | "rumble version" | "virtual console"
    )
}

/// `Beta`, `Beta 2`, `Proto`, `Prototype 2`, `Demo`, `Kiosk`, `Sample`.
fn release_class(g: &str) -> Option<ReleaseClass> {
    let lower = g.to_lowercase();
    let mut words = lower.split_whitespace();
    let class = match words.next()? {
        "beta" => ReleaseClass::Beta,
        "proto" | "prototype" => ReleaseClass::Proto,
        "demo" | "kiosk" => ReleaseClass::Demo,
        "sample" => ReleaseClass::Sample,
        _ => return None,
    };
    let rest: Vec<&str> = words.collect();
    let ok = match rest.as_slice() {
        [] => true,
        ["demo"] => class == ReleaseClass::Demo,
        [n] => n.chars().all(|c| c.is_ascii_digit()),
        _ => false,
    };
    ok.then_some(class)
}

/// `Rev 1`, `Rev A`, `Rev 1.1`, `V1.1`, `v1.02`.
fn revision_group(g: &str) -> Option<Revision> {
    let lower = g.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("rev") {
        let rest = rest.trim_start();
        if rest.len() == 1 && rest.chars().all(|c| c.is_ascii_lowercase()) {
            return Some(Revision {
                major: 0,
                minor: 0,
                letter: rest.as_bytes()[0].to_ascii_uppercase(),
            });
        }
        return version_number(rest);
    }
    let rest = lower.strip_prefix('v')?.trim_start();
    rest.starts_with(|c: char| c.is_ascii_digit())
        .then(|| version_number(rest))
        .flatten()
}

/// `1`, `1.1`, `1.02`, `1A`.
fn version_number(s: &str) -> Option<Revision> {
    let (num, letter) = match s.chars().last()? {
        c if c.is_ascii_alphabetic() => (&s[..s.len() - 1], c.to_ascii_uppercase() as u8),
        _ => (s, 0),
    };
    let mut parts = num.split('.');
    let major = parts.next()?;
    let minor = parts.next().unwrap_or("0");
    if parts.next().is_some()
        || major.is_empty()
        || minor.is_empty()
        || !major
            .chars()
            .chain(minor.chars())
            .all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(Revision {
        major: major.parse().ok()?,
        minor: minor.parse().ok()?,
        letter,
    })
}

/// `Disc 2`, `Disc 3 of 4`, `Disk 1`.
fn disc_group(g: &str) -> Option<u8> {
    let lower = g.to_lowercase();
    let mut words = lower.split_whitespace();
    if !matches!(words.next()?, "disc" | "disk" | "cd") {
        return None;
    }
    let n: u8 = words.next()?.parse().ok()?;
    match (words.next(), words.next(), words.next()) {
        (None, None, None) => Some(n),
        (Some("of"), Some(m), None) if m.parse::<u8>().is_ok() => Some(n),
        _ => None,
    }
}
