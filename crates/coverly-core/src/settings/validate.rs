//! Range and consistency rules; problems are data (`FieldError`), shown
//! next to the field in the UI.

use serde::Serialize;

use super::model::{ProxyChoice, Settings, KNOWN_SOURCES};
use crate::matching::parse_region_order;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldErrorCode {
    /// Outside `min..=max`.
    Range,
    /// Must not be empty.
    Required,
    /// Not a valid value (wrong type, bad syntax, locked setting).
    Invalid,
    /// Inconsistent with another field (review above auto threshold).
    Order,
    /// Names something that does not exist (profile, source).
    Unknown,
    /// The same value twice.
    Duplicate,
    /// A folder that must be an absolute path.
    NotAbsolute,
}

/// One problem with one field (`field` is a dotted path).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FieldError {
    pub field: String,
    pub code: FieldErrorCode,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl FieldError {
    pub fn new(field: impl Into<String>, code: FieldErrorCode) -> FieldError {
        FieldError {
            field: field.into(),
            code,
            min: None,
            max: None,
        }
    }
}

/// An inclusive numeric range of one setting (shared with the UI through
/// the IPC contract sample).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Range {
    pub field: &'static str,
    pub min: f64,
    pub max: f64,
}

const fn range(field: &'static str, min: f64, max: f64) -> Range {
    Range { field, min, max }
}

pub const RANGES: [Range; 12] = [
    range("art.jpeg_quality", 1.0, 100.0),
    range("art.max_size.width", 16.0, 4096.0),
    range("art.max_size.height", 16.0, 4096.0),
    range("matching.auto_threshold", 0.3, 1.0),
    range("matching.review_threshold", 0.3, 1.0),
    range("matching.max_hash_mb", 1.0, 4096.0),
    range("network.concurrency", 1.0, 6.0),
    range("network.timeout_secs", 5.0, 300.0),
    range("network.retries", 0.0, 10.0),
    range("network.cache_max_mb", 64.0, 102_400.0),
    range("network.index_refresh_hours", 1.0, 720.0),
    range("safety.undo_depth", 1.0, 100.0),
];

fn check(out: &mut Vec<FieldError>, rule: &str, field: String, value: f64) {
    let r = RANGES
        .iter()
        .find(|r| r.field == rule)
        .expect("every checked field has a range");
    if !(r.min..=r.max).contains(&value) || value.is_nan() {
        out.push(FieldError {
            field,
            code: FieldErrorCode::Range,
            min: Some(r.min),
            max: Some(r.max),
        });
    }
}

fn simple(out: &mut Vec<FieldError>, field: &str, value: f64) {
    check(out, field, field.to_string(), value);
}

impl Settings {
    /// Every problem, in a stable order. `known_profiles`: built-in and
    /// custom profile ids.
    pub fn validate(&self, known_profiles: &[String]) -> Vec<FieldError> {
        use FieldErrorCode::*;
        let mut out = Vec::new();
        if let Some(id) = &self.library.default_profile {
            if !known_profiles.iter().any(|k| k == id) {
                out.push(FieldError::new("library.default_profile", Unknown));
            }
        }
        if self.art.media.is_empty() {
            out.push(FieldError::new("art.media", Required));
        }
        for (id, size) in &self.art.max_size {
            if !known_profiles.iter().any(|k| k == id) {
                out.push(FieldError::new(format!("art.max_size.{id}"), Unknown));
                continue;
            }
            check(
                &mut out,
                "art.max_size.width",
                format!("art.max_size.{id}.width"),
                f64::from(size.width),
            );
            check(
                &mut out,
                "art.max_size.height",
                format!("art.max_size.{id}.height"),
                f64::from(size.height),
            );
        }
        simple(
            &mut out,
            "art.jpeg_quality",
            f64::from(self.art.jpeg_quality),
        );
        let m = &self.matching;
        if m.regions.is_empty() {
            out.push(FieldError::new("matching.regions", Required));
        } else if parse_region_order(&m.regions.join(",")).is_err() {
            out.push(FieldError::new("matching.regions", Invalid));
        } else {
            let lower: Vec<String> = m
                .regions
                .iter()
                .map(|r| r.trim().to_ascii_lowercase())
                .collect();
            if (1..lower.len()).any(|i| lower[..i].contains(&lower[i])) {
                out.push(FieldError::new("matching.regions", Duplicate));
            }
        }
        let lang_ok = |l: &String| l.len() == 2 && l.bytes().all(|b| b.is_ascii_lowercase());
        if m.languages.len() > 10 || !m.languages.iter().all(lang_ok) {
            out.push(FieldError::new("matching.languages", Invalid));
        }
        simple(
            &mut out,
            "matching.auto_threshold",
            f64::from(m.auto_threshold),
        );
        simple(
            &mut out,
            "matching.review_threshold",
            f64::from(m.review_threshold),
        );
        if m.review_threshold > m.auto_threshold {
            out.push(FieldError::new("matching.review_threshold", Order));
        }
        simple(&mut out, "matching.max_hash_mb", f64::from(m.max_hash_mb));
        let order = &self.sources.order;
        if order
            .iter()
            .any(|e| !KNOWN_SOURCES.contains(&e.id.as_str()))
        {
            out.push(FieldError::new("sources.order", Unknown));
        } else if (1..order.len()).any(|i| order[..i].iter().any(|o| o.id == order[i].id)) {
            out.push(FieldError::new("sources.order", Duplicate));
        } else if !order.iter().any(|e| e.enabled) {
            out.push(FieldError::new("sources.order", Required));
        }
        let n = &self.network;
        simple(&mut out, "network.concurrency", f64::from(n.concurrency));
        simple(&mut out, "network.timeout_secs", f64::from(n.timeout_secs));
        simple(&mut out, "network.retries", f64::from(n.retries));
        simple(&mut out, "network.cache_max_mb", f64::from(n.cache_max_mb));
        simple(
            &mut out,
            "network.index_refresh_hours",
            f64::from(n.index_refresh_hours),
        );
        if n.proxy == ProxyChoice::Manual {
            let url = n.proxy_url.trim();
            if url.is_empty() {
                out.push(FieldError::new("network.proxy_url", Required));
            } else if !(url.starts_with("http://") || url.starts_with("https://"))
                || url.chars().any(char::is_whitespace)
            {
                out.push(FieldError::new("network.proxy_url", Invalid));
            }
        }
        let s = &self.safety;
        if !s.backup_before_overwrite {
            out.push(FieldError::new("safety.backup_before_overwrite", Invalid));
        }
        if s.backup_dir.as_ref().is_some_and(|d| !d.is_absolute()) {
            out.push(FieldError::new("safety.backup_dir", NotAbsolute));
        }
        simple(&mut out, "safety.undo_depth", f64::from(s.undo_depth));
        if self
            .advanced
            .custom_dir
            .as_ref()
            .is_some_and(|d| !d.is_absolute())
        {
            out.push(FieldError::new("advanced.custom_dir", NotAbsolute));
        }
        out
    }
}
