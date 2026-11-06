//! Turns one line of Claude's stdout into a normalized event.
//!
//! This has to survive the CLI changing underneath it, so it reads loosely typed JSON and never
//! fails. Anything it doesn't recognize still reaches the log as `AgentOther` with the raw line.

use protocol::event::{AgentRunState, ApprovalOutcome, ContentBlock, EventBody, TurnOutcome};
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
use serde_json::Value;

#[derive(Debug, PartialEq)]
pub enum Parsed {
    /// Goes in the log, with the raw line.
    Event(EventBody),
    /// Live only. The finished message replaces these.
    Delta(StreamDelta),
    /// The CLI is waiting on an answer the adapter doesn't support. It needs an error reply, or
    /// the session hangs.
    UnsupportedControl { request_id: String, subtype: String },
    /// Carries nothing worth storing.
    Drop,
}

pub fn parse_line(line: &str) -> Parsed {
    let parsed = serde_json::from_str::<Value>(line)
        // JavaScript strings can hold half of a surrogate pair, and JSON.stringify writes it as an
        // escape. That's rare, and it can land in a line that matters, such as an approval.
        .or_else(|_| serde_json::from_str::<Value>(&repair_lone_surrogates(line)));
    let Ok(value) = parsed else {
        return Parsed::Event(EventBody::AgentOther { kind: "unparsed".into() });
    };
    let kind = str_of(&value, "type").unwrap_or("unknown");
    match kind {
        "assistant" => Parsed::Event(assistant(&value)),
        "user" => Parsed::Event(user(&value)),
        "result" => Parsed::Event(turn_ended(&value)),
        "system" => system(&value),
        "stream_event" => stream_event(&value),
        "control_request" => control_request(&value),
        "control_cancel_request" => match str_of(&value, "request_id") {
            Some(id) => Parsed::Event(EventBody::ApprovalResolved {
                approval_id: id.to_string(),
                outcome: ApprovalOutcome::Cancelled,
            }),
            None => other(kind),
        },
        // Replies to the adapter's own requests, such as an interrupt.
        "control_response" => Parsed::Drop,
        _ => other(kind),
    }
}

/// Replaces `\\uD800`-style escapes that aren't half of a proper pair with U+FFFD.
fn repair_lone_surrogates(line: &str) -> String {
    fn escape_at(bytes: &[u8], at: usize) -> Option<u16> {
        let hex = bytes.get(at + 2..at + 6)?;
        if bytes[at] != b'\\' || bytes[at + 1] != b'u' {
            return None;
        }
        u16::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()
    }
    const REPLACEMENT: &str = "\\ufffd";
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut copied_to = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        let Some(unit) = escape_at(bytes, i) else {
            // Some other escape, such as an escaped backslash. Its second byte isn't an escape start.
            i += 2;
            continue;
        };
        match unit {
            0xD800..=0xDBFF if matches!(escape_at(bytes, i + 6), Some(0xDC00..=0xDFFF)) => i += 12,
            0xD800..=0xDFFF => {
                out.push_str(&line[copied_to..i]);
                out.push_str(REPLACEMENT);
                i += 6;
                copied_to = i;
            }
            _ => i += 6,
        }
    }
    out.push_str(&line[copied_to..]);
    out
}

fn other(kind: &str) -> Parsed {
    Parsed::Event(EventBody::AgentOther { kind: kind.to_string() })
}

fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn string_of(value: &Value, key: &str) -> Option<String> {
    str_of(value, key).map(str::to_string)
}

fn system(value: &Value) -> Parsed {
    let subtype = str_of(value, "subtype").unwrap_or("unknown");
    match subtype {
        "init" => Parsed::Event(EventBody::AgentInit {
            agent_session_id: string_of(value, "session_id"),
            model: string_of(value, "model"),
            cwd: string_of(value, "cwd"),
            permission_mode: string_of(value, "permissionMode"),
            agent_version: string_of(value, "claude_code_version"),
        }),
        "session_state_changed" => Parsed::Event(EventBody::AgentState {
            state: match str_of(value, "state") {
                Some("idle") => AgentRunState::Idle,
                Some("running") => AgentRunState::Running,
                Some("requires_action") => AgentRunState::RequiresAction,
                _ => AgentRunState::Unknown,
            },
        }),
        // A running token count for a spinner. Several arrive per second while thinking.
        "thinking_tokens" => Parsed::Drop,
        other_subtype => other(&format!("system/{other_subtype}")),
    }
}

fn assistant(value: &Value) -> EventBody {
    let message = value.get("message").unwrap_or(&Value::Null);
    EventBody::Assistant {
        message_id: string_of(message, "id"),
        parent_tool_use_id: string_of(value, "parent_tool_use_id"),
        model: string_of(message, "model"),
        blocks: blocks(message.get("content")),
    }
}

fn user(value: &Value) -> EventBody {
    let message = value.get("message").unwrap_or(&Value::Null);
    EventBody::AgentUser {
        parent_tool_use_id: string_of(value, "parent_tool_use_id"),
        blocks: blocks(message.get("content")),
    }
}

fn blocks(content: Option<&Value>) -> Vec<ContentBlock> {
    match content {
        Some(Value::String(text)) => vec![ContentBlock::Text { text: text.clone() }],
        Some(Value::Array(items)) => items.iter().map(block).collect(),
        _ => Vec::new(),
    }
}

fn block(value: &Value) -> ContentBlock {
    match str_of(value, "type") {
        Some("text") => ContentBlock::Text { text: string_of(value, "text").unwrap_or_default() },
        Some("thinking") => ContentBlock::Thinking { text: string_of(value, "thinking").unwrap_or_default() },
        Some("tool_use") => ContentBlock::ToolUse {
            id: string_of(value, "id").unwrap_or_default(),
            name: string_of(value, "name").unwrap_or_default(),
            input: value.get("input").cloned().unwrap_or(Value::Null),
        },
        Some("tool_result") => {
            let (text, image_count) = tool_result_content(value.get("content"));
            ContentBlock::ToolResult {
                tool_use_id: string_of(value, "tool_use_id").unwrap_or_default(),
                is_error: value.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                text,
                image_count,
            }
        }
        Some("image") => {
            ContentBlock::Image { media_type: value.get("source").and_then(|s| string_of(s, "media_type")) }
        }
        _ => ContentBlock::Unknown,
    }
}

/// A tool result's content is a string, or an array of text and image blocks.
fn tool_result_content(content: Option<&Value>) -> (String, u32) {
    match content {
        Some(Value::String(text)) => (text.clone(), 0),
        Some(Value::Array(items)) => {
            let mut text = String::new();
            let mut images = 0;
            for item in items {
                match str_of(item, "type") {
                    Some("text") => {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(str_of(item, "text").unwrap_or_default());
                    }
                    Some("image") => images += 1,
                    _ => {}
                }
            }
            (text, images)
        }
        _ => (String::new(), 0),
    }
}

fn turn_ended(value: &Value) -> EventBody {
    let is_error = value.get("is_error").and_then(Value::as_bool).unwrap_or(false);
    let terminal_reason = str_of(value, "terminal_reason").unwrap_or_default();
    let outcome = if !is_error {
        TurnOutcome::Success
    } else if terminal_reason.starts_with("aborted") {
        TurnOutcome::Cancelled
    } else {
        TurnOutcome::Error
    };
    let errors = value
        .get("errors")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(|e| e.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    EventBody::TurnEnded {
        outcome,
        result: string_of(value, "result"),
        errors,
        duration_ms: value.get("duration_ms").and_then(Value::as_u64),
        cost_usd: value.get("total_cost_usd").and_then(Value::as_f64),
        num_turns: value.get("num_turns").and_then(Value::as_u64),
    }
}

fn control_request(value: &Value) -> Parsed {
    let request = value.get("request").unwrap_or(&Value::Null);
    let request_id = string_of(value, "request_id").unwrap_or_default();
    let subtype = str_of(request, "subtype").unwrap_or("unknown");
    if subtype != "can_use_tool" {
        return Parsed::UnsupportedControl { request_id, subtype: subtype.to_string() };
    }
    Parsed::Event(EventBody::ApprovalRequested {
        approval_id: request_id,
        tool_name: string_of(request, "tool_name").unwrap_or_default(),
        tool_use_id: string_of(request, "tool_use_id"),
        input: request.get("input").cloned().unwrap_or(Value::Null),
        description: string_of(request, "description"),
        suggestions: request.get("permission_suggestions").cloned().unwrap_or(Value::Null),
    })
}

fn stream_event(value: &Value) -> Parsed {
    let event = value.get("event").unwrap_or(&Value::Null);
    let message_id =
        string_of(value, "api_message_id").or_else(|| event.get("message").and_then(|m| string_of(m, "id")));
    let Some(message_id) = message_id else {
        return Parsed::Drop;
    };
    let index = event.get("index").and_then(Value::as_u64).map(|i| i as u32);
    let change = match (str_of(event, "type"), index) {
        (Some("content_block_start"), Some(index)) => {
            let block = event.get("content_block").unwrap_or(&Value::Null);
            let kind = match str_of(block, "type") {
                Some("text") => BlockKind::Text,
                Some("thinking") => BlockKind::Thinking,
                Some("tool_use") => BlockKind::ToolUse,
                _ => BlockKind::Unknown,
            };
            DeltaChange::BlockStart { index, kind, tool_name: string_of(block, "name") }
        }
        (Some("content_block_delta"), Some(index)) => {
            let delta = event.get("delta").unwrap_or(&Value::Null);
            match str_of(delta, "type") {
                Some("text_delta") => DeltaChange::Text { index, text: string_of(delta, "text").unwrap_or_default() },
                Some("thinking_delta") => {
                    DeltaChange::Thinking { index, text: string_of(delta, "thinking").unwrap_or_default() }
                }
                Some("input_json_delta") => {
                    DeltaChange::ToolInput { index, json: string_of(delta, "partial_json").unwrap_or_default() }
                }
                _ => return Parsed::Drop,
            }
        }
        (Some("message_stop"), _) => DeltaChange::MessageStop,
        // message_start, content_block_stop and message_delta add nothing to the tail.
        _ => return Parsed::Drop,
    };
    Parsed::Delta(StreamDelta { message_id, parent_tool_use_id: string_of(value, "parent_tool_use_id"), change })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn output_that_is_not_json_is_kept_rather_than_failing() {
        assert_eq!(parse_line("Warning: something"), Parsed::Event(EventBody::AgentOther { kind: "unparsed".into() }));
    }

    #[test]
    fn a_lone_surrogate_does_not_hide_an_approval() {
        // JSON.stringify writes half an emoji this way, and serde_json refuses it.
        let line = r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"echo \ud83d"}}}"#;
        let Parsed::Event(EventBody::ApprovalRequested { approval_id, input, .. }) = parse_line(line) else {
            panic!("{:?}", parse_line(line));
        };
        assert_eq!(approval_id, "r1");
        assert_eq!(input["command"], "echo \u{fffd}");
    }

    #[test]
    fn surrogate_repair_only_touches_what_is_broken() {
        let text = |line: &str| match parse_line(line) {
            Parsed::Event(EventBody::Assistant { blocks, .. }) => blocks,
            other => panic!("{other:?}"),
        };
        let wrap = |inner: &str| {
            format!(r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{inner}"}}]}}}}"#)
        };
        let block = |s: &str| vec![ContentBlock::Text { text: s.to_string() }];
        // A whole pair is an ordinary character.
        assert_eq!(text(&wrap(r"\ud83d\ude00 ok")), block("\u{1f600} ok"));
        // A low surrogate with no high one, and a high one followed by something else.
        assert_eq!(text(&wrap(r"a\ude00b")), block("a\u{fffd}b"));
        assert_eq!(text(&wrap(r"a\ud83d\u0041b")), block("a\u{fffd}Ab"));
        assert_eq!(text(&wrap(r"\ud83d\ud83d\ude00")), block("\u{fffd}\u{1f600}"));
        // An escaped backslash means this is literal text, not an escape.
        assert_eq!(text(&wrap(r"\\ud83d \ud83d")), block("\\ud83d \u{fffd}"));
    }

    #[test]
    fn unknown_types_and_subtypes_are_kept() {
        assert_eq!(
            parse_line(r#"{"type":"brand_new"}"#),
            Parsed::Event(EventBody::AgentOther { kind: "brand_new".into() })
        );
        assert_eq!(
            parse_line(r#"{"type":"system","subtype":"brand_new"}"#),
            Parsed::Event(EventBody::AgentOther { kind: "system/brand_new".into() })
        );
    }

    #[test]
    fn an_interrupted_turn_reads_as_cancelled_and_other_errors_as_errors() {
        let cancelled = json!({"type":"result","is_error":true,"terminal_reason":"aborted_tools"}).to_string();
        let Parsed::Event(EventBody::TurnEnded { outcome, .. }) = parse_line(&cancelled) else { panic!() };
        assert_eq!(outcome, TurnOutcome::Cancelled);
        let failed = json!({"type":"result","is_error":true,"errors":["boom"]}).to_string();
        let Parsed::Event(EventBody::TurnEnded { outcome, errors, .. }) = parse_line(&failed) else { panic!() };
        assert_eq!((outcome, errors), (TurnOutcome::Error, vec!["boom".to_string()]));
    }

    #[test]
    fn control_requests_other_than_approval_ask_for_an_error_reply() {
        let line =
            json!({"type":"control_request","request_id":"r1","request":{"subtype":"hook_callback"}}).to_string();
        assert_eq!(
            parse_line(&line),
            Parsed::UnsupportedControl { request_id: "r1".into(), subtype: "hook_callback".into() }
        );
    }

    #[test]
    fn tool_results_flatten_text_and_count_images() {
        let line = json!({"type":"user","message":{"role":"user","content":[{
            "type":"tool_result","tool_use_id":"t1","content":[
                {"type":"text","text":"a"},
                {"type":"image","source":{"data":"…"}},
                {"type":"text","text":"b"}
            ]
        }]}})
        .to_string();
        let Parsed::Event(EventBody::AgentUser { blocks, .. }) = parse_line(&line) else { panic!() };
        assert_eq!(
            blocks,
            vec![ContentBlock::ToolResult {
                tool_use_id: "t1".into(),
                is_error: false,
                text: "a\nb".into(),
                image_count: 1
            }]
        );
    }
}
