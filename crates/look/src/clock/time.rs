use gpui_kit::{App, Global};
use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ZonedDateTime};
use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();

/// Set by pictures, to hold the time at one moment.
struct Still {
    pub(super) at_ms: i64,
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
    cx.try_global::<Still>().map_or_else(|| Timestamp::now().as_millisecond(), |still| still.at_ms)
}

/// A moment, `at_ms`, in the local time zone, with the offset it had then, so a time from before a change to daylight
/// saving reads as the clock did. It's the one way to a local time, so no time is told with another's offset. In
/// pictures it's in UTC, wherever they're taken. `None` for a time too far off to say.
pub fn local_time(at_ms: i64, cx: &App) -> Option<ZonedDateTime> {
    Some(Timestamp::from_millisecond(at_ms).ok()?.to_zoned_datetime(zone(cx)))
}

/// The local time zone, as the system says it, or UTC in pictures.
fn zone(cx: &App) -> TimeZone {
    if cx.has_global::<Still>() { TimeZone::UTC } else { TimeZone::system() }
}

/// How many days `to_ms`'s day is after `from_ms`'s, by the calendar in the local time zone as it was at each, so late
/// one evening and early the next morning are a day apart. Less than nothing when `to_ms` is the earlier. `None` for a
/// time too far off to say.
pub fn days_apart(from_ms: i64, to_ms: i64, cx: &App) -> Option<i64> {
    let (from, to) = (local_time(from_ms, cx)?.date(), local_time(to_ms, cx)?.date());
    Some(i64::from((to - from).get_days()))
}

/// How many whole months `to_ms`'s day is after `from_ms`'s, by the calendar in the local time zone, as
/// [`days_apart`] counts days: a month from the 15th to the 15th of the next, and none from the 1st to the 31st. Less
/// than nothing when `to_ms` is the earlier. `None` for a time too far off to say.
pub fn months_apart(from_ms: i64, to_ms: i64, cx: &App) -> Option<i64> {
    Some(months_between(local_time(from_ms, cx)?.date(), local_time(to_ms, cx)?.date()))
}

/// When the day after `at_ms`'s starts, by the calendar in the local time zone: at midnight, or where a change to
/// daylight saving skips midnight, at the first moment after it. In pictures it's midnight in UTC. `None` for a time
/// too far off to say.
pub fn next_day_at_ms(at_ms: i64, cx: &App) -> Option<i64> {
    let at = local_time(at_ms, cx)?;
    start_of(at.date().tomorrow().ok()?, at.time_zone())
}

/// How many whole months `to` is after `from`. A month ends on the same day of the month as it started, or on the
/// last of a month too short to have that day, so from Jan 31 it's a month on Feb 28.
pub(super) fn months_between(from: Date, to: Date) -> i64 {
    if to < from {
        return -months_between(to, from);
    }
    let months = i64::from(to.year() - from.year()) * 12 + i64::from(to.month()) - i64::from(from.month());
    let short = to.day() < from.day() && to.day() < to.days_in_month();
    if short { months - 1 } else { months }
}

/// When `day` starts in `zone`: at midnight, the first of the two where the clock goes back over midnight, and where it
/// skips midnight, the moment the gap ends, as jiff's compatible way of reading a time the clock skipped or said twice
/// does. `None` for a day too far off to say.
pub(super) fn start_of(day: Date, zone: &TimeZone) -> Option<i64> {
    Some(day.to_zoned_datetime(zone.clone()).ok()?.timestamp().as_millisecond())
}

/// When something happened, or will, with only as much of the day as it takes to tell: none today, the day of the week
/// within a week either way, and the date beyond that, with the year if it's another one, as [`date_and_time`] says
/// it. "11:42 PM", "Sun 11:42 PM", "Oct 26 11:42 PM", "Dec 30, 2025, 11:42 PM". It's the one way a day and time are
/// said in a line, whether a turn's end or a limit's reset. A card with room to say the day apart says [`date`] and
/// [`time_of_day`]. `None` for a time too far off to say.
pub fn day_and_time(at_ms: i64, cx: &App) -> Option<String> {
    Some(day_and_time_from(&local_time(at_ms, cx)?, &local_time(now_ms(cx), cx)?))
}

/// The day something happened, with its year whenever it was: "Nov 6, 2026". `None` for a time too far off to say.
pub fn date(at_ms: i64, cx: &App) -> Option<String> {
    Some(date_of(&local_time(at_ms, cx)?))
}

/// The time of day something happened, by the clock it had then: "12:13 PM". `None` for a time too far off to say.
pub fn time_of_day(at_ms: i64, cx: &App) -> Option<String> {
    Some(time_of(&local_time(at_ms, cx)?))
}

/// [`date`] and [`time_of_day`] together, the one way a time is said with its year: "Nov 6, 2026, 12:13 PM". `None`
/// for a time too far off to say.
pub fn date_and_time(at_ms: i64, cx: &App) -> Option<String> {
    Some(date_and_time_of(&local_time(at_ms, cx)?))
}

pub(super) fn date_of(at: &ZonedDateTime) -> String {
    at.strftime("%b %-d, %Y").to_string()
}

pub(super) fn time_of(at: &ZonedDateTime) -> String {
    at.strftime("%-I:%M %p").to_string()
}

pub(super) fn date_and_time_of(at: &ZonedDateTime) -> String {
    format!("{}, {}", date_of(at), time_of(at))
}

/// [`day_and_time`], for `at` as seen from `now`, each in the local time zone as it was at that moment, so a time
/// from before a change to daylight saving says what the clock did then.
pub(super) fn day_and_time_from(at: &ZonedDateTime, now: &ZonedDateTime) -> String {
    let days_apart = (now.date() - at.date()).get_days().abs();
    let day = match days_apart {
        0 => return time_of(at),
        1..7 => "%a",
        _ if at.year() == now.year() => "%b %-d",
        _ => return date_and_time_of(at),
    };
    format!("{} {}", at.strftime(day), time_of(at))
}

/// Holds everything that moves still from now on, by turning Reduce Motion on, and the time at `at_ms`, so pictures of
/// the app come out the same every time. Marks stay where they are, and anything else shows as it ends.
#[cfg(feature = "testing")]
pub fn hold_still(at_ms: i64, cx: &mut App) {
    cx.set_reduce_motion(true);
    cx.set_global(Still { at_ms });
}
