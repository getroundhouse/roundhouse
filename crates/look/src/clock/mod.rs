//! Where the marks are in their motion, and what time it is for text on screen. A moving mark is redrawn on every frame
//! of the display, for the smoothest motion it can show. Reduce Motion holds every mark still, as it skips everything
//! else that moves. Pictures turn it on, and hold the time at one moment too.

#[cfg(test)]
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime, TimeZone, Utc};

#[cfg(test)]
mod tests;

mod time;

#[cfg(feature = "testing")]
pub use time::hold_still;
pub use time::{
    date, date_and_time, day_and_time, days_apart, local_time, months_apart, next_day_at_ms, now_ms, time_of_day,
};
#[cfg(test)]
use time::{date_and_time_of, date_of, day_and_time_from, months_between, start_of, time_of};
pub(crate) use time::{elapsed, moving};
