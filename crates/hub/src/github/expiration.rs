use jiff::fmt::strtime;

/// How GitHub writes the time a token ends. Its zone, `%Q`, is an IANA name, such as `UTC`, or an offset, such as
/// `+0000`.
const WRITTEN: &str = "%Y-%m-%d %H:%M:%S %Q";

/// When a token ends, in milliseconds since the epoch, from what GitHub's `github-authentication-token-expiration`
/// header `said`. GitHub's docs don't give its form, and GitHub was seen to send it as `2027-02-11 08:00:00 UTC`, for
/// a fine-grained token and for one from signing in, in November 2026. It also reads the same with an offset for its
/// zone, such as `2027-02-11 08:00:00 +0000`, as other clients of GitHub do, or with another zone's IANA name. `None`
/// for anything else.
pub(super) fn ends_at_ms(said: &str) -> Option<i64> {
    let ends = strtime::parse(WRITTEN, said.trim()).and_then(|read| read.to_zoned_datetime()).ok()?;
    Some(ends.timestamp().as_millisecond())
}
