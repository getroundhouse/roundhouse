//! What the advisor sent back. The advisor is a stronger model that reads the whole conversation and advises the agent.
//! The API runs it rather than the CLI, so its call is a `server_tool_use`, and its result comes in a later
//! `assistant` line of the same message rather than in a `user` line.

use protocol::event::ContentBlock;
use serde_json::Value;

use super::{str_of, string_of};

/// The stop reason of a reply the advisor refused, which the CLI counts as a failed call. It's also what the call says
/// went wrong, when the advisor gave no words of its own.
const REFUSAL: &str = "refusal";

/// The advisor's result, as any tool's: its advice, why asking failed, or, when the advice is encrypted for the model,
/// none of it.
pub(super) fn result(value: &Value) -> ContentBlock {
    let content = value.get("content").unwrap_or(&Value::Null);
    let text = string_of(content, "text").filter(|text| !text.is_empty());
    let (is_error, text, encrypted) = match str_of(content, "type") {
        Some("advisor_tool_result_error") => (true, string_of(content, "error_code").unwrap_or_default(), false),
        _ if str_of(content, "stop_reason") == Some(REFUSAL) => {
            (true, text.unwrap_or_else(|| REFUSAL.to_string()), false)
        }
        Some("advisor_redacted_result") => (false, String::new(), true),
        // An `advisor_result` has the advice in `text`. A kind this build doesn't know shows any text it has, and isn't
        // taken to be encrypted.
        _ => (false, text.unwrap_or_default(), false),
    };
    ContentBlock::ToolResult {
        tool_use_id: string_of(value, "tool_use_id").unwrap_or_default().into(),
        is_error,
        text,
        images: Vec::new(),
        change: None,
        encrypted,
        call: None,
        exit_code: None,
    }
}
