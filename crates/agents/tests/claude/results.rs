//! What the CLI's tools sent back, as the adapter records it, from the same captured output as `contract`.

use protocol::event::ContentBlock;
use protocol::model::ToolUseId;

use super::contract::blocks;

#[test]
fn a_command_that_printed_nothing_has_an_empty_result() {
    // Each of the fixture's commands writes to a file, and the CLI says it printed nothing in a sentence of its own.
    let worked: Vec<String> = results_of("Bash")
        .into_iter()
        .filter_map(|result| match result {
            ContentBlock::ToolResult { is_error: false, text, .. } => Some(text),
            _ => None,
        })
        .collect();

    assert!(!worked.is_empty(), "the fixture runs commands that work");
    assert!(worked.iter().all(String::is_empty), "{worked:?}");
}

/// The fixture's agent loads `ExitPlanMode` with `ToolSearch`, whose result has no text, only a reference to the tool.
#[test]
fn a_tool_search_s_result_names_the_tools_it_loaded() {
    let found: Vec<String> = results_of("ToolSearch")
        .into_iter()
        .filter_map(|result| match result {
            ContentBlock::ToolResult { text, .. } => Some(text),
            _ => None,
        })
        .collect();

    assert_eq!(found, ["ExitPlanMode"]);
}

/// The result of each call to `tool` in the fixture.
fn results_of(tool: &str) -> Vec<ContentBlock> {
    let blocks = blocks();
    let calls: Vec<ToolUseId> = blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, name, .. } if name == tool => Some(id.clone()),
            _ => None,
        })
        .collect();
    blocks
        .into_iter()
        .filter(|block| matches!(block, ContentBlock::ToolResult { tool_use_id, .. } if calls.contains(tool_use_id)))
        .collect()
}
