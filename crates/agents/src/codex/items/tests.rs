//! How Codex's items read as typed calls and what they sent back.

use protocol::capability::calls::CallOutput;
use protocol::capability::calls::output::McpContent;
use serde_json::json;

use super::translate::mcp_result;

/// As an MCP server can send it, which reads back as none, so it's kept as none.
#[test]
fn a_null_structured_result_is_none() {
    let sent = mcp_result(&json!({"content": [{"type": "text", "text": "ok"}], "structuredContent": null}));
    let given = mcp_result(&json!({"content": [], "structuredContent": {"rows": 2}}));

    let ok = vec![McpContent::Text { text: "ok".into() }];
    assert_eq!(sent, CallOutput::Mcp { content: ok, structured: None });
    assert_eq!(given, CallOutput::Mcp { content: Vec::new(), structured: Some(json!({"rows": 2})) });
}
