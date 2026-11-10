//! 3DS region (and country) from the last letter of a product code's
//! 4-character game code (`CTR-P-AXXE`, `KTR-N-AB2P`), spec part C §5.
//!
//! Product code shape (3dbrew «Serials», «NCCH Product Code»):
//! `[CTR|KTR]-[category]-[type][id][region]-[sub id]`, the 2-digit sub id
//! only on DLC, demos and Download Play copies. `CTR-P-CTAP` is the generic
//! default of secondary NCCHs and carries no region.
//!
//! Verified 2026-11-09 against:
//! - 3dbrew, «Serials»: https://www.3dbrew.org/wiki/Serials
//!   (E English/US, P PAL, J Japan, K Korea, C China/Taiwan, W Taiwan/Hong
//!   Kong, Y and Z multiple regions, A region-free).
//! - GameTDB 3DS database, https://www.gametdb.com/3dstdb.zip (snapshot
//!   2026-11-09): last letter vs `<region>` of every 4-character id
//!   (E 1304 NTSC-U, P 1302 PAL, J NTSC-J, K NTSC-K, W 29 NTSC-T, A 68 none,
//!   D 33 PAL, F 83 PAL, S 10 PAL, I 11 PAL, H 2 PAL (Dutch), R 1 PAL-R,
//!   V 6 PAL, X 14 PAL, Y 32/33 PAL, Z 170/180 PAL, C 3 untagged; no id ends
//!   in T, U, B, G, L, M, N, O or Q).
//! - WiiBrew, «Title database», region codes:
//!   https://wiibrew.org/wiki/Title_database (D German-, F French-,
//!   I Italian-, S Spanish-speaking; T Korea with English; W Taiwan/HK).
//!
//! A letter not attested for 3DS is `Unknown` and flagged, never guessed.

use serde::{Deserialize, Serialize};

use crate::identify::names::Region;

/// How sure the table is about a letter (spec §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Certainty {
    /// Both sources agree; may be written into a name.
    Certain,
    /// Mostly one region; shown in the panel only.
    Likely,
    /// Not attested for 3DS; flagged, never used.
    Unknown,
}

/// What the region letter of a 3DS product code says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionVerdict {
    pub letter: char,
    /// The product code as read, trimmed and upper-cased.
    pub code: String,
    pub region: Option<Region>,
    pub country: Option<Region>,
    pub certainty: Certainty,
}

impl RegionVerdict {
    /// What a name or `ConsoleId.region` may use: `country.or(region)` when
    /// `Certain`, else nothing.
    pub fn for_name(&self) -> Option<Region> {
        (self.certainty == Certainty::Certain)
            .then(|| self.country.or(self.region))
            .flatten()
    }
}

/// The region verdict of a 3DS product code, `None` for a malformed code or
/// the generic `CTR-P-CTAP`.
pub fn ctr_region(code: &str) -> Option<RegionVerdict> {
    let code = code.trim().to_ascii_uppercase();
    let b = code.as_bytes();
    // [CK]TR-<category>-<4-character game code>[-<2-digit sub id>]
    let sub_id_ok = match b.len() {
        10 => true,
        13 => b[10] == b'-' && b[11..].iter().all(u8::is_ascii_digit),
        _ => false,
    };
    let ok = sub_id_ok
        && matches!(&b[..4], b"CTR-" | b"KTR-")
        && b[4].is_ascii_alphabetic()
        && b[5] == b'-'
        && b[6..10].iter().all(u8::is_ascii_alphanumeric);
    if !ok || &code[..10] == "CTR-P-CTAP" {
        return None;
    }
    let letter = b[9] as char;
    use {Certainty::*, Region::*};
    let (region, country, certainty) = match letter {
        'E' => (Some(Usa), None, Certain),
        'P' => (Some(Europe), None, Certain),
        'J' => (Some(Japan), None, Certain),
        'K' => (Some(Korea), None, Certain),
        'W' => (Some(Taiwan), None, Certain),
        'A' => (Some(World), None, Certain),
        'D' => (Some(Europe), Some(Germany), Certain),
        'F' => (Some(Europe), Some(France), Certain),
        'S' => (Some(Europe), Some(Spain), Certain),
        'I' => (Some(Europe), Some(Italy), Certain),
        'H' => (Some(Europe), Some(Netherlands), Certain),
        'R' => (Some(Europe), Some(Russia), Certain),
        'V' | 'X' => (Some(Europe), None, Certain),
        'Y' | 'Z' => (Some(Europe), None, Likely),
        'C' => (Some(China), None, Likely),
        _ => (None, None, Unknown),
    };
    Some(RegionVerdict {
        letter,
        code,
        region,
        country,
        certainty,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use Certainty::*;
    use Region::*;

    #[test]
    fn every_row_of_the_verified_table() {
        let rows: &[(char, Option<Region>, Option<Region>, Certainty)] = &[
            ('E', Some(Usa), None, Certain),
            ('P', Some(Europe), None, Certain),
            ('J', Some(Japan), None, Certain),
            ('K', Some(Korea), None, Certain),
            ('W', Some(Taiwan), None, Certain),
            ('A', Some(World), None, Certain),
            ('D', Some(Europe), Some(Germany), Certain),
            ('F', Some(Europe), Some(France), Certain),
            ('S', Some(Europe), Some(Spain), Certain),
            ('I', Some(Europe), Some(Italy), Certain),
            ('H', Some(Europe), Some(Netherlands), Certain),
            ('R', Some(Europe), Some(Russia), Certain),
            ('V', Some(Europe), None, Certain),
            ('X', Some(Europe), None, Certain),
            ('Y', Some(Europe), None, Likely),
            ('Z', Some(Europe), None, Likely),
            ('C', Some(China), None, Likely),
        ];
        for &(letter, region, country, certainty) in rows {
            let code = format!("CTR-P-AXX{letter}");
            let v = ctr_region(&code).unwrap();
            assert_eq!(
                (v.letter, v.region, v.country, v.certainty),
                (letter, region, country, certainty),
                "{code}"
            );
        }
    }

    #[test]
    fn letters_not_attested_for_3ds_are_unknown() {
        for letter in ['T', 'U', 'B', 'G', 'L', 'M', 'N', 'O', 'Q', '1'] {
            let v = ctr_region(&format!("CTR-P-AXX{letter}")).unwrap();
            assert_eq!(
                (v.region, v.country, v.certainty),
                (None, None, Unknown),
                "{letter}"
            );
            assert_eq!(v.letter, letter);
        }
    }

    #[test]
    fn shapes() {
        assert_eq!(ctr_region("KTR-N-AB2S").unwrap().country, Some(Spain));
        assert_eq!(ctr_region(" ctr-p-axxe ").unwrap().region, Some(Usa));
        assert_eq!(ctr_region(" ctr-p-axxe ").unwrap().code, "CTR-P-AXXE");
        assert_eq!(ctr_region("CTR-T-AXXP").unwrap().region, Some(Europe)); // eShop demo
        assert_eq!(ctr_region("CTR-P-CTAP"), None); // generic default of secondary NCCHs
        for bad in [
            "",
            "CTR-P-",
            "CTR-P-AXX",
            "WUP-P-AXXE",
            "CTR-PAXXE",
            "CTR-P-AXXE-X",
            "CTR-1-AXXE",
            "CTR-P-AX-E",
            "CTR-P-AXXE-1",
            "CTR-P-AXXE-0A",
            "CTR-P-CTAP-00",
        ] {
            assert_eq!(ctr_region(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_two_digit_sub_id_keeps_the_region() {
        // 3dbrew: the sub id follows the game code on DLC and demos.
        let v = ctr_region("CTR-T-AXXP-01").unwrap();
        assert_eq!((v.letter, v.region), ('P', Some(Europe)));
        assert_eq!(v.code, "CTR-T-AXXP-01");
    }

    #[test]
    fn only_certain_verdicts_name_a_region_and_the_country_wins() {
        assert_eq!(ctr_region("CTR-P-AXXS").unwrap().for_name(), Some(Spain));
        assert_eq!(ctr_region("CTR-P-AXXP").unwrap().for_name(), Some(Europe));
        assert_eq!(ctr_region("CTR-P-AXXZ").unwrap().for_name(), None);
        assert_eq!(ctr_region("CTR-P-AXXC").unwrap().for_name(), None);
        assert_eq!(ctr_region("CTR-P-AXXT").unwrap().for_name(), None);
    }

    #[test]
    fn certainty_is_kebab_case_on_the_wire() {
        assert_eq!(serde_json::to_string(&Certain).unwrap(), "\"certain\"");
        assert_eq!(serde_json::to_string(&Unknown).unwrap(), "\"unknown\"");
    }
}
