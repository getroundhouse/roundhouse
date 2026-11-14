use protocol::capability::{
    Control,
    approvals::{Decision, Remember},
};
use protocol::event::{AgentRunState, ContentBlock, EventBody};
use protocol::model::Seq;
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::super::harness::Harness;
use super::fixtures::{activity, bodies, command, play, turn};
use crate::{AgentOutput, AgentProcess};

#[tokio::test]
async fn a_late_child_tool_result_stays_in_its_finished_invocation_and_is_recorded_once() {
    let result = command("child", "first", "old-command", true);
    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        activity("thread", "root-turn", "child", "spawn", "started", true),
        turn("child", "first", "inProgress"),
        command("child", "first", "old-command", false),
        turn("child", "first", "completed"),
        turn("child", "second", "inProgress"),
        result.clone(),
        result,
        turn("child", "second", "completed"),
    ])
    .await;
    let events = bodies(&output);
    let results: Vec<_> = events
        .iter()
        .filter_map(|body| {
            let EventBody::AgentUser { parent_tool_use_id, blocks } = body else { return None };
            blocks
                .iter()
                .any(|b| matches!(b,ContentBlock::CallResult{tool_use_id,..} if tool_use_id.as_str()=="old-command"))
                .then(|| parent_tool_use_id.as_ref().unwrap().as_str())
        })
        .collect();
    assert_eq!(results, ["spawn"]);
}

fn waiting_child() -> Vec<Value> {
    vec![
        json!({"result":{"turn":{"id":"root-turn"}}}),
        turn("thread", "root-turn", "inProgress"),
        activity("thread", "root-turn", "child", "spawn", "started", true),
        turn("child", "child-turn", "inProgress"),
        command("child", "child-turn", "approved-command", false),
        json!({"id":90,"method":"item/commandExecution/requestApproval","params":{"threadId":"child","turnId":"child-turn","itemId":"approved-command"}}),
        turn("thread", "root-turn", "completed"),
        json!({"method":"test/ready","params":{}}),
    ]
}

#[tokio::test]
async fn cancelling_a_completed_root_with_only_a_child_approval_emits_no_second_outcome() {
    let mut h = Harness::new(&json!([{"on":"turn/start","actions":waiting_child()}]));
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("Delegate")).await.unwrap();
    drain_to(&mut h, "test/ready").await;
    agent.interrupt().await.unwrap();
    agent.shared.rpc.write(&json!({"method":"test/barrier"})).await.unwrap();
    loop {
        let output = h.next().await;
        assert!(!matches!(
            output,
            AgentOutput::Event { body: EventBody::TurnEnded { .. } | EventBody::ApprovalResolved { .. }, .. }
        ));
        if matches!(output,AgentOutput::Event{body:EventBody::AgentOther{kind,..},..} if kind=="test/request") {
            break;
        }
    }
    assert_eq!(agent.shared.state.lock().waiting.len(), 1);
    agent.stop().await;
}

#[tokio::test]
async fn resolving_a_child_approval_restores_background_state_when_the_root_is_finished() {
    for native in [false, true] {
        let mut h = Harness::new(&json!([
            {"on":"turn/start","actions":waiting_child()},
            {"on":"test/resolve","actions":[{"method":"serverRequest/resolved","params":{"requestId":90}}]}
        ]));
        let agent = h.started().await;
        agent.send_prompt(Seq::FIRST, &Prompt::new("Delegate")).await.unwrap();
        drain_to(&mut h, "test/ready").await;
        let approval = agent.shared.state.lock().waiting.keys().next().unwrap().clone();
        if native {
            agent.shared.rpc.write(&json!({"method":"test/resolve"})).await.unwrap();
        } else {
            agent
                .control(&Control::Decide {
                    approval,
                    decision: Decision::Allow { edited: None, remember: Remember::Once },
                })
                .await
                .unwrap();
        }
        // Codex withdrawing its request is recorded here. A decision is the hub's to record, before it's sent.
        let mut resolved = 0;
        loop {
            let AgentOutput::Event { body, .. } = h.next().await else { continue };
            if let EventBody::AgentState { state } = body {
                assert_eq!(state, AgentRunState::Idle);
                break;
            }
            resolved += usize::from(matches!(body, EventBody::ApprovalResolved { .. }));
        }
        assert_eq!(resolved, usize::from(native));
        assert!(agent.shared.state.lock().subagents.runs.values().any(|run| !run.ended));
        agent.stop().await;
    }
}

#[tokio::test]
async fn a_child_ending_with_an_approval_leaves_other_children_in_background() {
    let mut actions = waiting_child();
    actions.remove(0);
    // The second child is registered while the root turn is still active.
    actions.splice(
        2..2,
        [
            activity("thread", "root-turn", "other", "other-spawn", "started", true),
            turn("other", "other-turn", "inProgress"),
        ],
    );
    actions.push(turn("child", "child-turn", "completed"));
    let output = play(actions).await;
    let events = bodies(&output);
    let last_tasks = events
        .iter()
        .rev()
        .find_map(|body| if let EventBody::AgentTasks { tasks, .. } = body { Some(tasks) } else { None })
        .unwrap();
    assert_eq!(last_tasks.len(), 1);
    assert_eq!(last_tasks[0].call.as_ref().unwrap().as_str(), "other-spawn");
    assert!(matches!(events.last(), Some(EventBody::AgentState { state: AgentRunState::Idle })));
}

async fn drain_to(h: &mut Harness, marker: &str) {
    loop {
        if matches!(h.next().await,AgentOutput::Event{body:EventBody::AgentOther{kind,..},..} if kind==marker) {
            return;
        }
    }
}
