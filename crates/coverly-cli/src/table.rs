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

#[cfg(test)]
mod tests {
    use super::*;

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
