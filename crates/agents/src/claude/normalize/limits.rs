//! How much of the account's limits is used, which the CLI says in a `rate_limit_event` line as it works.

use protocol::event::EventBody;
use protocol::model::{LimitWindow, UsageLimit};
use serde_json::Value;

use super::{Parsed, str_of};

/// A line that says how much of some limit is used is the limits it names. One that doesn't, such as one with only
/// the limit's status, is kept as it came, like any line with no form of its own.
pub(super) fn rate_limit_event(value: &Value) -> Parsed {
    let limits = limits(value.get("rate_limit_info").unwrap_or(&Value::Null));
    Parsed::Event(if limits.is_empty() {
        EventBody::AgentOther { kind: "rate_limit_event".to_string() }
    } else {
        EventBody::AgentLimits { limits }
    })
}

/// Each window the line says how much is used of, shortest first. It names every window under `unifiedWindows`, and
/// a line without that names one, as `rateLimitType`. A window with no name here is left out, such as the one for
/// use past the plan's limit, which repeats the week's.
fn limits(info: &Value) -> Vec<UsageLimit> {
    let mut limits: Vec<UsageLimit> = match info.get("unifiedWindows").and_then(Value::as_object) {
        Some(windows) => windows.iter().filter_map(|(name, window)| limit(name, window)).collect(),
        None => str_of(info, "rateLimitType").and_then(|name| limit(name, info)).into_iter().collect(),
    };
    limits.sort_by_key(|limit| limit.window);
    limits
}

fn limit(name: &str, window: &Value) -> Option<UsageLimit> {
    let window_kind = window_named(name)?;
    let share = window.get("utilization")?.as_f64()?;
    let resets_at_ms = window.get("resetsAt").and_then(Value::as_i64).map(|s| s.saturating_mul(1000));
    // A share from 0 to 1, unlike the whole percent `get_usage` answers with.
    Some(UsageLimit { window: window_kind, used_percent: whole_percent(share * 100.), resets_at_ms, heard_at_ms: None })
}

/// The window the CLI calls `name`, in this line and in what it answers `get_usage` alike. `None` for one with no
/// name here.
pub(in crate::claude) fn window_named(name: &str) -> Option<LimitWindow> {
    match name {
        "five_hour" => Some(LimitWindow::FiveHours),
        "seven_day" => Some(LimitWindow::SevenDays),
        _ => None,
    }
}

/// A percent of a limit used, as a whole one. More than all of it reads as all of it.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0 to 100 first, and a percent that isn't a number reads as none used"
)]
pub(in crate::claude) fn whole_percent(used: f64) -> u8 {
    if used.is_nan() {
        return 0;
    }
    used.round().clamp(0., 100.) as u8
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn limit(window: LimitWindow, used_percent: u8, resets_at_ms: Option<i64>) -> UsageLimit {
        UsageLimit { window, used_percent, resets_at_ms, heard_at_ms: None }
    }

    #[test]
    fn every_window_the_line_names_is_a_limit_shortest_first() {
        let info = json!({
            "status": "allowed", "resetsAt": 1_794_891_600, "rateLimitType": "seven_day",
            "unifiedWindows": {
                "seven_day": {"utilization": 0.04, "resetsAt": 1_794_891_600, "status": "allowed"},
                "seven_day_overage_included": {"utilization": 0.04, "resetsAt": 1_794_891_600, "status": "allowed"},
                "five_hour": {"utilization": 0.615, "resetsAt": 1_794_300_000, "status": "allowed"},
            },
        });

        assert_eq!(
            limits(&info),
            [
                limit(LimitWindow::FiveHours, 62, Some(1_794_300_000_000)),
                limit(LimitWindow::SevenDays, 4, Some(1_794_891_600_000))
            ]
        );
    }

    #[test]
    fn a_line_with_one_window_and_no_list_gives_that_one() {
        let info = json!({"status": "allowed", "rateLimitType": "five_hour", "utilization": 1.3, "resetsAt": 10});

        assert_eq!(limits(&info), [limit(LimitWindow::FiveHours, 100, Some(10_000))]);
    }

    #[test]
    fn a_line_that_doesnt_say_how_much_is_used_gives_no_limit_and_is_only_logged() {
        let line =
            json!({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed", "rateLimitType": "seven_day"}});

        assert_eq!(
            rate_limit_event(&line),
            Parsed::Event(EventBody::AgentOther { kind: "rate_limit_event".to_string() })
        );
        assert_eq!(limits(&json!({"unifiedWindows": {"thirty_day": {"utilization": 0.5}}})), []);
    }

    #[test]
    fn a_line_with_limits_is_the_limits_it_names() {
        let line = json!({"type": "rate_limit_event", "rate_limit_info": {
            "unifiedWindows": {"seven_day": {"utilization": 0.04, "resetsAt": 1_794_891_600}},
        }});

        let limits = vec![limit(LimitWindow::SevenDays, 4, Some(1_794_891_600_000))];
        assert_eq!(rate_limit_event(&line), Parsed::Event(EventBody::AgentLimits { limits }));
    }
}
