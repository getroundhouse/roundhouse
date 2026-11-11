//! What's left of the account's limits, from what the CLI answers `get_usage`. `docs/claude-cli.md` has how it
//! behaves.

use chrono::DateTime;
use protocol::model::UsageLimit;
use serde_json::Value;

use super::normalize::{whole_percent, window_named};

/// What the reply says is used of each limit, shortest window first. None for a reply that says the account has no
/// limits to tell of, such as a login with an API key, though it's asked again all the same, since nobody has seen
/// whether that can change. Only how much is used of each window and when it starts again is kept of the reply, which
/// also says how the account was used, by day and week, and what it was spent on.
pub fn usage_limits(reply: &Value) -> Vec<UsageLimit> {
    if reply.get("rate_limits_available").and_then(Value::as_bool) == Some(false) {
        return Vec::new();
    }
    // The reply names every window it knows of, with `null` for one this account doesn't have.
    let windows = reply.get("rate_limits").and_then(Value::as_object).into_iter().flatten();
    let mut limits: Vec<UsageLimit> = windows.filter_map(|(name, said)| limit(name, said)).collect();
    limits.sort_by_key(|limit| limit.window);
    limits
}

/// A window the account has, which says how much of it is used as a whole percent.
fn limit(name: &str, said: &Value) -> Option<UsageLimit> {
    let window = window_named(name)?;
    let used_percent = whole_percent(said.get("utilization")?.as_f64()?);
    let resets_at = said.get("resets_at").and_then(Value::as_str).and_then(|at| DateTime::parse_from_rfc3339(at).ok());
    Some(UsageLimit {
        window,
        used_percent,
        resets_at_ms: resets_at.map(|at| at.timestamp_millis()),
        heard_at_ms: None,
    })
}

#[cfg(test)]
mod tests {
    use protocol::model::LimitWindow;
    use serde_json::json;

    use super::*;

    /// A reply shaped like the CLI's, with made-up numbers, and only some of what it holds besides the limits.
    fn reply(rate_limits: &Value) -> Value {
        json!({
            "session": {"total_cost_usd": 0.02, "model_usage": {}},
            "subscription_type": "max",
            "rate_limits_available": true,
            "rate_limits": rate_limits,
            "behaviors": {"day": {"request_count": 3, "session_count": 1}},
        })
    }

    #[test]
    fn a_reply_gives_how_much_is_used_of_each_window_and_when_it_starts_again() {
        let reply = reply(&json!({
            "seven_day": {"utilization": 4, "resets_at": "2100-01-01T00:00:00.000000+00:00", "limit_dollars": null},
            "five_hour": {"utilization": 61, "resets_at": "2100-01-01T02:30:00+02:00"},
            "seven_day_opus": null,
            "model_scoped": [{"display_name": "Fable", "utilization": 9, "resets_at": "2100-01-01T00:00:00Z"}],
        }));

        assert_eq!(
            usage_limits(&reply),
            [
                UsageLimit {
                    window: LimitWindow::FiveHours,
                    used_percent: 61,
                    resets_at_ms: Some(4_102_446_600_000),
                    heard_at_ms: None
                },
                UsageLimit {
                    window: LimitWindow::SevenDays,
                    used_percent: 4,
                    resets_at_ms: Some(4_102_444_800_000),
                    heard_at_ms: None
                },
            ]
        );
    }

    #[test]
    fn a_window_the_account_doesnt_have_is_left_out() {
        let reply = reply(&json!({"five_hour": null, "seven_day": {"utilization": 4, "resets_at": null}}));

        let limits = usage_limits(&reply);

        assert_eq!(
            limits.iter().map(|limit| (limit.window, limit.resets_at_ms)).collect::<Vec<_>>(),
            [(LimitWindow::SevenDays, None)]
        );
    }

    #[test]
    fn a_reply_that_says_the_account_has_no_limits_to_tell_of_gives_none() {
        let reply = json!({"rate_limits_available": false, "rate_limits": {"seven_day": {"utilization": 4}}});

        assert_eq!(usage_limits(&reply), []);
    }

    #[test]
    fn a_reply_that_says_nothing_of_limits_gives_none() {
        assert_eq!(usage_limits(&json!({})), []);
        assert_eq!(usage_limits(&reply(&json!({"seven_day": {"utilization": "most"}}))), []);
        assert_eq!(usage_limits(&reply(&json!({"thirty_day": {"utilization": 4}}))), [], "a window with no name here");
    }

    #[test]
    fn more_than_all_of_a_limit_reads_as_all_of_it() {
        let reply = reply(&json!({"seven_day": {"utilization": 130.4}}));

        assert_eq!(usage_limits(&reply)[0].used_percent, 100);
    }
}
