//! Time helpers shared by reports, the cache and job journals.

use std::time::{SystemTime, UNIX_EPOCH};

/// `time` as RFC 3339 in UTC with whole seconds, e.g.
/// `2023-11-14T22:13:20Z`.
pub(crate) fn rfc3339_utc(time: SystemTime) -> String {
    let secs: i64 = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).unwrap_or(i64::MAX),
        Err(before) => {
            let d = before.duration();
            // Round towards the past: -0.5 s is 23:59:59 the day before.
            let whole = i64::try_from(d.as_secs()).unwrap_or(i64::MAX);
            -whole - i64::from(d.subsec_nanos() > 0)
        }
    };
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Proleptic Gregorian date of a day count since 1970-01-01 (Howard
/// Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `20261016T030000Z`: sortable, file-name safe.
// First used by job ids and backups (writer); until then only by tests.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn compact_utc(time: SystemTime) -> String {
    rfc3339_utc(time).replace(['-', ':'], "")
}

/// Seconds since the Unix epoch, now.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: i64) -> String {
        let time = if secs >= 0 {
            UNIX_EPOCH + Duration::from_secs(secs as u64)
        } else {
            UNIX_EPOCH - Duration::from_secs(secs.unsigned_abs())
        };
        rfc3339_utc(time)
    }

    #[test]
    fn formats_rfc3339_utc() {
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(at(4_102_444_799), "2099-12-31T23:59:59Z");
        assert_eq!(at(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn sub_second_times_before_the_epoch_round_down() {
        let time = UNIX_EPOCH - Duration::from_millis(500);
        assert_eq!(rfc3339_utc(time), "1969-12-31T23:59:59Z");
        let time = UNIX_EPOCH + Duration::from_millis(1500);
        assert_eq!(rfc3339_utc(time), "1970-01-01T00:00:01Z");
    }

    #[test]
    fn compact_utc_is_file_name_safe() {
        let time = UNIX_EPOCH + Duration::from_secs(1_792_119_600);
        assert_eq!(rfc3339_utc(time), "2026-10-16T03:00:00Z");
        assert_eq!(compact_utc(time), "20261016T030000Z");
    }

    #[test]
    fn now_is_after_2023() {
        assert!(now_unix() > 1_700_000_000);
    }
}
