//! `coverly fill` and `coverly undo` end to end, against a local mock
//! server (never the real network).

use std::fs;

use assert_cmd::Command;
use coverly_core::image::probe_dimensions;
use coverly_core::net::testing::{png_bytes, MockResponse, MockServer};
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

const GBA: &str = "Nintendo - Game Boy Advance";

struct Fixture {
    lib: TempDir,
    cache: TempDir,
    data: TempDir,
    server: MockServer,
}

fn serve_gba(server: &MockServer) {
    server.serve_libretro_root(&[GBA]);
    server.serve_libretro_index(
        GBA,
        "Named_Boxarts",
        &["Golden Sun (USA, Europe).png", "Golden Sun (Japan).png"],
    );
    server.serve_libretro_image(
        GBA,
        "Named_Boxarts",
        "Golden Sun (USA, Europe).png",
        png_bytes(500, 700),
    );
    let crc = format!("{:08X}", crc32fast::hash(b"golden-sun-bytes"));
    server.serve_dat(
        "metadat/no-intro/Nintendo - Game Boy Advance.dat",
        &format!(
            "game (\n name \"Golden Sun (USA, Europe)\"\n rom ( name \"g.gba\" size 16 crc {crc} )\n)\n"
        ),
    );
}

fn fixture() -> Fixture {
    let lib = TempDir::new().unwrap();
    fs::create_dir_all(lib.path().join("anbernic")).unwrap();
    fs::create_dir_all(lib.path().join("ROMS/GBA")).unwrap();
    fs::write(
        lib.path().join("ROMS/GBA/001 Golden Sun.gba"),
        b"golden-sun-bytes",
    )
    .unwrap();
    let server = MockServer::start();
    serve_gba(&server);
    Fixture {
        lib,
        cache: TempDir::new().unwrap(),
        data: TempDir::new().unwrap(),
        server,
    }
}

fn fill_cmd() -> Command {
    let mut cmd = Command::cargo_bin("coverly").unwrap();
    cmd.env("COVERLY_SETTINGS", "");
    cmd.arg("fill");
    cmd
}

fn fill_opts(f: &Fixture, cmd: &mut Command) {
    cmd.arg("--cache-dir")
        .arg(f.cache.path())
        .arg("--data-dir")
        .arg(f.data.path())
        .arg("--libretro-url")
        .arg(f.server.base())
        .arg("--libretro-raw-url")
        .arg(f.server.url("/raw"))
        .arg("--database-url")
        .arg(f.server.base());
}

fn fill(f: &Fixture) -> Command {
    let mut cmd = fill_cmd();
    cmd.arg(f.lib.path());
    fill_opts(f, &mut cmd);
    cmd
}

fn cover(f: &Fixture) -> std::path::PathBuf {
    f.lib.path().join("ROMS/GBA/Imgs/001 Golden Sun.png")
}

#[test]
fn dry_run_is_the_default_and_writes_nothing() {
    let f = fixture();
    fill(&f)
        .assert()
        .success()
        .stdout(contains("Dry run"))
        .stdout(contains("GBA"));
    assert!(!cover(&f).exists());
    assert_eq!(
        f.server.hits(
            "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Golden%20Sun%20(USA%2C%20Europe).png"
        ),
        0
    );
}

#[test]
fn apply_writes_a_resized_png_and_undo_removes_it() {
    let f = fixture();
    fill(&f)
        .arg("--apply")
        .assert()
        .success()
        .stdout(contains("filled 1"));
    let (w, h) = probe_dimensions(&fs::read(cover(&f)).unwrap()).unwrap();
    assert!(w <= 250 && h <= 360, "{w}x{h}");
    Command::cargo_bin("coverly")
        .unwrap()
        .env("COVERLY_SETTINGS", "")
        .args(["undo", "--data-dir"])
        .arg(f.data.path())
        .assert()
        .success()
        .stdout(contains("removed 1"));
    assert!(!cover(&f).exists());
}

#[test]
fn relative_root_is_journaled_absolute_so_undo_works_elsewhere() {
    let f = fixture();
    let mut cmd = fill_cmd();
    cmd.current_dir(f.lib.path().parent().unwrap())
        .arg(f.lib.path().file_name().unwrap())
        .arg("--apply");
    fill_opts(&f, &mut cmd);
    cmd.assert().success().stdout(contains("filled 1"));
    assert!(cover(&f).exists());
    let elsewhere = TempDir::new().unwrap();
    Command::cargo_bin("coverly")
        .unwrap()
        .env("COVERLY_SETTINGS", "")
        .current_dir(elsewhere.path())
        .args(["undo", "--data-dir"])
        .arg(f.data.path())
        .assert()
        .success()
        .stdout(contains("removed 1"));
    assert!(!cover(&f).exists());
}

#[test]
fn undo_list_shows_the_job() {
    let f = fixture();
    fill(&f).arg("--apply").assert().success();
    Command::cargo_bin("coverly")
        .unwrap()
        .env("COVERLY_SETTINGS", "")
        .args(["undo", "--list", "--data-dir"])
        .arg(f.data.path())
        .assert()
        .success()
        .stdout(contains("JOB"))
        .stdout(contains("done"));
}

#[test]
fn json_report_contains_the_plan() {
    let f = fixture();
    let out = f.cache.path().join("plan.json");
    fill(&f)
        .args(["--report", "json", "--output"])
        .arg(&out)
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(v["mode"], "dry-run");
    assert_eq!(v["plan"]["totals"]["auto"], 1);
    assert_eq!(
        v["plan"]["items"][0]["chosen"]["name"],
        "Golden Sun (USA, Europe)"
    );
}

#[test]
fn scope_all_requires_backups() {
    let f = fixture();
    fill(&f)
        .args(["--scope", "all"])
        .assert()
        .code(2)
        .stderr(contains("--overwrite backup"));
}

#[test]
fn bad_regions_and_concurrency_are_usage_errors() {
    let f = fixture();
    fill(&f)
        .args(["--regions", "usa,mars"])
        .assert()
        .code(2)
        .stderr(contains("mars"));
    fill(&f).args(["--concurrency", "9"]).assert().code(2);
}

#[test]
fn rate_limit_exits_5() {
    let f = fixture();
    f.server.mock(
        "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/",
        MockResponse::status(429).header("Retry-After", "0"),
    );
    fill(&f).assert().code(5).stderr(contains("rate limited"));
}

#[test]
fn undo_with_no_jobs_fails() {
    let data = TempDir::new().unwrap();
    Command::cargo_bin("coverly")
        .unwrap()
        .env("COVERLY_SETTINGS", "")
        .args(["undo", "--data-dir"])
        .arg(data.path())
        .assert()
        .code(1)
        .stderr(contains("nothing to undo"));
}

#[test]
fn fill_never_touches_paths_outside_the_library() {
    let f = fixture();
    fill(&f).arg("--apply").assert().success();
    let outside: Vec<_> = fs::read_dir(f.lib.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        outside.iter().all(|n| n == "anbernic" || n == "ROMS"),
        "{outside:?}"
    );
}

#[test]
fn batocera_merges_the_gamelist_without_the_old_warning() {
    let lib = TempDir::new().unwrap();
    fs::create_dir_all(lib.path().join("roms/gba")).unwrap();
    fs::write(
        lib.path().join("roms/gba/Golden Sun.gba"),
        b"golden-sun-bytes",
    )
    .unwrap();
    let server = MockServer::start();
    serve_gba(&server);
    let f = Fixture {
        lib,
        cache: TempDir::new().unwrap(),
        data: TempDir::new().unwrap(),
        server,
    };
    fill(&f)
        .args(["--profile", "batocera", "--media", "cover", "--apply"])
        .assert()
        .success()
        .stderr(contains("gamelist.xml is not updated yet").not());
    let gamelist = fs::read_to_string(f.lib.path().join("roms/gba/gamelist.xml")).unwrap();
    assert!(
        gamelist.contains("<path>./Golden Sun.gba</path>"),
        "{gamelist}"
    );
    assert!(
        gamelist.contains("<thumbnail>./images/Golden Sun-image.png</thumbnail>"),
        "{gamelist}"
    );
}

const SGDB_FIX: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../coverly-core/tests/fixtures/steamgriddb/"
);

/// SteamGridDB under `/sgdb` on `server`: search, grids and one CDN image.
fn serve_sgdb(server: &MockServer) {
    use coverly_core::sources::steamgriddb::{grids_path, search_path, SgdbConfig};
    let sg = SgdbConfig {
        base: server.url("/sgdb"),
        ..SgdbConfig::default()
    };
    let f = |n: &str| fs::read_to_string(format!("{SGDB_FIX}{n}")).unwrap();
    server.serve_json(
        &format!("/sgdb{}", search_path("Golden Sun")),
        &f("search_golden_sun.json"),
    );
    server.serve_json(
        &format!("/sgdb{}", grids_path(5248, &sg)),
        &f("grids_5248.json"),
    );
    server.mock(
        "/cdn/grid/80211.png",
        MockResponse::ok(png_bytes(600, 900)).header("Content-Type", "image/png"),
    );
}

#[test]
fn sources_flag_and_env_key_let_popularity_pick_by_votes() {
    let f = fixture();
    serve_sgdb(&f.server);
    let out = fill(&f)
        .arg("--sources")
        .arg("libretro,steamgriddb")
        .arg("--strategy")
        .arg("popularity")
        .arg("--steamgriddb-url")
        .arg(f.server.url("/sgdb"))
        .arg("--report")
        .arg("json")
        .env("COVERLY_SGDB_KEY", "sk-test")
        .assert()
        .success()
        .stdout(contains("\"source\": \"steamgriddb\"").and(contains("\"why\": \"votes\"")))
        .get_output()
        .clone();
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!all.contains("sk-test"), "the key is never printed");
    let auth: Vec<_> = f
        .server
        .requests()
        .into_iter()
        .filter(|r| r.path.starts_with("/sgdb"))
        .collect();
    assert!(!auth.is_empty());
}

#[test]
fn without_a_key_the_cli_warns_and_still_uses_libretro() {
    let f = fixture();
    serve_sgdb(&f.server);
    fill(&f)
        .arg("--sources")
        .arg("libretro,steamgriddb")
        .arg("--steamgriddb-url")
        .arg(f.server.url("/sgdb"))
        .arg("--apply")
        .env_remove("COVERLY_SGDB_KEY")
        .assert()
        .success()
        .stderr(contains("warning: steamgriddb: no API key stored; skipped"))
        .stdout(contains("filled 1"));
    assert!(f
        .server
        .requests()
        .iter()
        .all(|r| !r.path.starts_with("/sgdb")));
}

#[test]
fn an_unknown_source_is_refused() {
    let f = fixture();
    fill(&f)
        .arg("--sources")
        .arg("libretro,nope")
        .assert()
        .code(2)
        .stderr(contains("unknown source `nope`"));
}

#[test]
fn fill_help_names_the_key_variable() {
    fill_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("COVERLY_SGDB_KEY").and(contains("--sources")));
}
