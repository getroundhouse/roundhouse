//! Where the marks are in their motion, and what time it is for text on screen. A moving mark is redrawn on every frame
//! of the display, for the smoothest motion it can show. Reduce Motion holds every mark still, as it skips everything
//! else that moves. Pictures turn it on, and hold the time at one moment too.

use std::sync::OnceLock;
use std::time::Instant;

use chrono::{DateTime, Datelike as _, FixedOffset, Local, NaiveDate, NaiveTime, TimeDelta, TimeZone, Utc};
use gpui_kit::{App, Global};

static START: OnceLock<Instant> = OnceLock::new();

/// Set by pictures, to hold the time at one moment.
struct Still {
    at_ms: i64,
}

impl Global for Still {}

/// Whether marks move at all.
pub(crate) fn moving(cx: &App) -> bool {
    !cx.reduce_motion()
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

/// How many days `to_ms`'s day is after `from_ms`'s, by the calendar in the local time zone as it was at each, so late
/// one evening and early the next morning are a day apart. Less than nothing when `to_ms` is the earlier. `None` for a
/// time too far off to say.
pub fn days_apart(from_ms: i64, to_ms: i64, cx: &App) -> Option<i64> {
    let (from, to) = (local_time(from_ms, cx)?.date_naive(), local_time(to_ms, cx)?.date_naive());
    Some((to - from).num_days())
}

/// How many whole months `to_ms`'s day is after `from_ms`'s, by the calendar in the local time zone, as
/// [`days_apart`] counts days: a month from the 15th to the 15th of the next, and none from the 1st to the 31st. Less
/// than nothing when `to_ms` is the earlier. `None` for a time too far off to say.
pub fn months_apart(from_ms: i64, to_ms: i64, cx: &App) -> Option<i64> {
    Some(months_between(local_time(from_ms, cx)?.date_naive(), local_time(to_ms, cx)?.date_naive()))
}

/// When the day after `at_ms`'s starts, by the calendar in the local time zone: at midnight, or where a change to
/// daylight saving skips midnight, at the first moment after it. In pictures it's midnight in UTC. `None` for a time
/// too far off to say.
pub fn next_day_at_ms(at_ms: i64, cx: &App) -> Option<i64> {
    let next = local_time(at_ms, cx)?.date_naive().succ_opt()?;
    if cx.has_global::<Still>() { start_of(next, &Utc) } else { start_of(next, &Local) }
}

/// How many whole months `to` is after `from`. A month ends on the same day of the month as it started, or on the
/// last of a month too short to have that day, so from Jan 31 it's a month on Feb 28.
fn months_between(from: NaiveDate, to: NaiveDate) -> i64 {
    if to < from {
        return -months_between(to, from);
    }
    let months = i64::from(to.year() - from.year()) * 12 + i64::from(to.month()) - i64::from(from.month());
    let short = to.day() < from.day() && to.succ_opt().is_some_and(|next| next.month() == to.month());
    if short { months - 1 } else { months }
}

/// When `day` starts in `zone`: at midnight, the first of the two where the clock goes back over midnight, and where it
/// skips midnight, the first moment after the gap. A clock changes on a whole minute, so that's the first whole minute
/// the day has, within a day of midnight, as there was once a zone that skipped a whole day.
fn start_of<Tz: TimeZone>(day: NaiveDate, zone: &Tz) -> Option<i64> {
    let midnight = day.and_time(NaiveTime::MIN);
    (0..=MINUTES_IN_A_DAY)
        .find_map(|minute| zone.from_local_datetime(&(midnight + TimeDelta::minutes(minute))).earliest())
        .map(|starts| starts.timestamp_millis())
}

const MINUTES_IN_A_DAY: i64 = 24 * 60;

/// When something happened, or will, with only as much of the day as it takes to tell: none today, the day of the week
/// within a week either way, and the date beyond that, with the year if it's another one, as [`date_and_time`] says
/// it. "11:42 PM", "Sun 11:42 PM", "Oct 26 11:42 PM", "Dec 30, 2025, 11:42 PM". It's the one way a day and time are
/// said in a line, whether a turn's end or a limit's reset. A card with room to say the day apart says [`date`] and
/// [`time_of_day`]. `None` for a time too far off to say.
pub fn day_and_time(at_ms: i64, cx: &App) -> Option<String> {
    Some(day_and_time_from(local_time(at_ms, cx)?, local_time(now_ms(cx), cx)?))
}

/// The day something happened, with its year whenever it was: "Nov 6, 2026". `None` for a time too far off to say.
pub fn date(at_ms: i64, cx: &App) -> Option<String> {
    Some(date_of(local_time(at_ms, cx)?))
}

/// The time of day something happened, by the clock it had then: "12:13 PM". `None` for a time too far off to say.
pub fn time_of_day(at_ms: i64, cx: &App) -> Option<String> {
    Some(time_of(local_time(at_ms, cx)?))
}

/// [`date`] and [`time_of_day`] together, the one way a time is said with its year: "Nov 6, 2026, 12:13 PM". `None`
/// for a time too far off to say.
pub fn date_and_time(at_ms: i64, cx: &App) -> Option<String> {
    Some(date_and_time_of(local_time(at_ms, cx)?))
}

fn date_of(at: DateTime<FixedOffset>) -> String {
    at.format("%b %-d, %Y").to_string()
}

fn time_of(at: DateTime<FixedOffset>) -> String {
    at.format("%-I:%M %p").to_string()
}

fn date_and_time_of(at: DateTime<FixedOffset>) -> String {
    format!("{}, {}", date_of(at), time_of(at))
}

/// [`day_and_time`], for `at` as seen from `now`, each in the local time zone as it was at that moment, so a time
/// from before a change to daylight saving says what the clock did then.
fn day_and_time_from(at: DateTime<FixedOffset>, now: DateTime<FixedOffset>) -> String {
    let days_apart = (now.date_naive() - at.date_naive()).num_days().abs();
    let day = match days_apart {
        0 => return time_of(at),
        1..7 => "%a",
        _ if at.year() == now.year() => "%b %-d",
        _ => return date_and_time_of(at),
    };
    format!("{} {}", at.format(day), time_of(at))
}

/// Holds everything that moves still from now on, by turning Reduce Motion on, and the time at `at_ms`, so pictures of
/// the app come out the same every time. Marks stay where they are, and anything else shows as it ends.
#[cfg(feature = "testing")]
pub fn hold_still(at_ms: i64, cx: &mut App) {
    cx.set_reduce_motion(true);
    cx.set_global(Still { at_ms });
}

#[cfg(test)]
mod tests;
