//! The optional Switch name catalogue (adjustments D2–D6, D10): its own
//! `catalog.sqlite`, the streaming titledb / ncas reducers, the refresh
//! rule and the key rejection. Every body comes from a fake `Http` that
//! serves synthetic fixtures; nothing touches the network.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use coverly_core::cache::Cache;
use coverly_core::catalog::{CatalogStatus, SwitchCatalog, TitleNames, CATALOG_FILE};
use coverly_core::consoles::TitleSource;
use coverly_core::net::{Http, HttpRequest, HttpResponse, NetError};
use coverly_core::settings::SwitchCatalogSettings;

const TITLEDB: &str = include_str!("fixtures/catalog/titledb-small.json");
const NCAS: &str = include_str!("fixtures/catalog/ncas-small.json");

const US: &str = "https://raw.githubusercontent.com/blawar/titledb/master/US.en.json";
const ES: &str = "https://raw.githubusercontent.com/blawar/titledb/master/ES.es.json";
const NCAS_URL: &str = "https://raw.githubusercontent.com/blawar/titledb/master/ncas.json";

const QUEST: u64 = 0x0100_0000_00AA_0000;
const BIDI: u64 = 0x0100_0000_00CC_0000;
const CAFE: u64 = 0x0100_0000_00DD_0000;

type Route = Box<dyn Fn(&HttpRequest) -> Result<HttpResponse, NetError> + Send + Sync>;

/// A scripted `Http`: one route per URL, every request recorded.
struct Fake {
    routes: Mutex<Vec<(String, Route)>>,
    seen: Mutex<Vec<HttpRequest>>,
}

impl Fake {
    fn new() -> Fake {
        Fake {
            routes: Mutex::new(Vec::new()),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn route(
        &self,
        url: &str,
        f: impl Fn(&HttpRequest) -> Result<HttpResponse, NetError> + Send + Sync + 'static,
    ) {
        let mut routes = self.routes.lock().unwrap();
        routes.retain(|(u, _)| u != url);
        routes.push((url.to_string(), Box::new(f)));
    }

    fn serve(&self, url: &str, body: &str) {
        let body = body.as_bytes().to_vec();
        self.route(url, move |_| Ok(ok(body.clone())));
    }

    fn seen(&self) -> Vec<HttpRequest> {
        self.seen.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.seen.lock().unwrap().clear();
    }
}

impl Http for Fake {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError> {
        self.seen.lock().unwrap().push(req.clone());
        let routes = self.routes.lock().unwrap();
        match routes.iter().find(|(u, _)| *u == req.url) {
            Some((_, f)) => f(req),
            None => Ok(answer(404)),
        }
    }
}

fn ok(body: Vec<u8>) -> HttpResponse {
    HttpResponse {
        status: 200,
        etag: Some("\"v1\"".into()),
        last_modified: Some("Mon, 14 Sep 2026 05:33:00 GMT".into()),
        content_type: Some("text/plain; charset=utf-8".into()),
        retry_after: None,
        body,
    }
}

fn answer(code: u16) -> HttpResponse {
    HttpResponse {
        status: code,
        etag: None,
        last_modified: None,
        content_type: None,
        retry_after: None,
        body: Vec::new(),
    }
}

fn on() -> SwitchCatalogSettings {
    SwitchCatalogSettings {
        enabled: true,
        ..SwitchCatalogSettings::default()
    }
}

fn day(n: u64) -> SystemTime {
    // 2026-11-01 plus `n` days: a fixed clock, never "now".
    UNIX_EPOCH + Duration::from_secs(1_793_491_200 + n * 86_400)
}

fn ensure(
    cat: &SwitchCatalog,
    http: &dyn Http,
    s: &SwitchCatalogSettings,
    locale: &str,
    now: SystemTime,
) -> CatalogStatus {
    cat.ensure(
        http,
        s,
        locale,
        now,
        &AtomicBool::new(false),
        None,
        &|_, _| {},
    )
}

fn open(dir: &Path) -> SwitchCatalog {
    SwitchCatalog::open(dir).expect("open catalogue")
}

fn ready_count(status: &CatalogStatus) -> u64 {
    match status {
        CatalogStatus::Ready { count, .. } => *count,
        other => panic!("expected ready, got {other:?}"),
    }
}

#[test]
fn off_never_requests() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    let status = ensure(&cat, &http, &SwitchCatalogSettings::default(), "en", day(0));
    assert_eq!(status, CatalogStatus::Off);
    assert!(http.seen().is_empty());
    assert_eq!(
        cat.status(&SwitchCatalogSettings::default()),
        CatalogStatus::Off
    );
}

#[test]
fn never_downloaded_is_pending() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    assert_eq!(cat.status(&on()), CatalogStatus::Pending);
}

#[test]
fn reducer_keeps_only_whitelisted_fields_of_valid_entries() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    let status = ensure(&cat, &http, &on(), "en", day(0));
    assert_eq!(ready_count(&status), 3, "{status:?}");
    let CatalogStatus::Ready {
        updated, fallback, ..
    } = &status
    else {
        unreachable!()
    };
    assert_eq!(updated, "2026-11-01");
    assert!(!fallback);

    let quest = cat.title(QUEST).expect("valid entry kept");
    assert_eq!(
        quest.name, "Synthetic Quest",
        "the first non-empty name wins over the duplicate"
    );
    assert_eq!(quest.publisher.as_deref(), Some("Example Works"));
    assert_eq!(quest.release.as_deref(), Some("2020-03-15"));
    assert_eq!(quest.region.as_deref(), Some("US"));
    assert_eq!(quest.source, TitleSource::Titledb);
    // Icons are off: no icon URL is kept.
    assert_eq!(cat.icon_url(QUEST), None);

    let bidi = cat
        .title(BIDI)
        .expect("entry with rightsId and key: null kept");
    assert_eq!(
        bidi.name, "BidiTest Game",
        "control and bidi characters stripped"
    );
    assert_eq!(bidi.publisher, None);
    assert_eq!(bidi.release, None, "an impossible date is dropped");

    let cafe = cat.title(CAFE).expect("valid");
    assert_eq!(cafe.name, "Caf\u{e9} Tour", "names are NFC");
    assert_eq!(cafe.release.as_deref(), Some("2019-01-01"));
    assert_eq!(cafe.region, None);

    assert_eq!(cat.title(0x0100_0000_00BB_0000), None, "empty name skipped");
    assert_eq!(cat.title(0x0100_0000_0000_0000), None);
    assert_eq!(cat.status(&on()), status);
}

#[test]
fn schema_has_no_key_columns() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    http.serve(NCAS_URL, NCAS);
    ensure(&cat, &http, &on(), "en", day(0));
    cat.ensure_ncas(
        &http,
        &on(),
        day(0),
        &AtomicBool::new(false),
        None,
        &|_, _| {},
    );
    drop(cat);
    let conn = rusqlite::Connection::open(dir.path().join(CATALOG_FILE)).unwrap();
    let mut tables = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    tables.sort();
    assert_eq!(tables, ["catalog_meta", "ncas", "nlib", "titledb"]);
    for table in &tables {
        let cols = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        for col in &cols {
            let c = col.to_ascii_lowercase().replace('_', "");
            assert!(
                ![
                    "key",
                    "rightsid",
                    "titlekey",
                    "description",
                    "cnmts",
                    "cheats"
                ]
                .contains(&c.as_str()),
                "{table}.{col} must not exist"
            );
        }
    }
}

#[test]
fn icon_url_kept_only_with_icons_on_and_the_eshop_host() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    let s = SwitchCatalogSettings {
        icons: true,
        ..on()
    };
    ensure(&cat, &http, &s, "en", day(0));
    assert_eq!(
        cat.icon_url(QUEST).as_deref(),
        Some("https://img-eshop.cdn.nintendo.net/i/0000000000000000000000000000000000000000000000000000000000000001.jpg")
    );
    assert_eq!(cat.icon_url(BIDI), None, "another host is never kept");
    assert_eq!(cat.icon_url(CAFE), None);
}

#[test]
fn turning_icons_on_fetches_once_unconditionally() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    ensure(&cat, &http, &on(), "en", day(0));
    http.clear();
    let s = SwitchCatalogSettings {
        icons: true,
        ..on()
    };
    ensure(&cat, &http, &s, "en", day(1));
    let seen = http.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].if_none_match, None);
    assert_eq!(seen[0].if_modified_since, None);
    assert!(cat.icon_url(QUEST).is_some());
    http.clear();
    ensure(&cat, &http, &s, "en", day(2));
    assert!(http.seen().is_empty(), "only once");
}

#[test]
fn a_non_null_key_rejects_the_import_and_keeps_the_old_rows() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    ensure(&cat, &http, &on(), "en", day(0));

    let poisoned = r#"{"1": {"id": "0100000000EE0000", "name": "New", "key": "00112233445566778899AABBCCDDEEFF"},
                       "2": {"id": "0100000000FF0000", "name": "Other", "key": null}}"#;
    http.serve(US, poisoned);
    let status = ensure(&cat, &http, &on(), "en", day(8));
    assert_eq!(status, CatalogStatus::Rejected);
    assert_eq!(cat.status(&on()), CatalogStatus::Rejected);
    assert!(cat.title(QUEST).is_some(), "old rows intact");
    assert_eq!(cat.title(0x0100_0000_00EE_0000), None);
    assert_eq!(
        cat.title(0x0100_0000_00FF_0000),
        None,
        "nothing of the rejected file is kept"
    );

    // No retry until the next refresh window.
    http.clear();
    ensure(&cat, &http, &on(), "en", day(9));
    assert!(http.seen().is_empty());
}

#[test]
fn a_rejected_first_download_keeps_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(
        US,
        r#"{"1": {"id": "0100000000EE0000", "name": "New", "key": {"nested": 1}}}"#,
    );
    assert_eq!(
        ensure(&cat, &http, &on(), "en", day(0)),
        CatalogStatus::Rejected
    );
    assert_eq!(cat.title(0x0100_0000_00EE_0000), None);
}

#[test]
fn an_html_body_fails_without_touching_rows() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, "<!DOCTYPE html><html><body>Unicorn</body></html>");
    assert_eq!(
        ensure(&cat, &http, &on(), "en", day(0)),
        CatalogStatus::FailedNone
    );
    assert_eq!(cat.status(&on()), CatalogStatus::FailedNone);

    http.serve(US, TITLEDB);
    ensure(&cat, &http, &on(), "en", day(0));
    http.serve(US, "<!DOCTYPE html><html><body>Unicorn</body></html>");
    let status = ensure(&cat, &http, &on(), "en", day(8));
    assert_eq!(
        status,
        CatalogStatus::FailedOld {
            updated: "2026-11-01".into()
        }
    );
    assert!(cat.title(QUEST).is_some());
}

#[test]
fn a_truncated_body_keeps_no_partial_rows() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    let cut = &TITLEDB[..TITLEDB.find("70010000000006").unwrap()];
    http.serve(US, cut);
    assert_eq!(
        ensure(&cat, &http, &on(), "en", day(0)),
        CatalogStatus::FailedNone
    );
    assert_eq!(cat.title(QUEST), None, "the transaction rolled back");
}

#[test]
fn a_body_over_the_cap_fails_with_no_partial_rows() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path()).with_max_bytes(600);
    let http = Fake::new();
    http.serve(US, TITLEDB);
    assert_eq!(
        ensure(&cat, &http, &on(), "en", day(0)),
        CatalogStatus::FailedNone
    );
    assert_eq!(cat.title(QUEST), None);
    let seen = http.seen();
    assert_eq!(
        seen[0].max_bytes, 600,
        "the cap is also asked of the transport"
    );
}

#[test]
fn a_network_error_keeps_the_old_rows() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    ensure(&cat, &http, &on(), "en", day(0));
    http.route(US, |_| Err(NetError::Offline("unreachable".into())));
    let status = ensure(&cat, &http, &on(), "en", day(8));
    assert_eq!(
        status,
        CatalogStatus::FailedOld {
            updated: "2026-11-01".into()
        }
    );
    assert!(cat.title(QUEST).is_some());
    // A failure is retried on the next use.
    http.serve(US, TITLEDB);
    http.clear();
    assert_eq!(ready_count(&ensure(&cat, &http, &on(), "en", day(8))), 3);
    assert_eq!(http.seen().len(), 1);
}

#[test]
fn spanish_locale_reads_es_and_falls_back_to_us_on_404() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(ES, TITLEDB);
    let status = ensure(&cat, &http, &on(), "es", day(0));
    let CatalogStatus::Ready { fallback, .. } = status else {
        panic!("{status:?}")
    };
    assert!(!fallback);
    assert_eq!(
        http.seen()
            .iter()
            .map(|r| r.url.as_str())
            .collect::<Vec<_>>(),
        [ES]
    );

    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.route(ES, |_| Ok(answer(404)));
    http.serve(US, TITLEDB);
    let status = ensure(&cat, &http, &on(), "es-ES", day(0));
    let CatalogStatus::Ready {
        count, fallback, ..
    } = status
    else {
        panic!("{status:?}")
    };
    assert_eq!(count, 3);
    assert!(fallback, "the status says the US list is used");
    assert_eq!(cat.status(&on()), status);
    assert_eq!(
        http.seen()
            .iter()
            .map(|r| r.url.as_str())
            .collect::<Vec<_>>(),
        [ES, US]
    );
}

#[test]
fn spanish_file_that_fails_to_parse_falls_back_to_us() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(ES, "not json");
    http.serve(US, TITLEDB);
    let status = ensure(&cat, &http, &on(), "es", day(0));
    assert!(
        matches!(
            status,
            CatalogStatus::Ready {
                fallback: true,
                count: 3,
                ..
            }
        ),
        "{status:?}"
    );
}

#[test]
fn refresh_at_most_every_refresh_days_with_both_conditional_headers() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    ensure(&cat, &http, &on(), "en", day(0));
    assert_eq!(http.seen()[0].if_none_match, None);
    http.clear();

    ensure(&cat, &http, &on(), "en", day(6));
    assert!(http.seen().is_empty(), "fresh: no request");

    http.route(US, |req| {
        if req.if_none_match.as_deref() == Some("\"v1\"")
            && req.if_modified_since.as_deref() == Some("Mon, 14 Sep 2026 05:33:00 GMT")
        {
            Ok(answer(304))
        } else {
            Ok(ok(b"{}".to_vec()))
        }
    });
    let status = ensure(&cat, &http, &on(), "en", day(7));
    assert_eq!(http.seen().len(), 1);
    assert_eq!(ready_count(&status), 3, "304 keeps the rows");
    let CatalogStatus::Ready { updated, .. } = &status else {
        unreachable!()
    };
    assert_eq!(updated, "2026-11-01", "only the check time moves");
    assert!(cat.title(QUEST).is_some());

    // The 304 moved the check time: the next week is fresh again.
    http.clear();
    ensure(&cat, &http, &on(), "en", day(13));
    assert!(http.seen().is_empty());

    // A longer refresh setting is honoured.
    let s = SwitchCatalogSettings {
        refresh_days: 30,
        ..on()
    };
    ensure(&cat, &http, &s, "en", day(20));
    assert!(http.seen().is_empty());
}

#[test]
fn a_locale_change_fetches_the_other_file_unconditionally_when_due() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    http.serve(ES, TITLEDB);
    ensure(&cat, &http, &on(), "en", day(0));
    http.clear();
    ensure(&cat, &http, &on(), "es", day(1));
    assert!(http.seen().is_empty(), "not due yet");
    ensure(&cat, &http, &on(), "es", day(7));
    let seen = http.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].url, ES);
    assert_eq!(seen[0].if_none_match, None);
    assert_eq!(seen[0].if_modified_since, None);
}

#[test]
fn an_empty_catalogue_never_replaces_the_rows() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    ensure(&cat, &http, &on(), "en", day(0));
    http.serve(US, "{}");
    let status = ensure(&cat, &http, &on(), "en", day(8));
    assert_eq!(
        status,
        CatalogStatus::FailedOld {
            updated: "2026-11-01".into()
        }
    );
    assert!(cat.title(QUEST).is_some());
}

#[test]
fn a_cancel_or_a_passed_deadline_leaves_the_download_pending() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    let cancel = AtomicBool::new(true);
    let status = cat.ensure(&http, &on(), "en", day(0), &cancel, None, &|_, _| {});
    assert_eq!(status, CatalogStatus::Pending);
    assert_eq!(cat.title(QUEST), None);

    let past = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    let status = cat.ensure(
        &http,
        &on(),
        "en",
        day(0),
        &AtomicBool::new(false),
        Some(past),
        &|_, _| {},
    );
    assert_eq!(status, CatalogStatus::Pending);
    assert_eq!(cat.title(QUEST), None);
    assert_eq!(cat.status(&on()), CatalogStatus::Pending);
}

#[test]
fn progress_reports_the_bytes_read() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    let last = Mutex::new(0u64);
    cat.ensure(
        &http,
        &on(),
        "en",
        day(0),
        &AtomicBool::new(false),
        None,
        &|done, _| {
            *last.lock().unwrap() = done;
        },
    );
    assert_eq!(*last.lock().unwrap(), TITLEDB.len() as u64);
}

#[test]
fn the_shared_cache_keeps_its_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    let cache_path = dir.path().join(coverly_core::cache::CACHE_FILE);
    drop(Cache::open(&cache_path).unwrap());
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    ensure(&cat, &http, &on(), "en", day(0));
    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 3);
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name IN ('titledb', 'ncas')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0, "the catalogue never lives in the cache");
}

#[test]
fn only_the_catalogue_file_is_written_and_remove_deletes_it() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    ensure(&cat, &http, &on(), "en", day(0));
    // A read handle is open (the lookups) when remove is called.
    assert!(cat.title(QUEST).is_some());
    let mut names = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    for name in &names {
        assert!(
            [
                CATALOG_FILE.to_string(),
                format!("{CATALOG_FILE}-wal"),
                format!("{CATALOG_FILE}-shm")
            ]
            .contains(name),
            "unexpected file {name}"
        );
    }
    assert!(names.iter().any(|n| n == CATALOG_FILE));

    cat.remove(dir.path()).expect("remove");
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "every catalogue file deleted"
    );

    // Reopening after a remove starts from nothing.
    let cat = open(dir.path());
    assert_eq!(cat.status(&on()), CatalogStatus::Pending);
    assert_eq!(cat.title(QUEST), None);
}

#[test]
fn a_garbage_catalogue_file_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(CATALOG_FILE),
        b"this is not a database at all, not even close....",
    )
    .unwrap();
    let cat = open(dir.path());
    assert_eq!(cat.status(&on()), CatalogStatus::Pending);
}

#[test]
fn ncas_reducer_and_lookup_by_nca_id() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(NCAS_URL, NCAS);
    let status = cat.ensure_ncas(
        &http,
        &on(),
        day(0),
        &AtomicBool::new(false),
        None,
        &|_, _| {},
    );
    assert_eq!(ready_count(&status), 2, "bad rows skipped");
    let mut a1 = [0u8; 16];
    a1[15] = 0xa1;
    assert_eq!(cat.by_nca(&a1), Some((QUEST, 1)));
    let mut b2 = [0u8; 16];
    b2[15] = 0xb2;
    assert_eq!(
        cat.by_nca(&b2),
        Some((0x0100_0000_00AA_0800, 3)),
        "upper-case ids accepted"
    );
    let mut c3 = [0u8; 16];
    c3[15] = 0xc3;
    assert_eq!(cat.by_nca(&c3), None);
    let mut d4 = [0u8; 16];
    d4[15] = 0xd4;
    assert_eq!(
        cat.by_nca(&d4),
        None,
        "a content type out of range is a bad row"
    );

    // Same refresh rule as the region file.
    http.clear();
    cat.ensure_ncas(
        &http,
        &on(),
        day(3),
        &AtomicBool::new(false),
        None,
        &|_, _| {},
    );
    assert!(http.seen().is_empty());
    // The titles file is independent.
    assert_eq!(cat.status(&on()), CatalogStatus::Pending);
}

#[test]
fn ncas_off_never_requests() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(NCAS_URL, NCAS);
    let status = cat.ensure_ncas(
        &http,
        &SwitchCatalogSettings::default(),
        day(0),
        &AtomicBool::new(false),
        None,
        &|_, _| {},
    );
    assert_eq!(status, CatalogStatus::Off);
    assert!(http.seen().is_empty());
}

#[test]
fn status_serialises_with_a_kebab_case_state_tag() {
    let json = serde_json::to_value(CatalogStatus::Ready {
        count: 3,
        updated: "2026-11-01".into(),
        fallback: true,
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({"state": "ready", "count": 3, "updated": "2026-11-01", "fallback": true})
    );
    let json = serde_json::to_value(CatalogStatus::FailedOld {
        updated: "2026-11-01".into(),
    })
    .unwrap();
    assert_eq!(json["state"], "failed-old");
    assert_eq!(
        serde_json::to_value(CatalogStatus::RemoveFailed).unwrap()["state"],
        "remove-failed"
    );
}

#[test]
fn a_retry_after_a_failed_refresh_stays_conditional() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    ensure(&cat, &http, &on(), "en", day(0));
    http.route(US, |_| Err(NetError::Offline("unreachable".into())));
    ensure(&cat, &http, &on(), "en", day(8));

    // The server still has the file the rows came from: a 304, not a
    // second full download.
    http.route(US, |req| {
        if req.if_none_match.as_deref() == Some("\"v1\"")
            && req.if_modified_since.as_deref() == Some("Mon, 14 Sep 2026 05:33:00 GMT")
        {
            Ok(answer(304))
        } else {
            Ok(ok(b"{}".to_vec()))
        }
    });
    http.clear();
    let status = ensure(&cat, &http, &on(), "en", day(9));
    assert_eq!(http.seen().len(), 1);
    assert_eq!(ready_count(&status), 3);
    assert_eq!(cat.status(&on()), status, "the 304 clears the failure");
    http.clear();
    ensure(&cat, &http, &on(), "en", day(10));
    assert!(http.seen().is_empty(), "fresh again after the 304");
}

#[test]
fn a_refresh_cut_short_keeps_reporting_the_old_rows() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    let first = ensure(&cat, &http, &on(), "en", day(0));
    assert_eq!(ready_count(&first), 3);
    let cancel = AtomicBool::new(true);
    let status = cat.ensure(&http, &on(), "en", day(8), &cancel, None, &|_, _| {});
    assert_eq!(status, first, "pending is only for a first download");
    assert!(cat.title(QUEST).is_some());
}
