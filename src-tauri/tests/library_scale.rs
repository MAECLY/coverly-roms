mod common;

use std::time::{Duration, Instant};

use common::{anbernic_tree, open, TWENTY_SYSTEMS};

#[test]
fn opens_twelve_thousand_roms() {
    // 20 systems × 600 ROMs, every other one with a cover.
    let tree = anbernic_tree(&TWENTY_SYSTEMS, 600, 2);
    let started = Instant::now();
    let (_, view) = open(tree.path(), None).unwrap();
    let elapsed = started.elapsed();
    let bytes = serde_json::to_vec(&view).unwrap().len();
    eprintln!("12k tree: open {elapsed:?}, LibraryView JSON {bytes} bytes");
    assert_eq!(view.entries.len(), 12_000);
    assert_eq!(view.systems.iter().filter(|s| s.roms == 600).count(), 20);
    assert!(elapsed < Duration::from_secs(20), "open took {elapsed:?}");
    assert!(bytes < 6 * 1024 * 1024, "LibraryView is {bytes} bytes");
}
