//! Audit reports on the realistic fixture: JSON and CSV snapshots.

mod common;

use std::path::Path;

use coverly_core::audit::{build_report, write_csv, AuditReport};
use coverly_core::profiles::Media;
use coverly_core::scan::ScanOptions;
use coverly_core::systems::SystemDb;
use filetime::FileTime;
use serde_json::Value;
use walkdir_free::files;

use common::{profile, realistic_library, write};

/// 2023-11-14T22:13:20Z.
const FIXED_MTIME: i64 = 1_700_000_000;

/// The realistic fixture plus an orphan cover, every file dated
/// [`FIXED_MTIME`] so `modified` is stable.
fn fixture() -> tempfile::TempDir {
    let lib = realistic_library();
    write(lib.path(), "ROMS/SFC/Imgs/Old Game.png", "png");
    let time = FileTime::from_unix_time(FIXED_MTIME, 0);
    for file in files(lib.path()) {
        filetime::set_file_mtime(&file, time).unwrap();
    }
    lib
}

fn report(root: &Path) -> AuditReport {
    build_report(
        root,
        &profile("anbernic-stock"),
        &SystemDb::builtin(),
        &ScanOptions::default(),
        &Media::ALL,
    )
}

/// Rewrites every string of `value` so it does not depend on the temp
/// folder or the OS: the root becomes `<root>`, separators become `/`.
fn portable(value: &mut Value, root: &str) {
    match value {
        Value::String(s) => {
            let rest = s.strip_prefix(root).map(|rest| format!("<root>{rest}"));
            if let Some(rest) = rest {
                *s = rest;
            }
            *s = s.replace('\\', "/");
        }
        Value::Array(items) => items.iter_mut().for_each(|v| portable(v, root)),
        Value::Object(map) => map.values_mut().for_each(|v| portable(v, root)),
        _ => {}
    }
}

fn csv(report: &AuditReport, missing_only: bool) -> String {
    let mut out = Vec::new();
    write_csv(report, &mut out, missing_only).unwrap();
    String::from_utf8(out).unwrap().replace('\\', "/")
}

#[test]
fn report_json_snapshot() {
    let lib = fixture();
    let report = report(lib.path());
    let mut json = serde_json::to_value(&report).unwrap();
    portable(&mut json, &lib.path().to_string_lossy());
    insta::assert_json_snapshot!("report_json", json);
}

#[test]
fn report_totals() {
    let lib = fixture();
    let report = report(lib.path());
    let totals = &report.totals;
    assert_eq!(totals.systems_known, 3, "SFC, PS and MD");
    assert_eq!(totals.systems_unknown, 1, "APPS");
    assert_eq!(totals.roms, 4);
    assert_eq!(totals.missing.get(&Media::Cover), Some(&3));
    assert_eq!(totals.complete, 1, "only Zelda has every supported media");
    assert_eq!(totals.orphans, 1, "Old Game.png");
    assert_eq!(report.media, Media::ALL.to_vec());
}

#[test]
fn csv_snapshot() {
    let lib = fixture();
    insta::assert_snapshot!("report_csv", csv(&report(lib.path()), false));
}

#[test]
fn csv_missing_only_snapshot() {
    let lib = fixture();
    let text = csv(&report(lib.path()), true);
    assert!(text.contains("MARIO"));
    assert!(!text.contains("Zelda"));
    insta::assert_snapshot!("report_csv_missing_only", text);
}

#[test]
fn csv_quotes_fields_with_commas() {
    // `"` is not allowed in Windows file names; csv.rs unit-tests quotes.
    let lib = fixture();
    write(lib.path(), "ROMS/SFC/Tom, Jerry.sfc", "rom");
    let text = csv(&report(lib.path()), true);
    assert!(
        text.contains(",\"Tom, Jerry.sfc\",\"Tom, Jerry\","),
        "{text}"
    );
}

/// Every file below a folder, without pulling `walkdir` into the tests.
mod walkdir_free {
    use std::fs;
    use std::path::{Path, PathBuf};

    pub fn files(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.extend(files(&path));
            } else {
                out.push(path);
            }
        }
        out
    }
}
