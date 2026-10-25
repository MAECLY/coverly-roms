//! Plain-text, column-aligned tables (no colours, no box drawing).

/// Renders `rows` as columns separated by two spaces, each column as wide
/// as its widest cell (counted in `char`s); trailing spaces are trimmed.
pub fn render(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.get(c))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for row in rows {
        let mut line = String::new();
        for (c, cell) in row.iter().enumerate() {
            if c > 0 {
                line.push_str("  ");
            }
            line.push_str(cell);
            let pad = widths[c].saturating_sub(cell.chars().count());
            line.extend(std::iter::repeat_n(' ', pad));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// `text` with every control character except `\n` (ESC, BEL, CR, C1
/// CSI…) replaced by U+FFFD, so a hostile file name cannot move the
/// cursor, rewrite earlier lines or retitle the terminal when printed.
pub fn terminal_safe(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c != '\n' && c.is_control() {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

/// `text` with every control character, `\n` included, replaced by
/// U+FFFD: for a name printed inside one line of a report.
pub fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

/// `79.6 MB`: decimal units, one decimal.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_safe_strips_escape_sequences() {
        assert_eq!(
            terminal_safe("Game\u{1b}]0;pwned\u{7}\r(USA)\u{9b}2J\nnext"),
            "Game\u{FFFD}]0;pwned\u{FFFD}\u{FFFD}(USA)\u{FFFD}2J\nnext"
        );
        assert_eq!(terminal_safe("Pokémon (Japón)"), "Pokémon (Japón)");
    }

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn aligns_columns_and_trims_trailing_spaces() {
        let text = render(&[
            row(&["FOLDER", "ROMS"]),
            row(&["SFC", "2"]),
            row(&["Ñ", ""]),
        ]);
        assert_eq!(text, "FOLDER  ROMS\nSFC     2\nÑ\n");
    }

    #[test]
    fn ragged_rows_and_empty_input() {
        assert_eq!(render(&[]), "");
        assert_eq!(render(&[row(&["a", "b"]), row(&["ccc"])]), "a    b\nccc\n");
    }
}
