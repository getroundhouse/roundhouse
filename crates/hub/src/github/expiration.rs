use chrono::{DateTime, NaiveDateTime};

/// How GitHub writes the time a token ends, before its zone.
const WRITTEN: &str = "%Y-%m-%d %H:%M:%S";

/// When a token ends, in milliseconds since the epoch, from what GitHub's `github-authentication-token-expiration`
/// header `said`. GitHub's docs don't give its form, and GitHub was seen to send it as `2027-02-11 08:00:00 UTC`, for
/// a fine-grained token and for one from signing in, in November 2026. It also reads the same with an offset for its
/// zone, such as `2027-02-11 08:00:00 +0000`, as other clients of GitHub do. `None` for anything else.
pub(super) fn ends_at_ms(said: &str) -> Option<i64> {
    let said = said.trim();
    if let Some(utc) = said.strip_suffix(" UTC") {
        return NaiveDateTime::parse_from_str(utc, WRITTEN).ok().map(|at| at.and_utc().timestamp_millis());
    }
    DateTime::parse_from_str(said, &format!("{WRITTEN} %z")).ok().map(|at| at.timestamp_millis())
}
