use std::path::PathBuf;
use std::time::Instant;

use coverly_core::scan::ScanOptions;
use coverly_core::systems::SystemDb;
use coverly_roms_lib::library::view::ArtMark;
use coverly_roms_lib::library::{open_session, OpenRequest};

/// Read-only: scans and resolves art like the `open_library` command.
#[test]
#[ignore = "reads a real ROM library: COVERLY_REAL_ROMS=<dir> cargo test -p coverly-roms --release --test real_library -- --ignored --nocapture"]
fn opens_a_real_library_read_only() {
    let Some(root) = std::env::var_os("COVERLY_REAL_ROMS") else {
        eprintln!("COVERLY_REAL_ROMS is not set: nothing to do");
        return;
    };
    let root = PathBuf::from(root);
    let mut events = 0usize;
    let started = Instant::now();
    let (_, view) = open_session(
        &OpenRequest {
            path: &root,
            profile_id: None,
            scan: &ScanOptions::default(),
            generation: 1,
        },
        &SystemDb::builtin(),
        &mut |_| events += 1,
    )
    .expect("the real library opens");
    let elapsed = started.elapsed();
    let json_bytes = serde_json::to_vec(&view).unwrap().len();
    let missing_cover = view
        .entries
        .iter()
        .filter(|e| e.art.cover == ArtMark::Missing)
        .count();
    let systems = view.systems.iter().filter(|s| s.roms > 0).count();
    eprintln!(
        "root={} profile={} fitted={} systems_with_roms={systems} folders={} entries={} \
         missing_cover={missing_cover} warnings={} progress_events={events} open={elapsed:?} \
         scan_ms={} json_bytes={json_bytes}",
        root.display(),
        view.profile.id,
        view.profile.fitted,
        view.systems.len(),
        view.entries.len(),
        view.warnings.len(),
        view.scan_ms,
    );
    assert!(!view.entries.is_empty());
}
