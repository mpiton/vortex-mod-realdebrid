//! Turn Real-Debrid's ISO-8601 expiry string into a Unix timestamp.
//!
//! `/user` reports premium two ways: `premium` (seconds remaining) and
//! `expiration` (an ISO date). The Accounts view wants an absolute instant
//! and a WASM plugin has no clock, so the ISO string is the only one of the
//! two we can convert without asking the host what time it is.

/// Parse `YYYY-MM-DDTHH:MM:SS…` as UTC. Any fractional seconds and the zone
/// suffix are ignored — Real-Debrid always reports UTC.
pub fn parse_iso8601_utc(value: &str) -> Option<u64> {
    let (date, time) = value.split_once('T')?;
    let mut date = date.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: u32 = date.next()?.parse().ok()?;
    let day: u32 = date.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let time = time.trim_end_matches('Z');
    let time = time.split(['+', '.']).next()?;
    let mut time = time.split(':');
    let hour: u64 = time.next()?.parse().ok()?;
    let minute: u64 = time.next()?.parse().ok()?;
    let second: u64 = time.next().unwrap_or("0").parse().ok()?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let seconds =
        days_from_civil(year, month, day) * 86_400 + (hour * 3600 + minute * 60 + second) as i64;
    u64::try_from(seconds).ok()
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 } as i64;
    let day_of_year = (153 * shifted_month + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("1970-01-01T00:00:00Z", 0)]
    #[case("2000-01-01T00:00:00Z", 946_684_800)]
    #[case("2024-02-29T12:00:00Z", 1_709_208_000)] // leap day
    #[case("2026-09-18T12:00:00.000Z", 1_789_732_800)]
    #[case("2026-09-18T12:00:00", 1_789_732_800)] // no zone suffix
    #[case("2026-09-18T12:00:00+00:00", 1_789_732_800)]
    fn parse_iso8601_utc_returns_the_unix_timestamp(#[case] input: &str, #[case] expected: u64) {
        assert_eq!(parse_iso8601_utc(input), Some(expected));
    }

    #[rstest]
    #[case("")]
    #[case("2026-09-18")] // no time part
    #[case("not-a-date T x")]
    #[case("2026-13-01T00:00:00Z")] // month out of range
    #[case("2026-09-18T25:00:00Z")] // hour out of range
    #[case("1969-12-31T23:59:59Z")] // before the epoch
    fn parse_iso8601_utc_rejects_what_it_cannot_read(#[case] input: &str) {
        assert_eq!(parse_iso8601_utc(input), None);
    }
}
