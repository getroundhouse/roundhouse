#![cfg(test)]
use protocol::capability::{
    Control,
    approvals::{Asks, Decision, Remember},
    conversation::Start,
    laws,
    turns::OwnTurn,
};
use protocol::event::{
    AgentRunState, AgentTask, EndedTask, EventBody, ExitReason, SessionEvent, TaskKind, TaskOutcome, TurnOutcome,
};
use protocol::fixtures;
use protocol::model::{ApprovalId, RunId, Seq, ToolUseId};
use protocol::prompt::{Prompt, PromptFrom};
use serde_json::{Value, json};

use super::super::wakeup::{WakeupDelivery, delivery_for};
use super::harness::{Harness, append_transcript, msg_chunk, update};
use crate::testing::Asking;
use crate::{AgentOutput, AgentProcess};

const RUN: RunId = RunId::new(1);

fn init_1_5_12() -> Value {
    json!({
        "on": "initialize",
        "actions": [{ "result": { "protocolVersion": 1, "agentInfo": { "name": "Antigravity ACP Server", "version": "1.5.12" } } }]
    })
}

fn bg_tool(session_update: &str, id: &str, status: &str, raw_output: Option<Value>) -> Value {
    let args = json!({"CommandLine": "cargo test", "toolSummary": "Unit test", "WaitMsBeforeAsync": 500});
    let mut body = json!({
        "sessionUpdate": session_update,
        "toolCallId": id,
        "name": "run_command",
        "status": status,
        "args": args,
        "rawInput": args,
    });
    if let Some(out) = raw_output {
        body["rawOutput"] = out;
    }
    update(&body)
}

fn append_wakeup_done(room: &std::path::Path, content: &str) {
    append_transcript(
        room,
        &[
            json!({"source": "SYSTEM", "type": "SYSTEM_MESSAGE", "status": "DONE"}),
            json!({"source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "tool_calls": [], "content": content}),
        ],
    );
}

fn prompt_count(room: &std::path::Path) -> usize {
    std::fs::read_to_string(room.join("native-requests.jsonl"))
        .unwrap()
        .lines()
        .filter(|line| line.contains("\"session/prompt\""))
        .count()
}

fn push_event(events: &mut Vec<SessionEvent>, next_seq: &mut u64, run: Option<RunId>, body: EventBody) -> Seq {
    let seq = Seq::new(*next_seq);
    *next_seq += 1;
    events.push(SessionEvent { run, ..fixtures::event(seq, body) });
    seq
}

#[test]
fn wakeup_delivery_switches_to_async_stream_at_1_5_12() {
    assert_eq!(delivery_for(None), WakeupDelivery::BufferedDrain);
    assert_eq!(delivery_for(Some("1.5.9")), WakeupDelivery::BufferedDrain);
    assert_eq!(delivery_for(Some("1.5.11")), WakeupDelivery::BufferedDrain);
    assert_eq!(delivery_for(Some("1.5.12")), WakeupDelivery::AsyncStream);
    assert_eq!(delivery_for(Some("v1.5.12-beta.1")), WakeupDelivery::AsyncStream);
    assert_eq!(delivery_for(Some("1.6.0")), WakeupDelivery::AsyncStream);
    assert_eq!(delivery_for(Some("2.0.0")), WakeupDelivery::AsyncStream);
}

#[tokio::test]
async fn agy_1_5_12_streams_idle_wakeup_and_settles_own_turn_without_drain_prompt() {
    let completed = json!({"exit_code": 0, "output": "Command exited with code 0.\n\nCommand output:\nok\n"});
    let script = json!([
        init_1_5_12(),
        {
            "on": "session/prompt", "at": 1,
            "actions": [
                bg_tool("tool_call", "bg-1", "in_progress", None),
                msg_chunk("Running in background."),
                {"result": {"stopReason": "end_turn"}},
                bg_tool("tool_call_update", "bg-1", "completed", Some(completed)),
                msg_chunk("\n\nTests passed.")
            ]
        }
    ]);
    let mut h = Harness::new(&script, super::super::claims::full_access(), Start::New { conversation: None });
    let agent = h.started().await;
    let (mut events, mut next_seq) = (Vec::new(), 1_u64);
    let user =
        EventBody::UserPrompt { prompt: Prompt::new("run tests"), client_msg_id: None, from: PromptFrom::Person };
    let p1 = push_event(&mut events, &mut next_seq, None, user);
    agent.prompt(p1, &Prompt::new("run tests")).await.unwrap();

    let mut saw_own_turn = false;
    loop {
        if let AgentOutput::Event { body, .. } = h.next().await {
            if matches!(&body, EventBody::AgentOther { kind, .. } if kind == "test/request") {
                continue;
            }
            if matches!(&body, EventBody::OwnTurn(OwnTurn::TaskEnded { .. })) {
                saw_own_turn = true;
                append_wakeup_done(h.room.path(), "Tests passed.");
            }
            let done = saw_own_turn && matches!(&body, EventBody::AgentState { state: AgentRunState::Idle });
            push_event(&mut events, &mut next_seq, Some(RUN), body);
            if done {
                break;
            }
        }
    }
    agent.stop().await;
    let exited =
        EventBody::AgentExited { code: Some(0), signal: None, stderr_tail: Vec::new(), reason: ExitReason::Requested };
    push_event(&mut events, &mut next_seq, Some(RUN), exited);

    let claims = super::super::claims::claims();
    assert_eq!(laws::check(&events, |_| &claims), []);
    let task = AgentTask {
        description: Some("Unit test".into()),
        call: Some(ToolUseId::from("bg-1")),
        ..AgentTask::new("bg-1", TaskKind::Command)
    };
    let ended = EndedTask { task, outcome: TaskOutcome::Finished, wait: None };
    assert!(events.iter().any(|e| e.body == EventBody::OwnTurn(OwnTurn::TaskEnded { ended: vec![ended.clone()] })));
    assert!(events.iter().any(|e| matches!(
        &e.body,
        EventBody::TurnEnded { outcome: TurnOutcome::Success, result: Some(r), .. } if r.trim() == "Tests passed."
    )));
    assert_eq!(prompt_count(h.room.path()), 1, "must not send DRAIN_PROMPT");
}

fn plan_wakeup_script() -> Value {
    let step2_args = json!({"CommandLine": "echo step2", "toolSummary": "Step 2", "WaitMsBeforeAsync": 5000});
    let perm = json!({
        "jsonrpc": "2.0", "id": "perm-idle", "method": "session/request_permission",
        "params": {
            "sessionId": "session-1",
            "toolCall": {"toolCallId": "step-2", "kind": "execute", "name": "run_command", "status": "pending", "args": step2_args},
            "options": [{"optionId": "allow_once", "kind": "allow_once", "name": "Allow"}]
        }
    });
    json!([
        init_1_5_12(),
        {
            "on": "session/prompt", "at": 1,
            "actions": [
                bg_tool("tool_call", "bg-1", "in_progress", None),
                msg_chunk("Waiting."),
                {"result": {"stopReason": "end_turn"}},
                bg_tool("tool_call_update", "bg-1", "completed", Some(json!({"exit_code": 0, "output": "ok\n"}))),
                bg_tool("tool_call", "step-2", "pending", None),
                perm
            ]
        },
        {
            "on": "", "at": 1,
            "actions": [
                bg_tool("tool_call_update", "step-2", "completed", Some(json!({"exit_code": 0, "output": "step2\n"}))),
                msg_chunk("\n\n1. Final plan")
            ]
        }
    ])
}

#[tokio::test]
async fn agy_1_5_12_idle_wakeup_accepts_plan_permission_holds_plan_and_supports_cancel() {
    let mut h = Harness::new(&plan_wakeup_script(), super::super::claims::plan(), Start::New { conversation: None });
    let agent = h.started().await;
    agent.prompt(Seq::FIRST, &Prompt::new("draft")).await.unwrap();

    let perm_id: ApprovalId = loop {
        if let AgentOutput::Event { body: EventBody::ApprovalRequested { approval_id, request, .. }, .. } =
            h.next().await
            && matches!(request.asks, Asks::Call { .. })
        {
            break approval_id;
        }
    };
    let allow = Decision::Allow { remember: Remember::Once };
    agent.control(&Control::Decide { approval: perm_id, decision: allow }).await.unwrap();
    while !matches!(h.next().await, AgentOutput::Delta(_)) {}
    append_wakeup_done(h.room.path(), "1. Final plan");

    let plan = loop {
        if let AgentOutput::Event { body: EventBody::ApprovalRequested { request, .. }, .. } = h.next().await
            && let Asks::Plan { plan, .. } = request.asks
        {
            break plan;
        }
    };
    assert_eq!(plan, "1. Final plan");
    agent.cancel().await.unwrap();
    while !matches!(
        h.next().await,
        AgentOutput::Event { body: EventBody::TurnEnded { outcome: TurnOutcome::Cancelled, .. }, .. }
    ) {}
    agent.stop().await;
    assert_eq!(prompt_count(h.room.path()), 1, "must not send DRAIN_PROMPT");
}
