//! What a message holds, read from the agent's own blocks.

use protocol::event::ContentBlock;
use serde_json::Value;

use super::{advisor, result_text, str_of, string_of};

/// The blocks of a message, from its `content`, which is a string or a list of blocks.
pub(super) fn read(content: Option<&Value>) -> Vec<ContentBlock> {
    match content {
        Some(Value::String(text)) => vec![ContentBlock::Text { text: text.clone(), images: Vec::new() }],
        Some(Value::Array(items)) => items.iter().map(block).collect(),
        _ => Vec::new(),
    }
}

fn block(value: &Value) -> ContentBlock {
    match str_of(value, "type") {
        Some("text") => ContentBlock::Text { text: string_of(value, "text").unwrap_or_default(), images: Vec::new() },
        Some("thinking") => ContentBlock::Thinking { text: string_of(value, "thinking").unwrap_or_default() },
        // A tool the API runs itself, such as the advisor, is called the same way.
        Some("tool_use" | "server_tool_use") => ContentBlock::ToolUse {
            id: string_of(value, "id").unwrap_or_default().into(),
            name: string_of(value, "name").unwrap_or_default(),
            input: value.get("input").cloned().unwrap_or(Value::Null),
            call: None,
        },
        Some("tool_result") => ContentBlock::ToolResult {
            tool_use_id: string_of(value, "tool_use_id").unwrap_or_default().into(),
            is_error: value.get("is_error").and_then(Value::as_bool).unwrap_or(false),
            text: result_text::read(value.get("content")),
            // Only a user line's results have images, and `images` takes them from it.
            images: Vec::new(),
            change: None,
            encrypted: false,
            call: None,
            exit_code: None,
            denied: None,
        },
        Some("advisor_tool_result") => advisor::result(value),
        Some("image") => {
            ContentBlock::Image { media_type: value.get("source").and_then(|s| string_of(s, "media_type")) }
        }
        _ => ContentBlock::Unknown,
    }
}
