//! What's left of the account's limits, from what the CLI answers `get_usage`. `docs/claude-cli.md` has how it
//! behaves.

use std::cmp::Reverse;

use chrono::DateTime;
use protocol::model::{LimitWindow, ModelLabel, UsageLimit};
use serde_json::Value;
use tracing::debug;

use super::normalize::{whole_percent, window_named};

/// What the reply says is used of each limit, shortest window first, and models' weeks by their models. None for a
/// reply that says the account has no limits to tell of, such as a login with an API key, though it's asked again all
/// the same, since nobody has seen whether that can change. Only how much is used of each limit and when it starts
/// again is kept of the reply, which also says how the account was used, by day and week, and what it was spent on.
pub fn usage_limits(reply: &Value) -> Vec<UsageLimit> {
    if reply.get("rate_limits_available").and_then(Value::as_bool) == Some(false) {
        return Vec::new();
    }
    let Some(rate_limits) = reply.get("rate_limits") else { return Vec::new() };
    // The reply names every window it knows of, with `null` for one this account doesn't have.
    let windows = rate_limits.as_object().into_iter().flatten().filter_map(|(name, said)| limit(name, said));
    // And has the usage endpoint's own rows, where each model's own week is one, saying whose.
    let rows = rate_limits.get("limits").and_then(Value::as_array).into_iter().flatten();
    let mut limits: Vec<UsageLimit> = windows.chain(rows.filter_map(models_week)).collect();
    // Two rows of one model's week would be one limit, and the more used is the one that stops it first.
    limits.sort_by(|a, b| {
        (a.window, &a.model, Reverse(a.used_percent)).cmp(&(b.window, &b.model, Reverse(b.used_percent)))
    });
    limits.dedup_by(|later, kept| (later.window, &later.model) == (kept.window, &kept.model));
    limits
}

/// Whether the reply lists every model's own week the account has, which it does when it has the usage endpoint's
/// rows, even with none of them a model's. One from a CLI that doesn't pass the rows on says nothing of the models'
/// weeks it leaves out.
pub fn lists_every_models_week(reply: &Value) -> bool {
    reply.pointer("/rate_limits/limits").is_some_and(Value::is_array)
}

/// A window the account has, which says how much of it is used as a whole percent.
fn limit(name: &str, said: &Value) -> Option<UsageLimit> {
    let window = window_named(name)?;
    let used_percent = whole_percent(said.get("utilization")?.as_f64()?);
    Some(UsageLimit { window, model: None, used_percent, resets_at_ms: resets_at_ms(said), heard_at_ms: None })
}

/// The limit a row of the usage endpoint's is, when it's one model's own week: `weekly_scoped`, and scoped to a model
/// alone, which it names by the label its provider gives it. `get_usage`'s own schema doesn't declare the rows, which
/// it passes on as the endpoint sent them, but the schema of the CLI's `/usage` gives them, and says a row is told by
/// its `kind`, never by its label, and shown as it comes. So the label is the limit's model whatever it says, and a new
/// model's week needs no new build. A `weekly_all` row says what the reply's `seven_day` does, and a row of any other
/// kind isn't read. A week scoped to a surface, or to a model on one surface, has no window here.
fn models_week(row: &Value) -> Option<UsageLimit> {
    if row.get("kind").and_then(Value::as_str) != Some("weekly_scoped") {
        return None;
    }
    let of_a_surface = row.pointer("/scope/surface").is_some_and(|surface| !surface.is_null());
    let label = row.pointer("/scope/model/display_name").and_then(Value::as_str).map(str::trim);
    let Some(label) = label.filter(|label| !label.is_empty() && !of_a_surface) else {
        let scope = row.get("scope").unwrap_or(&Value::Null);
        debug!(%scope, "get_usage has a week of no model's own");
        return None;
    };
    Some(UsageLimit {
        window: LimitWindow::ModelSevenDays,
        model: Some(ModelLabel::new(label)),
        used_percent: whole_percent(row.get("percent")?.as_f64()?),
        resets_at_ms: resets_at_ms(row),
        heard_at_ms: None,
    })
}

/// When what `said` is of starts again, which it says as an ISO time with an offset.
fn resets_at_ms(said: &Value) -> Option<i64> {
    let at = DateTime::parse_from_rfc3339(said.get("resets_at")?.as_str()?).ok()?;
    Some(at.timestamp_millis())
}

#[cfg(test)]
mod tests {
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
                    model: None,
                    used_percent: 61,
                    resets_at_ms: Some(4_102_446_600_000),
                    heard_at_ms: None
                },
                UsageLimit {
                    window: LimitWindow::SevenDays,
                    model: None,
                    used_percent: 4,
                    resets_at_ms: Some(4_102_444_800_000),
                    heard_at_ms: None
                },
            ]
        );
    }

    #[test]
    fn each_models_own_week_is_a_row_told_by_its_kind_and_named_by_the_models_label() {
        let row = |kind: &str, percent, scope: Value| {
            json!({"kind": kind, "group": "weekly", "percent": percent, "resets_at": "2100-01-01T00:00:00Z",
                "scope": scope, "severity": "normal", "is_active": false})
        };
        let model = |label: &str| json!({"model": {"display_name": label, "id": null}, "surface": null});
        let reply = reply(&json!({
            "seven_day": {"utilization": 4, "resets_at": null},
            "limits": [
                row("weekly_all", 4, Value::Null),
                row("weekly_scoped", 88, model(" Fable 5.1 ")),
                row("weekly_scoped", 30, model("Opus")),
                row("weekly_scoped", 50, json!({"model": null, "surface": {"display_name": "Claude Code"}})),
                row("weekly_scoped", 60, model("")),
                row("session", 70, model("Opus")),
                row("weekly_scoped", 40, json!({"model": {"display_name": "Opus"}, "surface": {"display_name": "Web"}})),
                // The same model's week twice, of which the more used is kept.
                row("weekly_scoped", 20, model("Opus")),
            ],
        }));

        let limits = usage_limits(&reply);

        let said: Vec<_> = limits
            .iter()
            .map(|limit| (limit.window, limit.model.as_ref().map(ModelLabel::as_str), limit.used_percent))
            .collect();
        assert_eq!(
            said,
            [
                (LimitWindow::SevenDays, None, 4),
                (LimitWindow::ModelSevenDays, Some("Fable 5.1"), 88),
                (LimitWindow::ModelSevenDays, Some("Opus"), 30),
            ]
        );
        assert_eq!(limits[1].resets_at_ms, Some(4_102_444_800_000));
    }

    #[test]
    fn a_reply_with_the_endpoints_rows_lists_every_models_week_even_with_none_and_one_without_them_doesnt() {
        assert!(lists_every_models_week(&reply(&json!({"seven_day": null, "limits": []}))));
        assert!(!lists_every_models_week(&reply(&json!({"seven_day": {"utilization": 4, "resets_at": null}}))));
        assert!(!lists_every_models_week(&reply(&json!({"limits": null}))), "no rows at all, as the schema has it");
        assert!(!lists_every_models_week(&json!({})));
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
