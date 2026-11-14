use protocol::capability::calls::Call;
use protocol::event::{AgentRunState, ContentBlock, EventBody};
use serde_json::json;

use super::fixtures::{activity, bodies, command, message, play, turn};

#[tokio::test]
async fn followup_turns_keep_old_output_and_completion_with_the_old_invocation() {
    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        activity("thread", "root-turn", "child", "spawn", "started", true),
        turn("child", "first", "inProgress"),
        message("child", "first", "First report"),
        turn("child", "first", "completed"),
        turn("child", "second", "inProgress"),
        turn("child", "second", "inProgress"),
        message("child", "first", "Late old message"),
        turn("child", "first", "completed"),
        activity("thread", "root-turn", "child", "old-completion", "completed", true),
        activity("thread", "root-turn", "child", "message", "interacted", true),
        message("child", "second", "Second report"),
        turn("child", "second", "completed"),
        turn("thread", "root-turn", "completed"),
    ])
    .await;
    let events = bodies(&output);
    let starts: Vec<_> = events
        .iter()
        .filter_map(|body| if let EventBody::SubagentStarted { call, .. } = body { Some(call.as_str()) } else { None })
        .collect();
    assert_eq!(starts.len(), 2);
    assert_ne!(starts[0], starts[1]);
    assert!(events.iter().any(|body|matches!(body,EventBody::Assistant{blocks,..} if blocks.iter().any(|b|matches!(b,ContentBlock::Call{call:Call::Subagent(details),..} if details.resumes.as_ref().is_some_and(|id|id.as_str()=="spawn"))))));
    assert!(events.iter().any(|body|matches!(body,EventBody::Assistant{parent_tool_use_id:Some(parent),blocks,..} if parent.as_str()=="spawn" && blocks.iter().any(|b|matches!(b,ContentBlock::Text{text,..} if text=="Late old message")))));
    let ends: Vec<_> = events
        .iter()
        .filter_map(|body| {
            if let EventBody::SubagentEnded { call, report, .. } = body {
                Some((call.as_str(), report.as_deref()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(ends, [(starts[0], Some("First report")), (starts[1], Some("Second report"))]);
}

#[tokio::test]
async fn root_completion_preserves_child_calls_and_approvals_until_the_child_finishes() {
    let output=play(vec![
        turn("thread","root-turn","inProgress"),
        activity("thread","root-turn","child","spawn","started",true),
        turn("child","child-turn","inProgress"),
        command("child","child-turn","child-command",false),
        json!({"id":90,"method":"item/commandExecution/requestApproval","params":{"threadId":"child","turnId":"child-turn","itemId":"child-command"}}),
        turn("thread","root-turn","completed"),
        command("child","child-turn","child-command",true),
        message("child","child-turn","Child report"),
        turn("child","child-turn","completed"),
    ]).await;
    let events = bodies(&output);
    let requested = events
        .iter()
        .position(
            |body| matches!(body,EventBody::ApprovalRequested{asked_by:Some(parent),..} if parent.as_str()=="spawn"),
        )
        .unwrap();
    let root_end = events.iter().position(|body| matches!(body, EventBody::TurnEnded { .. })).unwrap();
    let resolved = events.iter().position(|body| matches!(body, EventBody::ApprovalResolved { .. })).unwrap();
    assert!(requested < root_end && root_end < resolved);
    assert!(matches!(events[root_end + 1], EventBody::AgentState { state: AgentRunState::RequiresAction }));
    assert!(events.iter().any(|body|matches!(body,EventBody::AgentUser{parent_tool_use_id:Some(parent),blocks} if parent.as_str()=="spawn" && blocks.iter().any(|b|matches!(b,ContentBlock::CallResult{tool_use_id,..} if tool_use_id.as_str()=="child-command")))));
    let snapshots: Vec<_> = events
        .iter()
        .filter_map(|body| if let EventBody::AgentTasks { tasks, .. } = body { Some(tasks.len()) } else { None })
        .collect();
    assert_eq!(snapshots, [1, 0]);
    assert!(matches!(events.last(), Some(EventBody::AgentState { state: AgentRunState::Idle })));
}

#[tokio::test]
async fn collaboration_waits_have_typed_calls_and_results() {
    let wait = json!({"type":"collabAgentToolCall","id":"wait","tool":"wait","status":"inProgress",
        "receiverThreadIds":["child"],"agentsStates":{},"prompt":null});
    let mut done = wait.clone();
    done["status"] = json!("completed");
    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        activity("thread", "root-turn", "child", "spawn", "started", true),
        super::fixtures::item("thread", "root-turn", wait, false),
        super::fixtures::item("thread", "root-turn", done, true),
    ])
    .await;
    let events = bodies(&output);
    assert!(events.iter().any(|body|matches!(body,EventBody::Assistant{blocks,..} if blocks.iter().any(|b|matches!(b,ContentBlock::Call{call:Call::WaitForSubagents(wait),..} if wait.subagents==["spawn".into()])))));
    assert!(events.iter().any(|body|matches!(body,EventBody::AgentUser{blocks,..} if blocks.iter().any(|b|matches!(b,ContentBlock::CallResult{tool_use_id,is_error:false,..} if tool_use_id.as_str()=="wait")))));
}
