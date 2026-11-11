//! A `user` line: what the agent's tool calls sent back, with any images among it, or something written for the model.

use protocol::event::{ContentBlock, EventBody};
use serde_json::Value;

use super::{Parsed, blocks, file_change, id_of, images};

pub(super) fn line(value: &Value) -> Parsed {
    let content = value.get("message").and_then(|message| message.get("content"));
    let mut blocks = blocks(content);
    // Written for the model rather than the person, such as an image's size once the CLI has shrunk it. A compaction's
    // summary is one too, but `compaction` reads it first. A result is shown even so, or its call would never get one.
    let for_the_model = value.get("isSynthetic").and_then(Value::as_bool) == Some(true);
    if for_the_model && !blocks.iter().any(|block| matches!(block, ContentBlock::ToolResult { .. })) {
        // A subagent's names the call that started it, as every `user` line from one does.
        let parent_tool_use_id = id_of(value, "parent_tool_use_id");
        return Parsed::Event(EventBody::AgentOther { kind: "user/synthetic".into(), parent_tool_use_id });
    }
    // Claude says what an edit changed beside the line's one result, rather than in it.
    if let [ContentBlock::ToolResult { change, text, .. }] = blocks.as_mut_slice() {
        *change = file_change::read(value);
        if printed_nothing(value, text) {
            text.clear();
        }
    }
    let taken = images::take(content, &mut blocks);
    let body = EventBody::AgentUser { parent_tool_use_id: id_of(value, "parent_tool_use_id"), blocks };
    if taken.is_empty() {
        return Parsed::Event(body);
    }
    let raw = images::without(value, &taken);
    Parsed::EventWithImages { body, images: taken.into_iter().map(|found| found.kept).collect(), raw }
}

/// What the CLI sends as a command's result when the command printed nothing.
const NOTHING_PRINTED: &str = "(Bash completed with no output)";

/// Whether a command's result only says that it printed nothing, which a client says in its own words, as it would
/// for any command that printed nothing. Only the CLI's own sentence, with nothing on either stream beside it, is
/// taken for that, so a message that's about something else, such as a command left running in the background, is
/// kept.
fn printed_nothing(line: &Value, text: &str) -> bool {
    let Some(result) = line.get("tool_use_result") else { return false };
    let empty = |stream| result.get(stream).and_then(Value::as_str) == Some("");
    text == NOTHING_PRINTED && empty("stdout") && empty("stderr")
}
