//! What the CLI streams of a message as its model writes it.

use protocol::model::SubagentId;
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
use serde_json::Value;

use super::{Parsed, request, str_of, string_of};

/// What a `stream_event` line says, which is `subagent`'s if it's one's: a piece of what the agent is writing, or one
/// of its requests to its model.
pub(super) fn event(value: &Value, subagent: Option<SubagentId>) -> Parsed {
    let event = value.get("event").unwrap_or(&Value::Null);
    if let Some(request) = request::request(value, event) {
        return Parsed::Request(request);
    }
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
                Some("tool_use" | "server_tool_use") => BlockKind::ToolUse,
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
        // A subagent's message_start, content_block_stop and message_delta add nothing to the tail.
        _ => return Parsed::Drop,
    };
    Parsed::Delta(StreamDelta {
        // The hub knows the run, and sets it.
        run: None,
        message_id: message_id.into(),
        subagent,
        change,
    })
}
