//! Safe archive entry names (spec `2026-10-23-organize-archives-and-discs.md`
//! §13.1 "Zip-slip", "Names that are not valid text", "Windows device and
//! illegal names").
//!
//! Every backend turns the stored name bytes into text with
//! [`decode_zip_name`] or [`decode_utf16_name`] and then into a [`RelPath`]
//! with [`RelPath::parse`]. A name that fails is kept on the entry as
//! [`UnsafeWhy`]: such an entry is never extracted, and is an error only
//! when a set needs it (spec §4.2). The rules apply on every OS, because a
//! card organised on macOS may later be opened on Windows.

use std::fmt;

use unicode_normalization::UnicodeNormalization;

/// Why an entry name cannot be extracted (spec §13 extras).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnsafeWhy {
    /// `/etc/x`, `C:\x`, `\\server\share`.
    Absolute,
    /// A `..` component (zip-slip).
    DotDot,
    /// A Windows device name (`CON`, `LPT1.cue`, `CONIN$`…).
    Device,
    /// A character no target file system accepts, a trailing dot or space,
    /// or an empty path.
    Illegal,
    /// Undecodable bytes (unpaired surrogate, bad UTF-8 with the UTF-8 flag).
    Encoding,
    /// A symlink or hardlink entry.
    Link,
    /// A component longer than 255 UTF-16 units or UTF-8 bytes.
    TooLong,
}

/// A validated, `/`-separated, NFC-normalised relative entry path.
///
/// Only [`RelPath::parse`] builds one, so every value is free of absolute
/// prefixes, `.`/`..` components, empty components, Windows device names,
/// illegal characters and over-long components.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RelPath(String);

impl RelPath {
    /// Validates `raw` (already decoded text). Both `/` and `\` separate
    /// components; empty components (`a//b`, a trailing `/` of a directory
    /// entry) and `.` components (`./Game.gba`, as `tar -C dir .` writes)
    /// are dropped; the result is NFC-normalised once.
    pub fn parse(raw: &str) -> Result<RelPath, UnsafeWhy> {
        let nfc: String = raw.nfc().collect();
        if nfc.contains('\0') {
            return Err(UnsafeWhy::Illegal);
        }
        // `/x`, `\x`, `\\server\share`, `\\?\C:\x`.
        if nfc.starts_with(['/', '\\']) {
            return Err(UnsafeWhy::Absolute);
        }
        // `C:\x`, `C:/x`, `C:`. A drive-relative `C:x` has a `:` inside a
        // component and is refused below as illegal (spec table: `a:b`).
        let mut head = nfc.chars();
        if let (Some(d), Some(':'), None | Some('/' | '\\')) =
            (head.next(), head.next(), head.next())
        {
            if d.is_ascii_alphabetic() {
                return Err(UnsafeWhy::Absolute);
            }
        }
        let mut out = String::with_capacity(nfc.len());
        for component in nfc
            .split(['/', '\\'])
            .filter(|c| !c.is_empty() && *c != ".")
        {
            check_component(component)?;
            if !out.is_empty() {
                out.push('/');
            }
            out.push_str(component);
        }
        if out.is_empty() {
            return Err(UnsafeWhy::Illegal);
        }
        Ok(RelPath(out))
    }

    /// The stand-in name of entry `index` whose stored name failed
    /// validation (the entry carries the reason in `unsafe_name` and is
    /// never extracted). Its raw bytes stay on the entry for diagnostics.
    pub fn placeholder(index: usize) -> RelPath {
        RelPath(format!("unsafe-entry-{index}"))
    }

    /// The path as `/`-separated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The components, first to last; never empty, never `.` or `..`.
    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    /// The last component.
    pub fn file_name(&self) -> &str {
        self.0
            .rsplit_once('/')
            .map_or(self.0.as_str(), |(_, name)| name)
    }

    /// Everything before the last component, `None` at the top level.
    pub fn parent(&self) -> Option<&str> {
        self.0.rsplit_once('/').map(|(parent, _)| parent)
    }
}

/// One component of a path: `..`, illegal characters, a trailing dot
/// or space (Windows drops them, so `game.` would land as `game`), Windows
/// device names, and the 255 unit/byte limit.
fn check_component(c: &str) -> Result<(), UnsafeWhy> {
    if c == ".." {
        return Err(UnsafeWhy::DotDot);
    }
    if c.chars()
        .any(|ch| ch < '\u{20}' || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return Err(UnsafeWhy::Illegal);
    }
    if c.ends_with(['.', ' ']) {
        return Err(UnsafeWhy::Illegal);
    }
    if is_device(c) {
        return Err(UnsafeWhy::Device);
    }
    if !component_fits(c) {
        return Err(UnsafeWhy::TooLong);
    }
    Ok(())
}

/// A Windows device name, with any extension and in any case: `CON`,
/// `PRN`, `AUX`, `NUL`, `COM0`–`COM9`, `LPT0`–`LPT9` (also with the
/// superscript digits `¹²³`), `CONIN$`, `CONOUT$`. Windows ignores
/// trailing spaces before the extension (`CON .txt`).
fn is_device(c: &str) -> bool {
    let base = c.split('.').next().unwrap_or("").trim_end_matches(' ');
    let upper = base.to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let Some(rest) = upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
    else {
        return false;
    };
    let mut digits = rest.chars();
    matches!(
        (digits.next(), digits.next()),
        (Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
    )
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Decodes a zip entry name. The Info-ZIP Unicode Path extra (0x7075),
/// given as `(crc32 of the raw name, UTF-8 name)`, wins only when its CRC
/// matches `raw`; else the UTF-8 flag (general purpose bit 11) means UTF-8,
/// and without it the name is CP437 (APPNOTE appendix D).
pub fn decode_zip_name(
    raw: &[u8],
    utf8_flag: bool,
    unicode_extra: Option<(u32, &[u8])>,
) -> Result<String, UnsafeWhy> {
    if let Some((crc, utf8)) = unicode_extra {
        if crc32fast::hash(raw) == crc {
            if let Ok(name) = std::str::from_utf8(utf8) {
                return Ok(name.to_owned());
            }
        }
    }
    if utf8_flag {
        return std::str::from_utf8(raw)
            .map(str::to_owned)
            .map_err(|_| UnsafeWhy::Encoding);
    }
    Ok(raw
        .iter()
        .map(|&b| match b {
            0..=0x7f => char::from(b),
            _ => CP437_HIGH[usize::from(b - 0x80)],
        })
        .collect())
}

/// Decodes a UTF-16 name (7z, RAR5 through a tool); an unpaired surrogate
/// is [`UnsafeWhy::Encoding`].
pub fn decode_utf16_name(units: &[u16]) -> Result<String, UnsafeWhy> {
    char::decode_utf16(units.iter().copied())
        .collect::<Result<String, _>>()
        .map_err(|_| UnsafeWhy::Encoding)
}

/// The key names are compared by (NFC, lower case): FAT/exFAT and default
/// APFS/NTFS are case-insensitive (spec §8.2). Same rule as the writer's.
pub fn name_key(s: &str) -> String {
    crate::write::name_key(s)
}

/// Whether one component fits every target: ≤ 255 UTF-16 units (FAT,
/// exFAT, NTFS) **and** ≤ 255 UTF-8 bytes (APFS, ext4).
pub fn component_fits(s: &str) -> bool {
    s.len() <= 255 && s.encode_utf16().count() <= 255
}

/// CP437 bytes 0x80–0xFF (bytes below 0x80 are ASCII).
#[rustfmt::skip]
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å',
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ',
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»',
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐',
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧',
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀',
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩',
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

#[cfg(test)]
mod tests {
    use super::*;

    fn why(raw: &str) -> UnsafeWhy {
        RelPath::parse(raw).expect_err(raw)
    }

    #[test]
    fn zip_slip_dotdot() {
        assert_eq!(why("../evil.bin"), UnsafeWhy::DotDot);
        assert_eq!(why("games/../../evil.bin"), UnsafeWhy::DotDot);
        assert_eq!(why(".."), UnsafeWhy::DotDot);
        assert_eq!(why("./../evil.bin"), UnsafeWhy::DotDot);
        assert_eq!(why(".\\..\\evil.bin"), UnsafeWhy::DotDot);
    }

    /// `tar czf roms.tgz .` stores every entry as `./Game.gba`: a `.`
    /// component names the same directory, so it is dropped like an empty
    /// one instead of making every entry of such an archive unsafe.
    #[test]
    fn dot_components_dropped() {
        assert_eq!(RelPath::parse("./Game.gba").unwrap().as_str(), "Game.gba");
        assert_eq!(RelPath::parse("a/./b.bin").unwrap().as_str(), "a/b.bin");
        assert_eq!(
            RelPath::parse(".\\PS\\.\\Game.cue").unwrap().as_str(),
            "PS/Game.cue"
        );
        // The archive root itself (`./`, `.`) names nothing.
        assert_eq!(why("./"), UnsafeWhy::Illegal);
        assert_eq!(why("."), UnsafeWhy::Illegal);
        // A `.` never hides an absolute prefix: `./` is relative already,
        // and `/./x` is absolute before any component is looked at.
        assert_eq!(why("/./etc/passwd"), UnsafeWhy::Absolute);
    }

    #[test]
    fn absolute_unix() {
        assert_eq!(why("/etc/passwd"), UnsafeWhy::Absolute);
        assert_eq!(why("//etc/passwd"), UnsafeWhy::Absolute);
        assert_eq!(why("/"), UnsafeWhy::Absolute);
    }

    #[test]
    fn absolute_windows_drive() {
        assert_eq!(why("C:\\Windows\\x.dll"), UnsafeWhy::Absolute);
        assert_eq!(why("c:/x.bin"), UnsafeWhy::Absolute);
        assert_eq!(why("D:"), UnsafeWhy::Absolute);
        // Drive-relative: refused for its `:`.
        assert_eq!(why("D:x.bin"), UnsafeWhy::Illegal);
        assert_eq!(why("\\x.bin"), UnsafeWhy::Absolute);
    }

    #[test]
    fn unc_path() {
        assert_eq!(why("\\\\server\\share\\x.bin"), UnsafeWhy::Absolute);
        assert_eq!(why("\\\\?\\C:\\x.bin"), UnsafeWhy::Absolute);
    }

    #[test]
    fn backslash_dotdot() {
        assert_eq!(why("..\\evil.bin"), UnsafeWhy::DotDot);
        assert_eq!(why("a\\..\\..\\evil.bin"), UnsafeWhy::DotDot);
        assert_eq!(
            RelPath::parse("PS\\Game\\Game.cue").unwrap().as_str(),
            "PS/Game/Game.cue"
        );
    }

    #[test]
    fn nul_rejected() {
        assert_eq!(why("game.gba\0.txt"), UnsafeWhy::Illegal);
        assert_eq!(why("\0"), UnsafeWhy::Illegal);
    }

    #[test]
    fn empty_components_normalised() {
        let p = RelPath::parse("a//b///c.bin").unwrap();
        assert_eq!(p.as_str(), "a/b/c.bin");
        assert_eq!(p.components().collect::<Vec<_>>(), ["a", "b", "c.bin"]);
        assert_eq!(p.file_name(), "c.bin");
        assert_eq!(p.parent(), Some("a/b"));
        let dir = RelPath::parse("Game (Disc 1)/").unwrap();
        assert_eq!(dir.as_str(), "Game (Disc 1)");
        assert_eq!(dir.parent(), None);
        assert_eq!(dir.file_name(), "Game (Disc 1)");
        assert_eq!(why(""), UnsafeWhy::Illegal);
    }

    #[test]
    fn device_names_rejected() {
        let device = [
            "CON",
            "con",
            "prn.txt",
            "Aux.bin",
            "NUL",
            "nul.tar.gz",
            "CON .txt",
            "LPT1.cue",
            "lpt9",
            "LPT0",
            "COM¹",
            "com².bin",
            "LPT³",
            "CONIN$",
            "conout$",
            "dir/CON",
            "AUX/game.bin",
        ];
        for name in device {
            assert_eq!(why(name), UnsafeWhy::Device, "{name}");
        }
        for n in 0..=9 {
            assert_eq!(why(&format!("COM{n}")), UnsafeWhy::Device, "COM{n}");
            assert_eq!(why(&format!("lpt{n}.sav")), UnsafeWhy::Device, "LPT{n}");
        }
        let illegal = [
            "game.",
            "game ",
            "dir./x.bin",
            "a:b",
            "game.bin:stream",
            "a<b",
            "a>b",
            "a\"b",
            "a|b",
            "a?b",
            "a*b",
            "a\u{1}b",
            "a\u{1f}b",
        ];
        for name in illegal {
            assert_eq!(why(name), UnsafeWhy::Illegal, "{name:?}");
        }
        // Near misses stay legal.
        for ok in [
            "CONSOLE.bin",
            "COM10",
            "LPT",
            "Context.txt",
            "nul_x.bin",
            "COM1x.cue",
            "Track 01.bin",
            ".hidden",
            "a..b.bin",
        ] {
            assert!(RelPath::parse(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn nfc_normalised_once() {
        let decomposed = "Pokemon\u{301}/Poke\u{301}mon.gb";
        let p = RelPath::parse(decomposed).unwrap();
        assert_eq!(p.as_str(), "Pokemoń/Pokémon.gb");
        assert_eq!(p, RelPath::parse("Pokemoń/Pokémon.gb").unwrap());
        // Parsing an already-normalised path is the identity.
        assert_eq!(RelPath::parse(p.as_str()).unwrap(), p);
    }

    #[test]
    fn zip_cp437_name_decoded() {
        assert_eq!(
            decode_zip_name(b"Pok\x82mon.gb", false, None).unwrap(),
            "Pokémon.gb"
        );
        assert_eq!(
            decode_zip_name(b"\x80\xa4\xe1\xfa\xff", false, None).unwrap(),
            "Çñß·\u{a0}"
        );
        assert_eq!(
            decode_zip_name(b"plain.gba", false, None).unwrap(),
            "plain.gba"
        );
        // With the UTF-8 flag the same bytes are UTF-8, or invalid.
        assert_eq!(
            decode_zip_name("Pokémon.gb".as_bytes(), true, None).unwrap(),
            "Pokémon.gb"
        );
        assert_eq!(
            decode_zip_name(b"Pok\x82mon.gb", true, None),
            Err(UnsafeWhy::Encoding)
        );
    }

    #[test]
    fn zip_unicode_extra_bad_crc_ignored() {
        let raw = b"Pok\x82mon.gb";
        let bad = crc32fast::hash(raw) ^ 1;
        assert_eq!(
            decode_zip_name(raw, false, Some((bad, "../evil.gb".as_bytes()))).unwrap(),
            "Pokémon.gb"
        );
    }

    #[test]
    fn zip_unicode_extra_good_crc_used() {
        let raw = b"Pok?mon.gb";
        let crc = crc32fast::hash(raw);
        assert_eq!(
            decode_zip_name(raw, false, Some((crc, "Pokémon.gb".as_bytes()))).unwrap(),
            "Pokémon.gb"
        );
        // A matching extra that is not UTF-8 is ignored, not trusted.
        assert_eq!(
            decode_zip_name(raw, false, Some((crc, b"\xff\xfe"))).unwrap(),
            "Pok?mon.gb"
        );
    }

    #[test]
    fn sevenz_unpaired_surrogate_unsafe() {
        let good: Vec<u16> = "Tetris Plus (Track 01).bin".encode_utf16().collect();
        assert_eq!(
            decode_utf16_name(&good).unwrap(),
            "Tetris Plus (Track 01).bin"
        );
        let astral: Vec<u16> = "🎮.bin".encode_utf16().collect();
        assert_eq!(decode_utf16_name(&astral).unwrap(), "🎮.bin");
        assert_eq!(
            decode_utf16_name(&[0x61, 0xD800, 0x62]),
            Err(UnsafeWhy::Encoding)
        );
        assert_eq!(decode_utf16_name(&[0xDC00]), Err(UnsafeWhy::Encoding));
    }

    #[test]
    fn name_key_folds_case_and_nfc() {
        assert_eq!(name_key("Track 01.BIN"), name_key("track 01.bin"));
        assert_eq!(name_key("Poke\u{301}mon.GB"), "pokémon.gb");
        assert_eq!(name_key("ÉTÉ"), name_key("e\u{301}te\u{301}"));
        assert_ne!(name_key("a.bin"), name_key("b.bin"));
    }

    #[test]
    fn component_255_units_and_bytes() {
        assert!(component_fits(&"a".repeat(255)));
        assert!(!component_fits(&"a".repeat(256)));
        // 'é' is 2 UTF-8 bytes, 1 UTF-16 unit: bytes decide.
        assert!(component_fits(&"é".repeat(127)));
        assert!(!component_fits(&"é".repeat(128)));
        // '🎮' is 4 bytes and 2 units: 63 fit (252 bytes), 64 do not.
        assert!(component_fits(&"🎮".repeat(63)));
        assert!(!component_fits(&"🎮".repeat(64)));
        assert_eq!(why(&format!("dir/{}", "a".repeat(256))), UnsafeWhy::TooLong);
        assert!(RelPath::parse(&format!("{}/x", "a".repeat(255))).is_ok());
    }
}
