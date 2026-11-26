//! What Codex says for the person to read, as its adapter records it.

use protocol::event::{EventBody, NoticeLevel};
use serde_json::json;

use super::script::{bodies, others, play, turn};
use crate::AgentOutput;

/// Codex says why a server failed with what can be secret, such as its URL's query, so none of it is kept, whichever
/// thread it's of: the agent's own, none, or one the adapter doesn't know.
#[tokio::test]
async fn a_warning_is_a_notice_and_nothing_codex_says_of_an_mcp_server_starting_is_recorded() {
    let mcp = |name: &str, status: &str, thread: Option<&str>| {
        let error = (status == "failed").then_some("sending to https://mcp.example.com/?key=secret");
        json!({"method":"mcpServer/startupStatus/updated",
            "params":{"name":name,"status":status,"threadId":thread,"error":error}})
    };
    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        json!({"method":"warning","params":{"message":"Your config has an unknown key.","threadId":"thread"}}),
        mcp("docs", "starting", Some("thread")),
        mcp("docs", "ready", Some("thread")),
        mcp("tickets", "failed", Some("thread")),
        mcp("calendar", "failed", None),
        mcp("mail", "failed", Some("another")),
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
    assert_eq!(notices, [(NoticeLevel::Warning, "Your config has an unknown key.")]);
    assert_eq!(others(&output), Vec::<&str>::new());
    for said in &output {
        if let AgentOutput::Event { body, raw, .. } = said {
            assert!(raw.as_deref().is_none_or(|raw| !raw.contains("secret")), "{body:?}");
        }
    }
}
