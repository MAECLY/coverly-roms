//! CSV output shared by the audit and job reports: RFC 4180 quoting, `\n` line ends, spreadsheet formulas neutralised.

use std::io::{self, Write};

use crate::profiles::Media;

/// The serialised name of a media, e.g. `cover`.
pub(crate) fn media_name(media: Media) -> String {
    match media {
        Media::Cover => "cover",
        Media::Screenshot => "screenshot",
        Media::Title => "title",
    }
    .to_string()
}

pub(crate) fn write_row<W: Write>(w: &mut W, fields: &[String]) -> io::Result<()> {
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
pub(crate) fn quote(field: &str) -> String {
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
