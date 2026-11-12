//! What has to stay readable of a tool call and its result, which the log holds for good.

use protocol::event::ContentBlock;

/// Spelled out, since the log holds this form for good. A call's ID is a plain string, as its result's is.
#[test]
fn a_call_and_its_result_keep_their_form() {
    let call: &[u8] = br#"{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls"}}"#;
    let result: &[u8] = br#"{"type":"tool_result","tool_use_id":"toolu_1","is_error":false,"text":"a.rs"}"#;
    let read = |block: &[u8]| -> ContentBlock { serde_json::from_slice(block).unwrap() };

    let (ContentBlock::ToolUse { id, .. }, ContentBlock::ToolResult { tool_use_id, .. }) = (read(call), read(result))
    else {
        panic!("a call and a result read as themselves");
    };

    assert_eq!(id, tool_use_id, "a result names its call by the call's own ID");
    for block in [call, result] {
        assert_eq!(serde_json::to_vec(&read(block)).unwrap(), block, "it's written the way it's read");
    }
}
