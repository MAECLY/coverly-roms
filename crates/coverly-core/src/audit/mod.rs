//! Audit reports: which ROMs of a library lack which art.
//!
//! [`build_report`] scans a library and resolves its art into an
//! [`AuditReport`], a plain serialisable model: JSON comes from `serde`,
//! CSV from [`write_csv`]. Read-only, like the scan it is built on.

mod csv;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::art::{resolve_art, ArtState, RomArt};
use crate::profiles::{Media, Profile};
use crate::scan::{scan_library, ScanOptions, ScanWarning};
use crate::systems::SystemDb;

pub use csv::write_csv;

/// Art audit of one library.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AuditReport {
    /// Library root as given.
    pub root: PathBuf,
    /// Id of the layout profile used.
    pub profile_id: String,
    /// Media audited, in the order requested.
    pub media: Vec<Media>,
    /// Counts over the whole library.
    pub totals: Totals,
    /// Every folder of the ROMs root, known or not, in scan order.
    pub systems: Vec<SystemReport>,
    /// Problems met while scanning folders and art folders.
    pub warnings: Vec<ScanWarning>,
}

/// Counts over the whole library.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Totals {
    /// Folders matching a known system.
    pub systems_known: usize,
    /// Folders matching no system (listed, not scanned).
    pub systems_unknown: usize,
    /// ROMs found (a multi-file game counts once).
    pub roms: usize,
    /// ROMs missing each audited media.
    pub missing: BTreeMap<Media, usize>,
    /// ROMs missing none of the audited media (unsupported media do not
    /// count as missing).
    pub complete: usize,
    /// Images in art folders that belong to no ROM.
    pub orphans: usize,
}

/// Audit of one folder of the ROMs root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SystemReport {
    /// Folder name as on disk, e.g. `SFC`.
    pub folder: String,
    /// Id of the matching system; `None` for unknown folders.
    pub system_id: Option<String>,
    /// ROMs found.
    pub roms: usize,
    /// ROMs missing each audited media.
    pub missing: BTreeMap<Media, usize>,
    /// Images in this system's art folders that belong to no ROM, sorted.
    pub orphans: Vec<PathBuf>,
    /// Every ROM with its art states, in scan order.
    pub entries: Vec<EntryReport>,
}

/// Audit of one ROM.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EntryReport {
    /// Primary file relative to the system folder, e.g. `Hacks/Sonic.md`.
    pub rel_path: PathBuf,
    /// File name of the primary file without its extension.
    pub stem: String,
    /// Size of the primary file plus its companions, in bytes.
    pub size: u64,
    /// Last modification of the primary file, RFC 3339 in UTC
    /// (`2023-11-14T22:13:20Z`).
    pub modified: String,
    /// State of each audited media.
    pub art: BTreeMap<Media, ArtState>,
}

impl EntryReport {
    /// Whether any audited media is missing.
    pub fn is_missing_any(&self) -> bool {
        self.art.values().any(|s| *s == ArtState::Missing)
    }
}

/// Scans `root` laid out as `profile` and audits `media` for every ROM.
pub fn build_report(
    root: &Path,
    profile: &Profile,
    systems: &SystemDb,
    opts: &ScanOptions,
    media: &[Media],
) -> AuditReport {
    let scan = scan_library(root, profile, systems, opts);
    let art = resolve_art(&scan, profile, systems, media);
    let zero = || media.iter().map(|&m| (m, 0)).collect::<BTreeMap<_, _>>();

    let mut totals = Totals {
        systems_known: 0,
        systems_unknown: 0,
        roms: 0,
        missing: zero(),
        complete: 0,
        orphans: 0,
    };
    let mut warnings = scan.warnings.clone();
    let mut reports = Vec::with_capacity(art.len());

    for system in art {
        if system.scan.system_id.is_some() {
            totals.systems_known += 1;
        } else {
            totals.systems_unknown += 1;
        }
        let mut missing = zero();
        let entries: Vec<EntryReport> = system
            .roms
            .into_iter()
            .map(|rom_art| entry(&system.scan.dir, rom_art))
            .collect();
        for entry in &entries {
            for (m, state) in &entry.art {
                if *state == ArtState::Missing {
                    *missing.entry(*m).or_default() += 1;
                    *totals.missing.entry(*m).or_default() += 1;
                }
            }
            if !entry.is_missing_any() {
                totals.complete += 1;
            }
        }
        totals.roms += entries.len();
        totals.orphans += system.orphans.len();
        warnings.extend(system.warnings);
        reports.push(SystemReport {
            folder: system.scan.folder_name,
            system_id: system.scan.system_id,
            roms: entries.len(),
            missing,
            orphans: system.orphans,
            entries,
        });
    }

    AuditReport {
        root: scan.root,
        profile_id: scan.profile_id,
        media: media.to_vec(),
        totals,
        systems: reports,
        warnings,
    }
}

fn entry(system_dir: &Path, rom_art: RomArt) -> EntryReport {
    let rom = rom_art.rom;
    let rel_path = rom
        .path
        .strip_prefix(system_dir)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| rom.rel_dir.join(rom.path.file_name().unwrap_or_default()));
    EntryReport {
        rel_path,
        stem: rom.stem,
        size: rom.size,
        modified: rfc3339_utc(rom.modified),
        art: rom_art.art,
    }
}

/// `time` as RFC 3339 in UTC with whole seconds, e.g.
/// `2023-11-14T22:13:20Z`.
fn rfc3339_utc(time: SystemTime) -> String {
    let secs: i64 = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).unwrap_or(i64::MAX),
        Err(before) => {
            let d = before.duration();
            // Round towards the past: -0.5 s is 23:59:59 the day before.
            let whole = i64::try_from(d.as_secs()).unwrap_or(i64::MAX);
            -whole - i64::from(d.subsec_nanos() > 0)
        }
    };
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Proleptic Gregorian date of a day count since 1970-01-01 (Howard
/// Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: i64) -> String {
        let time = if secs >= 0 {
            UNIX_EPOCH + Duration::from_secs(secs as u64)
        } else {
            UNIX_EPOCH - Duration::from_secs(secs.unsigned_abs())
        };
        rfc3339_utc(time)
    }

    #[test]
    fn formats_rfc3339_utc() {
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(at(4_102_444_799), "2099-12-31T23:59:59Z");
        assert_eq!(at(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn sub_second_times_before_the_epoch_round_down() {
        let time = UNIX_EPOCH - Duration::from_millis(500);
        assert_eq!(rfc3339_utc(time), "1969-12-31T23:59:59Z");
        let time = UNIX_EPOCH + Duration::from_millis(1500);
        assert_eq!(rfc3339_utc(time), "1970-01-01T00:00:01Z");
    }
}
