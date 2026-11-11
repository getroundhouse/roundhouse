//! What the CLI's tools sent back, as the adapter records it, from the same captured output as `contract`.

use protocol::event::{ContentBlock, EventBody};

use super::contract::events;

#[test]
fn a_command_that_printed_nothing_has_an_empty_result() {
    let blocks: Vec<ContentBlock> = events()
        .into_iter()
        .filter_map(|e| match e {
            EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => Some(blocks),
            _ => None,
        })
        .flatten()
        .collect();
    let commands: Vec<&str> = blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, name, .. } if name == "Bash" => Some(id.as_str()),
            _ => None,
        })
        .collect();
    // Each of the fixture's commands writes to a file, and the CLI says it printed nothing in a sentence of its own.
    let worked: Vec<&str> = blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolResult { tool_use_id, is_error: false, text, .. }
                if commands.contains(&tool_use_id.as_str()) =>
            {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect();

    assert!(!worked.is_empty(), "the fixture runs commands that work");
    assert!(worked.iter().all(|text| text.is_empty()), "{worked:?}");
}
