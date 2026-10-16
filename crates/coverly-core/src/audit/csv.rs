//! CSV output of an [`AuditReport`] (RFC 4180 quoting, `\n` line ends).

use std::io::{self, Write};

use crate::art::ArtState;
use crate::profiles::Media;

use super::AuditReport;

/// Writes one row per ROM of `report` to `w`, or only the ROMs missing at
/// least one audited media when `missing_only` is set.
///
/// Columns: `system,folder,rel_path,stem,size,modified`, then one column
/// per audited media (named as the media, e.g. `cover`) holding
/// `present`, `missing` or `unsupported`. `system` is empty for unknown
/// folders (which have no ROMs, so never appear).
pub fn write_csv<W: Write>(report: &AuditReport, mut w: W, missing_only: bool) -> io::Result<()> {
    let mut header: Vec<String> = ["system", "folder", "rel_path", "stem", "size", "modified"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    header.extend(report.media.iter().map(|m| media_name(*m)));
    write_row(&mut w, &header)?;

    for system in &report.systems {
        for entry in &system.entries {
            if missing_only && !entry.is_missing_any() {
                continue;
            }
            let mut row = vec![
                system.system_id.clone().unwrap_or_default(),
                system.folder.clone(),
                entry.rel_path.to_string_lossy().into_owned(),
                entry.stem.clone(),
                entry.size.to_string(),
                entry.modified.clone(),
            ];
            row.extend(report.media.iter().map(|m| {
                match entry.art.get(m) {
                    Some(ArtState::Present { .. }) => "present",
                    Some(ArtState::Missing) => "missing",
                    Some(ArtState::Unsupported) | None => "unsupported",
                }
                .to_string()
            }));
            write_row(&mut w, &row)?;
        }
    }
    w.flush()
}

/// The serialised name of a media, e.g. `cover`.
fn media_name(media: Media) -> String {
    match media {
        Media::Cover => "cover",
        Media::Screenshot => "screenshot",
        Media::Title => "title",
    }
    .to_string()
}

fn write_row<W: Write>(w: &mut W, fields: &[String]) -> io::Result<()> {
    let line: Vec<String> = fields.iter().map(|f| quote(f)).collect();
    writeln!(w, "{}", line.join(","))
}

/// `field` as a CSV field: quoted (with `"` doubled) when it holds a
/// comma, a quote or a line break, or starts or ends with a space.
///
/// ROM and folder names come from files the user does not control, so a
/// field a spreadsheet would read as a formula (starting with `=`, `+`,
/// `-`, `@`, a tab or a carriage return) is prefixed with `'` (OWASP CSV
/// injection guidance).
fn quote(field: &str) -> String {
    let guarded;
    let field = if field.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        guarded = format!("'{field}");
        guarded.as_str()
    } else {
        field
    };
    let needs =
        field.contains([',', '"', '\n', '\r']) || field.starts_with(' ') || field.ends_with(' ');
    if needs {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_names_match_their_serialised_form() {
        for media in Media::ALL {
            let json = serde_json::to_value(media).unwrap();
            assert_eq!(json.as_str(), Some(media_name(media).as_str()));
        }
    }

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(quote("Zelda (USA)"), "Zelda (USA)");
        assert_eq!(quote("Tom, Jerry"), "\"Tom, Jerry\"");
        assert_eq!(quote("Say \"hi\""), "\"Say \"\"hi\"\"\"");
        assert_eq!(quote("two\nlines"), "\"two\nlines\"");
        assert_eq!(quote(" padded"), "\" padded\"");
        assert_eq!(quote(""), "");
    }

    #[test]
    fn neutralises_spreadsheet_formulas() {
        assert_eq!(
            quote("=HYPERLINK(\"http://x\")"),
            "\"'=HYPERLINK(\"\"http://x\"\")\""
        );
        assert_eq!(quote("+1"), "'+1");
        assert_eq!(quote("-cmd"), "'-cmd");
        assert_eq!(quote("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(quote("\tTab"), "'\tTab");
        assert_eq!(quote("Mega Man X"), "Mega Man X");
    }
}
