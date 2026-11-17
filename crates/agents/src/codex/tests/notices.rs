//! What Codex says for the person to read, as its adapter records it.

use protocol::event::{EventBody, NoticeLevel};
use serde_json::json;

use super::script::{bodies, play, turn};

#[tokio::test]
async fn a_warning_and_a_failed_mcp_server_are_notices_and_its_other_states_are_not_kept() {
    let mcp = |name: &str, status: &str, more: serde_json::Value| {
        let mut params = json!({"name":name,"status":status,"threadId":null});
        params.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        json!({"method":"mcpServer/startupStatus/updated","params":params})
    };
    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        json!({"method":"warning","params":{"message":"Your config has an unknown key.","threadId":"thread"}}),
        mcp("docs", "starting", json!({})),
        mcp("docs", "ready", json!({})),
        mcp("tickets", "failed", json!({"error":"connection refused"})),
        mcp("calendar", "failed", json!({"failureReason":"reauthenticationRequired"})),
        turn("thread", "root-turn", "completed"),
    ])
    .await;

    let notices: Vec<_> = bodies(&output)
        .into_iter()
        .filter_map(|body| {
            let EventBody::Notice { level, message, subagent: None } = body else { return None };
            Some((*level, message.as_str()))
        })
        .collect();
    assert_eq!(
        notices,
        [
            (NoticeLevel::Warning, "Your config has an unknown key."),
            (
                NoticeLevel::Warning,
                "The MCP server `tickets` didn't start, so its tools aren't available: connection refused"
            ),
            (
                NoticeLevel::Warning,
                "The MCP server `calendar` needs you to sign in to it again, so its tools aren't available."
            ),
        ]
    );
    let others = bodies(&output)
        .into_iter()
        .filter(|body| matches!(body, EventBody::AgentOther { kind, .. } if kind != "test/request"))
        .count();
    assert_eq!(others, 0, "a server starting or ready leaves nothing");
}
