//! `coverly catalog switch …` and the catalogue inside `coverly organize`
//! (adjustments spec D2–D5, plan P-2), against a MockServer only.

mod common;

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use coverly_core::catalog::CATALOG_FILE;
use coverly_core::net::testing::{MockResponse, MockServer};
use predicates::str::contains;
use tempfile::TempDir;

/// `crate::settings::SETTINGS_ENV` of the CLI.
const SETTINGS_ENV: &str = "COVERLY_SETTINGS";

const TITLEDB: &str = include_str!("../../coverly-core/tests/fixtures/catalog/titledb-small.json");

/// A settings file in `dir` with the catalogue on or off (English UI, so
/// the United States list is asked for).
fn settings(dir: &Path, enabled: bool) -> std::path::PathBuf {
    let path = dir.join("settings.json");
    let json = serde_json::json!({
        "general": { "language": "en" },
        "catalogs": { "switch": { "enabled": enabled } }
    });
    fs::write(&path, json.to_string()).unwrap();
    path
}

fn catalog(settings: &Path, data: &Path, args: &[&str]) -> Command {
    let mut cmd = common::coverly();
    cmd.env(SETTINGS_ENV, settings)
        .args(["catalog", "switch"])
        .args(args)
        .arg("--data-dir")
        .arg(data);
    cmd
}

fn serve(server: &MockServer) {
    server.mock(
        "/US.en.json",
        MockResponse::ok(TITLEDB.as_bytes().to_vec()).header("Content-Type", "application/json"),
    );
}

#[test]
fn status_with_the_setting_off_is_off_and_creates_nothing() {
    let (cfg, data) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let s = settings(cfg.path(), false);
    catalog(&s, data.path(), &["status"])
        .assert()
        .success()
        .stdout("off\n");
    assert!(!data.path().join(CATALOG_FILE).exists());
}

#[test]
fn update_with_the_setting_off_says_how_to_turn_it_on() {
    let (cfg, data) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let s = settings(cfg.path(), false);
    let server = MockServer::start();
    serve(&server);
    catalog(
        &s,
        data.path(),
        &["update", "--titledb-url", &server.base()],
    )
    .assert()
    .code(2)
    .stderr(contains("the Switch catalogue is off"))
    .stderr(contains("catalogs.switch.enabled"));
    assert_eq!(server.requests().len(), 0);
    assert!(!data.path().join(CATALOG_FILE).exists());
}

#[test]
fn update_status_and_remove() {
    let (cfg, data) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let s = settings(cfg.path(), true);
    let server = MockServer::start();
    serve(&server);
    catalog(&s, data.path(), &["status"])
        .assert()
        .success()
        .stdout("on; not downloaded yet\n");
    catalog(
        &s,
        data.path(),
        &["update", "--titledb-url", &server.base()],
    )
    .assert()
    .success()
    .stdout(contains("ready: 3 titles, updated "));
    assert_eq!(server.hits("/US.en.json"), 1);
    // Not due again for a week: no request.
    catalog(
        &s,
        data.path(),
        &["update", "--titledb-url", &server.base()],
    )
    .assert()
    .success()
    .stdout(contains("ready: 3 titles"));
    assert_eq!(server.hits("/US.en.json"), 1);
    catalog(&s, data.path(), &["status"])
        .assert()
        .success()
        .stdout(contains("ready: 3 titles"));
    catalog(&s, data.path(), &["remove"])
        .assert()
        .success()
        .stdout("removed the Switch catalogue\n");
    assert!(!data.path().join(CATALOG_FILE).exists());
    catalog(&s, data.path(), &["remove"])
        .assert()
        .success()
        .stdout("no Switch catalogue to remove\n");
}

#[test]
fn a_failed_download_is_an_error() {
    let (cfg, data) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let s = settings(cfg.path(), true);
    let server = MockServer::start();
    server.mock("/US.en.json", MockResponse::ok(b"<html>no</html>".to_vec()));
    catalog(
        &s,
        data.path(),
        &["update", "--titledb-url", &server.base()],
    )
    .assert()
    .code(1)
    .stderr(contains(
        "Switch catalogue: the download failed; no list yet",
    ));
}

/// `coverly organize` on a pile with one Switch file, catalogue on.
fn organize(settings: &Path, data: &Path, cache: &Path, pile: &Path, extra: &[&str]) -> Command {
    let mut cmd = common::coverly();
    cmd.env(SETTINGS_ENV, settings)
        .arg("organize")
        .args(extra)
        .arg("--data-dir")
        .arg(data)
        .arg("--cache-dir")
        .arg(cache)
        .arg(pile);
    cmd
}

fn switch_pile() -> TempDir {
    let p = TempDir::new().unwrap();
    let file = p.path().join("Synthetic Quest [0100000000AA0000].nsp");
    fs::write(&file, vec![0u8; 4096]).unwrap();
    let old = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    filetime::set_file_mtime(&file, old).unwrap();
    p
}

#[test]
fn offline_organize_never_touches_the_catalogue() {
    let (cfg, data, cache) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let s = settings(cfg.path(), true);
    let server = MockServer::start();
    serve(&server);
    let p = switch_pile();
    let base = server.base();
    organize(
        &s,
        data.path(),
        cache.path(),
        p.path(),
        &["--offline", "--titledb-url", &base],
    )
    .assert()
    .success();
    assert_eq!(server.requests().len(), 0);
    assert!(!data.path().join(CATALOG_FILE).exists());

    // Online, the same pile fetches the list once.
    organize(
        &s,
        data.path(),
        cache.path(),
        p.path(),
        &["--titledb-url", &base, "--database-url", &base],
    )
    .assert()
    .success();
    assert_eq!(server.hits("/US.en.json"), 1);
}

#[test]
fn a_pile_without_switch_items_does_not_fetch_the_catalogue() {
    let (cfg, data, cache) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let s = settings(cfg.path(), true);
    let server = MockServer::start();
    serve(&server);
    let p = TempDir::new().unwrap();
    let file = p.path().join("Game.gba");
    fs::write(&file, vec![0u8; 1024]).unwrap();
    let old = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    filetime::set_file_mtime(&file, old).unwrap();
    let base = server.base();
    organize(
        &s,
        data.path(),
        cache.path(),
        p.path(),
        &["--titledb-url", &base, "--database-url", &base],
    )
    .assert()
    .success();
    assert_eq!(server.hits("/US.en.json"), 0);
}

#[test]
fn online_organize_names_a_switch_file_from_the_catalogue() {
    let (cfg, data, cache) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let s = settings(cfg.path(), true);
    let server = MockServer::start();
    serve(&server);
    // Only the title id in the file name: the name comes from the list.
    let p = TempDir::new().unwrap();
    let file = p.path().join("[0100000000AA0000].nsp");
    fs::write(
        &file,
        coverly_core::consoles::testing::nsp_pfs0(&["a.nca"]).bytes(),
    )
    .unwrap();
    let old = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    filetime::set_file_mtime(&file, old).unwrap();
    let base = server.base();
    let offline = organize(
        &s,
        data.path(),
        cache.path(),
        p.path(),
        &["--offline", "--report", "json"],
    )
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    assert!(!String::from_utf8_lossy(&offline).contains("Synthetic Quest"));
    let online = organize(
        &s,
        data.path(),
        cache.path(),
        p.path(),
        &[
            "--report",
            "json",
            "--titledb-url",
            &base,
            "--database-url",
            &base,
        ],
    )
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let report: serde_json::Value = serde_json::from_slice(&online).unwrap();
    let item = &report["plan"]["items"][0];
    assert_eq!(item["catalogue_name"], "Synthetic Quest", "{report}");
    assert_eq!(server.hits("/US.en.json"), 1);
}
