mod common;

use common::{profile, write};
use coverly_core::scan::{scan_library, scan_library_with_progress, ScanOptions, ScanProgress};
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

#[test]
fn reports_one_event_per_system_folder() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "ROMS/SFC/A.sfc", "a");
    write(root, "ROMS/SFC/B.sfc", "b");
    write(root, "ROMS/GBA/C.gba", "c");
    write(root, "ROMS/APPS/tool.sh", "t");
    write(root, "ROMS/.hidden/x.sfc", "x");
    let db = SystemDb::builtin();
    let p = profile("anbernic-stock");
    let opts = ScanOptions::default();

    let mut events = Vec::new();
    let scan = scan_library_with_progress(root, &p, &db, &opts, &mut |e| events.push(e));

    assert_eq!(events.len(), 4, "{events:?}");
    assert_eq!(
        events[0],
        ScanProgress {
            done: 0,
            total: 3,
            folder: String::new(),
            roms: 0
        }
    );
    let last = events.last().unwrap();
    assert_eq!((last.done, last.total, last.roms), (3, 3, 3));
    let mut folders: Vec<String> = events[1..].iter().map(|e| e.folder.clone()).collect();
    folders.sort();
    assert_eq!(folders, ["APPS", "GBA", "SFC"]);
    assert!(events.windows(2).all(|w| w[0].done + 1 == w[1].done));
    assert_eq!(scan, scan_library(root, &p, &db, &opts));
}

#[test]
fn a_missing_roms_folder_sends_no_progress() {
    let dir = TempDir::new().unwrap();
    let mut events = Vec::new();
    let scan = scan_library_with_progress(
        dir.path(),
        &profile("anbernic-stock"),
        &SystemDb::builtin(),
        &ScanOptions::default(),
        &mut |e| events.push(e),
    );
    assert!(events.is_empty());
    assert_eq!(scan.warnings.len(), 1);
}
