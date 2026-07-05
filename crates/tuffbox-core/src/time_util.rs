//! Minimal UTC timestamp formatting shared across the crate.
//!
//! Several places (lockfile `generated_at`, snapshot `created_at`/IDs) need
//! a real wall-clock RFC 3339 UTC timestamp. Rather than pull in a full
//! datetime crate, this converts `SystemTime` to civil UTC components using
//! Howard Hinnant's well-known `civil_from_days` algorithm (correct
//! proleptic Gregorian conversion for any date representable by the type).

use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the current UTC time as an RFC 3339 timestamp, e.g.
/// `2026-06-29T12:34:56Z`.
pub fn rfc3339_now() -> String {
    format_rfc3339(SystemTime::now())
}

/// Returns the current UTC time formatted for use as a path-safe ID
/// component (`YYYYMMDDTHHMMSSZ`, no colons).
pub fn compact_now() -> String {
    format_compact_utc(SystemTime::now())
}

pub fn format_rfc3339(time: SystemTime) -> String {
    let (y, mo, d, h, mi, s) = civil_utc_from_system_time(time);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

pub fn format_compact_utc(time: SystemTime) -> String {
    let (y, mo, d, h, mi, s) = civil_utc_from_system_time(time);
    format!("{y:04}{mo:02}{d:02}T{h:02}{mi:02}{s:02}Z")
}

fn civil_utc_from_system_time(time: SystemTime) -> (i64, u32, u32, u32, u32, u32) {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let total_secs = duration.as_secs() as i64;
    let days = total_secs.div_euclid(86_400);
    let secs_of_day = total_secs.rem_euclid(86_400);

    let hour = (secs_of_day / 3600) as u32;
    let minute = ((secs_of_day % 3600) / 60) as u32;
    let second = (secs_of_day % 60) as u32;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };

    (year, month, day, hour, minute, second)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn matches_known_epoch_values() {
        assert_eq!(format_rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        // 2024-03-15T13:45:30Z
        assert_eq!(
            format_rfc3339(UNIX_EPOCH + Duration::from_secs(1_710_510_330)),
            "2024-03-15T13:45:30Z"
        );
        // Leap day: 2000-02-29T00:00:00Z
        assert_eq!(
            format_rfc3339(UNIX_EPOCH + Duration::from_secs(951_782_400)),
            "2000-02-29T00:00:00Z"
        );
    }

    #[test]
    fn compact_format_has_no_separators() {
        assert_eq!(
            format_compact_utc(UNIX_EPOCH + Duration::from_secs(1_710_510_330)),
            "20240315T134530Z"
        );
    }

    #[test]
    fn now_is_not_a_hardcoded_date() {
        assert_ne!(rfc3339_now(), "2026-06-29T00:00:00Z");
        assert_ne!(compact_now(), "20260629T000000Z");
    }
}
