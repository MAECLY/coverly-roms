//! The optional Switch name catalogue (adjustments D2–D6, D10): its own
//! `catalog.sqlite`, the streaming titledb / ncas reducers, the refresh
//! rule and the key rejection. Every body comes from a fake `Http` that
//! serves synthetic fixtures; nothing touches the network.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use coverly_core::cache::Cache;
use coverly_core::catalog::{
    CatalogStatus, CatalogTitle, NlibOutcome, SwitchCatalog, TitleNames, CATALOG_FILE,
};
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

// ---- nlib fallback (D9) and eShop icons (D8) ----

const NLIB_BODY: &str = include_str!("fixtures/catalog/nlib-0100000000010000.json");
const ODYSSEY: u64 = 0x0100_0000_0001_0000;
const NLIB_EN: &str = "https://api.nlib.cc/nx/0100000000010000?lang=en";
const NLIB_ES: &str = "https://api.nlib.cc/nx/0100000000010000?lang=es";
const QUEST_ICON: &str = "https://img-eshop.cdn.nintendo.net/i/0000000000000000000000000000000000000000000000000000000000000001.jpg";

fn fallback() -> SwitchCatalogSettings {
    SwitchCatalogSettings {
        fallback: true,
        ..on()
    }
}

fn with_icons() -> SwitchCatalogSettings {
    SwitchCatalogSettings {
        icons: true,
        ..on()
    }
}

fn ask(
    cat: &SwitchCatalog,
    http: &dyn Http,
    s: &SwitchCatalogSettings,
    locale: &str,
    tids: &[u64],
    now: SystemTime,
) -> NlibOutcome {
    cat.ensure_nlib(http, s, locale, tids, now, &AtomicBool::new(false), None)
}

#[test]
fn nlib_asks_the_upper_case_id_and_caches_a_hit_for_30_days() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(NLIB_EN, NLIB_BODY);
    let out = ask(&cat, &http, &fallback(), "en", &[ODYSSEY], day(0));
    assert_eq!((out.asked, out.found), (1, 1));
    assert!(out.paused.is_none());
    let seen = http.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].url, NLIB_EN);
    assert_eq!(seen[0].max_bytes, 64 * 1024);
    assert_eq!(
        cat.title(ODYSSEY),
        Some(CatalogTitle {
            name: "Synthetic Odyssey".into(),
            publisher: Some("Example Works".into()),
            release: Some("2017-10-27".into()),
            region: None,
            source: TitleSource::Nlib,
        })
    );
    http.clear();
    ask(&cat, &http, &fallback(), "en", &[ODYSSEY], day(29));
    assert!(http.seen().is_empty(), "a hit is fresh for 30 days");
    ask(&cat, &http, &fallback(), "en", &[ODYSSEY], day(30));
    assert_eq!(http.seen().len(), 1, "asked again after 30 days");
}

#[test]
fn nlib_asks_in_spanish_for_a_spanish_ui_and_never_twice_per_id() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(NLIB_ES, NLIB_BODY);
    let out = ask(
        &cat,
        &http,
        &fallback(),
        "es-ES",
        &[ODYSSEY, ODYSSEY],
        day(0),
    );
    assert_eq!((out.asked, out.found), (1, 1));
    assert_eq!(http.seen().len(), 1);
    assert_eq!(http.seen()[0].url, NLIB_ES);
}

#[test]
fn nlib_misses_are_cached_for_7_days() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.route(NLIB_EN, |_| {
        Ok(HttpResponse {
            body: br#"{"success":false,"error":"Game not found"}"#.to_vec(),
            ..answer(404)
        })
    });
    let out = ask(&cat, &http, &fallback(), "en", &[ODYSSEY], day(0));
    assert_eq!((out.asked, out.found), (1, 0));
    assert_eq!(cat.title(ODYSSEY), None);
    http.clear();
    ask(&cat, &http, &fallback(), "en", &[ODYSSEY], day(6));
    assert!(http.seen().is_empty(), "a miss is remembered for 7 days");
    ask(&cat, &http, &fallback(), "en", &[ODYSSEY], day(7));
    assert_eq!(http.seen().len(), 1);
}

#[test]
fn nlib_body_without_a_name_is_a_miss_and_unknown_fields_are_never_stored() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(NLIB_EN, NLIB_BODY);
    let other = "https://api.nlib.cc/nx/0100000000020000?lang=en";
    http.serve(
        other,
        r#"{"id":"0100000000020000","name":"  ","rightsId":"x"}"#,
    );
    let out = ask(
        &cat,
        &http,
        &fallback(),
        "en",
        &[ODYSSEY, 0x0100_0000_0002_0000],
        day(0),
    );
    assert_eq!((out.asked, out.found), (2, 1));
    assert_eq!(cat.title(0x0100_0000_0002_0000), None);
    drop(cat);
    let mut raw = Vec::new();
    for suffix in ["", "-wal"] {
        if let Ok(b) = std::fs::read(dir.path().join(format!("{CATALOG_FILE}{suffix}"))) {
            raw.extend(b);
        }
    }
    let text = String::from_utf8_lossy(&raw);
    for never in [
        "01000000000100000000000000000000",
        "Synthetic description",
        "Synthetic intro",
        "70010000009999",
    ] {
        assert!(!text.contains(never), "{never} stored");
    }
}

#[test]
fn nlib_is_never_asked_while_off_or_for_a_title_the_index_knows() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    http.serve(NLIB_EN, NLIB_BODY);
    ensure(&cat, &http, &on(), "en", day(0));
    http.clear();
    // Fallback off.
    let out = ask(&cat, &http, &on(), "en", &[ODYSSEY], day(0));
    assert_eq!(out.asked, 0);
    // Catalogue off, fallback on: the fallback never runs alone.
    let alone = SwitchCatalogSettings {
        enabled: false,
        fallback: true,
        ..SwitchCatalogSettings::default()
    };
    assert_eq!(ask(&cat, &http, &alone, "en", &[ODYSSEY], day(0)).asked, 0);
    // A title the index resolves.
    assert_eq!(
        ask(&cat, &http, &fallback(), "en", &[QUEST], day(0)).asked,
        0
    );
    // A non-HTTPS URL (validation already refuses it) is never used.
    let plain = SwitchCatalogSettings {
        fallback_url: "http://api.nlib.cc".into(),
        ..fallback()
    };
    assert_eq!(ask(&cat, &http, &plain, "en", &[ODYSSEY], day(0)).asked, 0);
    assert!(http.seen().is_empty());
}

#[test]
fn nlib_uses_the_user_url_which_the_registry_admits() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    let url = "https://mirror.example/api/nx/0100000000010000?lang=en";
    http.serve(url, NLIB_BODY);
    let s = SwitchCatalogSettings {
        fallback_url: "https://mirror.example/api/".into(),
        ..fallback()
    };
    let out = ask(&cat, &http, &s, "en", &[ODYSSEY], day(0));
    assert_eq!(out.found, 1);
    assert_eq!(http.seen()[0].url, url);
    assert!(coverly_core::net::hosts::allowed(
        url,
        &["https://mirror.example/api"]
    ));
    assert!(!coverly_core::net::hosts::allowed(url, &[]));
}

#[test]
fn nlib_rate_limit_pauses_and_caches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.route(NLIB_EN, |_| {
        Err(NetError::RateLimited {
            host: "api.nlib.cc".into(),
            status: 429,
        })
    });
    let out = ask(
        &cat,
        &http,
        &fallback(),
        "en",
        &[ODYSSEY, 0x0100_0000_0002_0000],
        day(0),
    );
    assert!(out.paused.is_some());
    assert_eq!(http.seen().len(), 1, "the rest waits for the next run");
    http.clear();
    ask(&cat, &http, &fallback(), "en", &[ODYSSEY], day(0));
    assert_eq!(http.seen().len(), 1, "nothing was cached");
}

#[test]
fn turning_the_fallback_off_forgets_its_answers() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(NLIB_EN, NLIB_BODY);
    ask(&cat, &http, &fallback(), "en", &[ODYSSEY], day(0));
    assert!(cat.title(ODYSSEY).is_some());
    cat.apply_settings(&on()).unwrap();
    assert_eq!(cat.title(ODYSSEY), None);
}

/// A synthetic JPEG of `w`×`h` pixels.
fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 40, 40]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
    out.into_inner()
}

fn icons_ready(dir: &Path, http: &Fake) -> SwitchCatalog {
    let cat = open(dir);
    http.serve(US, TITLEDB);
    ensure(&cat, http, &with_icons(), "en", day(0));
    http.clear();
    cat
}

#[test]
fn an_icon_is_fetched_once_and_cached_as_a_256_px_png() {
    let dir = tempfile::tempdir().unwrap();
    let http = Fake::new();
    let cat = icons_ready(dir.path(), &http);
    let body = jpeg(512, 300);
    http.route(QUEST_ICON, move |_| Ok(ok(body.clone())));
    let path = cat
        .icon(&http, &with_icons(), QUEST)
        .expect("icon")
        .expect("cached");
    let seen = http.seen();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].same_host_redirects,
        "redirects stay on the eShop host"
    );
    assert_eq!(seen[0].max_bytes, 1024 * 1024);
    assert_eq!(path.parent().unwrap(), cat.icon_dir());
    assert_eq!(path.file_name().unwrap(), "0100000000AA0000.png");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..4], b"\x89PNG");
    let png = image::load_from_memory(&bytes).unwrap();
    assert_eq!((png.width(), png.height()), (256, 150));
    http.clear();
    let again = cat.icon(&http, &with_icons(), QUEST).unwrap();
    assert_eq!(again.as_deref(), Some(path.as_path()));
    assert!(http.seen().is_empty(), "served from the cache");
}

#[test]
fn icons_off_never_fetch_and_turning_them_off_deletes_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    let http = Fake::new();
    let cat = icons_ready(dir.path(), &http);
    let body = jpeg(64, 64);
    http.route(QUEST_ICON, move |_| Ok(ok(body.clone())));
    let path = cat.icon(&http, &with_icons(), QUEST).unwrap().unwrap();
    http.clear();
    assert_eq!(cat.icon(&http, &on(), QUEST).unwrap(), None);
    assert!(http.seen().is_empty());

    cat.apply_settings(&on()).unwrap();
    assert!(!path.exists(), "cached icon deleted");
    assert!(!cat.icon_dir().exists());
    assert_eq!(cat.icon_url(QUEST), None, "icon_url cleared");
    // Turning them on again refetches the list once, unconditionally.
    ensure(&cat, &http, &with_icons(), "en", day(1));
    let seen = http.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].if_none_match, None);
    assert_eq!(cat.icon_url(QUEST).as_deref(), Some(QUEST_ICON));
}

#[test]
fn an_icon_that_is_not_a_small_jpeg_or_png_is_never_cached() {
    let dir = tempfile::tempdir().unwrap();
    let http = Fake::new();
    let cat = icons_ready(dir.path(), &http);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("html", b"<!doctype html><title>x</title>".to_vec()),
        ("gif", b"GIF89a\x01\x00\x01\x00\x00\x00\x00;".to_vec()),
        ("huge", {
            let mut b = jpeg(8, 8);
            b.resize(1024 * 1024 + 1, 0);
            b
        }),
    ];
    for (what, body) in cases {
        http.route(QUEST_ICON, move |_| Ok(ok(body.clone())));
        assert!(cat.icon(&http, &with_icons(), QUEST).is_err(), "{what}");
        assert!(
            !cat.icon_dir().join("0100000000AA0000.png").exists(),
            "{what}"
        );
    }
    http.route(QUEST_ICON, |_| Ok(answer(404)));
    assert_eq!(cat.icon(&http, &with_icons(), QUEST).unwrap(), None);
    // No stored URL (another host was dropped at import): no request.
    http.clear();
    assert_eq!(cat.icon(&http, &with_icons(), BIDI).unwrap(), None);
    assert!(http.seen().is_empty());
}

#[test]
fn nlib_is_never_asked_for_an_update_or_dlc_whose_base_the_index_knows() {
    let dir = tempfile::tempdir().unwrap();
    let cat = open(dir.path());
    let http = Fake::new();
    http.serve(US, TITLEDB);
    ensure(&cat, &http, &on(), "en", day(0));
    http.clear();
    // QUEST's update (base | 0x800) and one of its DLC (base ^ 0x1000 + 1)
    // are named from the base title (D7): no title id leaves the machine.
    let update = QUEST | 0x800;
    let dlc = (QUEST ^ 0x1000) + 1;
    let out = ask(&cat, &http, &fallback(), "en", &[update, dlc], day(0));
    assert_eq!(out.asked, 0);
    assert!(http.seen().is_empty(), "{:?}", http.seen());
}

#[test]
fn catalogue_fixtures_are_plain_ascii() {
    // Bidi and combining characters are written as JSON escapes, so the
    // bytes under test are visible in review and never reorder the source.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/catalog");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let bytes = std::fs::read(&path).unwrap();
        if let Some(at) = bytes.iter().position(|b| !b.is_ascii()) {
            panic!("{} has a non-ASCII byte at {at}", path.display());
        }
    }
}
