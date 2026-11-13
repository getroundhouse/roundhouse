use chrono::{MappedLocalTime, NaiveDateTime};

use super::*;

fn at(text: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(text).unwrap()
}

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
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
    assert_eq!(day_and_time_from(at("2025-12-30T23:42:00Z"), now), "Dec 30, 2025, 11:42 PM");
}

#[test]
fn a_day_has_its_year_and_a_time_of_day_the_clock_it_had_then() {
    assert_eq!(date_of(at("2026-11-06T12:13:00Z")), "Nov 6, 2026");
    assert_eq!(time_of(at("2026-11-06T12:13:00Z")), "12:13 PM");
    assert_eq!(date_and_time_of(at("2026-11-06T12:13:00Z")), "Nov 6, 2026, 12:13 PM");
    assert_eq!(date_of(at("2025-12-30T23:42:00-08:00")), "Dec 30, 2025", "not the next day, by UTC");
    assert_eq!(time_of(at("2025-12-30T23:42:00-08:00")), "11:42 PM");
}

#[test]
fn a_time_to_come_is_said_the_same_way() {
    let now = at("2026-11-10T14:00:00-08:00");

    assert_eq!(day_and_time_from(at("2026-11-10T19:05:00-08:00"), now), "7:05 PM", "later today");
    assert_eq!(day_and_time_from(at("2026-11-16T20:59:00-08:00"), now), "Mon 8:59 PM", "in six days");
    assert_eq!(day_and_time_from(at("2026-11-17T20:59:00-08:00"), now), "Nov 17 8:59 PM", "a week on isn't today");
    assert_eq!(day_and_time_from(at("2027-01-02T09:00:00-08:00"), now), "Jan 2, 2027, 9:00 AM");
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

/// A zone whose clock changes `at` a moment, in UTC, from `before` hours from UTC to `after`.
#[derive(Clone)]
struct Changes {
    at: NaiveDateTime,
    before: FixedOffset,
    after: FixedOffset,
}

impl Changes {
    fn at(changes: &str, before: i32, after: i32) -> Changes {
        let hours = |hours| FixedOffset::east_opt(hours * 3600).unwrap();
        let at = NaiveDateTime::parse_from_str(changes, "%Y-%m-%d %H:%M").unwrap();
        Changes { at, before: hours(before), after: hours(after) }
    }
}

impl TimeZone for Changes {
    type Offset = FixedOffset;

    fn from_offset(offset: &FixedOffset) -> Changes {
        Changes { at: NaiveDateTime::MAX, before: *offset, after: *offset }
    }

    fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<FixedOffset> {
        self.offset_from_local_datetime(&local.and_time(NaiveTime::MIN))
    }

    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> MappedLocalTime<FixedOffset> {
        let before = (*local - self.before < self.at).then_some(self.before);
        let after = (*local - self.after >= self.at).then_some(self.after);
        match (before, after) {
            (Some(before), Some(after)) => MappedLocalTime::Ambiguous(before, after),
            (Some(one), None) | (None, Some(one)) => MappedLocalTime::Single(one),
            (None, None) => MappedLocalTime::None,
        }
    }

    fn offset_from_utc_date(&self, utc: &NaiveDate) -> FixedOffset {
        self.offset_from_utc_datetime(&utc.and_time(NaiveTime::MIN))
    }

    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> FixedOffset {
        if *utc < self.at { self.before } else { self.after }
    }
}

/// When `day_text` starts in `zone`.
fn starts(day_text: &str, zone: &Changes) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(start_of(day(day_text), zone).unwrap()).unwrap()
}

#[test]
fn a_day_starts_at_midnight_by_the_clock_it_has_then_rather_than_by_the_clock_of_the_day_before() {
    // Eight hours behind UTC until 2 AM, then seven.
    let springs = Changes::at("2026-03-08 10:00", -8, -7);

    assert_eq!(starts("2026-03-08", &springs), at("2026-03-08T00:00:00-08:00"));
    assert_eq!(starts("2026-03-09", &springs), at("2026-03-09T00:00:00-07:00"), "not 1 AM");
}

#[test]
fn a_day_whose_midnight_is_skipped_starts_as_the_clock_comes_back_and_one_gone_over_twice_at_the_first() {
    // At midnight, three hours behind UTC, the clock goes on an hour, to 1 AM.
    let springs = Changes::at("2026-10-04 03:00", -3, -2);
    // At 1 AM, two hours behind UTC, the clock goes back an hour, to midnight again.
    let falls = Changes::at("2027-02-21 03:00", -2, -3);

    assert_eq!(starts("2026-10-04", &springs), at("2026-10-04T01:00:00-02:00"));
    assert_eq!(starts("2027-02-21", &falls), at("2027-02-21T00:00:00-02:00"));
}
