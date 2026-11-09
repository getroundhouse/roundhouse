//! Where the marks are in their motion, and what time it is for text on screen. A moving mark is redrawn on every frame
//! of the display, for the smoothest motion it can show. Pictures, and the system's Reduce Motion setting, hold every
//! mark still, and pictures hold the time at one moment too.

use std::sync::OnceLock;
use std::time::Instant;

use chrono::{DateTime, FixedOffset, Local, Utc};
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

/// Holds every mark still from now on, and the time at `at_ms`, so pictures of the app come out the same every time.
#[cfg(feature = "testing")]
pub fn hold_still(at_ms: i64, cx: &mut App) {
    cx.set_global(Still { at_ms });
}
