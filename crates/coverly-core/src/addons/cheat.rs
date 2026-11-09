//! Cheats and emulator patches (mods spec D3): told by their name and a
//! capped look at their text, with the key their name or text carries.
//!
//! Everything here is pure: the caller reads at most 1 MiB of a file as
//! UTF-8 lossy and passes it in. Text is only scanned line by line, never
//! executed or fully parsed. Global cheat databases (`usrcheat.dat`,
//! `cheat.db` …) are system data: [`is_global_cheat_db`] recognises them
//! and [`sniff_cheat`] never takes them for one game's cheat (D7).

use std::collections::BTreeSet;

use super::{one_component, valid_build_id, valid_game_id, valid_sony_serial, valid_title_id16};
use super::{AddonFormat, AddonKey};

/// Global cheat databases, compared without case (D3, D7).
const GLOBAL_DBS: [&str; 5] = [
    "usrcheat.dat",
    "cheat.db",
    "mupencheat.txt",
    "cheats_json.txt",
    "patch_config.yml",
];

/// RPCS3 files holding patches for many games, line-scanned for serials.
const RPCS3_SHARED: [&str; 3] = ["patch.yml", "imported_patch.yml", "cheats.yml"];

/// Extensions a cheat or emulator patch may have (the walk pre-filter).
const CHEAT_EXTS: [&str; 13] = [
    "cht", "pnach", "ini", "txt", "yml", "json", "xml", "toml", "patch", "pchtxt", "mch", "cheats",
    "dct",
];

/// Game Genie letters (NES).
const GG_LETTERS: &str = "APZLGITYEOXUKSVN";

/// `usrcheat.dat`, `cheat.db`, `mupencheat.txt`, `CHEATS_JSON.txt`,
/// `patch_config.yml` (case-insensitive): system data (D7).
pub fn is_global_cheat_db(name: &str) -> bool {
    GLOBAL_DBS.iter().any(|db| name.eq_ignore_ascii_case(db))
}

/// Name patterns cheap enough to test before reading any text (walk
/// pre-filter): `true` when [`sniff_cheat`] may say yes for this name.
pub fn may_be_cheat(name: &str) -> bool {
    ext_of(name).is_some_and(|ext| CHEAT_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

/// D3. `name` = file name; `parents` = up to two parent folder names,
/// nearest first; `text` = capped UTF-8 lossy (≤ 1 MiB).
/// `Some((AddonFormat::…, key))` for a cheat file; `None` for anything
/// else, global databases included.
pub fn sniff_cheat(name: &str, parents: &[&str], text: &str) -> Option<(AddonFormat, AddonKey)> {
    let name = one_component(name)?;
    if is_global_cheat_db(name) {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".patch.toml") {
        let stem = &name[..name.len() - ".patch.toml".len()];
        let key = xenia_title(stem)
            .or_else(|| xenia_title_line(text))
            .map_or(AddonKey::None, |id| AddonKey::TitleId { id });
        return Some((AddonFormat::XeniaPatch, key));
    }
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    let rom_name = || AddonKey::RomName {
        name: stem.to_string(),
    };
    match ext.to_ascii_lowercase().as_str() {
        "cht" => cht(stem, text),
        "pnach" => Some((AddonFormat::Pnach, pnach_key(stem, text))),
        "ini" => ini(stem, text),
        "txt" => txt(&lower, stem, parents, text),
        "pchtxt" => pchtxt(parents, text),
        "yml" => rpcs3(&lower, stem, text),
        "json" => shadps4_json(stem),
        "xml" => shadps4_xml(text),
        "patch" => (stem.len() == 8 && all_hex(stem)).then(|| {
            let id = stem.to_ascii_uppercase();
            (AddonFormat::XeniaPatch, AddonKey::TitleId { id })
        }),
        "mch" => Some((AddonFormat::MelonDsCheats, rom_name())),
        "cheats" => Some((AddonFormat::MgbaCheats, rom_name())),
        "dct" => Some((AddonFormat::DesmumeCheats, rom_name())),
        _ => None,
    }
}

fn ext_of(name: &str) -> Option<&str> {
    name.rsplit_once('.')
        .map(|(_, e)| e)
        .filter(|e| !e.is_empty())
}

fn all_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The trimmed lines of `text` that are not empty and not a comment.
fn code_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|l| {
        !l.is_empty() && !l.starts_with('#') && !l.starts_with(';') && !l.starts_with("//")
    })
}

/// `key = value` with `key` compared without case: the trimmed value.
fn assignment<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let (k, v) = line.split_once('=')?;
    k.trim().eq_ignore_ascii_case(key).then(|| v.trim())
}

/// A `[Name]` or `{Name}` section line.
fn is_section(line: &str) -> bool {
    (line.starts_with('[') && line.ends_with(']') || line.starts_with('{') && line.ends_with('}'))
        && line.len() > 2
}

/// Words of eight hex digits, as in 3DS and Switch cheat lines.
fn is_hex_words(line: &str) -> bool {
    let mut words = line.split_whitespace().peekable();
    words.peek().is_some() && words.all(|w| w.len() == 8 && all_hex(w))
}

/// A Sony product id of four letters and five digits without a dash
/// (`BLUS30001`, `CUSA07023`, `PCSE00001`), upper-cased.
fn compact_serial(s: &str) -> Option<String> {
    let b = s.as_bytes();
    (b.len() == 9
        && b[..4].iter().all(u8::is_ascii_alphabetic)
        && b[4..].iter().all(u8::is_ascii_digit))
    .then(|| s.to_ascii_uppercase())
}

/// A Switch title id (`0100…`), upper-cased.
fn switch_title(s: &str) -> Option<String> {
    valid_title_id16(s).filter(|id| id.starts_with("01"))
}

fn cht(stem: &str, text: &str) -> Option<(AddonFormat, AddonKey)> {
    if code_lines(text).any(|l| assignment(l, "cheats").is_some()) {
        let key = AddonKey::RomName {
            name: stem.to_string(),
        };
        return Some((AddonFormat::RetroArchCht, key));
    }
    let serial = valid_sony_serial(stem).or_else(|| {
        let (serial, hash) = stem.rsplit_once('-')?;
        (hash.len() >= 8 && all_hex(hash)).then(|| valid_sony_serial(serial))?
    });
    if let Some(serial) = serial {
        let sections = code_lines(text).any(is_section);
        let typed = code_lines(text).any(|l| assignment(l, "type").is_some());
        return (sections && typed)
            .then_some((AddonFormat::DuckStationCht, AddonKey::Serial { serial }));
    }
    let mut lines = code_lines(text).peekable();
    (lines.peek().is_some() && lines.all(is_fceux_line)).then(|| {
        (
            AddonFormat::FceuxCht,
            AddonKey::RomName {
                name: stem.to_string(),
            },
        )
    })
}

/// FCEUX: `[S|C|SC]:ADDR:VAL[:name]` or `0xADDR:VAL`.
fn is_fceux_line(line: &str) -> bool {
    let mut parts: Vec<&str> = line.split(':').collect();
    if parts
        .first()
        .is_some_and(|p| p.chars().all(|c| matches!(c, 'S' | 'C' | 's' | 'c')))
    {
        parts.remove(0);
    }
    let hex = |s: &str, max: usize| {
        let s = s.trim();
        let s = s
            .strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .unwrap_or(s);
        s.len() <= max && all_hex(s)
    };
    parts.len() >= 2 && hex(parts[0], 6) && hex(parts[1], 2)
}

fn pnach_key(stem: &str, text: &str) -> AddonKey {
    if let Some(serial) = stem
        .rsplit_once('_')
        .filter(|(_, crc)| crc.len() == 8 && all_hex(crc))
        .and_then(|(serial, _)| valid_sony_serial(serial))
    {
        return AddonKey::Serial { serial };
    }
    if stem.len() == 8 && all_hex(stem) {
        if let Some(title) = code_lines(text)
            .find_map(|l| assignment(l, "gametitle"))
            .filter(|t| !t.is_empty())
            .and_then(one_component)
        {
            return AddonKey::RomName {
                name: title.to_string(),
            };
        }
    }
    AddonKey::None
}

fn ini(stem: &str, text: &str) -> Option<(AddonFormat, AddonKey)> {
    let id = valid_game_id(stem)?;
    if id.len() == 9 {
        let ppsspp = code_lines(text).any(|l| ["_S", "_C", "_L"].iter().any(|p| l.starts_with(p)));
        return ppsspp.then_some((AddonFormat::PpssppIni, AddonKey::GameId { id }));
    }
    let codes = code_lines(text).any(|l| {
        ["[Gecko]", "[ActionReplay]", "[OnFrame]"]
            .iter()
            .any(|s| l.eq_ignore_ascii_case(s))
    });
    codes.then_some((AddonFormat::DolphinIni, AddonKey::GameId { id }))
}

fn txt(lower: &str, stem: &str, parents: &[&str], text: &str) -> Option<(AddonFormat, AddonKey)> {
    let cheat_shape = || code_lines(text).any(is_section) && code_lines(text).any(is_hex_words);
    if let Some(id) = valid_title_id16(stem).filter(|id| id.starts_with("0004")) {
        let in_switch = parents
            .first()
            .is_some_and(|p| p.eq_ignore_ascii_case("cheats"))
            && parents.get(1).and_then(|p| switch_title(p)).is_some();
        if !in_switch {
            return cheat_shape().then_some((AddonFormat::CtrCheats, AddonKey::TitleId { id }));
        }
    }
    if let Some(build_id) = valid_build_id(stem) {
        if !cheat_shape() {
            return None;
        }
        let title_id = parents
            .first()
            .filter(|p| p.eq_ignore_ascii_case("cheats"))
            .and_then(|_| parents.get(1))
            .and_then(|p| switch_title(p));
        return Some((
            AddonFormat::SwitchCheats,
            AddonKey::BuildId { build_id, title_id },
        ));
    }
    let headers: BTreeSet<String> = code_lines(text)
        .filter_map(|l| l.strip_prefix('['))
        .filter_map(|l| l.split([',', ']']).next())
        .filter_map(compact_serial)
        .collect();
    if lower == "patchlist.txt" {
        return match headers.len() {
            0 => None,
            1 => headers
                .into_iter()
                .next()
                .map(|id| (AddonFormat::Vita3kPatch, AddonKey::TitleId { id })),
            count => Some((AddonFormat::Vita3kPatch, AddonKey::Many { count })),
        };
    }
    if let Some(id) = stem.get(..9).and_then(compact_serial).filter(|_| {
        stem.as_bytes()
            .get(9)
            .is_none_or(|b| !b.is_ascii_alphanumeric())
    }) {
        if headers.contains(&id) {
            return Some((AddonFormat::Vita3kPatch, AddonKey::TitleId { id }));
        }
    }
    let mut lines = code_lines(text).peekable();
    (lines.peek().is_some() && lines.all(is_code)).then(|| {
        (
            AddonFormat::CodeText,
            AddonKey::RomName {
                name: stem.to_string(),
            },
        )
    })
}

/// A Game Genie, Action Replay or GameShark code on its own line.
fn is_code(line: &str) -> bool {
    let gg = (line.len() == 6 || line.len() == 8) && line.chars().all(|c| GG_LETTERS.contains(c));
    // `XXXX-XXXX` (SNES, Mega Drive) or `XXX-XXX[-XXX]` (Game Boy).
    let groups: Vec<&str> = line.split('-').collect();
    let snes = groups.len() == 2
        && groups.iter().all(|g| {
            g.len() == 4
                && g.bytes()
                    .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
        });
    let gb = matches!(groups.len(), 2 | 3) && groups.iter().all(|g| g.len() == 3 && all_hex(g));
    let dashed = snes || gb;
    let ar = line
        .split_once([' ', ':'])
        .map_or(line.len() == 8 && all_hex(line), |(a, v)| {
            a.len() == 8 && all_hex(a) && matches!(v.len(), 2 | 4 | 8) && all_hex(v)
        });
    gg || dashed || ar
}

fn pchtxt(parents: &[&str], text: &str) -> Option<(AddonFormat, AddonKey)> {
    let build_id = code_lines(text)
        .find_map(|l| l.strip_prefix("@nsobid-"))
        .and_then(|b| valid_build_id(b.trim()));
    let title_id = parents.iter().take(2).find_map(|p| switch_title(p));
    let key = build_id.map_or(AddonKey::None, |build_id| AddonKey::BuildId {
        build_id,
        title_id,
    });
    Some((AddonFormat::Pchtxt, key))
}

/// An RPCS3 serial in a patch file: `[BN][LCP][AEJUK][SD]\d{5}` or
/// `NP[A-Z]{2}\d{5}`.
fn is_rpcs3_serial(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 9 || !b[4..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let disc = matches!(b[0], b'B' | b'N')
        && matches!(b[1], b'L' | b'C' | b'P')
        && matches!(b[2], b'A' | b'E' | b'J' | b'U' | b'K')
        && matches!(b[3], b'S' | b'D');
    let psn =
        b[0] == b'N' && b[1] == b'P' && b[2].is_ascii_uppercase() && b[3].is_ascii_uppercase();
    disc || psn
}

fn rpcs3(lower: &str, stem: &str, text: &str) -> Option<(AddonFormat, AddonKey)> {
    if let Some(id) = stem
        .len()
        .checked_sub("_patch".len())
        .filter(|&n| stem[n..].eq_ignore_ascii_case("_patch"))
        .and_then(|n| compact_serial(&stem[..n]))
    {
        return Some((AddonFormat::Rpcs3Patch, AddonKey::TitleId { id }));
    }
    if !RPCS3_SHARED.contains(&lower) {
        return None;
    }
    let serials: BTreeSet<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| is_rpcs3_serial(t))
        .collect();
    let key = match serials.len() {
        0 => AddonKey::None,
        1 => AddonKey::TitleId {
            id: serials.into_iter().next()?.to_string(),
        },
        count => AddonKey::Many { count },
    };
    Some((AddonFormat::Rpcs3Patch, key))
}

fn shadps4_json(stem: &str) -> Option<(AddonFormat, AddonKey)> {
    let mut parts = stem.split('_');
    let serial = compact_serial(parts.next()?)?;
    let version = parts.next()?;
    let v = version.as_bytes();
    let version_ok =
        v.len() == 5 && v[2] == b'.' && v[..2].iter().chain(&v[3..]).all(u8::is_ascii_digit);
    let rest_ok = match (parts.next(), parts.next()) {
        (None, _) => true,
        (Some(n), None) => !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()),
        _ => false,
    };
    (version_ok && rest_ok).then(|| {
        let key = AddonKey::SerialVersion {
            serial,
            version: version.to_string(),
        };
        (AddonFormat::Shadps4Cheat, key)
    })
}

fn shadps4_xml(text: &str) -> Option<(AddonFormat, AddonKey)> {
    if !text.contains("<TitleID>") {
        return None;
    }
    let ids: BTreeSet<String> = text
        .split("<ID>")
        .skip(1)
        .filter_map(|rest| rest.split_once("</ID>"))
        .filter_map(|(id, _)| compact_serial(id.trim()))
        .collect();
    let key = match ids.len() {
        0 => AddonKey::None,
        1 => AddonKey::TitleId {
            id: ids.into_iter().next()?,
        },
        count => AddonKey::Many { count },
    };
    Some((AddonFormat::Shadps4Patch, key))
}

/// The eight-hex title id a Xenia patch name starts with
/// (`415607D1 - Halo 3`).
fn xenia_title(stem: &str) -> Option<String> {
    let id = stem.get(..8)?;
    let rest = &stem[8..];
    (all_hex(id) && (rest.is_empty() || rest.starts_with([' ', '-'])))
        .then(|| id.to_ascii_uppercase())
}

/// `title_id = "415607D1"` in a Xenia patch.
fn xenia_title_line(text: &str) -> Option<String> {
    code_lines(text)
        .find_map(|l| assignment(l, "title_id"))
        .map(|v| v.trim_matches('"'))
        .filter(|v| v.len() == 8 && all_hex(v))
        .map(str::to_ascii_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cheat_format_gives_its_key() {
        use AddonFormat as F;
        let k = |s: &str| AddonKey::Serial { serial: s.into() };
        let name = |s: &str| AddonKey::RomName { name: s.into() };
        let title = |s: &str| AddonKey::TitleId { id: s.into() };
        let build = |b: &str, t: Option<&str>| AddonKey::BuildId {
            build_id: b.into(),
            title_id: t.map(Into::into),
        };
        type Row<'a> = (
            &'a str,
            &'a [&'a str],
            &'a str,
            Option<(AddonFormat, AddonKey)>,
        );
        let rows: Vec<Row<'_>> = vec![
            (
                "Chrono Trigger (USA).cht",
                &[],
                "cheats = 2\ncheat0_desc = \"x\"\n",
                Some((F::RetroArchCht, name("Chrono Trigger (USA)"))),
            ),
            (
                "Contra (USA).cht",
                &[],
                "0x0032:09\n0x0033:09\n",
                Some((F::FceuxCht, name("Contra (USA)"))),
            ),
            (
                "SLUS-00594.cht",
                &[],
                "[Infinite HP]\nType = Gameshark\n8009C6E4 03E7\n",
                Some((F::DuckStationCht, k("SLUS-00594"))),
            ),
            (
                "SLUS-00594-3A1B2C3D4E5F6A7B.cht",
                &[],
                "[X]\nType = Gameshark\n",
                Some((F::DuckStationCht, k("SLUS-00594"))),
            ),
            (
                "SLUS-20946_A0E8A6B4.pnach",
                &[],
                "gametitle=Game\npatch=1,EE,0,word,0\n",
                Some((F::Pnach, k("SLUS-20946"))),
            ),
            (
                "A0E8A6B4.pnach",
                &[],
                "gametitle=Kingdom Hearts (USA)\n",
                Some((F::Pnach, name("Kingdom Hearts (USA)"))),
            ),
            (
                "C0FFEE00.pnach",
                &[],
                "patch=1,EE,0,word,0\n",
                Some((F::Pnach, AddonKey::None)),
            ),
            (
                "GALE01.ini",
                &[],
                "[Gecko]\n$Infinite\n04000000 00000000\n",
                Some((
                    F::DolphinIni,
                    AddonKey::GameId {
                        id: "GALE01".into(),
                    },
                )),
            ),
            (
                "RMC.ini",
                &[],
                "[ActionReplay]\n$x\n",
                Some((F::DolphinIni, AddonKey::GameId { id: "RMC".into() })),
            ),
            ("GALE01.ini", &[], "[Core]\nCPUThread = True\n", None),
            (
                "ULUS10041.ini",
                &[],
                "_S ULUS-10041\n_C0 Infinite\n_L 0x2000\n",
                Some((
                    F::PpssppIni,
                    AddonKey::GameId {
                        id: "ULUS10041".into(),
                    },
                )),
            ),
            (
                "0004000000055D00.txt",
                &[],
                "[Infinite money]\nD3000000 00000000\n",
                Some((F::CtrCheats, title("0004000000055D00"))),
            ),
            (
                "A1B2C3D4E5F60718.txt",
                &["cheats", "0100000000010000"],
                "[Moon jump]\n04000000 00000000\n",
                Some((
                    F::SwitchCheats,
                    build("A1B2C3D4E5F60718", Some("0100000000010000")),
                )),
            ),
            (
                "A1B2C3D4E5F60718.txt",
                &[],
                "{Master}\n04000000 00000000\n",
                Some((F::SwitchCheats, build("A1B2C3D4E5F60718", None))),
            ),
            (
                "60fps.pchtxt",
                &["0100000000010000"],
                "@nsobid-A1B2C3D4E5F60718\n@enabled\n",
                Some((
                    F::Pchtxt,
                    build("A1B2C3D4E5F60718", Some("0100000000010000")),
                )),
            ),
            (
                "BLUS30001_patch.yml",
                &[],
                "Version: 1.2\n",
                Some((F::Rpcs3Patch, title("BLUS30001"))),
            ),
            (
                "patch.yml",
                &[],
                "PPU-abc:\n  \"x\":\n    Games:\n      \"G\":\n        BLUS30001: [ 01.00 ]\n",
                Some((F::Rpcs3Patch, title("BLUS30001"))),
            ),
            (
                "patch.yml",
                &[],
                "BLUS30001: x\nBLES00001: y\nNPUA80001: z\n",
                Some((F::Rpcs3Patch, AddonKey::Many { count: 3 })),
            ),
            (
                "CUSA07023_01.03.json",
                &[],
                "{\"name\":\"x\"}",
                Some((
                    F::Shadps4Cheat,
                    AddonKey::SerialVersion {
                        serial: "CUSA07023".into(),
                        version: "01.03".into(),
                    },
                )),
            ),
            (
                "CUSA07023_01.03_2.json",
                &[],
                "{}",
                Some((
                    F::Shadps4Cheat,
                    AddonKey::SerialVersion {
                        serial: "CUSA07023".into(),
                        version: "01.03".into(),
                    },
                )),
            ),
            (
                "Bloodborne.xml",
                &[],
                "<Patch><TitleID><ID>CUSA00900</ID></TitleID></Patch>",
                Some((F::Shadps4Patch, title("CUSA00900"))),
            ),
            (
                "415607D1 - Halo 3.patch.toml",
                &[],
                "title_id = \"415607D1\"\n",
                Some((F::XeniaPatch, title("415607D1"))),
            ),
            (
                "PCSE00001_60fps.txt",
                &[],
                "[PCSE00001,eboot.bin]\n@0x1234 0x00\n",
                Some((F::Vita3kPatch, title("PCSE00001"))),
            ),
            (
                "patchlist.txt",
                &[],
                "[PCSE00001,eboot.bin]\n[PCSB00002,eboot.bin]\n",
                Some((F::Vita3kPatch, AddonKey::Many { count: 2 })),
            ),
            (
                "Mario Kart DS (USA).mch",
                &[],
                "",
                Some((F::MelonDsCheats, name("Mario Kart DS (USA)"))),
            ),
            (
                "Pokemon Emerald (USA).cheats",
                &[],
                "",
                Some((F::MgbaCheats, name("Pokemon Emerald (USA)"))),
            ),
            ("Game.dct", &[], "", Some((F::DesmumeCheats, name("Game")))),
            (
                "Super Mario Bros. (World).txt",
                &[],
                "SXIOPO\n# lives\nAATOZE\n",
                Some((F::CodeText, name("Super Mario Bros. (World)"))),
            ),
            ("readme.txt", &[], "This is a translation patch.\n", None),
            // A crafted name is not one component: never a key.
            ("../../x_patch.yml", &[], "", None),
        ];
        for (name, parents, text, want) in rows {
            assert_eq!(sniff_cheat(name, parents, text), want, "{name}");
        }
    }

    #[test]
    fn more_shapes_and_their_limits() {
        use AddonFormat as F;
        // shadPS4 XML naming several titles holds patches for many games.
        assert_eq!(
            sniff_cheat(
                "patches.xml",
                &[],
                "<Patch><TitleID><ID>CUSA00900</ID><ID>CUSA03173</ID></TitleID></Patch>"
            ),
            Some((F::Shadps4Patch, AddonKey::Many { count: 2 }))
        );
        // An XML without a title id block is not a shadPS4 patch.
        assert_eq!(sniff_cheat("x.xml", &[], "<wiidisc/>"), None);
        // The older Xenia `<TITLEID8>.patch`.
        assert_eq!(
            sniff_cheat("4D5307E6.patch", &[], ""),
            Some((
                F::XeniaPatch,
                AddonKey::TitleId {
                    id: "4D5307E6".into()
                }
            ))
        );
        // A 16-hex `.txt` whose text is prose is no cheat.
        assert_eq!(sniff_cheat("0004000000055D00.txt", &[], "hello\n"), None);
        // A `.cht` that is neither RetroArch, FCEUX nor DuckStation.
        assert_eq!(sniff_cheat("Game.cht", &[], "hello world\n"), None);
        // A Switch build id under a parent that is not a title id.
        assert_eq!(
            sniff_cheat(
                "A1B2C3D4E5F60718.txt",
                &["cheats", "../x"],
                "[a]\n04000000 00000000\n"
            ),
            Some((
                F::SwitchCheats,
                AddonKey::BuildId {
                    build_id: "A1B2C3D4E5F60718".into(),
                    title_id: None
                }
            ))
        );
        // Action Replay and Game Genie text of other shapes.
        for text in [
            "82003228 0063\n",
            "02000000:63\n",
            "C2AB-6D0F\n; comment\n",
            "00A-17B-C49\n",
            "00A-17B\n",
        ] {
            assert_eq!(
                sniff_cheat("Game (USA).txt", &[], text),
                Some((
                    F::CodeText,
                    AddonKey::RomName {
                        name: "Game (USA)".into()
                    }
                )),
                "{text:?}"
            );
        }
        // Only comments or nothing: not a cheat.
        assert_eq!(sniff_cheat("Game (USA).txt", &[], "# x\n\n"), None);
        // A gametitle that is not a safe name gives no key.
        assert_eq!(
            sniff_cheat("A0E8A6B4.pnach", &[], "gametitle=../../etc\n"),
            Some((F::Pnach, AddonKey::None))
        );
        // Global databases are never a game's cheat.
        for n in [
            "usrcheat.dat",
            "cheat.db",
            "mupencheat.txt",
            "CHEATS_JSON.txt",
            "patch_config.yml",
        ] {
            assert_eq!(sniff_cheat(n, &[], "SXIOPO\n"), None, "{n}");
        }
    }

    #[test]
    fn global_databases_are_system_data() {
        for n in [
            "usrcheat.dat",
            "CHEAT.DB",
            "mupencheat.txt",
            "CHEATS_JSON.txt",
            "patch_config.yml",
        ] {
            assert!(is_global_cheat_db(n), "{n}");
        }
        assert!(!is_global_cheat_db("patch.yml"));
    }

    #[test]
    fn the_pre_filter_is_by_extension() {
        for n in [
            "Game.cht",
            "x.PNACH",
            "GALE01.ini",
            "a.txt",
            "patch.yml",
            "CUSA07023_01.03.json",
            "a.xml",
            "415607D1 - Halo 3.patch.toml",
            "4D5307E6.patch",
            "a.pchtxt",
            "a.mch",
            "a.cheats",
            "a.dct",
        ] {
            assert!(may_be_cheat(n), "{n}");
        }
        for n in ["Game.iso", "Game.bps", "cover.png", "noext"] {
            assert!(!may_be_cheat(n), "{n}");
        }
    }
}
