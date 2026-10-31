//! Disc serials in one normal form, so a serial read from a disc, written
//! in a DAT or typed by a user compares equal (dedupe level 3 keys on it).
//!
//! - PlayStation boot names and their variants become `AAAA-NNNNN`:
//!   `SLUS_008.62`, `slus-00862`, `SLUS00862` → `SLUS-00862`.
//! - Sega product numbers lose the padding of their field and are
//!   upper-cased: `"T-5704H   "` → `T-5704H`.
//! - A Sega product written with no dash, as Dreamcast `IP.BIN` does,
//!   gets the dash DATs write after its letter prefix: `T40214N` →
//!   `T-40214N`, `MK51000` → `MK-51000`.
//! - A Sega CD field `GM <serial, 8 chars>-<version>` keeps the serial:
//!   `GM MK-4407 -00` → `MK-4407`.
//!
//! Anything else is not a serial (`None`): a disc's bytes are untrusted, so
//! only short ASCII codes of letters, digits and inner dashes pass.

/// Longest serial kept, after normalisation.
pub const MAX_SERIAL: usize = 16;

/// The normal form of a serial, or `None` when `raw` is not one.
pub fn normalise_serial(raw: &str) -> Option<String> {
    let trimmed = raw.trim_matches(|c| c == ' ' || c == '\0');
    // An ISO 9660 file name may still carry its `;1` version.
    let trimmed = match trimmed.rsplit_once(';') {
        Some((stem, version)) if is_digits(version) => stem,
        _ => trimmed,
    };
    if !trimmed.is_ascii() {
        return None;
    }
    let upper = trimmed.to_ascii_uppercase();
    if let Some(ps) = playstation(&upper) {
        return Some(ps);
    }
    let core = sega_cd_field(&upper).unwrap_or(&upper);
    let core = sega_dash(core.trim_matches(' '));
    let core = core.as_str();
    let ok = (3..=MAX_SERIAL).contains(&core.len())
        && core
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
        && core.bytes().any(|b| b.is_ascii_digit())
        && !core.starts_with('-')
        && !core.ends_with('-');
    ok.then(|| core.to_owned())
}

/// The DAT form of a console serial (spec D22): ID6/ID4 for Nintendo
/// discs, `CTR-P-XXXX` product codes, `AAAA-NNNNN` for Sony, `XX-NNN`
/// for Xbox, 8 hex for Xbox 360. `None` when `raw` is not that shape.
///
/// - `gamecube`: ID6 (`GALE01`); `wii`: ID6 or the ID4 of a WAD (`WAAE`);
///   `nds`: the DSi game code (`KJUJ`).
/// - `wiiu`: the 4-character code, also out of `WUP-P-ARPE` (GameTDB keys
///   on it).
/// - `3ds`: `CTR-` or `KTR-` (New 3DS), a `P`/`N`/`M`/`U` letter, four
///   characters (No-Intro's form).
/// - `ps3`, `psvita`: four letters and five digits, `BLUS30443` →
///   `BLUS-30443` (the PlayStation rule of [`normalise_serial`]).
/// - `xbox`: `EA-013`; `xbox360`: the 8-hex title id, upper-case.
///
/// Unlike [`normalise_serial`], ID6/ID4 need no digit: `RMGE01` has one,
/// `WAAE` does not.
pub fn normalise_console_serial(system: &str, raw: &str) -> Option<String> {
    let trimmed = raw.trim_end_matches(['\0', ' ']).trim_start_matches(' ');
    if !trimmed.is_ascii() {
        return None;
    }
    let s = trimmed.to_ascii_uppercase();
    let alnum = |t: &str, n: usize| {
        t.len() == n
            && t.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    };
    let ok = match system {
        "gamecube" => alnum(&s, 6),
        "wii" => alnum(&s, 6) || alnum(&s, 4),
        "nds" => alnum(&s, 4),
        "wiiu" => {
            let code = match s.strip_prefix("WUP-") {
                Some(rest) => match rest.as_bytes() {
                    [b'P' | b'N' | b'M', b'-', ..] => rest[2..].to_owned(),
                    _ => return None,
                },
                None => s.clone(),
            };
            return alnum(&code, 4).then_some(code);
        }
        "3ds" => {
            let b = s.as_bytes();
            s.len() == 10
                && (s.starts_with("CTR-") || s.starts_with("KTR-"))
                && matches!(b[4], b'P' | b'N' | b'M' | b'U')
                && b[5] == b'-'
                && alnum(&s[6..], 4)
        }
        "ps3" | "psvita" => return playstation(&s),
        "xbox" => {
            let b = s.as_bytes();
            s.len() == 6
                && alnum(&s[..2], 2)
                && b[2] == b'-'
                && b[3..].iter().all(u8::is_ascii_digit)
        }
        "xbox360" => s.len() == 8 && s.bytes().all(|b| b.is_ascii_hexdigit()),
        _ => false,
    };
    ok.then_some(s)
}

/// The serial named by a `SYSTEM.CNF` `BOOT`/`BOOT2` value
/// (`cdrom:\SLUS_008.62;1`, `cdrom0:\SLUS_200.62;1`): its file name,
/// normalised. `None` when the boot file is not named like a serial
/// (`PSX.EXE`).
pub fn boot_file_serial(boot: &str) -> Option<String> {
    let path = boot.trim();
    let name = path.rsplit(['\\', '/', ':']).next().unwrap_or(path);
    let serial = normalise_serial(name)?;
    // Only the PlayStation shape is a serial in a boot line.
    playstation(&serial).is_some().then_some(serial)
}

/// `AAAA-NNNNN` from four letters, an optional `_`, `-` or space, and
/// either `NNN.NN` or `NNNNN` (`upper` is upper-case already).
fn playstation(upper: &str) -> Option<String> {
    let (prefix, rest) = upper.split_at_checked(4)?;
    if !prefix.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let rest = rest.strip_prefix(['_', '-', ' ']).unwrap_or(rest);
    let digits = match rest.split_once('.') {
        Some((a, b)) if a.len() == 3 && b.len() == 2 => format!("{a}{b}"),
        Some(_) => return None,
        None => rest.to_owned(),
    };
    (digits.len() == 5 && is_digits(&digits)).then(|| format!("{prefix}-{digits}"))
}

/// The serial part of a Sega CD `GM` field: the fixed 14-byte form
/// `GM XXXXXXXX-NN` keeps `XXXXXXXX`; a shorter `GM <serial>` keeps the
/// rest. `None` when `upper` does not start with `GM `.
fn sega_cd_field(upper: &str) -> Option<&str> {
    let rest = upper.strip_prefix("GM ")?;
    if upper.len() == 14 && upper.as_bytes()[11] == b'-' && is_digits(&upper[12..]) {
        return Some(&upper[3..11]);
    }
    Some(rest)
}

/// `core` with a dash after a leading 1–3 letter prefix when it has no
/// dash and a digit follows the prefix (`T40214N` → `T-40214N`); unchanged
/// otherwise.
fn sega_dash(core: &str) -> String {
    let letters = core.bytes().take_while(u8::is_ascii_uppercase).count();
    let digit_next = core.as_bytes().get(letters).is_some_and(u8::is_ascii_digit);
    if core.contains('-') || !(1..=3).contains(&letters) || !digit_next {
        return core.to_owned();
    }
    format!("{}-{}", &core[..letters], &core[letters..])
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    #[test]
    fn console_serial_forms() {
        let n = super::normalise_console_serial;
        assert_eq!(n("gamecube", "GALE01").as_deref(), Some("GALE01"));
        assert_eq!(n("wii", "rmge01").as_deref(), Some("RMGE01"));
        assert_eq!(n("wii", "WAAE").as_deref(), Some("WAAE")); // WAD ID4
        assert_eq!(n("3ds", "CTR-P-AMKP").as_deref(), Some("CTR-P-AMKP"));
        assert_eq!(
            n("3ds", "CTR-P-AMKP\0\0\0\0\0\0").as_deref(),
            Some("CTR-P-AMKP")
        );
        assert_eq!(n("nds", "KJUJ").as_deref(), Some("KJUJ"));
        assert_eq!(n("ps3", "BLUS30443").as_deref(), Some("BLUS-30443"));
        assert_eq!(n("psvita", "PCSE00844").as_deref(), Some("PCSE-00844"));
        assert_eq!(n("wiiu", "WUP-P-ARPE").as_deref(), Some("ARPE"));
        assert_eq!(n("xbox", "EA-013").as_deref(), Some("EA-013"));
        assert_eq!(n("xbox360", "4d5307e6").as_deref(), Some("4D5307E6"));
        assert_eq!(n("gamecube", "GA E01"), None);
        assert_eq!(n("ps3", "BLUS3044"), None);
        // The PlayStation path is untouched.
        assert_eq!(
            super::normalise_serial("SLUS_008.62").as_deref(),
            Some("SLUS-00862")
        );
    }

    #[test]
    fn console_serial_rejects_other_shapes() {
        let n = super::normalise_console_serial;
        assert_eq!(n("gamecube", "GALE0"), None);
        assert_eq!(n("nds", "KJUJ01"), None);
        assert_eq!(n("3ds", "CTR-X-AMKP"), None);
        assert_eq!(n("3ds", "KTR-N-AAAA").as_deref(), Some("KTR-N-AAAA"));
        assert_eq!(n("wiiu", "WUP-Q-ARPE"), None);
        assert_eq!(n("wiiu", "arpe").as_deref(), Some("ARPE"));
        assert_eq!(n("xbox", "EA-01A"), None);
        assert_eq!(n("xbox360", "4D5307E"), None);
        assert_eq!(n("xbox360", "4D5307EG"), None);
        assert_eq!(n("psx", "SLUS00862"), None);
        assert_eq!(n("wii", "RMGÉ01"), None);
    }
}
