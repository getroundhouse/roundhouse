//! A `user` line: what the agent's tool calls sent back, or something written for the model.

use protocol::event::{ContentBlock, EventBody};
use serde_json::Value;

use super::{Parsed, blocks, file_change, id_of, other};

pub(super) fn line(value: &Value) -> Parsed {
    let content = value.get("message").and_then(|message| message.get("content"));
    let mut blocks = blocks(content);
    // Written for the model rather than the person, such as an image's size once the CLI has shrunk it, or the summary
    // a compacted conversation carries on from. A result is shown even so, or its call would never get one.
    let for_the_model = value.get("isSynthetic").and_then(Value::as_bool) == Some(true);
    if for_the_model && !blocks.iter().any(|block| matches!(block, ContentBlock::ToolResult { .. })) {
        return other("user/synthetic");
    }
    // Claude says what an edit changed beside the line's one result, rather than in it.
    if let [ContentBlock::ToolResult { change, .. }] = blocks.as_mut_slice() {
        *change = file_change::read(value);
    }
    Parsed::Event(EventBody::AgentUser { parent_tool_use_id: id_of(value, "parent_tool_use_id"), blocks })
}
