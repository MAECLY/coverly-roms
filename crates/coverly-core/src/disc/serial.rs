//! Disc serials in one normal form, so a serial read from a disc, written
//! in a DAT or typed by a user compares equal (dedupe level 3 keys on it).
//!
//! - PlayStation boot names and their variants become `AAAA-NNNNN`:
//!   `SLUS_008.62`, `slus-00862`, `SLUS00862` → `SLUS-00862`.
//! - Sega product numbers lose the padding of their field and are
//!   upper-cased: `"T-5704H   "` → `T-5704H`.
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
    let core = core.trim_matches(' ');
    let ok = (3..=MAX_SERIAL).contains(&core.len())
        && core
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
        && core.bytes().any(|b| b.is_ascii_digit())
        && !core.starts_with('-')
        && !core.ends_with('-');
    ok.then(|| core.to_owned())
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

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}
