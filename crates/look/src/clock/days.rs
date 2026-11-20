//! Which day something happened, as the app says it, by the calendar: today, yesterday, so many days, weeks or months
//! ago, and a date from a year on.

use gpui_kit::App;

#[cfg(doc)]
use super::time::next_day_at_ms;
use super::time::{date, days_apart, months_apart};

const DAYS_IN_A_WEEK: i64 = 7;
const MONTHS_IN_A_YEAR: i64 = 12;

/// Which day `then_ms` was, as seen from `now_ms`, by the calendar in the local time zone: "today", "yesterday" and "3
/// days ago", then by the week, "1 week ago", then by the month, "2 months ago", and from twelve months on, the date,
/// "on Nov 6, 2025". A time after now, which a hub whose clock runs ahead of this one's can give, is today. What it
/// says changes as a day starts, at [`next_day_at_ms`]. `None` for a time too far off to say.
pub fn day_ago(then_ms: i64, now_ms: i64, cx: &App) -> Option<String> {
    let days = days_apart(then_ms, now_ms, cx)?;
    let months = months_apart(then_ms, now_ms, cx)?;
    match by_the_day(days, months) {
        Some(said) => Some(said),
        None => Some(format!("on {}", date(then_ms, cx)?)),
    }
}

/// `days` days ago, which is `months` whole months by the calendar, in its largest whole unit: days for a week, then
/// weeks until it's a month, then months. `None` from twelve months on, which is said as a date.
fn by_the_day(days: i64, months: i64) -> Option<String> {
    let (count, unit) = match (days, months) {
        (..1, _) => return Some("today".to_string()),
        (1, _) => return Some("yesterday".to_string()),
        (2..DAYS_IN_A_WEEK, _) => (days, "day"),
        (_, ..1) => (days / DAYS_IN_A_WEEK, "week"),
        (_, 1..MONTHS_IN_A_YEAR) => (months, "month"),
        _ => return None,
    };
    Some(format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" }))
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;

    use super::*;

    const NOW_MS: i64 = 1_793_577_600_000;
    const MINUTE_MS: i64 = 60_000;

    #[test]
    fn a_day_is_said_by_the_day_then_the_week_then_the_month() {
        let said = |days, months| by_the_day(days, months);

        assert_eq!(said(0, 0).as_deref(), Some("today"));
        assert_eq!(said(1, 0).as_deref(), Some("yesterday"));
        assert_eq!(said(2, 0).as_deref(), Some("2 days ago"));
        assert_eq!(said(6, 0).as_deref(), Some("6 days ago"));
        assert_eq!(said(7, 0).as_deref(), Some("1 week ago"));
        assert_eq!(said(13, 0).as_deref(), Some("1 week ago"));
        assert_eq!(said(14, 0).as_deref(), Some("2 weeks ago"));
        assert_eq!(said(30, 0).as_deref(), Some("4 weeks ago"), "thirty days, within one month");
        assert_eq!(said(28, 1).as_deref(), Some("1 month ago"), "from Feb 28 to Mar 28");
        assert_eq!(said(364, 11).as_deref(), Some("11 months ago"));
        assert_eq!(said(365, 12), None, "a date, from twelve months on");
        assert_eq!(said(-1, 0).as_deref(), Some("today"), "a time after now");
    }

    /// Times held still are in UTC, wherever the tests run. [`NOW_MS`] is 2 November 2026, at midnight.
    #[gpui_kit::test]
    fn a_day_goes_by_the_calendar_rather_than_by_how_long_ago(cx: &mut TestAppContext) {
        let (now_ms, hour_ms) = (NOW_MS + 15 * 60 * MINUTE_MS, 60 * MINUTE_MS);
        let said = |then_ms| {
            cx.update(|cx| {
                super::super::hold_still(now_ms, cx);
                day_ago(then_ms, now_ms, cx).unwrap()
            })
        };

        let day_ms = 24 * hour_ms;

        assert_eq!(said(NOW_MS), "today", "fifteen hours ago, at midnight");
        assert_eq!(said(NOW_MS - 1), "yesterday", "fifteen hours ago and a moment");
        assert_eq!(said(NOW_MS - 30 * day_ms), "4 weeks ago", "on Oct 3, under a month before Nov 2");
        assert_eq!(said(NOW_MS - 31 * day_ms), "1 month ago", "on Oct 2");
        assert_eq!(said(NOW_MS - 364 * day_ms), "11 months ago", "on Nov 3, 2025");
        assert_eq!(said(NOW_MS - 365 * day_ms), "on Nov 2, 2025");
        assert_eq!(said(NOW_MS - 400 * day_ms), "on Sep 28, 2025");
    }
}
