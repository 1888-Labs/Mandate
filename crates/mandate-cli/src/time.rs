//! Durations like `48h` and human-readable UTC timestamps.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::CliError;

/// Current Unix time in seconds.
pub fn now() -> u64 {
    // A clock before 1970 is a broken host; treat it as the epoch.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A unique-enough nonce: nanoseconds since the epoch.
pub fn nonce() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

/// Parse `90s`, `30m`, `48h` or `7d` into seconds.
pub fn parse_duration(s: &str) -> Result<u64, CliError> {
    let bad = || {
        CliError::Usage(format!(
            "invalid duration `{s}`: use e.g. 90s, 30m, 48h, 7d"
        ))
    };
    if !s.is_ascii() {
        return Err(bad());
    }
    let split = s.len().checked_sub(1).ok_or_else(bad)?;
    let (digits, unit) = s.split_at(split);
    let n: u64 = digits.parse().map_err(|_| bad())?;
    let scale = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => return Err(bad()),
    };
    match n.checked_mul(scale) {
        Some(secs) if secs > 0 => Ok(secs),
        _ => Err(bad()),
    }
}

/// `2026-10-15 12:00 UTC`
pub fn format_utc(ts: u64) -> String {
    let days = (ts / 86_400) as i64;
    let secs = ts % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        secs / 3600,
        (secs % 3600) / 60
    )
}

/// `in 23h 59m` or `2h 5m ago`.
pub fn relative(ts: u64, now: u64) -> String {
    if ts >= now {
        format!("in {}", span(ts - now))
    } else {
        format!("{} ago", span(now - ts))
    }
}

fn span(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60);
    match (d, h) {
        (0, 0) => format!("{m}m"),
        (0, _) => format!("{h}h {m}m"),
        _ => format!("{d}d {h}h"),
    }
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("90s").unwrap(), 90);
        assert_eq!(parse_duration("30m").unwrap(), 1800);
        assert_eq!(parse_duration("48h").unwrap(), 172_800);
        assert_eq!(parse_duration("7d").unwrap(), 604_800);
        for bad in [
            "",
            "h",
            "48",
            "0h",
            "-1h",
            "1.5h",
            "1w",
            "48ħ",
            "99999999999999999999d",
        ] {
            assert!(parse_duration(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn formats_utc_dates() {
        assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(format_utc(1_760_000_000), "2025-10-09 08:53 UTC");
    }

    #[test]
    fn formats_relative_times() {
        assert_eq!(relative(1000 + 86_340, 1000), "in 23h 59m");
        assert_eq!(relative(1000 + 172_800, 1000), "in 2d 0h");
        assert_eq!(relative(1000, 1000 + 125 * 60), "2h 5m ago");
    }
}
