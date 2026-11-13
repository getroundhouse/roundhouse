//! Checks the adapter against the real `claude` CLI's answers to control requests. `../fixtures/claude_controls.jsonl`
//! was captured by `tools/smoke/capture_claude_fixture.py --controls`, which spends nothing. Rerun it after a CLI
//! upgrade: a failure here means the CLI answers in a way the adapter doesn't expect.

#![cfg(test)]

use agents::claude::{ModelWindows, Normalizer, Parsed};
use protocol::model::{LimitWindow, MaxSetBy, UsageLimit};
use serde_json::Value;

/// Control requests and the CLI's replies, from `../fixtures/claude_controls.jsonl`, by the same script with
/// `--controls`. Its process was launched with the adapter's flags, which include `--effort high`.
const CONTROLS: &str = include_str!("../fixtures/claude_controls.jsonl");
/// A conversation's lines, from `../fixtures/claude.jsonl`, by the same script without `--controls`.
const CONVERSATION: &str = include_str!("../fixtures/claude.jsonl");

/// Each request sent, with what the adapter makes of the CLI's reply to it.
fn exchanges() -> Vec<(Value, Result<Value, String>)> {
    let mut normalizer = Normalizer::default();
    let lines: Vec<&str> = CONTROLS.lines().collect();
    lines
        .chunks(2)
        .map(|pair| {
            let sent: Value = serde_json::from_str(pair[0]).unwrap();
            let Parsed::ControlReply { request_id, answer } = normalizer.line(pair[1]).parsed else {
                panic!("not a reply: {}", pair[1]);
            };
            assert_eq!(sent["request_id"], request_id.as_str());
            (sent["request"].clone(), answer)
        })
        .collect()
}

/// The CLI's answer to `request`, and to the `get_settings` sent next.
fn and_then_settings(request: &Value) -> (Result<Value, String>, Value) {
    let exchanges = exchanges();
    let at = exchanges.iter().position(|(sent, _)| sent == request).unwrap();
    let (asked, settings) = &exchanges[at + 1];
    assert_eq!(asked["subtype"], "get_settings");
    (exchanges[at].1.clone(), settings.clone().unwrap())
}

#[test]
fn the_effort_launched_with_is_the_one_reported_in_use() {
    let (asked, settings) = exchanges().remove(0);

    assert_eq!(asked["subtype"], "get_settings");
    assert_eq!(agents::claude::applied_effort(&settings.unwrap()), Some("high"));
}

#[test]
fn an_effort_the_cli_takes_is_the_one_it_then_reports_in_use() {
    let request = serde_json::json!({"subtype": "apply_flag_settings", "settings": {"effortLevel": "low"}});

    let (answer, settings) = and_then_settings(&request);

    assert!(answer.is_ok(), "{answer:?}");
    assert_eq!(agents::claude::applied_effort(&settings), Some("low"));
}

/// Why the adapter reads the level back: success alone doesn't say the level took.
#[test]
fn an_effort_the_cli_doesnt_know_is_answered_success_and_changes_nothing() {
    let request = serde_json::json!({"subtype": "apply_flag_settings", "settings": {"effortLevel": "ultra"}});

    let (answer, settings) = and_then_settings(&request);

    assert!(answer.is_ok(), "{answer:?}");
    assert_eq!(agents::claude::applied_effort(&settings), Some("low"));
}

/// The hub keeps a session's effort when only its model changes, which is right only while the CLI does too.
#[test]
fn a_new_model_keeps_the_effort_in_use() {
    let request = serde_json::json!({"subtype": "set_model", "model": "sonnet"});

    let (answer, settings) = and_then_settings(&request);

    assert!(answer.unwrap()["model"].as_str().is_some_and(|model| model.contains("sonnet")));
    assert!(settings["applied"]["model"].as_str().is_some_and(|model| model.contains("sonnet")), "{settings}");
    assert_eq!(agents::claude::applied_effort(&settings), Some("low"));
}

/// The CLI's answer to the request with this subtype.
fn answer_to(subtype: &str) -> Result<Value, String> {
    let (_, answer) = exchanges().into_iter().find(|(sent, _)| sent["subtype"] == subtype).unwrap();
    answer
}

#[test]
fn the_cli_says_how_full_its_context_is_and_what_its_made_of() {
    // How much each model holds, from the conversation's `result` lines, since the reply doesn't say.
    let mut windows = ModelWindows::default();
    for line in CONVERSATION.lines().filter(|line| line.contains(r#""type": "result""#)) {
        windows.heard(&serde_json::from_str(line).unwrap());
    }
    let context = agents::claude::context_usage(&answer_to("get_context_usage").unwrap(), &windows).unwrap();

    let tokens = |name: &str| context.parts.iter().find(|part| part.name == name).map(|part| part.tokens);
    assert!(context.used_tokens > 0, "the system prompt and tools are there before anything is said");
    assert!(tokens("System prompt").is_some() && tokens("Messages").is_some(), "{:?}", context.parts);
    // What's used, what's kept for compacting and what's free are the whole context, so the ring's share is of
    // everything the context holds. Tools loaded only when needed are listed too, and counted in none of them.
    let (buffer, free) = (tokens("Autocompact buffer").unwrap(), tokens("Free space").unwrap());
    assert_eq!(context.used_tokens + buffer + free, context.max_tokens);
    assert!(tokens("System tools (deferred)").is_some());
    // Captured with nothing set to make the context smaller, so it holds as much as its model does.
    assert_eq!(context.max_set_by, Some(MaxSetBy::Model));
    assert_eq!(context.model_tokens, Some(context.max_tokens));
}

/// Its numbers are made up as it's captured, since they're the account's, so this checks the keys the adapter reads.
/// The five hours' window is there only when the account has been used in them, and a model's own week only when the
/// account has one.
#[test]
fn the_cli_says_how_much_is_used_of_each_of_its_accounts_limits_and_when_it_starts_again() {
    let answer = answer_to("get_usage").unwrap();
    let limits = agents::claude::usage_limits(&answer);

    assert!(limits.iter().any(|limit| limit.window == LimitWindow::SevenDays), "{limits:?}");
    // The rows are every model's week, and each is a limit, named by the model's label.
    assert!(agents::claude::lists_every_models_week(&answer), "the CLI passes the usage endpoint's rows on");
    // A model's on one surface isn't its own week, and two rows with one label are one.
    let rows = answer["rate_limits"]["limits"].as_array().unwrap();
    let scoped = rows.iter().filter(|row| row["kind"] == "weekly_scoped" && row["scope"]["surface"].is_null());
    let labels = scoped.filter_map(|row| row["scope"]["model"]["display_name"].as_str()).map(str::trim);
    let mut labels: Vec<&str> = labels.filter(|label| !label.is_empty()).collect();
    labels.sort_unstable();
    labels.dedup();
    let models = limits.iter().filter(|limit| limit.window == LimitWindow::ModelSevenDays);
    let named: Vec<&str> = models.map(|limit| limit.model.as_ref().unwrap().as_str()).collect();
    assert_eq!(named, labels, "{limits:?}");
    // What it says of its use beyond the plan's limits has no window here. A row with no time it starts again is
    // captured with none.
    let made_up = |limit: &UsageLimit| {
        limit.used_percent == 25 && limit.resets_at_ms.is_none_or(|at_ms| at_ms == 4_102_444_800_000)
    };
    assert!(limits.iter().all(made_up), "{limits:?}");
}

#[test]
fn a_request_the_cli_doesnt_know_is_refused_rather_than_left_unanswered() {
    let refused = answer_to("not_a_request").unwrap_err();

    assert!(refused.contains("Unsupported control request subtype"), "{refused}");
}
