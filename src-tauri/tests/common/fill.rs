//! A mock libretro server and a small GBA library for fill job tests.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use coverly_core::fill::Endpoints;
use coverly_core::matching::StrategyKind;
use coverly_core::net::testing::{png_bytes, MockServer};
use coverly_core::net::NetConfig;
use coverly_core::profiles::Media;
use coverly_core::write::OverwritePolicy;
use coverly_roms_lib::jobs::view::{FillRequest, JobProgressEvent, JobState, JobView};
use coverly_roms_lib::jobs::{FillConfig, JobEvents};
use tempfile::TempDir;

pub const GBA: &str = "Nintendo - Game Boy Advance";
pub const BOX: &str = "Named_Boxarts";
/// The USA box of `002 Golden Adventure` (an HTML page: it falls back to Japan).
pub const ADVENTURE_USA: &str =
    "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Golden%20Adventure%20(USA).png";

pub struct FillWorld {
    pub lib: TempDir,
    pub dirs: TempDir,
    pub server: MockServer,
    pub cfg: Arc<FillConfig>,
}

// CAUTION (Task 3 review): title matching does not strip numbered
// prefixes yet (tracker M2-13): `001 Golden Sun` is a fuzzy `review`, not
// `auto`, and `004 Gldn Rsh` gets no candidates. If these stems fail as
// written, drop the numbers (or add a DAT hit per ROM) and re-derive the
// plan order and entry ids below; do not copy them blindly.
/// `ROMS/GBA`: `001 Golden Sun` (auto), `002 Golden Adventure` (auto, its
/// USA box is an HTML page, the Japanese one works), `003 Missing Game`
/// (auto, no image on either host: `no-art`), `004 Gldn Rsh` (review).
/// Entry ids are 0..=3 in that order.
pub fn fill_world() -> FillWorld {
    let lib = TempDir::new().unwrap();
    let r = lib.path();
    std::fs::create_dir_all(r.join("anbernic")).unwrap();
    for (name, bytes) in [
        ("001 Golden Sun.gba", "golden-sun-bytes"),
        ("002 Golden Adventure.gba", "adv"),
        ("003 Missing Game.gba", "missing"),
        ("004 Gldn Rsh.gba", "rush"),
    ] {
        super::write(r, &format!("ROMS/GBA/{name}"), bytes.as_bytes());
    }
    let server = MockServer::start();
    server.serve_libretro_root(&[GBA]);
    server.serve_libretro_index(
        GBA,
        BOX,
        &[
            "Golden Sun (USA, Europe).png",
            "Golden Adventure (USA).png",
            "Golden Adventure (Japan).png",
            "Missing Game (USA).png",
            "Golden Rush (USA).png",
        ],
    );
    server.serve_libretro_image(
        GBA,
        BOX,
        "Golden Sun (USA, Europe).png",
        png_bytes(1000, 1400),
    );
    server.serve_libretro_image(
        GBA,
        BOX,
        "Golden Adventure (USA).png",
        b"<html>oops</html>".to_vec(),
    );
    server.serve_libretro_image(
        GBA,
        BOX,
        "Golden Adventure (Japan).png",
        png_bytes(300, 300),
    );
    server.serve_libretro_image(GBA, BOX, "Golden Rush (USA).png", png_bytes(200, 200));
    let dirs = TempDir::new().unwrap();
    let cfg = FillConfig {
        cache_dir: dirs.path().join("cache"),
        data_dir: dirs.path().join("data"),
        endpoints: Endpoints {
            libretro: server.base(),
            libretro_raw: server.url("/raw"),
            database: server.base(),
        },
        net: NetConfig {
            retries: 0,
            ..NetConfig::default()
        },
    };
    FillWorld {
        lib,
        dirs,
        server,
        cfg: Arc::new(cfg),
    }
}

/// Covers for `entry_ids` (or everything), default strategy, only missing.
pub fn request(entry_ids: Option<Vec<u32>>) -> FillRequest {
    FillRequest {
        entry_ids,
        media: vec![Media::Cover],
        strategy: StrategyKind::RomRegion,
        regions: ["rom", "world", "usa", "europe", "japan"]
            .map(String::from)
            .to_vec(),
        overwrite: OverwritePolicy::Never,
    }
}

/// Records every event.
#[derive(Default)]
pub struct Recorder {
    pub progress: Mutex<Vec<JobProgressEvent>>,
    pub changed: Mutex<Vec<JobView>>,
    pub written: Mutex<Vec<(u64, Vec<u32>)>>,
}

impl JobEvents for Recorder {
    fn progress(&self, event: JobProgressEvent) {
        self.progress.lock().unwrap().push(event);
    }
    fn changed(&self, view: JobView) {
        self.changed.lock().unwrap().push(view);
    }
    fn written(&self, generation: u64, ids: Vec<u32>) {
        self.written.lock().unwrap().push((generation, ids));
    }
}

/// Waits (at most 20 s) until the last `changed` event is in `state`.
pub fn wait_for(rec: &Recorder, state: JobState) -> JobView {
    let start = Instant::now();
    loop {
        if let Some(v) = rec.changed.lock().unwrap().last() {
            if v.state == state {
                return v.clone();
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "timed out waiting for {state:?}: {:?}",
            rec.changed.lock().unwrap().last()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
