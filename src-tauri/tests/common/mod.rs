//! Fixture libraries for the app's integration tests.

#![allow(dead_code)] // each test binary uses a different subset

use std::fs;
use std::path::Path;

use coverly_core::scan::ScanOptions;
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::library::view::LibraryView;
use coverly_roms_lib::library::{open_session, OpenRequest, Session};
use tempfile::TempDir;

/// Twenty folder names the built-in system database resolves.
pub const TWENTY_SYSTEMS: [&str; 20] = [
    "A2600", "A7800", "A800", "ATARIST", "C64", "FC", "FDS", "GB", "GBA", "GBC", "GG", "LYNX",
    "MD", "MSX", "NDS", "NGP", "PCE", "SFC", "SMS", "WS",
];

/// Writes `bytes` to `root/rel`, creating parent folders.
pub fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// An Anbernic-stock SD (`anbernic/`, `ROMS/<SYS>/…`) with `per_system`
/// ROMs `Game NNNNN (USA).<first extension of the system>` in each folder;
/// every `art_every`-th ROM (0 = none) has a cover in `Imgs`.
pub fn anbernic_tree(folders: &[&str], per_system: usize, art_every: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    let db = SystemDb::builtin();
    fs::create_dir_all(dir.path().join("anbernic")).unwrap();
    for folder in folders {
        let system = db
            .resolve_folder(folder)
            .unwrap_or_else(|| panic!("{folder} is not a known system folder"));
        let ext = system
            .extensions
            .first()
            .unwrap_or_else(|| panic!("{folder} lists no extensions"));
        for i in 0..per_system {
            let stem = format!("Game {i:05} (USA)");
            write(dir.path(), &format!("ROMS/{folder}/{stem}.{ext}"), b"x");
            if art_every > 0 && i % art_every == 0 {
                write(
                    dir.path(),
                    &format!("ROMS/{folder}/Imgs/{stem}.png"),
                    b"png",
                );
            }
        }
    }
    dir
}

/// Opens `path` with default scan options as generation 7.
pub fn open(path: &Path, profile_id: Option<&str>) -> Result<(Session, LibraryView), IpcError> {
    open_session(
        &OpenRequest {
            path,
            profile_id,
            scan: &ScanOptions::default(),
            generation: 7,
        },
        &SystemDb::builtin(),
        &mut |_| {},
    )
}
