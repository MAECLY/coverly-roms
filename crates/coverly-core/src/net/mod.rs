//! HTTP for art sources and DATs (spec §6 `net/`, §9 Network, §10, §11):
//! blocking ureq 3 with rustls (as Pluryprint), HTTPS only (loopback HTTP
//! for tests), proxy from the environment, `CoverlyROMs/<ver>` User-Agent,
//! a global limiter, retries with exponential backoff and Retry-After, and
//! body size caps. Error statuses come back as responses; only transport
//! failures, exhausted retries and rate limits are errors. Every HTTPS URL
//! must start with a prefix of the endpoint registry ([`hosts`], spec D30)
//! or one of the user's own URLs.

pub mod hosts;
mod limiter;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;

use std::sync::Arc;
use std::time::Duration;

pub use limiter::{Limiter, Permit};

use crate::credentials::Secret;

pub const USER_AGENT: &str = concat!("CoverlyROMs/", env!("CARGO_PKG_VERSION"));
pub const DEFAULT_MAX_BYTES: u64 = 32 * 1024 * 1024;
const PROXY_ENV: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"];
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// A GET request: URL, optional conditional ETag and Last-Modified, a body
/// size cap, an optional Bearer key (redacted in `Debug`) and whether a 403
/// pauses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    pub if_none_match: Option<String>,
    /// Sent as `If-Modified-Since`: the `Last-Modified` stored from the
    /// previous answer (spec D4).
    pub if_modified_since: Option<String>,
    pub max_bytes: u64,
    /// Sent as `Authorization: Bearer …` (SteamGridDB).
    pub bearer: Option<Secret>,
    /// A 403 pauses the phase (libretro/GitHub rate limit, spec §10)
    /// unless an API source answers 403 for a bad key: then `false`.
    pub forbidden_pauses: bool,
}

/// Any HTTP response, including error statuses such as 404 or 304.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
    pub retry_after: Option<Duration>,
    pub body: Vec<u8>,
}

/// Why a request produced no usable response.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
    #[error("offline or unreachable: {0}")]
    Offline(String),
    #[error("timed out")]
    Timeout,
    #[error("rate limited by {host} (HTTP {status})")]
    RateLimited { host: String, status: u16 },
    #[error("HTTP {status} for {url}")]
    Status { status: u16, url: String },
    #[error("response larger than {limit} bytes")]
    TooLarge { limit: u64 },
    #[error("refused non-HTTPS URL {0}")]
    Insecure(String),
    /// An HTTPS URL outside the endpoint registry ([`hosts::ENDPOINTS`]) and
    /// the user's own URLs: refused before any connection (spec D30).
    #[error("refused unregistered URL {0}")]
    Unregistered(String),
    /// The request could not be built (a malformed URL or header value):
    /// a fault of this request, never a pause.
    #[error("invalid request: {0}")]
    Invalid(String),
}

impl NetError {
    /// Whether this error pauses a whole phase or job instead of failing
    /// one item (spec §10): a rate limit, or a transport failure after the
    /// retries (offline, unreachable, timed out). Pausing errors are never
    /// remembered as permanent failures, so a re-run retries them.
    pub fn pauses(&self) -> bool {
        matches!(
            self,
            NetError::RateLimited { .. } | NetError::Offline(_) | NetError::Timeout
        )
    }

    /// The kind of pause this error causes, if it pauses (see [`pauses`](Self::pauses)).
    pub fn pause_kind(&self) -> Option<crate::progress::PauseKind> {
        use crate::progress::PauseKind;
        match self {
            NetError::RateLimited { .. } => Some(PauseKind::RateLimit),
            NetError::Offline(_) | NetError::Timeout => Some(PauseKind::Offline),
            _ => None,
        }
    }
}

/// The receiver of a streamed body: the response head (with an empty
/// `body`) and a reader over the decoded body.
pub type BodySink<'a> =
    dyn FnMut(&HttpResponse, &mut dyn std::io::Read) -> Result<(), NetError> + 'a;

/// Blocking HTTP GET, injectable so tests and sources can wrap or fake it.
pub trait Http: Send + Sync {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError>;

    /// Like [`get`](Self::get), but a 2xx body goes to `sink` as a reader
    /// instead of into memory (spec D5); the returned response has an empty
    /// `body`. Other statuses (304, 404…) return without calling `sink`.
    /// This default buffers through `get()`, so `PoliteHttp` wrappers and
    /// test fakes keep working; `UreqHttp` passes the live body, capped at
    /// `max_bytes` decoded bytes.
    fn get_stream(&self, req: &HttpRequest, sink: &mut BodySink) -> Result<HttpResponse, NetError> {
        let mut resp = self.get(req)?;
        if !(200..300).contains(&resp.status) {
            return Ok(resp);
        }
        let body = std::mem::take(&mut resp.body);
        sink(&resp, &mut std::io::Cursor::new(body))?;
        Ok(resp)
    }
}

/// Where the proxy comes from: the environment, none, or a fixed URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProxyMode {
    Env,
    Off,
    Manual(String),
}

/// Network settings (spec §9 Network) with the spec defaults.
#[derive(Clone, Debug)]
pub struct NetConfig {
    pub concurrency: usize,
    pub min_interval: Duration,
    pub timeout: Duration,
    pub retries: u32,
    pub backoff_base: Duration,
    pub proxy: ProxyMode,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> HttpRequest {
        HttpRequest {
            url: url.into(),
            if_none_match: None,
            if_modified_since: None,
            max_bytes: DEFAULT_MAX_BYTES,
            bearer: None,
            forbidden_pauses: true,
        }
    }
    pub fn max_bytes(mut self, n: u64) -> HttpRequest {
        self.max_bytes = n;
        self
    }
    pub fn if_none_match(mut self, etag: Option<String>) -> HttpRequest {
        self.if_none_match = etag;
        self
    }
    pub fn if_modified_since(mut self, last_modified: Option<String>) -> HttpRequest {
        self.if_modified_since = last_modified;
        self
    }
    pub fn bearer(mut self, key: Secret) -> HttpRequest {
        self.bearer = Some(key);
        self
    }
    /// A 403 comes back as a response (the source decides what it means).
    pub fn forbidden_passes(mut self) -> HttpRequest {
        self.forbidden_pauses = false;
        self
    }
}

impl Default for NetConfig {
    fn default() -> NetConfig {
        NetConfig {
            concurrency: 4,
            min_interval: Duration::from_millis(150),
            timeout: Duration::from_secs(30),
            retries: 3,
            backoff_base: Duration::from_millis(500),
            proxy: ProxyMode::Env,
        }
    }
}

/// Same rule as Pluryprint's updater: first parseable HTTP(S) CONNECT proxy.
pub fn proxy_from(lookup: impl Fn(&str) -> Option<String>) -> Option<ureq::Proxy> {
    PROXY_ENV.iter().find_map(|name| {
        let value = lookup(name)?;
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        let proxy = ureq::Proxy::new(value).ok()?;
        matches!(
            proxy.protocol(),
            ureq::ProxyProtocol::Http | ureq::ProxyProtocol::Https
        )
        .then_some(proxy)
    })
}

enum UrlKind {
    Https,
    Loopback,
    Refused,
}

/// The authority's host, lower-case; empty when the URL has none.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or("");
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = if authority.starts_with('[') {
        authority
            .split(']')
            .next()
            .map(|h| format!("{h}]"))
            .unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or("").to_string()
    };
    host.to_lowercase()
}

/// Whether the URL's authority is a plain `host[:port]`: a host of ASCII
/// letters, digits, `.` and `-` (or a bracketed IPv6 literal) and an
/// optional numeric port. Rejects userinfo (`@`), `\\`, `%`, whitespace and
/// control characters, which parsers read differently (spec §11).
pub fn plain_authority(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let Some((inner, after)) = v6.split_once(']') else {
            return false;
        };
        let ok = !inner.is_empty()
            && inner
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.');
        if !ok {
            return false;
        }
        match after {
            "" => return true,
            p => match p.strip_prefix(':') {
                Some(port) => ("v6", Some(port)),
                None => return false,
            },
        }
    } else {
        match authority.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        }
    };
    let host_ok = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    let port_ok = port.is_none_or(|p| {
        !p.is_empty()
            && p.len() <= 5
            && p.chars().all(|c| c.is_ascii_digit())
            && p.parse::<u16>().is_ok()
    });
    host_ok && port_ok
}

fn url_kind(url: &str) -> UrlKind {
    let lower = url.to_ascii_lowercase();
    let authority = lower
        .split_once("://")
        .map(|(_, r)| r.split(['/', '?', '#']).next().unwrap_or(""))
        .unwrap_or("");
    if authority.contains('@') || authority.is_empty() || !plain_authority(url) {
        return UrlKind::Refused;
    }
    if lower.starts_with("https://") {
        return UrlKind::Https;
    }
    if lower.starts_with("http://")
        && matches!(host_of(url).as_str(), "127.0.0.1" | "localhost" | "[::1]")
    {
        return UrlKind::Loopback;
    }
    UrlKind::Refused
}

pub struct UreqHttp {
    secure: ureq::Agent,
    loopback: ureq::Agent,
    /// Set when a manual proxy was given but cannot be used: HTTPS requests
    /// then fail instead of silently going direct.
    proxy_error: Option<String>,
    /// The user's own HTTPS URLs admitted beside the registry (spec D30).
    user_urls: Vec<String>,
}

impl UreqHttp {
    pub fn new(cfg: &NetConfig) -> UreqHttp {
        let (proxy, proxy_error) = match &cfg.proxy {
            ProxyMode::Env => (proxy_from(|n| std::env::var(n).ok()), None),
            ProxyMode::Off => (None, None),
            ProxyMode::Manual(url) => match ureq::Proxy::new(url.trim()) {
                Ok(p) => (Some(p), None),
                Err(e) => (None, Some(format!("invalid manual proxy: {e}"))),
            },
        };
        // Loopback (tests, local setups) never follows redirects, so a
        // local URL cannot bounce to a remote plain-HTTP host. A redirect
        // never carries the Authorization header (the ureq 3 default, made
        // explicit): a key sent to an API never reaches its CDN.
        let build = |https_only: bool, redirects: u32, proxy: Option<ureq::Proxy>| {
            ureq::Agent::config_builder()
                .timeout_global(Some(cfg.timeout))
                .http_status_as_error(false)
                .https_only(https_only)
                .max_redirects(redirects)
                .redirect_auth_headers(ureq::config::RedirectAuthHeaders::Never)
                .user_agent(USER_AGENT)
                .proxy(proxy)
                .build()
                .into()
        };
        UreqHttp {
            secure: build(true, 5, proxy),
            loopback: build(false, 0, None),
            proxy_error,
            user_urls: Vec::new(),
        }
    }

    /// Admits the user's own HTTPS URLs (the nlib fallback, an update
    /// endpoint override) by prefix, beside the registry. Settings validate
    /// them; anything that is not a plain HTTPS URL is ignored.
    pub fn with_user_urls(mut self, urls: Vec<String>) -> UreqHttp {
        self.user_urls = urls;
        self
    }

    /// Picks the agent for `url`, refusing it before any connection when it
    /// is not HTTPS (or loopback HTTP) or not in the registry. Loopback HTTP
    /// stays open: it never leaves the machine and never follows redirects
    /// (tests and the CLI's hidden base-URL flags point at local mocks).
    fn agent_for(&self, url: &str) -> Result<&ureq::Agent, NetError> {
        match url_kind(url) {
            UrlKind::Https => {
                let user: Vec<&str> = self.user_urls.iter().map(String::as_str).collect();
                if !hosts::allowed(url, &user) {
                    // Without query or fragment: they may carry credentials
                    // (ScreenScraper) or catalogue data.
                    let public = url.split(['?', '#']).next().unwrap_or_default();
                    return Err(NetError::Unregistered(public.to_string()));
                }
                match &self.proxy_error {
                    Some(e) => Err(NetError::Offline(e.clone())),
                    None => Ok(&self.secure),
                }
            }
            UrlKind::Loopback => Ok(&self.loopback),
            UrlKind::Refused => Err(NetError::Insecure(url.to_string())),
        }
    }

    /// Sends `req` and returns the live response with its head parsed
    /// (`body` still empty).
    fn send(
        &self,
        req: &HttpRequest,
    ) -> Result<(ureq::http::Response<ureq::Body>, HttpResponse), NetError> {
        let agent = self.agent_for(&req.url)?;
        let mut builder = agent.get(&req.url);
        if let Some(tag) = &req.if_none_match {
            builder = builder.header("If-None-Match", tag);
        }
        if let Some(stamp) = &req.if_modified_since {
            builder = builder.header("If-Modified-Since", stamp);
        }
        if let Some(key) = &req.bearer {
            builder = builder.header("Authorization", format!("Bearer {}", key.expose()));
        }
        let resp = builder.call().map_err(transport)?;
        let header = |name: &str| {
            resp.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let head = HttpResponse {
            status: resp.status().as_u16(),
            etag: header("etag"),
            last_modified: header("last-modified"),
            content_type: header("content-type"),
            retry_after: header("retry-after")
                .and_then(|v| retry_after(&v, crate::time::now_unix())),
            body: Vec::new(),
        };
        Ok((resp, head))
    }
}

impl Http for UreqHttp {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError> {
        let (mut resp, mut head) = self.send(req)?;
        if head.status != 304 {
            head.body = read_capped(resp.body_mut(), req.max_bytes)?;
        }
        Ok(head)
    }

    fn get_stream(&self, req: &HttpRequest, sink: &mut BodySink) -> Result<HttpResponse, NetError> {
        let (mut resp, head) = self.send(req)?;
        if !(200..300).contains(&head.status) {
            return Ok(head);
        }
        let mut reader = CappedReader::new(resp.body_mut(), req.max_bytes);
        let outcome = sink(&head, &mut reader);
        // A cap or transport failure inside the body wins over whatever the
        // sink made of the I/O error it saw.
        if let Some(e) = reader.failure.take() {
            return Err(e);
        }
        outcome.map(|()| head)
    }
}

/// The live body capped at `max` decoded bytes (see [`read_capped`]). A
/// failure is kept so `get_stream` reports `TooLarge` or the transport
/// error itself, not the sink's wrapping of it.
struct CappedReader<R> {
    inner: std::io::Take<R>,
    max: u64,
    read: u64,
    failure: Option<NetError>,
}

impl<'a> CappedReader<ureq::BodyReader<'a>> {
    fn new(body: &'a mut ureq::Body, max: u64) -> Self {
        use std::io::Read;
        let inner = body
            .with_config()
            .limit(max.saturating_add(WIRE_SLACK))
            .reader()
            .take(max.saturating_add(1));
        CappedReader {
            inner,
            max,
            read: 0,
            failure: None,
        }
    }
}

impl<R: std::io::Read> std::io::Read for CappedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(e) = &self.failure {
            return Err(std::io::Error::other(e.to_string()));
        }
        match self.inner.read(buf) {
            Ok(n) => {
                self.read = self.read.saturating_add(n as u64);
                if self.read > self.max {
                    let e = NetError::TooLarge { limit: self.max };
                    let io = std::io::Error::other(e.to_string());
                    self.failure = Some(e);
                    return Err(io);
                }
                Ok(n)
            }
            Err(e) => {
                let net = match ureq::Error::from(e) {
                    ureq::Error::BodyExceedsLimit(_) => NetError::TooLarge { limit: self.max },
                    other => transport(other),
                };
                let io = std::io::Error::other(net.to_string());
                self.failure = Some(net);
                Err(io)
            }
        }
    }
}

/// `Retry-After` as delta-seconds or as an HTTP-date relative to `now`
/// (Unix seconds); a date in the past means no wait.
fn retry_after(value: &str, now: i64) -> Option<Duration> {
    let value = value.trim();
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let at = crate::time::unix_from_http_date(value)?;
    Some(Duration::from_secs(at.saturating_sub(now).max(0) as u64))
}

/// Slack on the wire cap so a small body may still carry gzip framing.
const WIRE_SLACK: u64 = 64 * 1024;

/// Reads at most `max` decoded bytes. ureq decodes `Content-Encoding: gzip`
/// itself and its `limit` counts wire bytes, so a gzip bomb would pass it:
/// the decoded stream gets its own cap here.
fn read_capped(body: &mut ureq::Body, max: u64) -> Result<Vec<u8>, NetError> {
    use std::io::Read;
    let too_large = || NetError::TooLarge { limit: max };
    let reader = body
        .with_config()
        .limit(max.saturating_add(WIRE_SLACK))
        .reader();
    let mut out = Vec::new();
    reader
        .take(max.saturating_add(1))
        .read_to_end(&mut out)
        .map_err(|e| match ureq::Error::from(e) {
            ureq::Error::BodyExceedsLimit(_) => too_large(),
            other => transport(other),
        })?;
    if out.len() as u64 > max {
        return Err(too_large());
    }
    Ok(out)
}

fn transport(e: ureq::Error) -> NetError {
    match e {
        ureq::Error::Timeout(_) => NetError::Timeout,
        // Faults of the request itself: retrying or pausing cannot help.
        e @ (ureq::Error::BadUri(_) | ureq::Error::Http(_)) => NetError::Invalid(e.to_string()),
        other => NetError::Offline(other.to_string()),
    }
}

pub struct PoliteHttp<H: Http> {
    inner: H,
    limiter: Arc<Limiter>,
    retries: u32,
    backoff_base: Duration,
}

impl<H: Http> PoliteHttp<H> {
    pub fn new(inner: H, cfg: &NetConfig) -> Self {
        Self::with_limiter(inner, Limiter::new(cfg.concurrency, cfg.min_interval), cfg)
    }
    pub fn with_limiter(inner: H, limiter: Arc<Limiter>, cfg: &NetConfig) -> Self {
        PoliteHttp {
            inner,
            limiter,
            retries: cfg.retries,
            backoff_base: cfg.backoff_base,
        }
    }
    fn backoff(&self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        retry_after
            .map(|d| d.min(MAX_RETRY_AFTER))
            .unwrap_or_else(|| {
                self.backoff_base
                    .checked_mul(2u32.saturating_pow(attempt))
                    .map_or(MAX_BACKOFF, |d| d.min(MAX_BACKOFF))
            })
    }
}

impl<H: Http> PoliteHttp<H> {
    /// The retry loop around one `attempt`, which returns its result and
    /// whether a body already reached the caller: then it is final, since a
    /// retry would feed a sink a second body (spec D5, same retries).
    fn retrying(
        &self,
        req: &HttpRequest,
        attempt_once: &mut dyn FnMut() -> (Result<HttpResponse, NetError>, bool),
    ) -> Result<HttpResponse, NetError> {
        let mut attempt = 0;
        loop {
            let (result, delivered) = {
                let _permit = self.limiter.acquire();
                attempt_once()
            };
            if delivered {
                return result;
            }
            let wait = match &result {
                Ok(r) if r.status == 403 && req.forbidden_pauses => {
                    return Err(NetError::RateLimited {
                        host: host_of(&req.url),
                        status: 403,
                    });
                }
                Ok(r) if r.status == 429 || r.status >= 500 => r.retry_after,
                Err(NetError::Offline(_) | NetError::Timeout) => None,
                _ => return result,
            };
            if attempt >= self.retries {
                return match result {
                    Ok(r) if r.status == 429 => Err(NetError::RateLimited {
                        host: host_of(&req.url),
                        status: 429,
                    }),
                    Ok(r) => Err(NetError::Status {
                        status: r.status,
                        url: req.url.clone(),
                    }),
                    Err(e) => Err(e),
                };
            }
            std::thread::sleep(self.backoff(attempt, wait));
            attempt += 1;
        }
    }
}

impl<H: Http> Http for PoliteHttp<H> {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError> {
        self.retrying(req, &mut || (self.inner.get(req), false))
    }

    fn get_stream(&self, req: &HttpRequest, sink: &mut BodySink) -> Result<HttpResponse, NetError> {
        self.retrying(req, &mut || {
            let mut delivered = false;
            let result = self.inner.get_stream(req, &mut |head, body| {
                delivered = true;
                sink(head, body)
            });
            (result, delivered)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Never;
    impl Http for Never {
        fn get(&self, _: &HttpRequest) -> Result<HttpResponse, NetError> {
            Err(NetError::Timeout)
        }
    }

    #[test]
    fn plain_authority_refuses_ports_out_of_range() {
        assert!(plain_authority("https://cdn2.steamgriddb.com/a.png"));
        assert!(plain_authority("http://127.0.0.1:65535/a.png"));
        assert!(plain_authority("http://[::1]:8080/a.png"));
        assert!(!plain_authority("http://127.0.0.1:65536/a.png"));
        assert!(!plain_authority("http://127.0.0.1:99999/a.png"));
        assert!(!plain_authority("http://[::1]:70000/a.png"));
        assert!(!plain_authority("http://127.0.0.1:/a.png"));
    }

    #[test]
    fn retry_after_takes_seconds_or_an_http_date() {
        let now = 1_700_000_000;
        assert_eq!(retry_after(" 7 ", now), Some(Duration::from_secs(7)));
        assert_eq!(
            retry_after("Tue, 14 Nov 2023 22:13:50 GMT", now),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            retry_after("Tue, 14 Nov 2023 22:00:00 GMT", now),
            Some(Duration::ZERO),
            "a date in the past means retry now"
        );
        assert_eq!(retry_after("soon", now), None);
    }

    #[test]
    fn backoff_is_capped_and_never_overflows() {
        let cfg = NetConfig {
            backoff_base: Duration::from_secs(u64::MAX / 2),
            ..NetConfig::default()
        };
        let polite = PoliteHttp::new(Never, &cfg);
        assert_eq!(polite.backoff(31, None), MAX_BACKOFF);
        assert_eq!(
            polite.backoff(0, Some(Duration::from_secs(3600))),
            MAX_RETRY_AFTER
        );
    }
}
