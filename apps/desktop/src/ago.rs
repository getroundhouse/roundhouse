//! How long ago something happened, as the app says it.

const MINUTE_MS: i64 = 60_000;
const MINUTES_IN_AN_HOUR: i64 = 60;
const MINUTES_IN_A_DAY: i64 = 24 * MINUTES_IN_AN_HOUR;

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

#[cfg(test)]
mod tests {
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
    fn a_time_after_now_is_just_now() {
        assert_eq!(ago(NOW_MS + 5 * MINUTE_MS, NOW_MS), "just now");
        assert_eq!(ago(i64::MAX, i64::MIN), "just now");
    }
}
