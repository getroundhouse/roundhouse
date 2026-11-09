//! How long a turn has taken so far, or took, and when it ended, as the transcript says them.

use std::time::Duration;

use chrono::{DateTime, Datelike as _, FixedOffset};

/// While the agent works, to a tenth of a second, so the count moving says it's still at it: "4m, 59.9s".
pub(crate) fn so_far(elapsed: Duration) -> String {
    let tenths = elapsed.as_millis() / 100;
    let seconds = tenths % 600;
    with_minutes(tenths / 600, format!("{}.{}s", seconds / 10, seconds % 10))
}

/// Once the turn's over, to the second: "1m, 42s".
pub(crate) fn took(duration: Duration) -> String {
    let seconds = duration.as_secs();
    with_minutes(u128::from(seconds / 60), format!("{}s", seconds % 60))
}

fn with_minutes(minutes: u128, seconds: String) -> String {
    match (minutes / 60, minutes % 60) {
        (0, 0) => seconds,
        (0, minutes) => format!("{minutes}m, {seconds}"),
        (hours, minutes) => format!("{hours}h, {minutes}m, {seconds}"),
    }
}

/// When a turn ended, with only as much of the day as it takes to tell: none today, the day of the week within the
/// last week, and the date before that, with the year if it's another one. "11:42 PM", "Sun 11:42 PM",
/// "Oct 26 11:42 PM". `at` and `now` are each in the local time zone as it was at that moment, as `look` gives them,
/// so a time from before a change to daylight saving says what the clock did then.
pub(crate) fn ended(at: DateTime<FixedOffset>, now: DateTime<FixedOffset>) -> String {
    let days_ago = (now.date_naive() - at.date_naive()).num_days();
    let day = match days_ago {
        0 => "",
        1..7 => "%a ",
        _ if at.year() == now.year() => "%b %-d ",
        _ => "%b %-d, %Y ",
    };
    at.format(&format!("{day}%-I:%M %p")).to_string()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::{DateTime, FixedOffset};

    use super::{ended, so_far, took};

    fn at(text: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(text).unwrap()
    }

    #[test]
    fn the_time_so_far_counts_tenths_and_shows_minutes_and_hours_once_there_are_any() {
        assert_eq!(so_far(Duration::from_millis(0)), "0.0s");
        assert_eq!(so_far(Duration::from_millis(12_345)), "12.3s");
        assert_eq!(so_far(Duration::from_millis(299_999)), "4m, 59.9s", "it never rounds up to a minute it hasn't had");
        assert_eq!(so_far(Duration::from_millis(3_723_400)), "1h, 2m, 3.4s");
    }

    #[test]
    fn a_finished_turn_says_whole_seconds() {
        assert_eq!(took(Duration::from_millis(48_999)), "48s");
        assert_eq!(took(Duration::from_secs(102)), "1m, 42s");
        assert_eq!(took(Duration::from_secs(3_600)), "1h, 0m, 0s");
    }

    #[test]
    fn a_turn_says_only_as_much_of_the_day_it_ended_as_it_takes_to_tell() {
        let now = at("2026-11-02T09:00:00Z");

        assert_eq!(ended(at("2026-11-02T08:05:00Z"), now), "8:05 AM", "today");
        assert_eq!(ended(at("2026-11-01T23:42:00Z"), now), "Sun 11:42 PM", "yesterday, a few hours ago");
        assert_eq!(ended(at("2026-10-27T12:00:00Z"), now), "Tue 12:00 PM", "six days ago");
        assert_eq!(ended(at("2026-10-26T23:42:00Z"), now), "Oct 26 11:42 PM", "a Monday a week ago isn't today");
        assert_eq!(ended(at("2025-12-30T23:42:00Z"), now), "Dec 30, 2025 11:42 PM");
    }

    #[test]
    fn the_end_keeps_the_clock_it_had_then_across_a_change_to_daylight_saving() {
        let winter = at("2026-11-02T09:00:00-08:00");

        assert_eq!(ended(at("2026-11-01T00:30:00-07:00"), winter), "Sun 12:30 AM", "not Sat 11:30 PM by today's clock");
    }
}
