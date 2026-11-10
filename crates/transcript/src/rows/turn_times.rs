//! How long a turn has taken so far, or took, as the transcript says them.

use std::time::Duration;

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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{so_far, took};

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
}
