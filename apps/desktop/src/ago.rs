//! How long ago something happened, as the app says it: to the minute, or by the day.

use gpui_kit::App;

const MINUTE_MS: i64 = 60_000;
const MINUTES_IN_AN_HOUR: i64 = 60;
const MINUTES_IN_A_DAY: i64 = 24 * MINUTES_IN_AN_HOUR;
const DAYS_IN_A_WEEK: i64 = 7;
const MONTHS_IN_A_YEAR: i64 = 12;

/// How long before `now_ms` `then_ms` was, in its largest whole unit, such as "9m ago" or "4h ago". Under a minute
/// is "just now", and so is a time after now, which a hub whose clock runs ahead of this one's can give.
pub(crate) fn ago(then_ms: i64, now_ms: i64) -> String {
    let minutes = now_ms.saturating_sub(then_ms) / MINUTE_MS;
    match minutes {
        ..1 => "just now".to_string(),
        1..MINUTES_IN_AN_HOUR => format!("{minutes}m ago"),
        MINUTES_IN_AN_HOUR..MINUTES_IN_A_DAY => format!("{}h ago", minutes / MINUTES_IN_AN_HOUR),
        _ => format!("{}d ago", minutes / MINUTES_IN_A_DAY),
    }
}

/// How long after `now_ms` what [`ago`] says of `then_ms` changes: as the next minute goes in the first hour, the next
/// hour in the first day, and the next day after that. A time after now is looked at again as each minute before it
/// goes.
pub(crate) fn changes_in_ms(then_ms: i64, now_ms: i64) -> i64 {
    let ago_ms = now_ms.saturating_sub(then_ms);
    let unit = match ago_ms / MINUTE_MS {
        ..MINUTES_IN_AN_HOUR => MINUTE_MS,
        MINUTES_IN_AN_HOUR..MINUTES_IN_A_DAY => MINUTES_IN_AN_HOUR * MINUTE_MS,
        _ => MINUTES_IN_A_DAY * MINUTE_MS,
    };
    unit - ago_ms.rem_euclid(unit)
}

/// Which day `then_ms` was, as seen from `now_ms`, by the calendar in the local time zone: "today", "yesterday" and "3
/// days ago", then by the week, "1 week ago", then by the month, "2 months ago", and from twelve months on, the date,
/// "on Nov 6, 2025". A time after now, which a hub whose clock runs ahead of this one's can give, is today. What it
/// says changes as a day starts, at [`look::next_day_at_ms`]. `None` for a time too far off to say.
pub(crate) fn day_ago(then_ms: i64, now_ms: i64, cx: &App) -> Option<String> {
    let days = look::days_apart(then_ms, now_ms, cx)?;
    let months = look::months_apart(then_ms, now_ms, cx)?;
    match by_the_day(days, months) {
        Some(said) => Some(said),
        None => Some(format!("on {}", look::date(then_ms, cx)?)),
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

    #[test]
    fn each_time_is_in_its_largest_whole_unit() {
        let before = |minutes: i64| ago(NOW_MS - minutes * MINUTE_MS, NOW_MS);

        assert_eq!(before(0), "just now");
        assert_eq!(before(1), "1m ago");
        assert_eq!(before(59), "59m ago");
        assert_eq!(before(60), "1h ago");
        assert_eq!(before(23 * 60 + 59), "23h ago");
        assert_eq!(before(24 * 60), "1d ago");
        assert_eq!(before(45 * 24 * 60), "45d ago");
    }

    #[test]
    fn what_is_said_changes_as_its_unit_goes() {
        let changes = |minutes: i64, ms: i64| changes_in_ms(NOW_MS - minutes * MINUTE_MS - ms, NOW_MS);

        assert_eq!(changes(0, 0), MINUTE_MS, "just now, until it's a minute ago");
        assert_eq!(changes(12, 1_000), MINUTE_MS - 1_000);
        assert_eq!(changes(125, 0), 55 * MINUTE_MS, "then by the hour");
        assert_eq!(changes(24 * 60 + 1, 0), (24 * 60 - 1) * MINUTE_MS, "then by the day");
        assert_eq!(changes_in_ms(NOW_MS + 5_000, NOW_MS), 5_000, "a time after now");
    }

    #[test]
    fn a_time_after_now_is_just_now() {
        assert_eq!(ago(NOW_MS + 5 * MINUTE_MS, NOW_MS), "just now");
        assert_eq!(ago(i64::MAX, i64::MIN), "just now");
    }

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
                look::hold_still(now_ms, cx);
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
