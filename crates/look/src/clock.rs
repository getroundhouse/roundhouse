//! Where the marks are in their motion, and what time it is for text on screen. A moving mark is redrawn on every frame
//! of the display, for the smoothest motion it can show. Pictures, and the system's Reduce Motion setting, hold every
//! mark still, and pictures hold the time at one moment too.

use std::sync::OnceLock;
use std::time::Instant;

use chrono::{DateTime, Datelike as _, FixedOffset, Local, Utc};
use gpui_kit::{App, Global};

static START: OnceLock<Instant> = OnceLock::new();

/// Set by pictures, to hold every mark still, and the time at one moment.
struct Still {
    at_ms: i64,
}

impl Global for Still {}

/// Whether marks move at all.
pub(crate) fn moving(cx: &App) -> bool {
    !cx.has_global::<Still>() && !cx.reduce_motion()
}

/// Seconds since the first mark asked. Each motion takes its own phase from this.
pub(crate) fn elapsed() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// The time now, in milliseconds since the Unix epoch, for text that says when something happened or how long it's
/// taken. In pictures it's the moment they hold. [`local_time`] says it as a clock on the wall would.
pub fn now_ms(cx: &App) -> i64 {
    cx.try_global::<Still>().map_or_else(|| Utc::now().timestamp_millis(), |still| still.at_ms)
}

/// A moment, `at_ms`, in the local time zone as it was then, so a time from before a change to daylight saving reads as
/// the clock did. It's the one way to a local time, so no time is told with another's offset. In pictures it's in UTC,
/// wherever they're taken. `None` for a time too far off to say.
pub fn local_time(at_ms: i64, cx: &App) -> Option<DateTime<FixedOffset>> {
    let at = DateTime::from_timestamp_millis(at_ms)?;
    Some(if cx.has_global::<Still>() { at.fixed_offset() } else { at.with_timezone(&Local).fixed_offset() })
}

/// When something happened, or will, with only as much of the day as it takes to tell: none today, the day of the week
/// within a week either way, and the date beyond that, with the year if it's another one. "11:42 PM", "Sun 11:42 PM",
/// "Oct 26 11:42 PM". It's the one way a day and time are said, whether a turn's end or a limit's reset. `None` for a
/// time too far off to say.
pub fn day_and_time(at_ms: i64, cx: &App) -> Option<String> {
    Some(day_and_time_from(local_time(at_ms, cx)?, local_time(now_ms(cx), cx)?))
}

/// [`day_and_time`], for `at` as seen from `now`, each in the local time zone as it was at that moment, so a time
/// from before a change to daylight saving says what the clock did then.
fn day_and_time_from(at: DateTime<FixedOffset>, now: DateTime<FixedOffset>) -> String {
    let days_apart = (now.date_naive() - at.date_naive()).num_days().abs();
    let day = match days_apart {
        0 => "",
        1..7 => "%a ",
        _ if at.year() == now.year() => "%b %-d ",
        _ => "%b %-d, %Y ",
    };
    at.format(&format!("{day}%-I:%M %p")).to_string()
}

/// Holds every mark still from now on, and the time at `at_ms`, so pictures of the app come out the same every time.
#[cfg(feature = "testing")]
pub fn hold_still(at_ms: i64, cx: &mut App) {
    cx.set_global(Still { at_ms });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(text).unwrap()
    }

    #[test]
    fn a_time_past_says_only_as_much_of_its_day_as_it_takes_to_tell() {
        let now = at("2026-11-02T09:00:00Z");

        assert_eq!(day_and_time_from(at("2026-11-02T08:05:00Z"), now), "8:05 AM", "today");
        assert_eq!(day_and_time_from(at("2026-11-01T23:42:00Z"), now), "Sun 11:42 PM", "yesterday, a few hours ago");
        assert_eq!(day_and_time_from(at("2026-10-27T12:00:00Z"), now), "Tue 12:00 PM", "six days ago");
        assert_eq!(
            day_and_time_from(at("2026-10-26T23:42:00Z"), now),
            "Oct 26 11:42 PM",
            "a Monday a week ago isn't today"
        );
        assert_eq!(day_and_time_from(at("2025-12-30T23:42:00Z"), now), "Dec 30, 2025 11:42 PM");
    }

    #[test]
    fn a_time_to_come_is_said_the_same_way() {
        let now = at("2026-11-10T14:00:00-08:00");

        assert_eq!(day_and_time_from(at("2026-11-10T19:05:00-08:00"), now), "7:05 PM", "later today");
        assert_eq!(day_and_time_from(at("2026-11-16T20:59:00-08:00"), now), "Mon 8:59 PM", "in six days");
        assert_eq!(day_and_time_from(at("2026-11-17T20:59:00-08:00"), now), "Nov 17 8:59 PM", "a week on isn't today");
        assert_eq!(day_and_time_from(at("2027-01-02T09:00:00-08:00"), now), "Jan 2, 2027 9:00 AM");
    }

    #[test]
    fn a_time_keeps_the_clock_it_had_then_across_a_change_to_daylight_saving() {
        let winter = at("2026-11-02T09:00:00-08:00");

        assert_eq!(
            day_and_time_from(at("2026-11-01T00:30:00-07:00"), winter),
            "Sun 12:30 AM",
            "not Sat 11:30 PM by today's clock"
        );
    }
}
