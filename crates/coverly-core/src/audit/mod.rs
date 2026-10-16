//! Audit reports: which ROMs of a library lack which art.
//!
//! [`build_report`] scans a library and resolves its art into an
//! [`AuditReport`], a plain serialisable model: JSON comes from `serde`,
//! CSV from [`write_csv`]. Read-only, like the scan it is built on.

mod csv;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
        modified: crate::time::rfc3339_utc(rom.modified),
        art: rom_art.art,
    }
}
