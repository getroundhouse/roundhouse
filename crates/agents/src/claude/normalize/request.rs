//! One of the agent's own requests to its model, from the `message_start` the CLI passes on as the model starts to
//! answer: how its input met the API's cache. `docs/claude-cli.md` has how the CLI says it.

use protocol::model::{CacheRequest, CacheTtl, ModelName};
use serde_json::Value;

/// What a `stream_event` line's `message_start` says of its request, or `None` for a subagent's, whose cache is a
/// conversation of its own, and for any other line. Whether it's the first request for a prompt is the process's to
/// add, since only the process knows when the CLI took one.
pub(super) fn request(line: &Value, event: &Value) -> Option<CacheRequest> {
    if event.get("type").and_then(Value::as_str) != Some("message_start")
        || line.get("parent_tool_use_id").is_some_and(|parent| !parent.is_null())
    {
        return None;
    }
    let message = event.get("message").unwrap_or(&Value::Null);
    let usage = message.get("usage").unwrap_or(&Value::Null);
    let tokens = |of: &Value, key: &str| of.get(key).and_then(Value::as_u64).unwrap_or(0);
    let written = usage.get("cache_creation").unwrap_or(&Value::Null);
    // A request can write at both, with a breakpoint each. What it writes at 5m comes after what it writes at 1h, as
    // the API asks, so it's the conversation, and it lasts 5m.
    let ttl = if tokens(written, "ephemeral_5m_input_tokens") > 0 {
        Some(CacheTtl::FiveMinutes)
    } else if tokens(written, "ephemeral_1h_input_tokens") > 0 {
        Some(CacheTtl::OneHour)
    } else {
        None
    };
    Some(CacheRequest {
        sent_ms_ago: line.get("ttft_ms").and_then(Value::as_u64).unwrap_or(0),
        ttl,
        read_tokens: tokens(usage, "cache_read_input_tokens"),
        written_tokens: tokens(usage, "cache_creation_input_tokens"),
        model: message.get("model").and_then(Value::as_str).and_then(|name| ModelName::try_from(name.to_string()).ok()),
        first_for_prompt: false,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The first of the agent's requests in the CLI's real output, which `tests/claude/contract/mod.rs` reads whole, for
    /// each test to change only what it's about.
    fn real() -> Value {
        let fixture = include_str!("../../../tests/fixtures/claude.jsonl");
        let line = fixture.lines().find(|line| line.contains(r#""type": "message_start""#)).unwrap();
        serde_json::from_str(line).unwrap()
    }

    fn of(line: &Value) -> Option<CacheRequest> {
        request(line, &line["event"])
    }

    #[test]
    fn one_that_wrote_at_both_lasts_the_shorter_and_one_that_wrote_nothing_doesnt_say() {
        let (mut both, mut neither) = (real(), real());
        both["event"]["message"]["usage"]["cache_creation"]["ephemeral_5m_input_tokens"] = json!(300);
        neither["event"]["message"]["usage"]["cache_creation"] =
            json!({"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 0});

        assert_eq!(of(&real()).unwrap().ttl, Some(CacheTtl::OneHour));
        assert_eq!(of(&both).unwrap().ttl, Some(CacheTtl::FiveMinutes));
        assert_eq!(of(&neither).unwrap().ttl, None);
    }

    #[test]
    fn a_subagents_request_and_any_other_event_say_nothing() {
        let (mut subagent, mut other) = (real(), real());
        subagent["parent_tool_use_id"] = json!("toolu_1");
        other["event"] = json!({"type": "message_stop"});

        assert_eq!(of(&subagent), None);
        assert_eq!(of(&other), None);
    }

    #[test]
    fn a_line_that_says_less_still_says_what_it_can() {
        let mut bare = real();
        bare.as_object_mut().unwrap().remove("ttft_ms");
        bare["event"]["message"] = json!({"id": "msg_1"});

        let request = of(&bare).unwrap();

        assert_eq!((request.sent_ms_ago, request.ttl, request.model), (0, None, None));
    }
}
