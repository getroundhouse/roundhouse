use protocol::capability::calls::Call;
use protocol::event::{AgentRunState, ContentBlock, EventBody};
use serde_json::json;

use super::fixtures::activity;
use crate::codex::tests::script::{bodies, command, item, message, play, turn};

/// A child's thread is one subagent however many turns it takes, so a follow-up starts it again, under the call that
/// first started it, and its late output stays with it. Only the turn it's taking ends it, and only its first end
/// answers its call.
#[tokio::test]
async fn a_followup_turn_starts_the_same_subagent_again_and_only_the_turn_it_takes_ends_it() {
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
        .filter_map(|body| {
            let EventBody::SubagentStarted { call, .. } = body else { return None };
            Some((call.as_str(), body.subagent_started()?))
        })
        .collect();
    assert_eq!(starts, [("spawn", "child".into()), ("spawn", "child".into())], "started, and again for the follow-up");
    let calls = |body: &EventBody| {
        let EventBody::Assistant { blocks, .. } = body else { return 0 };
        blocks.iter().filter(|b| matches!(b, ContentBlock::Call { call: Call::Subagent(_), .. })).count()
    };
    assert_eq!(events.iter().copied().map(calls).sum::<usize>(), 1, "one call starts it");
    assert!(events.iter().any(|body|matches!(body,EventBody::Assistant{subagent:Some(parent),blocks,..} if parent.as_str()=="child" && blocks.iter().any(|b|matches!(b,ContentBlock::Text{text,..} if text=="Late old message")))));
    let ends: Vec<_> = events
        .iter()
        .filter_map(|body| {
            if let EventBody::SubagentEnded { subagent, report, .. } = body {
                Some((subagent.as_str(), report.as_deref()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(ends, [("child", Some("First report")), ("child", Some("Second report"))], "not by the late end");
    let answers = |body: &EventBody| {
        matches!(body, EventBody::AgentUser { blocks, .. }
            if blocks.iter().any(|b| matches!(b, ContentBlock::CallResult { tool_use_id, .. } if tool_use_id.as_str() == "spawn")))
    };
    assert_eq!(events.iter().filter(|body| answers(body)).count(), 1, "its first end answers its call");
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
            |body| matches!(body,EventBody::ApprovalRequested{asked_by:Some(parent),..} if parent.as_str()=="child"),
        )
        .unwrap();
    let root_end = events.iter().position(|body| matches!(body, EventBody::TurnEnded { .. })).unwrap();
    let resolved = events.iter().position(|body| matches!(body, EventBody::ApprovalResolved { .. })).unwrap();
    assert!(requested < root_end && root_end < resolved);
    assert!(matches!(events[root_end + 1], EventBody::AgentState { state: AgentRunState::RequiresAction }));
    assert!(events.iter().any(|body|matches!(body,EventBody::AgentUser{subagent:Some(parent),blocks} if parent.as_str()=="child" && blocks.iter().any(|b|matches!(b,ContentBlock::CallResult{tool_use_id,..} if tool_use_id.as_str()=="child-command")))));
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
        item("thread", "root-turn", wait, false),
        item("thread", "root-turn", done, true),
    ])
    .await;
    let events = bodies(&output);
    assert!(events.iter().any(|body|matches!(body,EventBody::Assistant{blocks,..} if blocks.iter().any(|b|matches!(b,ContentBlock::Call{call:Call::WaitForSubagents(wait),..} if wait.subagents==["child".into()])))));
    assert!(events.iter().any(|body|matches!(body,EventBody::AgentUser{blocks,..} if blocks.iter().any(|b|matches!(b,ContentBlock::CallResult{tool_use_id,is_error:false,..} if tool_use_id.as_str()=="wait")))));
}

/// A collaboration call names each subagent it's for by its thread, so one the adapter didn't hear start, such as
/// before it started again, is named too. One for the root's own thread isn't for a subagent.
#[tokio::test]
async fn a_collaboration_call_names_its_subagents_by_their_threads_but_not_the_root() {
    let collaboration = |id: &str, tool: &str, thread: &str| {
        let call = json!({"type":"collabAgentToolCall","id":id,"tool":tool,"status":"inProgress",
            "receiverThreadIds":[thread],"agentsStates":{},"prompt":"Carry on."});
        item("thread", "root-turn", call, false)
    };
    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        collaboration("wait", "wait", "unheard"),
        collaboration("send", "sendInput", "unheard"),
        collaboration("to-root", "sendInput", "thread"),
    ])
    .await;
    let calls: Vec<_> = bodies(&output)
        .into_iter()
        .filter_map(|body| {
            let EventBody::Assistant { blocks, .. } = body else { return None };
            blocks.iter().find_map(|b| if let ContentBlock::Call { call, .. } = b { Some(call.clone()) } else { None })
        })
        .collect();

    assert!(matches!(&calls[0], Call::WaitForSubagents(wait) if wait.subagents == ["unheard".into()]), "{calls:?}");
    assert!(matches!(&calls[1], Call::MessageSubagent(message) if message.subagent.as_str() == "unheard"));
    assert!(matches!(&calls[2], Call::Other { .. }), "the root's own thread");
}
