//! The optional Switch catalogue and the credit links from the app backend
//! (adjustments spec D2–D5, D10, D27, plan P-2): status, update, remove,
//! when a plan asks for it, and how `open_credit` resolves an id. Every
//! request goes to a local mock server; no Tauri runtime.

mod common;

use std::fs::{self, File};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use common::write;
use coverly_core::catalog::{CatalogStatus, CATALOG_FILE};
use coverly_core::credits::CreditLink;
use coverly_core::identify::sniff::fixtures;
use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::organize::view::{OrganizeProgressEvent, OrganizeRequest};
use coverly_roms_lib::organize::OrganizeEvents;
use coverly_roms_lib::prefs::credit_url;
use coverly_roms_lib::state::AppState;
use serde_json::json;
use tempfile::TempDir;

const TITLEDB: &str =
    include_str!("../../crates/coverly-core/tests/fixtures/catalog/titledb-small.json");

/// A state whose catalogue and DAT requests all reach `server`.
fn state(config: &TempDir, server: &MockServer) -> AppState {
    let mut base = FillConfig::new(config.path().join("cache"), config.path().join("data"));
    base.endpoints.database = server.url("/db");
    base.net.min_interval = Duration::ZERO;
    AppState::new(config.path().to_path_buf(), base).with_catalog_base(server.base())
}

fn serve_titledb(server: &MockServer) {
    server.mock(
        "/US.en.json",
        MockResponse::ok(TITLEDB.as_bytes().to_vec())
            .header("ETag", "\"v1\"")
            .header("Content-Type", "application/json"),
    );
}

fn switch_on(st: &AppState) {
    let view = st
        .update_settings(&json!({ "catalogs": { "switch": { "enabled": true } } }))
        .unwrap();
    assert!(view.errors.is_empty(), "{:?}", view.errors);
}

fn data_dir(config: &TempDir) -> std::path::PathBuf {
    config.path().join("data")
}

/// Writes `bytes` to `root/rel`, dated an hour ago.
fn write_old(root: &Path, rel: &str, bytes: &[u8]) {
    write(root, rel, bytes);
    File::options()
        .write(true)
        .open(root.join(rel))
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(3600))
        .unwrap();
}

#[derive(Default)]
struct Quiet(Mutex<Vec<OrganizeProgressEvent>>);

impl OrganizeEvents for Quiet {
    fn progress(&self, event: OrganizeProgressEvent) {
        self.0.lock().unwrap().push(event);
    }
}

fn request(source: &Path, offline: bool) -> OrganizeRequest {
    OrganizeRequest {
        source: source.display().to_string(),
        offline: Some(offline),
        ..OrganizeRequest::default()
    }
}

#[test]
fn status_is_off_and_creates_nothing_while_the_setting_is_off() {
    let server = MockServer::start();
    let config = TempDir::new().unwrap();
    let st = state(&config, &server);
    assert_eq!(st.catalog_status(Some("en")), CatalogStatus::Off);
    assert!(!data_dir(&config).join(CATALOG_FILE).exists());
    let json = serde_json::to_value(CatalogStatus::Off).unwrap();
    assert_eq!(json, json!({ "state": "off" }));
}

#[test]
fn update_with_the_setting_off_is_a_bad_request_and_asks_nothing() {
    let server = MockServer::start();
    serve_titledb(&server);
    let config = TempDir::new().unwrap();
    let st = state(&config, &server);
    let err = st.catalog_update(Some("en"), &|_| {}).unwrap_err();
    assert!(matches!(err, IpcError::BadRequest(_)), "{err:?}");
    assert_eq!(server.hits("/US.en.json"), 0);
}

#[test]
fn update_downloads_reports_progress_and_remove_deletes_the_file() {
    let server = MockServer::start();
    serve_titledb(&server);
    let config = TempDir::new().unwrap();
    let st = state(&config, &server);
    switch_on(&st);
    assert_eq!(st.catalog_status(Some("en")), CatalogStatus::Pending);

    let seen = Mutex::new(Vec::new());
    let status = st
        .catalog_update(Some("en"), &|s| seen.lock().unwrap().push(s))
        .unwrap();
    assert!(
        matches!(
            status,
            CatalogStatus::Ready {
                count: 3,
                fallback: false,
                ..
            }
        ),
        "{status:?}"
    );
    assert_eq!(server.hits("/US.en.json"), 1);
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .all(|s| matches!(s, CatalogStatus::Downloading { .. })));
    assert_eq!(st.catalog_status(Some("en")), status);
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["state"], "ready");
    assert_eq!(json["count"], 3);

    // Fresh: a second update sends nothing (at most once a week, D4).
    st.catalog_update(Some("en"), &|_| {}).unwrap();
    assert_eq!(server.hits("/US.en.json"), 1);

    let after = st.catalog_remove().unwrap();
    assert_eq!(after, CatalogStatus::Pending, "still on, nothing kept");
    assert!(!data_dir(&config).join(CATALOG_FILE).exists());
}

#[test]
fn turning_the_setting_off_deletes_the_catalogue() {
    let server = MockServer::start();
    serve_titledb(&server);
    let config = TempDir::new().unwrap();
    let st = state(&config, &server);
    switch_on(&st);
    st.catalog_update(Some("en"), &|_| {}).unwrap();
    assert!(data_dir(&config).join(CATALOG_FILE).exists());
    st.update_settings(&json!({ "catalogs": { "switch": { "enabled": false } } }))
        .unwrap();
    assert!(!data_dir(&config).join(CATALOG_FILE).exists());
    assert_eq!(st.catalog_status(Some("en")), CatalogStatus::Off);
}

#[test]
fn an_icon_is_never_fetched_while_icons_are_off_and_a_bad_id_is_refused() {
    let server = MockServer::start();
    let config = TempDir::new().unwrap();
    let st = state(&config, &server);
    switch_on(&st);
    assert!(matches!(
        st.catalog_icon("0100000000AA0000"),
        Err(IpcError::ArtMissing(_))
    ));
    assert!(matches!(
        st.catalog_icon("not-hex"),
        Err(IpcError::BadRequest(_))
    ));
    assert!(server.requests().is_empty());
}

#[test]
fn a_plan_without_a_switch_item_never_asks_for_the_catalogue() {
    let server = MockServer::start();
    serve_titledb(&server);
    let config = TempDir::new().unwrap();
    let st = state(&config, &server);
    switch_on(&st);
    let pile = TempDir::new().unwrap();
    write_old(pile.path(), "Alpha (USA).gba", &fixtures::gba_tagged(b'A'));
    st.organize_plan(
        &request(pile.path(), false),
        &[],
        Arc::new(Quiet::default()),
    )
    .unwrap();
    assert_eq!(server.hits("/US.en.json"), 0);
    assert!(!data_dir(&config).join(CATALOG_FILE).exists());
}

#[test]
fn an_offline_plan_never_asks_for_the_catalogue() {
    let server = MockServer::start();
    serve_titledb(&server);
    let config = TempDir::new().unwrap();
    let st = state(&config, &server);
    switch_on(&st);
    let pile = TempDir::new().unwrap();
    write_old(pile.path(), "Quest [0100000000AA0000][v0].nsp", b"PFS0");
    st.organize_plan(&request(pile.path(), true), &[], Arc::new(Quiet::default()))
        .unwrap();
    assert_eq!(server.hits("/US.en.json"), 0);
}

#[test]
fn an_online_plan_with_a_switch_item_fetches_the_catalogue_first() {
    let server = MockServer::start();
    serve_titledb(&server);
    let config = TempDir::new().unwrap();
    let st = state(&config, &server);
    switch_on(&st);
    st.catalog_status(Some("en"));
    let pile = TempDir::new().unwrap();
    write_old(pile.path(), "Quest [0100000000AA0000][v0].nsp", b"PFS0");
    st.organize_plan(
        &request(pile.path(), false),
        &[],
        Arc::new(Quiet::default()),
    )
    .unwrap();
    assert_eq!(server.hits("/US.en.json"), 1);
    assert!(matches!(
        st.catalog_status(Some("en")),
        CatalogStatus::Ready { .. }
    ));
    // With the setting off, no plan asks.
    st.update_settings(&json!({ "catalogs": { "switch": { "enabled": false } } }))
        .unwrap();
    st.organize_plan(
        &OrganizeRequest {
            rescan: true,
            ..request(pile.path(), false)
        },
        &[],
        Arc::new(Quiet::default()),
    )
    .unwrap();
    assert_eq!(server.hits("/US.en.json"), 1);
    let _ = fs::remove_dir_all(pile.path());
}

#[test]
fn credit_links_resolve_from_the_compiled_credits_only() {
    assert_eq!(
        credit_url("screenscraper", CreditLink::Site).unwrap(),
        "https://www.screenscraper.fr/"
    );
    assert!(credit_url("screenscraper", CreditLink::License)
        .unwrap()
        .starts_with("https://creativecommons.org/"));
    // Emulator sources carry no link (Q8).
    assert!(matches!(
        credit_url("dolphin", CreditLink::Site),
        Err(IpcError::BadRequest(_))
    ));
    assert!(matches!(
        credit_url("no-such-credit", CreditLink::Site),
        Err(IpcError::BadRequest(_))
    ));
    assert_eq!(
        serde_json::from_value::<CreditLink>(json!("license")).unwrap(),
        CreditLink::License
    );
    assert_eq!(
        serde_json::from_value::<CreditLink>(json!("site")).unwrap(),
        CreditLink::Site
    );
}

#[test]
fn turning_the_setting_off_while_a_plan_downloads_keeps_nothing() {
    // Spec §11: «Catalogue toggled off during a download: download
    // cancelled, tables cleared» — also when the download is a plan's.
    let server = MockServer::start();
    let hang = coverly_core::net::testing::HangingServer::start();
    let config = TempDir::new().unwrap();
    let mut base = FillConfig::new(config.path().join("cache"), config.path().join("data"));
    base.endpoints.database = server.url("/db");
    base.net.min_interval = Duration::ZERO;
    let st = AppState::new(config.path().to_path_buf(), base).with_catalog_base(hang.base());
    let view = st
        .update_settings(&json!({
            "catalogs": { "switch": { "enabled": true } },
            "network": { "timeout_secs": 5, "retries": 0 }
        }))
        .unwrap();
    assert!(view.errors.is_empty(), "{:?}", view.errors);
    let pile = TempDir::new().unwrap();
    write_old(pile.path(), "Quest [0100000000AA0000][v0].nsp", b"PFS0");

    std::thread::scope(|s| {
        let plan = s.spawn(|| {
            st.organize_plan(
                &request(pile.path(), false),
                &[],
                Arc::new(Quiet::default()),
            )
        });
        let start = std::time::Instant::now();
        while hang.connections() == 0 {
            assert!(start.elapsed() < Duration::from_secs(20), "never asked");
            std::thread::sleep(Duration::from_millis(20));
        }
        st.update_settings(&json!({ "catalogs": { "switch": { "enabled": false } } }))
            .unwrap();
        plan.join().unwrap().unwrap();
    });
    assert!(
        !data_dir(&config).join(CATALOG_FILE).exists(),
        "the catalogue the plan held is deleted once the plan lets it go"
    );
    assert_eq!(st.catalog_status(Some("en")), CatalogStatus::Off);
}
