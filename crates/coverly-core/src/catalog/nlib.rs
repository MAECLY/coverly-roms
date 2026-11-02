//! The optional `api.nlib.cc` fallback (adjustments D9): a title id the
//! index does not resolve is asked as `GET {url}/nx/{TID}?lang={en|es}`,
//! with `TID` re-formatted from the parsed `u64` (16 upper-case hex, never
//! the raw file-name text).
//!
//! Response shape, verified against the live API on 2026-11-01 (pinned in
//! `tests/fixtures/catalog/nlib-0100000000010000.json` with synthetic
//! values): a JSON object with `id`, `name`, `publisher`, `releaseDate`
//! (`"YYYY-MM-DD"`), `region` (null) and many fields Coverly never keeps
//! (`description`, `rightsId`, `nsuId`, `screens`, `icon`…). A miss is a
//! 404 with `{"success":false,"error":…}`. Only `name`, `publisher` and
//! `releaseDate` are read, each optional and of any type; every other
//! field is skipped without being held (D1).

use serde::Deserialize;

use super::store::NlibRow;
use super::titledb::{clean_text, iso_date, Lenient};
use crate::net::hosts::is_user_base;

/// Largest nlib answer read (a full answer is about 2 KB).
pub(crate) const MAX_NLIB_BYTES: u64 = 64 * 1024;

/// A hit is asked again after 30 days, a miss after 7.
const HIT_TTL_SECS: i64 = 30 * 24 * 60 * 60;
const MISS_TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// The request URL, or `None` when `base` is not a plain HTTPS base URL
/// (settings already refuse such a value; this never sends one).
pub(crate) fn url(base: &str, tid: u64, lang: &str) -> Option<String> {
    let base = base.trim().trim_end_matches('/');
    is_user_base(base).then(|| format!("{base}/nx/{tid:016X}?lang={lang}"))
}

/// The answer language for a UI locale: Spanish or English.
pub(crate) fn lang(locale: &str) -> &'static str {
    let lang = locale.split(['-', '_']).next().unwrap_or("");
    if lang.eq_ignore_ascii_case("es") {
        "es"
    } else {
        "en"
    }
}

/// Whether a cached answer is still fresh at `now_s`. A clock that went
/// back makes it stale.
pub(crate) fn is_fresh(row: &NlibRow, now_s: i64) -> bool {
    let ttl = if row.miss {
        MISS_TTL_SECS
    } else {
        HIT_TTL_SECS
    };
    now_s >= row.fetched_at && now_s - row.fetched_at < ttl
}

/// The kept fields of a 2xx body; `None` when it is not a JSON object or
/// has no usable name (cached as a miss).
pub(crate) fn parse(body: &[u8]) -> Option<(String, Option<String>, Option<String>)> {
    #[derive(Deserialize)]
    struct Answer {
        #[serde(default)]
        name: Option<Lenient>,
        #[serde(default)]
        publisher: Option<Lenient>,
        #[serde(default, rename = "releaseDate")]
        release_date: Option<Lenient>,
    }
    let answer: Answer = serde_json::from_slice(body).ok()?;
    let text = |v: Option<Lenient>| {
        v.and_then(Lenient::into_str)
            .as_deref()
            .and_then(clean_text)
    };
    let name = text(answer.name)?;
    let release = answer.release_date.and_then(release);
    Some((name, text(answer.publisher), release))
}

/// `"YYYY-MM-DD"` (the live shape) or an integer `YYYYMMDD` (titledb's),
/// validated; anything else is `None`.
fn release(v: Lenient) -> Option<String> {
    match v {
        Lenient::Uint(n) => iso_date(n),
        Lenient::Str(s) => {
            let b = s.as_bytes();
            let shaped = b.len() == 10
                && b[4] == b'-'
                && b[7] == b'-'
                && b.iter()
                    .enumerate()
                    .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
            if !shaped {
                return None;
            }
            iso_date(s.replace('-', "").parse().ok()?)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_use_the_parsed_id_in_upper_case_hex() {
        assert_eq!(
            url("https://api.nlib.cc", 0x0100_0000_00aa_0000, "en").as_deref(),
            Some("https://api.nlib.cc/nx/0100000000AA0000?lang=en")
        );
        assert_eq!(
            url(" https://m.example/api/ ", 1, "es").as_deref(),
            Some("https://m.example/api/nx/0000000000000001?lang=es")
        );
        assert_eq!(url("http://api.nlib.cc", 1, "en"), None);
        assert_eq!(url("https://a.example/?x=1", 1, "en"), None);
        assert_eq!(url("https://u@a.example", 1, "en"), None);
    }

    #[test]
    fn languages() {
        assert_eq!(lang("es"), "es");
        assert_eq!(lang("es-MX"), "es");
        assert_eq!(lang("en"), "en");
        assert_eq!(lang("fr"), "en");
    }

    #[test]
    fn freshness() {
        let hit = NlibRow {
            name: Some("n".into()),
            publisher: None,
            release: None,
            fetched_at: 100,
            miss: false,
        };
        assert!(is_fresh(&hit, 100 + HIT_TTL_SECS - 1));
        assert!(!is_fresh(&hit, 100 + HIT_TTL_SECS));
        assert!(!is_fresh(&hit, 99), "clock went back");
        let miss = NlibRow { miss: true, ..hit };
        assert!(is_fresh(&miss, 100 + MISS_TTL_SECS - 1));
        assert!(!is_fresh(&miss, 100 + MISS_TTL_SECS));
    }

    #[test]
    fn parse_is_defensive() {
        let full = br#"{"id":"x","name":"A\u202eB","publisher":"P","releaseDate":"2017-10-27","rightsId":"r","screens":{"count":1}}"#;
        assert_eq!(
            parse(full),
            Some(("AB".into(), Some("P".into()), Some("2017-10-27".into())))
        );
        assert_eq!(
            parse(br#"{"name":"A","releaseDate":20200315,"publisher":7}"#),
            Some(("A".into(), None, Some("2020-03-15".into())))
        );
        assert_eq!(
            parse(br#"{"name":"A","releaseDate":"2017-13-01"}"#),
            Some(("A".into(), None, None))
        );
        assert_eq!(
            parse(br#"{"name":"A","releaseDate":"+017-10-27"}"#),
            Some(("A".into(), None, None))
        );
        assert_eq!(
            parse(br#"{"success":false,"error":"Game not found"}"#),
            None
        );
        assert_eq!(parse(br#"{"name":null}"#), None);
        assert_eq!(parse(br#"{"name":["x"]}"#), None);
        assert_eq!(parse(b"[1,2]"), None);
        assert_eq!(parse(b"<html>"), None);
    }
}
