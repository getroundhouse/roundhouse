use std::time::Duration;

use gpui_kit::TestAppContext;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ZonedDateTime};

use super::*;

/// A time, `text`, in RFC 9557's form, with the zone it's in after it: `[UTC]`, or an offset such as `[-08:00]` that
/// holds all year.
fn at(text: &str) -> ZonedDateTime {
    text.parse().unwrap()
}

fn day(text: &str) -> Date {
    text.parse().unwrap()
}

/// So a test or a clip moves a mark as it moves everything else, and a moment of it is the same on every run.
#[gpui_kit::test]
fn marks_move_by_the_app_s_clock_from_when_it_started_theirs(cx: &mut TestAppContext) {
    let before = cx.update(|cx| elapsed(cx));
    cx.update(start);
    let started = cx.update(|cx| elapsed(cx));
    cx.executor().advance_clock(Duration::from_millis(1_500));
    cx.update(start);

    assert_eq!((before, started), (0., 0.), "no time has passed before the clock starts, or as it does");
    assert!((cx.update(|cx| elapsed(cx)) - 1.5).abs() < 1e-9, "as far as the app's clock went, from the first start");
}

#[test]
fn a_time_past_says_only_as_much_of_its_day_as_it_takes_to_tell() {
    let now = at("2026-11-02T09:00:00[UTC]");

    assert_eq!(day_and_time_from(&at("2026-11-02T08:05:00[UTC]"), &now), "8:05 AM", "today");
    assert_eq!(day_and_time_from(&at("2026-11-01T23:42:00[UTC]"), &now), "Sun 11:42 PM", "yesterday, a few hours ago");
    assert_eq!(day_and_time_from(&at("2026-10-27T12:00:00[UTC]"), &now), "Tue 12:00 PM", "six days ago");
    assert_eq!(
        day_and_time_from(&at("2026-10-26T23:42:00[UTC]"), &now),
        "Oct 26 11:42 PM",
        "a Monday a week ago isn't today"
    );
    assert_eq!(day_and_time_from(&at("2025-12-30T23:42:00[UTC]"), &now), "Dec 30, 2025, 11:42 PM");
}

#[test]
fn a_day_has_its_year_and_a_time_of_day_the_clock_it_had_then() {
    assert_eq!(date_of(&at("2026-11-06T12:13:00[UTC]")), "Nov 6, 2026");
    assert_eq!(time_of(&at("2026-11-06T12:13:00[UTC]")), "12:13 PM");
    assert_eq!(date_and_time_of(&at("2026-11-06T12:13:00[UTC]")), "Nov 6, 2026, 12:13 PM");
    assert_eq!(date_of(&at("2025-12-30T23:42:00[-08:00]")), "Dec 30, 2025", "not the next day, by UTC");
    assert_eq!(time_of(&at("2025-12-30T23:42:00[-08:00]")), "11:42 PM");
}

#[test]
fn a_time_to_come_is_said_the_same_way() {
    let now = at("2026-11-10T14:00:00[-08:00]");

    assert_eq!(day_and_time_from(&at("2026-11-10T19:05:00[-08:00]"), &now), "7:05 PM", "later today");
    assert_eq!(day_and_time_from(&at("2026-11-16T20:59:00[-08:00]"), &now), "Mon 8:59 PM", "in six days");
    assert_eq!(day_and_time_from(&at("2026-11-17T20:59:00[-08:00]"), &now), "Nov 17 8:59 PM", "a week on isn't today");
    assert_eq!(day_and_time_from(&at("2027-01-02T09:00:00[-08:00]"), &now), "Jan 2, 2027, 9:00 AM");
}

#[test]
fn a_time_keeps_the_clock_it_had_then_across_a_change_to_daylight_saving() {
    let winter = at("2026-11-02T09:00:00[-08:00]");

    assert_eq!(
        day_and_time_from(&at("2026-11-01T00:30:00[-07:00]"), &winter),
        "Sun 12:30 AM",
        "not Sat 11:30 PM by today's clock"
    );
}

#[test]
fn a_month_goes_by_the_calendar_from_a_day_to_the_same_day_of_another() {
    assert_eq!(months_between(day("2026-10-01"), day("2026-10-31")), 0, "thirty days, in one month");
    assert_eq!(months_between(day("2026-10-15"), day("2026-11-14")), 0);
    assert_eq!(months_between(day("2026-10-15"), day("2026-11-15")), 1);
    assert_eq!(months_between(day("2026-01-31"), day("2026-02-27")), 0);
    assert_eq!(months_between(day("2026-01-31"), day("2026-02-28")), 1, "February has no 31st");
    assert_eq!(months_between(day("2025-11-02"), day("2026-11-01")), 11);
    assert_eq!(months_between(day("2025-11-02"), day("2026-11-02")), 12);
    assert_eq!(months_between(day("2026-11-15"), day("2026-10-15")), -1, "a day before");
}

/// When `day_text` starts in `zone`.
fn starts(day_text: &str, zone: &TimeZone) -> Timestamp {
    Timestamp::from_millisecond(start_of(day(day_text), zone).unwrap()).unwrap()
}

fn instant(text: &str) -> Timestamp {
    text.parse().unwrap()
}

#[test]
fn a_day_starts_at_midnight_by_the_clock_it_has_then_rather_than_by_the_clock_of_the_day_before() {
    // Eight hours behind UTC until 2 AM on the second Sunday of March, then seven.
    let springs = TimeZone::posix("PST8PDT,M3.2.0,M11.1.0").unwrap();

    assert_eq!(starts("2026-03-08", &springs), instant("2026-03-08T00:00:00-08:00"));
    assert_eq!(starts("2026-03-09", &springs), instant("2026-03-09T00:00:00-07:00"), "not 1 AM");
}

#[test]
fn a_day_whose_midnight_is_skipped_starts_as_the_clock_comes_back_and_one_gone_over_twice_at_the_first() {
    // Three hours behind UTC, until midnight on the first Sunday of October, when the clock goes on an hour, to 1 AM.
    // Then two, until 1 AM on the third Sunday of February, when it goes back an hour, to midnight again.
    let zone = TimeZone::posix("<-03>3<-02>,M10.1.0/0,M2.3.0/1").unwrap();

    assert_eq!(starts("2026-10-04", &zone), instant("2026-10-04T01:00:00-02:00"));
    assert_eq!(starts("2027-02-21", &zone), instant("2027-02-21T00:00:00-02:00"));
}
