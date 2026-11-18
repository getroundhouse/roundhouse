use protocol::event::{ContentBlock, EventBody, TaskOutcome, TurnOutcome};
use protocol::model::{Seq, SubagentId};
use serde_json::json;

use super::fixtures::activity;
use crate::AgentOutput;
use crate::codex::tests::script::{bodies, command, item, message, play, turn};

#[tokio::test]
async fn child_output_and_completion_do_not_take_the_roots_prompt_calls_or_reply() {
    let output = play(vec![
        turn("thread","root-turn","inProgress"),
        command("thread","root-turn","root-command",false),
        activity("thread","root-turn","child","spawn","started",false),
        activity("thread","root-turn","child","spawn","started",true),
        turn("child","child-turn","inProgress"),
        item("child","child-turn",json!({"type":"userMessage"}),true),
        command("child","child-turn","child-command",false),
        json!({"method":"item/agentMessage/delta","params":{"threadId":"child","turnId":"child-turn","itemId":"answer","delta":"Child"}}),
        command("child","child-turn","child-command",true),
        item("child","child-turn",json!({"type":"reasoning","id":"child-thought","summary":["Checking the work."]}),true),
        item("child","child-turn",json!({"type":"plan","id":"child-plan","text":"Child plan"}),true),
        message("child","child-turn","Child report"),
        turn("child","child-turn","completed"),
        activity("thread","root-turn","child","finished","completed",true),
        item("thread","root-turn",json!({"type":"userMessage"}),true),
        command("thread","root-turn","root-command",true),
        message("thread","root-turn","Root reply"),
        turn("thread","root-turn","completed"),
    ]).await;
    let events = bodies(&output);
    assert!(events.iter().any(|body| matches!(body, EventBody::Assistant { subagent:Some(parent), blocks,.. } if parent.as_str()=="child" && blocks.iter().any(|b| matches!(b,ContentBlock::Thinking{text} if text=="Checking the work.")))));
    assert_eq!(events.iter().filter(|body| matches!(body, EventBody::SubagentStarted { .. })).count(), 1);
    assert_eq!(
        events
            .iter()
            .filter_map(|body| if let EventBody::TurnEnded { outcome, result, .. } = body {
                Some((*outcome, result.as_deref()))
            } else {
                None
            })
            .collect::<Vec<_>>(),
        [(TurnOutcome::Success, Some("Root reply"))]
    );
    assert!(events.iter().any(|body| matches!(body,EventBody::SubagentEnded {subagent,outcome:TaskOutcome::Finished,report:Some(report),..} if subagent.as_str()=="child" && report=="Child report")));
    assert!(events.iter().any(|body| matches!(body,EventBody::AgentUser {subagent:None,blocks} if blocks.iter().any(|block| matches!(block,ContentBlock::CallResult {tool_use_id,..} if tool_use_id.as_str()=="root-command")))));
    assert!(
        !events.iter().any(|body| matches!(body, EventBody::ToolCallDropped { .. } | EventBody::PromptDropped { .. }))
    );
    let taken =
        events.iter().position(|body| matches!(body,EventBody::PromptTaken {prompt} if *prompt==Seq::FIRST)).unwrap();
    let ended = events.iter().position(|body| matches!(body, EventBody::SubagentEnded { .. })).unwrap();
    assert!(taken > ended, "the child must not consume the user's pending prompt");
    assert!(output.iter().any(|output| matches!(output,AgentOutput::Delta(delta) if delta.subagent.as_ref().is_some_and(|id|id.as_str()=="child"))));
    assert!(events.iter().any(|body| matches!(body,EventBody::Assistant {subagent:Some(parent),model:Some(model),..} if parent.as_str()=="child" && model=="gpt-6-luna")));
}

#[tokio::test]
async fn nested_agents_and_foreign_threads_cannot_change_the_root_turn() {
    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        activity("thread", "root-turn", "child", "spawn", "started", true),
        turn("child", "child-turn", "inProgress"),
        activity("child", "child-turn", "nested", "nested-spawn", "started", true),
        turn("nested", "nested-turn", "inProgress"),
        message("nested", "nested-turn", "Nested reply"),
        turn("nested", "nested-turn", "failed"),
        turn("foreign", "foreign-turn", "inProgress"),
        message("foreign", "foreign-turn", "Unassociated reply"),
        turn("foreign", "foreign-turn", "completed"),
        message("child", "child-turn", "Child reply"),
        turn("child", "child-turn", "completed"),
        message("thread", "root-turn", "Root reply"),
        turn("thread", "root-turn", "completed"),
    ])
    .await;
    let events = bodies(&output);
    assert!(events.iter().any(|body| matches!(body,EventBody::Assistant {subagent:Some(parent),blocks,..} if parent.as_str()=="child" && blocks.iter().any(|b| matches!(b,ContentBlock::Call{id,..} if id.as_str()=="nested-spawn")))));
    assert!(events.iter().any(|body| matches!(body,EventBody::Assistant {subagent:Some(parent),blocks,..} if parent.as_str()=="nested" && blocks.iter().any(|b| matches!(b,ContentBlock::Text{text,..} if text=="Nested reply")))));
    assert!(events.iter().any(|body| matches!(body,EventBody::SubagentEnded {subagent,outcome:TaskOutcome::Failed,..} if subagent.as_str()=="nested")));
    assert_eq!(events.iter().filter(|body| matches!(body, EventBody::TurnEnded { .. })).count(), 1);
    assert!(
        events.iter().any(|body| matches!(body,EventBody::TurnEnded {result:Some(result),..} if result=="Root reply"))
    );
    assert!(!events.iter().any(|body| matches!(body,EventBody::Assistant {blocks,..} if blocks.iter().any(|b|matches!(b,ContentBlock::Text{text,..} if text=="Unassociated reply")))));
    let unassociated =
        |body: &&&EventBody| matches!(body, EventBody::AgentOther { kind, .. } if kind.starts_with("unassociated"));
    let kept: Vec<_> = events.iter().filter(unassociated).map(|body| body.subagent().map(SubagentId::as_str)).collect();
    assert!(
        !kept.is_empty() && kept.iter().all(|thread| *thread == Some("foreign")),
        "kept apart, by its thread: {kept:?}"
    );
}

#[tokio::test]
async fn a_subagents_tokens_are_an_event_about_it_and_not_the_agents_own_context() {
    let tokens = json!({"method":"thread/tokenUsage/updated","params":{"threadId":"child","turnId":"child-turn",
        "tokenUsage":{"last":{"totalTokens":4321},"total":{"totalTokens":9999},"modelContextWindow":258_400}}});
    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        activity("thread", "root-turn", "child", "spawn", "started", true),
        turn("child", "child-turn", "inProgress"),
        tokens,
        turn("child", "child-turn", "completed"),
        turn("thread", "root-turn", "completed"),
    ])
    .await;

    let child = SubagentId::from("child");
    assert!(bodies(&output).contains(&&EventBody::SubagentContext { subagent: child, tokens: 4321 }));
    assert!(
        !output.iter().any(|out| matches!(out, AgentOutput::Report(protocol::capability::Report::Context { .. }))),
        "the agent's own context is its own"
    );
}
