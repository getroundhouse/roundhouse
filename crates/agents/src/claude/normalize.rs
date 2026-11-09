//! Turns one line of Claude's stdout into a normalized event.
//!
//! This has to survive the CLI changing underneath it, so it reads loosely typed JSON and never
//! fails. Anything it doesn't recognize still reaches the log as `AgentOther` with the raw line.

use std::collections::HashMap;

use protocol::event::{AgentRunState, ApprovalOutcome, ContentBlock, EventBody, TurnOutcome};
use protocol::model::MessageId;
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
use serde_json::Value;

use tasks::StoppedTasks;

mod file_change;
mod tasks;
#[cfg(test)]
mod tests;

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
    /// The CLI's answer to a control request of ours.
    ControlReply { request_id: String, error: Option<String> },
}

/// Parses lines in the order they came, and remembers which block of each message is streaming. Claude
/// sends a block's deltas, then the finished block, so the finished block can say where it sits in its
/// message, as its deltas did. It also says it stopped a background task before the list that leaves it
/// out, so the list can say which left without finishing.
#[derive(Debug, Default)]
pub struct Normalizer {
    streaming: HashMap<MessageId, u32>,
    stopped_tasks: StoppedTasks,
}

impl Normalizer {
    pub fn line(&mut self, line: &str) -> Parsed {
        let Some(value) = value_of(line) else { return unparsed() };
        self.stopped_tasks.note(&value);
        let mut parsed = parse_value(&value);
        if let Parsed::Event(EventBody::AgentTasks { tasks, stopped }) = &mut parsed {
            *stopped = self.stopped_tasks.gone_from(tasks);
        }
        if let Parsed::Delta(delta) = &parsed {
            match delta.change {
                DeltaChange::BlockStart { index, .. }
                | DeltaChange::Text { index, .. }
                | DeltaChange::Thinking { index, .. }
                | DeltaChange::ToolInput { index, .. } => {
                    self.streaming.insert(delta.message_id.clone(), index);
                }
                DeltaChange::MessageStop => {
                    self.streaming.remove(&delta.message_id);
                }
                DeltaChange::Unknown => {}
            }
        }
        if let Parsed::Event(EventBody::Assistant { message_id: Some(id), blocks, first_index, .. }) = &mut parsed {
            // The block that streamed is this one, so no later block can take its place. Claude sends one block
            // per event, and if several come at once, which of them streamed can't be told.
            let streamed = self.streaming.remove(id);
            *first_index = streamed.filter(|_| blocks.len() == 1);
        }
        // Whatever a turn left streaming, such as when it was cancelled, is over.
        if let Parsed::Event(EventBody::TurnEnded { .. }) = &parsed {
            self.streaming.clear();
        }
        parsed
    }
}

/// One line on its own, for tests of what needs nothing before it. A [`Normalizer`] also says where a finished
/// block sits in its message, and which background tasks were stopped.
#[cfg(test)]
fn parse_line(line: &str) -> Parsed {
    value_of(line).map_or_else(unparsed, |value| parse_value(&value))
}

fn value_of(line: &str) -> Option<Value> {
    serde_json::from_str::<Value>(line)
        // JavaScript strings can hold half of a surrogate pair, and JSON.stringify writes it as an
        // escape. That's rare, and it can land in a line that matters, such as an approval.
        .or_else(|_| serde_json::from_str::<Value>(&repair_lone_surrogates(line)))
        .ok()
}

fn unparsed() -> Parsed {
    Parsed::Event(EventBody::AgentOther { kind: "unparsed".into() })
}

fn parse_value(value: &Value) -> Parsed {
    let kind = str_of(value, "type").unwrap_or("unknown");
    match kind {
        "assistant" => Parsed::Event(assistant(value)),
        "user" => Parsed::Event(user(value)),
        "result" => Parsed::Event(turn_ended(value)),
        "system" => system(value),
        "stream_event" => stream_event(value),
        "control_request" => control_request(value),
        "control_cancel_request" => match str_of(value, "request_id") {
            Some(id) => Parsed::Event(EventBody::ApprovalResolved {
                approval_id: id.into(),
                outcome: ApprovalOutcome::Cancelled,
            }),
            None => other(kind),
        },
        // Replies to the adapter's own requests, such as an interrupt.
        "control_response" => control_reply(value),
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

fn id_of<T: From<String>>(value: &Value, key: &str) -> Option<T> {
    string_of(value, key).map(T::from)
}

fn string_of(value: &Value, key: &str) -> Option<String> {
    str_of(value, key).map(str::to_string)
}

/// The name to launch with. The running CLI calls this mode `default`, and `--permission-mode` only
/// takes `manual`.
fn launch_name(mode: &str) -> &str {
    if mode == "default" { "manual" } else { mode }
}

fn control_reply(value: &Value) -> Parsed {
    let response = value.get("response").unwrap_or(&Value::Null);
    match str_of(response, "request_id") {
        Some(request_id) => Parsed::ControlReply {
            request_id: request_id.to_string(),
            error: (str_of(response, "subtype") == Some("error"))
                .then(|| str_of(response, "error").unwrap_or("the agent refused").to_string()),
        },
        None => Parsed::Drop,
    }
}

fn system(value: &Value) -> Parsed {
    let subtype = str_of(value, "subtype").unwrap_or("unknown");
    match subtype {
        "init" => Parsed::Event(EventBody::AgentInit {
            agent_session_id: id_of(value, "session_id"),
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
        "background_tasks_changed" => tasks::listed(value),
        "status" => match str_of(value, "permissionMode") {
            Some(mode) => Parsed::Event(EventBody::AgentMode { permission_mode: launch_name(mode).to_string() }),
            None => other("system/status"),
        },
        // A running token count for a spinner. Several arrive per second while thinking.
        "thinking_tokens" => Parsed::Drop,
        other_subtype => other(&format!("system/{other_subtype}")),
    }
}

fn assistant(value: &Value) -> EventBody {
    let message = value.get("message").unwrap_or(&Value::Null);
    EventBody::Assistant {
        message_id: id_of(message, "id"),
        parent_tool_use_id: id_of(value, "parent_tool_use_id"),
        model: string_of(message, "model"),
        blocks: blocks(message.get("content")),
        // A Normalizer knows it from the deltas before.
        first_index: None,
    }
}

fn user(value: &Value) -> EventBody {
    let message = value.get("message").unwrap_or(&Value::Null);
    let mut blocks = blocks(message.get("content"));
    // Claude says what an edit changed beside the line's one result, rather than in it.
    if let [ContentBlock::ToolResult { change, .. }] = blocks.as_mut_slice() {
        *change = file_change::read(value);
    }
    EventBody::AgentUser { parent_tool_use_id: id_of(value, "parent_tool_use_id"), blocks }
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
                tool_use_id: string_of(value, "tool_use_id").unwrap_or_default().into(),
                is_error: value.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                text,
                image_count,
                change: None,
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
        approval_id: request_id.into(),
        tool_name: string_of(request, "tool_name").unwrap_or_default(),
        tool_use_id: id_of(request, "tool_use_id"),
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
    let index = event.get("index").and_then(Value::as_u64).and_then(|i| u32::try_from(i).ok());
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
    Parsed::Delta(StreamDelta {
        // The hub knows the run, and sets it.
        run: None,
        message_id: message_id.into(),
        parent_tool_use_id: id_of(value, "parent_tool_use_id"),
        change,
    })
}
