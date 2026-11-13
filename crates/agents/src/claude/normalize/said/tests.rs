use protocol::event::{EventBody, FailureKind, NoticeLevel, RequestFailure, TurnOutcome};
use serde_json::{Value, json};

use super::super::{Normalizer, Parsed, parse_line};

/// A line the CLI wrote itself in the model's place, as it writes one saying the API failed, with `more` on it.
fn written_by_the_cli(text: &str, more: &Value) -> Value {
    let mut line = json!({
        "type": "assistant",
        "parent_tool_use_id": null,
        "message": {"id": "m-1", "model": "<synthetic>", "role": "assistant", "content": [{"type": "text", "text": text}]},
    });
    line.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
    line
}

/// A usage limit, as the CLI says one, which it ends the turn on.
fn limit_reached() -> Value {
    written_by_the_cli(
        "You've reached your limit. Manage usage at https://example.com/usage, to continue.",
        &json!({
            "error": "rate_limit",
            "is_api_error_message": true,
            "api_error_status": 429,
            "api_error": "model_requires_usage_credits",
            "api_error_params": {"rate_limit_info": {"status": "rejected", "resetsAt": 1_700_000_000}},
        }),
    )
}

fn parsed(line: &Value) -> EventBody {
    let Parsed::Event(body) = parse_line(&line.to_string()) else { panic!("{line}") };
    body
}

#[test]
fn a_failed_request_says_what_kind_of_failure_it_was_in_the_clis_words_and_when_a_limit_resets() {
    let failure = RequestFailure {
        message: "You've reached your limit. Manage usage at https://example.com/usage, to continue.".into(),
        kind: FailureKind::Limit,
        resets_at_ms: Some(1_700_000_000_000),
    };

    assert_eq!(parsed(&limit_reached()), EventBody::RequestFailed { parent_tool_use_id: None, failure });
}

#[test]
fn a_subagents_failed_request_is_its_own() {
    let mut line = limit_reached();
    line["parent_tool_use_id"] = json!("call-1");

    let EventBody::RequestFailed { parent_tool_use_id, .. } = parsed(&line) else { panic!() };

    assert_eq!(parent_tool_use_id.as_ref().map(ToString::to_string).as_deref(), Some("call-1"));
}

#[test]
fn a_failure_is_the_kind_the_cli_says_and_one_it_doesnt_name_is_unknown() {
    let kind = |error: Option<&str>| {
        let more = match error {
            Some(error) => json!({"is_api_error_message": true, "error": error}),
            None => json!({"is_api_error_message": true}),
        };
        let EventBody::RequestFailed { failure, .. } = parsed(&written_by_the_cli("API Error", &more)) else {
            panic!()
        };
        failure.kind
    };

    assert_eq!(kind(Some("authentication_failed")), FailureKind::SignIn);
    assert_eq!(kind(Some("billing_error")), FailureKind::Account);
    assert_eq!(kind(Some("overloaded")), FailureKind::Overloaded);
    assert_eq!(kind(Some("server_error")), FailureKind::ServerError);
    assert_eq!(kind(Some("model_not_found")), FailureKind::Refused);
    assert_eq!(kind(Some("something_newer")), FailureKind::Unknown);
    assert_eq!(kind(None), FailureKind::Unknown);
}

#[test]
fn what_the_cli_wrote_itself_without_an_error_is_a_note_of_its_own_and_not_the_agents_words() {
    let line = written_by_the_cli("Auto-compact window set to 500k tokens", &json!({}));

    let note = EventBody::Notice {
        level: NoticeLevel::Info,
        message: "Auto-compact window set to 500k tokens".into(),
        parent_tool_use_id: None,
    };
    assert_eq!(parsed(&line), note);
}

#[test]
fn what_a_command_printed_is_a_notice_as_the_cli_wrote_it_and_what_it_printed_as_an_error_a_warning() {
    let replayed = |text: &str| json!({"type": "user", "isReplay": true, "message": {"role": "user", "content": text}, "session_id": "s"});
    let notice = |level, message: &str| EventBody::Notice { level, message: message.into(), parent_tool_use_id: None };

    let model_set = replayed("<local-command-stdout>Set model to `sonnet (claude-sonnet-5-5)`</local-command-stdout>");
    let refused = replayed("<local-command-stderr>No such model</local-command-stderr>");

    assert_eq!(parsed(&model_set), notice(NoticeLevel::Info, "Set model to `sonnet (claude-sonnet-5-5)`"));
    assert_eq!(parsed(&refused), notice(NoticeLevel::Warning, "No such model"));
    let typed = json!({"type": "user", "message": {"role": "user", "content": "<local-command-stdout>x</local-command-stdout>"}});
    assert!(matches!(parsed(&typed), EventBody::AgentUser { .. }), "only a replayed line is the CLI's");
}

/// The CLI ends the turn for the API's error, with the failure's words as its result.
fn ended_for_an_error_from_the_api() -> String {
    json!({"type": "result", "subtype": "success", "is_error": true, "terminal_reason": "api_error", "result": "x"})
        .to_string()
}

fn request_failed(end: &Parsed) -> bool {
    let Parsed::Event(EventBody::TurnEnded { outcome: TurnOutcome::Error, request_failed, .. }) = end else {
        panic!("{end:?}")
    };
    *request_failed
}

#[test]
fn a_turn_that_ends_on_the_agents_own_failed_request_has_nothing_to_add_to_it() {
    let mut normalizer = Normalizer::default();

    normalizer.line(&limit_reached().to_string());
    let ended = normalizer.line(&ended_for_an_error_from_the_api());
    let next = normalizer.line(&ended_for_an_error_from_the_api());

    assert!(request_failed(&ended.parsed));
    assert!(!request_failed(&next.parsed), "the next turn starts with none");
}

#[test]
fn a_turn_that_ends_after_a_subagents_failed_request_says_why_it_ended_itself() {
    let mut normalizer = Normalizer::default();
    let mut subagents = limit_reached();
    subagents["parent_tool_use_id"] = json!("call-1");

    normalizer.line(&subagents.to_string());
    let ended = normalizer.line(&ended_for_an_error_from_the_api());

    assert!(!request_failed(&ended.parsed));
}

/// The CLI ends a turn on its own failed request for more than an error from the API, as for a conversation that
/// keeps filling up as fast as it's compacted, which its code says is a `blocking_limit`.
#[test]
fn a_turn_that_ends_on_its_failed_request_for_any_reason_has_nothing_to_add_to_it() {
    let mut normalizer = Normalizer::default();
    let too_full = written_by_the_cli(
        "Autocompact is thrashing.",
        &json!({"is_api_error_message": true, "error": "invalid_request"}),
    );
    let blocked =
        json!({"type": "result", "subtype": "success", "is_error": true, "terminal_reason": "blocking_limit"});

    normalizer.line(&too_full.to_string());
    let ended = normalizer.line(&blocked.to_string());

    assert!(request_failed(&ended.parsed));
}

#[test]
fn a_turn_that_went_on_past_a_failed_request_and_failed_for_something_else_says_why_itself() {
    let mut normalizer = Normalizer::default();
    let went_on = json!({
        "type": "assistant", "parent_tool_use_id": null,
        "message": {"id": "m-2", "model": "claude-opus-5-5", "role": "assistant", "content": [{"type": "text", "text": "Trying again."}]},
    });
    let failed_otherwise = json!({
        "type": "result", "subtype": "error_during_execution", "is_error": true, "terminal_reason": "max_turns",
        "errors": ["Reached the most turns"],
    });

    normalizer.line(&limit_reached().to_string());
    normalizer.line(&went_on.to_string());
    let ended = normalizer.line(&failed_otherwise.to_string());

    assert!(!request_failed(&ended.parsed));
}
