//! CSV output of an [`AuditReport`] (RFC 4180 quoting, `\n` line ends).

use std::io::{self, Write};

use crate::art::ArtState;
use crate::csv_util::{media_name, write_row};

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
