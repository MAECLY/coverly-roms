//! Updates (M7, ADR-0008/0011): which manifest each channel reads, the one
//! committed public key, the manifest itself, SemVer, and a verified
//! download. The CLI uses all of it; the app uses the endpoints and the key
//! and lets `tauri-plugin-updater` do the rest with the same rules.

#[cfg(feature = "test-util")]
pub mod testing;
pub mod verify;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::net::{Http, HttpRequest, HttpResponse, NetError};
use crate::settings::UpdateChannel;

pub const STABLE_ENDPOINT: &str =
    "https://github.com/MAECLY/coverly-roms/releases/latest/download/latest.json";
/// The fixed prerelease whose `latest.json` release-channels.yml moves forward.
pub const BETA_TAG: &str = "channel-beta";
pub const BETA_ENDPOINT: &str =
    "https://github.com/MAECLY/coverly-roms/releases/download/channel-beta/latest.json";
/// Replaces the channel's manifest URL (local end-to-end tests).
pub const ENDPOINT_ENV: &str = "COVERLY_UPDATE_ENDPOINT";
/// What `updater.pub` holds until the owner commits the real key.
pub const PUBKEY_PLACEHOLDER: &str = "UNSET";
pub const MAX_MANIFEST_BYTES: u64 = 1 << 20;
pub const MAX_ARTIFACT_BYTES: u64 = 256 << 20;

const PUBKEY_FILE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/updater.pub"));

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    #[error("offline or unreachable: {0}")]
    Offline(String),
    #[error("no update is published at {0}")]
    NotPublished(String),
    #[error("the update manifest is not valid: {0}")]
    BadManifest(String),
    #[error("no update for this platform ({0})")]
    NoPlatform(String),
    #[error("the download failed its signature check and was NOT installed: {0}")]
    Signature(String),
    #[error("this build has no update key, so it cannot update itself; download new versions from the releases page")]
    NoKey,
    #[error("{0}")]
    Network(String),
}

impl From<NetError> for UpdateError {
    fn from(e: NetError) -> Self {
        match e {
            NetError::Offline(_) | NetError::Timeout => UpdateError::Offline(e.to_string()),
            NetError::TooLarge { .. } => UpdateError::BadManifest(e.to_string()),
            other => UpdateError::Network(other.to_string()),
        }
    }
}

pub fn endpoint_for(channel: UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Stable => STABLE_ENDPOINT,
        UpdateChannel::Beta => BETA_ENDPOINT,
    }
}

/// The override when set and non-blank (http or https), else the channel's.
pub fn resolve_endpoint(
    override_value: Option<&str>,
    channel: UpdateChannel,
) -> Result<String, UpdateError> {
    match override_value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(endpoint_for(channel).to_string()),
        Some(v)
            if (v.starts_with("https://") || v.starts_with("http://"))
                && !v.contains(char::is_whitespace) =>
        {
            Ok(v.to_string())
        }
        Some(v) => Err(UpdateError::Network(format!(
            "invalid update endpoint {v:?}"
        ))),
    }
}

/// The first line that is not blank or a `#` comment, unless it is the
/// placeholder. A leading UTF-8 BOM (some Windows editors add one) is ignored.
pub fn parse_pubkey_file(text: &str) -> Option<&str> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .filter(|l| *l != PUBKEY_PLACEHOLDER)
}

/// The key this build trusts (`updater.pub`), or `None` while it is `UNSET`.
pub fn release_pubkey() -> Option<&'static str> {
    parse_pubkey_file(PUBKEY_FILE)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlatformEntry {
    pub url: String,
    pub signature: String,
}

/// `latest.json` as tauri-action writes it, plus our `cli-<os>-<arch>` keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub pub_date: Option<String>,
    #[serde(default)]
    pub platforms: BTreeMap<String, PlatformEntry>,
}

fn parse_version(v: &str) -> Option<semver::Version> {
    semver::Version::parse(v.trim().trim_start_matches(['v', 'V'])).ok()
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, UpdateError> {
    let m: Manifest =
        serde_json::from_slice(bytes).map_err(|e| UpdateError::BadManifest(e.to_string()))?;
    if parse_version(&m.version).is_none() {
        return Err(UpdateError::BadManifest(format!(
            "{:?} is not a version",
            m.version
        )));
    }
    Ok(m)
}

/// SemVer precedence (build metadata ignored); unparseable is never newer.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    match (parse_version(current), parse_version(candidate)) {
        (Some(cur), Some(new)) => new.cmp_precedence(&cur).is_gt(),
        _ => false,
    }
}

pub fn cli_platform_key_for(os: &str, arch: &str) -> Option<String> {
    let os = match os {
        "macos" => "darwin",
        "windows" => "windows",
        "linux" => "linux",
        _ => return None,
    };
    matches!(arch, "x86_64" | "aarch64").then(|| format!("cli-{os}-{arch}"))
}

pub fn cli_platform_key() -> Option<String> {
    cli_platform_key_for(std::env::consts::OS, std::env::consts::ARCH)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    UpToDate { latest: String },
    Available(Manifest),
}

fn get(http: &dyn Http, url: &str, cap: u64) -> Result<Vec<u8>, UpdateError> {
    ok_body(http.get(&HttpRequest::get(url).max_bytes(cap))?, url)
}

fn ok_body(resp: HttpResponse, url: &str) -> Result<Vec<u8>, UpdateError> {
    match resp.status {
        200..=299 => Ok(resp.body),
        404 => Err(UpdateError::NotPublished(url.to_string())),
        s => Err(UpdateError::Network(format!("HTTP {s} for {url}"))),
    }
}

/// Reads the manifest at `url` (one request) and compares it with `current`.
pub fn check(http: &dyn Http, url: &str, current: &str) -> Result<Check, UpdateError> {
    let m = parse_manifest(&get(http, url, MAX_MANIFEST_BYTES)?)?;
    Ok(if is_newer(current, &m.version) {
        Check::Available(m)
    } else {
        Check::UpToDate { latest: m.version }
    })
}

/// Downloads `platform_key`'s file and returns it only when its signature
/// verifies with `pubkey` and is bound to `manifest.version`.
pub fn download_verified(
    http: &dyn Http,
    manifest: &Manifest,
    platform_key: &str,
    pubkey: &str,
) -> Result<Vec<u8>, UpdateError> {
    let entry = manifest
        .platforms
        .get(platform_key)
        .ok_or_else(|| UpdateError::NoPlatform(platform_key.to_string()))?;
    // An oversized file is not a bad manifest: report it as such.
    let bytes = match http.get(&HttpRequest::get(&entry.url).max_bytes(MAX_ARTIFACT_BYTES)) {
        Err(NetError::TooLarge { limit }) => {
            return Err(UpdateError::Network(format!(
                "the update file is larger than {limit} bytes"
            )))
        }
        Err(e) => return Err(e.into()),
        Ok(resp) => ok_body(resp, &entry.url)?,
    };
    verify::verify_signed(&bytes, &entry.signature, pubkey, &manifest.version)
        .map_err(|e| UpdateError::Signature(e.to_string()))?;
    Ok(bytes)
}
