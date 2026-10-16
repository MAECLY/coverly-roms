//! HTTP for art sources and DATs (spec §6 `net/`, §9 Network, §10, §11):
//! blocking ureq 3 with rustls (as Pluryprint), HTTPS only (loopback HTTP
//! for tests), proxy from the environment, `CoverlyROMs/<ver>` User-Agent,
//! a global limiter, retries with exponential backoff and Retry-After, and
//! body size caps. Error statuses come back as responses; only transport
//! failures, exhausted retries and rate limits are errors.

mod limiter;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;

use std::sync::Arc;
use std::time::Duration;

pub use limiter::{Limiter, Permit};

pub const USER_AGENT: &str = concat!("CoverlyROMs/", env!("CARGO_PKG_VERSION"));
pub const DEFAULT_MAX_BYTES: u64 = 32 * 1024 * 1024;
const PROXY_ENV: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"];
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// A GET request: URL, optional conditional ETag and a body size cap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    pub if_none_match: Option<String>,
    pub max_bytes: u64,
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
}

/// Blocking HTTP GET, injectable so tests and sources can wrap or fake it.
pub trait Http: Send + Sync {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError>;
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
            max_bytes: DEFAULT_MAX_BYTES,
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

fn url_kind(url: &str) -> UrlKind {
    let lower = url.to_ascii_lowercase();
    let authority = lower
        .split_once("://")
        .map(|(_, r)| r.split(['/', '?', '#']).next().unwrap_or(""))
        .unwrap_or("");
    if authority.contains('@') || authority.is_empty() {
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
}

impl UreqHttp {
    pub fn new(cfg: &NetConfig) -> UreqHttp {
        let proxy = match &cfg.proxy {
            ProxyMode::Env => proxy_from(|n| std::env::var(n).ok()),
            ProxyMode::Off => None,
            ProxyMode::Manual(url) => ureq::Proxy::new(url).ok(),
        };
        let build = |https_only: bool, proxy: Option<ureq::Proxy>| -> ureq::Agent {
            ureq::Agent::config_builder()
                .timeout_global(Some(cfg.timeout))
                .http_status_as_error(false)
                .https_only(https_only)
                .max_redirects(5)
                .user_agent(USER_AGENT)
                .proxy(proxy)
                .build()
                .into()
        };
        UreqHttp {
            secure: build(true, proxy),
            loopback: build(false, None),
        }
    }
}

impl Http for UreqHttp {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError> {
        let agent = match url_kind(&req.url) {
            UrlKind::Https => &self.secure,
            UrlKind::Loopback => &self.loopback,
            UrlKind::Refused => return Err(NetError::Insecure(req.url.clone())),
        };
        let mut builder = agent.get(&req.url);
        if let Some(tag) = &req.if_none_match {
            builder = builder.header("If-None-Match", tag);
        }
        let mut resp = builder.call().map_err(transport)?;
        let status = resp.status().as_u16();
        let header = |name: &str| {
            resp.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let (etag, last_modified, content_type) = (
            header("etag"),
            header("last-modified"),
            header("content-type"),
        );
        let retry_after = header("retry-after")
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let body = if status == 304 {
            Vec::new()
        } else {
            read_capped(resp.body_mut(), req.max_bytes)?
        };
        Ok(HttpResponse {
            status,
            etag,
            last_modified,
            content_type,
            retry_after,
            body,
        })
    }
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

impl<H: Http> Http for PoliteHttp<H> {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError> {
        let mut attempt = 0;
        loop {
            let result = {
                let _permit = self.limiter.acquire();
                self.inner.get(req)
            };
            let wait = match &result {
                Ok(r) if r.status == 403 => {
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
