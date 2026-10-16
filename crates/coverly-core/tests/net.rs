use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::net::{
    proxy_from, Http, HttpRequest, Limiter, NetConfig, NetError, PoliteHttp, UreqHttp, USER_AGENT,
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
