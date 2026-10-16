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
                    if open == '(' {
                        groups.push(current.trim().to_string());
                    }
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
