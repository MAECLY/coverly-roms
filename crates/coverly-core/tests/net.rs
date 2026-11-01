use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::net::{
    proxy_from, Http, HttpRequest, Limiter, NetConfig, NetError, PoliteHttp, ProxyMode, UreqHttp,
    USER_AGENT,
};

fn cfg() -> NetConfig {
    NetConfig {
        retries: 2,
        backoff_base: Duration::from_millis(1),
        min_interval: Duration::ZERO,
        timeout: Duration::from_secs(5),
        ..NetConfig::default()
    }
}

fn client() -> PoliteHttp<UreqHttp> {
    let c = cfg();
    PoliteHttp::new(UreqHttp::new(&c), &c)
}

#[test]
fn sends_the_user_agent_and_returns_body_and_etag() {
    let server = MockServer::start();
    server.mock("/a", MockResponse::ok("hello").header("ETag", "\"x1\""));
    let resp = client().get(&HttpRequest::get(server.url("/a"))).unwrap();
    assert_eq!(
        (resp.status, resp.body.as_slice(), resp.etag.as_deref()),
        (200, b"hello".as_slice(), Some("\"x1\""))
    );
    assert!(USER_AGENT.starts_with("CoverlyROMs/"));
    assert_eq!(server.requests()[0].header("user-agent"), Some(USER_AGENT));
}

#[test]
fn conditional_get_sends_if_none_match_and_returns_304() {
    let server = MockServer::start();
    server.mock("/i", MockResponse::status(304));
    let resp = client()
        .get(&HttpRequest::get(server.url("/i")).if_none_match(Some("\"x1\"".into())))
        .unwrap();
    assert_eq!(resp.status, 304);
    assert_eq!(server.requests()[0].header("if-none-match"), Some("\"x1\""));
}

#[test]
fn retries_server_errors_then_succeeds() {
    let server = MockServer::start();
    server.mock_seq(
        "/r",
        vec![
            MockResponse::status(503),
            MockResponse::status(500),
            MockResponse::ok("fine"),
        ],
    );
    assert_eq!(
        client()
            .get(&HttpRequest::get(server.url("/r")))
            .unwrap()
            .body,
        b"fine"
    );
    assert_eq!(server.hits("/r"), 3);
}

#[test]
fn persistent_429_becomes_rate_limited() {
    let server = MockServer::start();
    server.mock("/l", MockResponse::status(429).header("Retry-After", "0"));
    let err = client()
        .get(&HttpRequest::get(server.url("/l")))
        .unwrap_err();
    assert!(
        matches!(err, NetError::RateLimited { status: 429, .. }),
        "{err}"
    );
    assert_eq!(server.hits("/l"), 3);
}

#[test]
fn forbidden_is_rate_limited_without_retry() {
    let server = MockServer::start();
    server.mock("/f", MockResponse::status(403));
    assert!(matches!(
        client().get(&HttpRequest::get(server.url("/f"))),
        Err(NetError::RateLimited { status: 403, .. })
    ));
    assert_eq!(server.hits("/f"), 1);
}

#[test]
fn not_found_is_a_response() {
    let server = MockServer::start();
    assert_eq!(
        client()
            .get(&HttpRequest::get(server.url("/missing")))
            .unwrap()
            .status,
        404
    );
}

#[test]
fn bodies_over_the_limit_are_refused() {
    let server = MockServer::start();
    server.mock("/big", MockResponse::ok(vec![0u8; 4096]));
    let err = client()
        .get(&HttpRequest::get(server.url("/big")).max_bytes(1024))
        .unwrap_err();
    assert_eq!(err, NetError::TooLarge { limit: 1024 });
}

#[test]
fn gzip_bodies_are_capped_after_decompression() {
    use std::io::Write;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(&vec![0u8; 4 * 1024 * 1024]).unwrap();
    let bomb = gz.finish().unwrap();
    assert!(
        bomb.len() < 64 * 1024,
        "wire size {} is under the cap",
        bomb.len()
    );
    let server = MockServer::start();
    server.mock(
        "/bomb",
        MockResponse::ok(bomb).header("Content-Encoding", "gzip"),
    );
    let err = client()
        .get(&HttpRequest::get(server.url("/bomb")).max_bytes(64 * 1024))
        .unwrap_err();
    assert_eq!(err, NetError::TooLarge { limit: 64 * 1024 });
}

#[test]
fn gzip_bodies_within_the_cap_are_decoded() {
    use std::io::Write;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(b"hello gzip").unwrap();
    let server = MockServer::start();
    server.mock(
        "/gz",
        MockResponse::ok(gz.finish().unwrap()).header("Content-Encoding", "gzip"),
    );
    let resp = client()
        .get(&HttpRequest::get(server.url("/gz")).max_bytes(10))
        .unwrap();
    assert_eq!(resp.body, b"hello gzip");
}

#[test]
fn plain_http_to_a_remote_host_is_refused_before_connecting() {
    let err = UreqHttp::new(&cfg())
        .get(&HttpRequest::get("http://example.com/x"))
        .unwrap_err();
    assert!(matches!(err, NetError::Insecure(_)));
    let err = UreqHttp::new(&cfg())
        .get(&HttpRequest::get("http://127.0.0.1@example.com/x"))
        .unwrap_err();
    assert!(
        matches!(err, NetError::Insecure(_)),
        "userinfo tricks are refused"
    );
}

#[test]
fn an_invalid_manual_proxy_fails_closed() {
    let c = NetConfig {
        proxy: ProxyMode::Manual("http://[::bad".into()),
        ..cfg()
    };
    let err = UreqHttp::new(&c)
        .get(&HttpRequest::get("https://thumbnails.libretro.com/x"))
        .unwrap_err();
    assert!(
        matches!(&err, NetError::Offline(m) if m.contains("proxy")),
        "{err}"
    );
}

#[test]
fn loopback_does_not_follow_redirects() {
    let server = MockServer::start();
    server.mock(
        "/from",
        MockResponse::status(302).header("Location", &server.url("/to")),
    );
    server.mock("/to", MockResponse::ok("moved"));
    let resp = client()
        .get(&HttpRequest::get(server.url("/from")))
        .unwrap();
    assert_eq!(resp.status, 302);
    assert_eq!(server.hits("/to"), 0);
}

#[test]
fn a_closed_port_is_offline() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let err = client()
        .get(&HttpRequest::get(format!("http://127.0.0.1:{port}/x")))
        .unwrap_err();
    assert!(matches!(err, NetError::Offline(_)), "{err}");
}

#[test]
fn limiter_caps_concurrency() {
    let limiter = Limiter::new(2, Duration::ZERO);
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|s| {
        for _ in 0..6 {
            let (limiter, active, peak) = (limiter.clone(), active.clone(), peak.clone());
            s.spawn(move || {
                let _permit = limiter.acquire();
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20));
                active.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    assert!(peak.load(Ordering::SeqCst) <= 2);
}

#[test]
fn limiter_spaces_request_starts() {
    let limiter = Limiter::new(4, Duration::from_millis(50));
    let start = Instant::now();
    for _ in 0..3 {
        drop(limiter.acquire());
    }
    assert!(start.elapsed() >= Duration::from_millis(100));
}

#[test]
fn concurrency_is_clamped_to_one_to_six() {
    assert_eq!(Limiter::new(0, Duration::ZERO).capacity(), 1);
    assert_eq!(Limiter::new(50, Duration::ZERO).capacity(), 6);
}

#[test]
fn proxy_from_environment_values() {
    assert!(
        proxy_from(|n| (n == "HTTPS_PROXY").then(|| "http://proxy:8080".to_string())).is_some()
    );
    assert!(proxy_from(|_| Some("socks5://proxy:1080".to_string())).is_none());
    assert!(proxy_from(|_| Some("  ".to_string())).is_none());
}

use coverly_core::credentials::Secret;

#[test]
fn a_bearer_key_is_sent_and_never_debug_printed() {
    let server = MockServer::start();
    server.mock("/api", MockResponse::ok("{}"));
    let http = UreqHttp::new(&NetConfig::default());
    let req = HttpRequest::get(server.url("/api")).bearer(Secret::new("sk-123"));
    assert_eq!(http.get(&req).unwrap().status, 200);
    assert_eq!(
        server.requests()[0].header("Authorization"),
        Some("Bearer sk-123")
    );
    assert!(!format!("{req:?}").contains("sk-123"));
}

#[test]
fn no_authorization_header_without_a_key() {
    let server = MockServer::start();
    server.mock("/api", MockResponse::ok("{}"));
    let http = UreqHttp::new(&NetConfig::default());
    http.get(&HttpRequest::get(server.url("/api"))).unwrap();
    assert_eq!(server.requests()[0].header("Authorization"), None);
}

#[test]
fn a_403_pauses_unless_the_request_passes_it_through() {
    let server = MockServer::start();
    server.mock("/x", MockResponse::status(403));
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    let polite = PoliteHttp::new(UreqHttp::new(&cfg), &cfg);
    assert!(matches!(
        polite.get(&HttpRequest::get(server.url("/x"))),
        Err(NetError::RateLimited { status: 403, .. })
    ));
    let r = polite
        .get(&HttpRequest::get(server.url("/x")).forbidden_passes())
        .unwrap();
    assert_eq!(r.status, 403);
}

#[test]
fn serve_json_substitutes_the_base_url() {
    let server = MockServer::start();
    server.serve_json("/j", r#"{"url":"{{BASE}}/img.png"}"#);
    let http = UreqHttp::new(&NetConfig::default());
    let body = http.get(&HttpRequest::get(server.url("/j"))).unwrap().body;
    assert_eq!(
        String::from_utf8(body).unwrap(),
        format!(r#"{{"url":"{}/img.png"}}"#, server.base())
    );
}

#[test]
fn an_authority_with_odd_characters_is_refused_before_connecting() {
    for url in [
        "https://evil.com\\.steamgriddb.com/a.png",
        "https://ev%69l.com/a.png",
        "https://evil.com .steamgriddb.com/a.png",
        "https://evil.com\t.steamgriddb.com/a.png",
        "https://host:po rt/a.png",
        "https://host:/a.png",
    ] {
        let err = UreqHttp::new(&cfg())
            .get(&HttpRequest::get(url))
            .unwrap_err();
        assert!(matches!(err, NetError::Insecure(_)), "{url:?} → {err:?}");
        assert!(!err.pauses());
    }
}

#[test]
fn a_request_that_cannot_be_built_fails_without_pausing() {
    let server = MockServer::start();
    server.mock("/k", MockResponse::ok("x"));
    let err = client()
        .get(
            &HttpRequest::get(server.url("/k"))
                .bearer(coverly_core::credentials::Secret::new("a\r\nX")),
        )
        .unwrap_err();
    assert!(matches!(err, NetError::Invalid(_)), "{err:?}");
    assert!(!err.pauses());
    assert_eq!(err.pause_kind(), None);
}

// ── Conditional GET with Last-Modified, streaming bodies (spec D4, D5) ──

#[test]
fn if_modified_since_is_sent_and_last_modified_read_back() {
    let server = MockServer::start();
    let stamp = "Mon, 14 Sep 2026 10:00:00 GMT";
    server.mock_seq(
        "/c",
        vec![
            MockResponse::ok("v1")
                .header("ETag", "\"e1\"")
                .header("Last-Modified", stamp),
            MockResponse::status(304)
                .header("ETag", "\"e1\"")
                .header("Last-Modified", stamp),
        ],
    );
    let first = client().get(&HttpRequest::get(server.url("/c"))).unwrap();
    assert_eq!(first.last_modified.as_deref(), Some(stamp));
    assert_eq!(server.requests()[0].header("if-modified-since"), None);
    let again = client()
        .get(
            &HttpRequest::get(server.url("/c"))
                .if_none_match(first.etag.clone())
                .if_modified_since(first.last_modified.clone()),
        )
        .unwrap();
    assert_eq!(again.status, 304);
    assert!(again.body.is_empty());
    assert_eq!(again.last_modified.as_deref(), Some(stamp));
    let sent = &server.requests()[1];
    assert_eq!(sent.header("if-none-match"), Some("\"e1\""));
    assert_eq!(sent.header("if-modified-since"), Some(stamp));
}

fn stream_all(
    http: &dyn Http,
    req: &HttpRequest,
) -> (
    Result<coverly_core::net::HttpResponse, NetError>,
    Vec<u8>,
    usize,
    usize,
) {
    let (mut got, mut reads, mut calls) = (Vec::new(), 0usize, 0usize);
    let result = http.get_stream(req, &mut |resp, body| {
        calls += 1;
        assert!(resp.body.is_empty(), "the sink never sees a buffered body");
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = body
                .read(&mut buf)
                .map_err(|e| NetError::Offline(e.to_string()))?;
            if n == 0 {
                return Ok(());
            }
            reads += 1;
            got.extend_from_slice(&buf[..n]);
        }
    });
    (result, got, reads, calls)
}

#[test]
fn get_stream_delivers_a_live_body_in_chunks() {
    let body: Vec<u8> = (0..3 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    let server = MockServer::start();
    server.mock(
        "/s",
        MockResponse::ok(body.clone())
            .header("ETag", "\"s1\"")
            .header("Last-Modified", "Mon, 14 Sep 2026 10:00:00 GMT"),
    );
    let req = HttpRequest::get(server.url("/s")).max_bytes(4 * 1024 * 1024);
    let (result, got, reads, calls) = stream_all(&client(), &req);
    let resp = result.unwrap();
    assert_eq!((resp.status, calls), (200, 1));
    assert!(resp.body.is_empty());
    assert_eq!(resp.etag.as_deref(), Some("\"s1\""));
    assert!(reads > 1, "read in chunks, not all at once ({reads})");
    assert_eq!(got, body);
}

#[test]
fn get_stream_stops_at_the_decoded_byte_cap() {
    let server = MockServer::start();
    server.mock("/big", MockResponse::ok(vec![7u8; 3 * 1024 * 1024]));
    let req = HttpRequest::get(server.url("/big")).max_bytes(1024 * 1024);
    let (result, got, _, _) = stream_all(&client(), &req);
    assert_eq!(
        result.unwrap_err(),
        NetError::TooLarge { limit: 1024 * 1024 }
    );
    assert!(got.len() as u64 <= 1024 * 1024);
}

#[test]
fn get_stream_caps_gzip_after_decompression() {
    use std::io::Write;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(&vec![0u8; 4 * 1024 * 1024]).unwrap();
    let server = MockServer::start();
    server.mock(
        "/bomb",
        MockResponse::ok(gz.finish().unwrap()).header("Content-Encoding", "gzip"),
    );
    let req = HttpRequest::get(server.url("/bomb")).max_bytes(64 * 1024);
    let (result, _, _, _) = stream_all(&client(), &req);
    assert_eq!(result.unwrap_err(), NetError::TooLarge { limit: 64 * 1024 });
}

#[test]
fn get_stream_skips_the_sink_for_non_success_statuses() {
    let server = MockServer::start();
    server.mock("/nm", MockResponse::status(304));
    for path in ["/nm", "/missing"] {
        let (result, _, _, calls) = stream_all(&client(), &HttpRequest::get(server.url(path)));
        let status = result.unwrap().status;
        assert!(status == 304 || status == 404, "{status}");
        assert_eq!(calls, 0, "{path}: no sink call without a 2xx body");
    }
}

#[test]
fn polite_get_stream_retries_before_the_body_starts() {
    let server = MockServer::start();
    server.mock_seq(
        "/rs",
        vec![MockResponse::status(503), MockResponse::ok("streamed")],
    );
    let (result, got, _, calls) = stream_all(&client(), &HttpRequest::get(server.url("/rs")));
    assert_eq!(result.unwrap().status, 200);
    assert_eq!((got.as_slice(), calls), (b"streamed".as_slice(), 1));
    assert_eq!(server.hits("/rs"), 2);
}

struct Canned(Vec<u8>);
impl Http for Canned {
    fn get(&self, _: &HttpRequest) -> Result<coverly_core::net::HttpResponse, NetError> {
        Ok(coverly_core::net::HttpResponse {
            status: 200,
            etag: Some("\"f\"".into()),
            last_modified: None,
            content_type: None,
            retry_after: None,
            body: self.0.clone(),
        })
    }
}

#[test]
fn the_default_get_stream_wraps_get_for_fakes_and_polite() {
    let fake = Canned(b"fake body".to_vec());
    let (result, got, _, calls) = stream_all(&fake, &HttpRequest::get("https://x/"));
    let resp = result.unwrap();
    assert_eq!(
        (resp.status, calls, got.as_slice()),
        (200, 1, b"fake body".as_slice())
    );
    assert!(resp.body.is_empty());
    assert_eq!(resp.etag.as_deref(), Some("\"f\""));
    let polite = PoliteHttp::new(Canned(b"polite".to_vec()), &cfg());
    let (_, got, _, _) = stream_all(&polite, &HttpRequest::get("https://x/"));
    assert_eq!(got, b"polite");
}

// ── The endpoint registry (spec D30) ──

/// A client whose HTTPS requests go through `proxy`: a request that passes
/// the registry shows up there as a `CONNECT`, a refused one never does.
fn proxied(proxy: &MockServer, user: Vec<String>) -> UreqHttp {
    let c = NetConfig {
        proxy: ProxyMode::Manual(proxy.base()),
        ..cfg()
    };
    UreqHttp::new(&c).with_user_urls(user)
}

#[test]
fn an_unregistered_prefix_is_refused_before_any_connection() {
    let proxy = MockServer::start();
    let http = proxied(&proxy, Vec::new());
    for url in [
        "https://example.com/x",
        "https://raw.githubusercontent.com/other/repo/master/x.json",
        "https://raw.githubusercontent.com/blawar/titledb/../../other/x",
        "https://thumbnails.libretro.com.evil.com/x",
    ] {
        let err = http.get(&HttpRequest::get(url)).unwrap_err();
        assert!(matches!(err, NetError::Unregistered(_)), "{url} → {err:?}");
        assert!(!err.pauses());
        let (result, _, _, calls) = stream_all(&http, &HttpRequest::get(url));
        assert!(matches!(result, Err(NetError::Unregistered(_))), "{url}");
        assert_eq!(calls, 0);
    }
    assert!(proxy.requests().is_empty(), "{:?}", proxy.requests());
}

#[test]
fn a_refused_url_is_reported_without_its_query_or_fragment() {
    let http = UreqHttp::new(&cfg());
    let err = http
        .get(&HttpRequest::get(
            "https://example.com/api/x?devpassword=hunter2#frag",
        ))
        .unwrap_err();
    assert_eq!(
        err,
        NetError::Unregistered("https://example.com/api/x".into())
    );
    assert!(!format!("{err} {err:?}").contains("hunter2"));
}

#[test]
fn a_registered_prefix_reaches_the_network() {
    let proxy = MockServer::start();
    let http = proxied(&proxy, Vec::new());
    let err = http
        .get(&HttpRequest::get(
            "https://raw.githubusercontent.com/blawar/titledb/master/US.en.json",
        ))
        .unwrap_err();
    assert!(!matches!(err, NetError::Unregistered(_)), "{err:?}");
    assert_eq!(
        proxy.requests()[0].path,
        "raw.githubusercontent.com:443",
        "the request went out through the proxy"
    );
}

#[test]
fn the_users_own_url_passes_by_prefix() {
    let proxy = MockServer::start();
    let http = proxied(&proxy, vec!["https://mirror.example/nlib/".into()]);
    for url in [
        "https://mirror.example/other/x",
        "https://mirror.example.evil/nlib/x",
    ] {
        let err = http.get(&HttpRequest::get(url)).unwrap_err();
        assert!(matches!(err, NetError::Unregistered(_)), "{url} → {err:?}");
    }
    assert!(proxy.requests().is_empty());
    let err = http
        .get(&HttpRequest::get(
            "https://mirror.example/nlib/0100000000010000",
        ))
        .unwrap_err();
    assert!(!matches!(err, NetError::Unregistered(_)), "{err:?}");
    assert_eq!(proxy.requests()[0].path, "mirror.example:443");
}

#[test]
fn loopback_stays_open_for_local_tests_and_mocks() {
    let server = MockServer::start();
    server.mock("/l", MockResponse::ok("local"));
    let resp = UreqHttp::new(&cfg())
        .get(&HttpRequest::get(server.url("/l")))
        .unwrap();
    assert_eq!(resp.body, b"local");
}
