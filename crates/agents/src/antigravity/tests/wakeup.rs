#![cfg(test)]
use std::io::Write as _;
use std::path::Path;
use std::time::Duration;

use protocol::capability::{
    Control,
    approvals::{Asks, Decision, Remember},
    calls::{
        CallOutput,
        output::{CommandOutput, Ended},
    },
    conversation::Start,
    laws,
    turns::OwnTurn,
};
use protocol::event::{
    AgentRunState, AgentTask, ApprovalOutcome, ContentBlock, EndedTask, EventBody, ExitReason, QuestionAnswer,
    SessionEvent, TaskKind, TaskOutcome, TurnOutcome,
};
use protocol::fixtures;
use protocol::model::{AgentSessionId, ApprovalId, PermissionMode, RunId, Seq, TaskId, ToolUseId};
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::harness::Harness;
use crate::{AgentOutput, AgentProcess};

const RUN: RunId = RunId::new(1);

fn update(session_id: &str, payload: &Value) -> Value {
    json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": session_id, "update": payload}})
}

fn user_chunk(session_id: &str, text: &str) -> Value {
    update(session_id, &json!({"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": text}}))
}

fn msg_chunk(session_id: &str, text: &str) -> Value {
    update(session_id, &json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}))
}

fn thought_chunk(session_id: &str, text: &str) -> Value {
    update(session_id, &json!({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": text}}))
}

fn tool_event(
    session_id: &str,
    session_update: &str,
    id: &str,
    status: &str,
    args: &Value,
    raw_output: Option<Value>,
) -> Value {
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
    update(session_id, &body)
}

fn sys_step() -> Value {
    json!({"source": "SYSTEM", "type": "SYSTEM_MESSAGE", "status": "DONE"})
}

fn planner_step(tools: &[&str]) -> Value {
    let calls: Vec<Value> = tools.iter().map(|name| json!({"name": name})).collect();
    json!({"source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "tool_calls": calls})
}

fn generic_step() -> Value {
    json!({"source": "MODEL", "type": "GENERIC", "status": "DONE"})
}

fn append_transcript(room: &Path, session_id: &str, steps: &[Value]) {
    let dir =
        room.join(".gemini").join("antigravity-cli").join("brain").join(session_id).join(".system_generated/logs");
    std::fs::create_dir_all(&dir).unwrap();
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("transcript.jsonl")).unwrap();
    for step in steps {
        writeln!(file, "{step}").unwrap();
    }
    file.flush().unwrap();
}

async fn wait_init(h: &mut Harness) {
    loop {
        if matches!(h.next().await, AgentOutput::Event { body: EventBody::AgentInit { .. }, .. }) {
            return;
        }
    }
}

struct Ledger {
    next_seq: u64,
    events: Vec<SessionEvent>,
}

impl Ledger {
    fn new() -> Self {
        Self { next_seq: 1, events: Vec::new() }
    }

    fn push(&mut self, run: Option<RunId>, body: EventBody) -> Seq {
        let seq = Seq::new(self.next_seq);
        self.next_seq += 1;
        self.events.push(SessionEvent { run, ..fixtures::event(seq, body) });
        seq
    }

    async fn collect_until_approval(&mut self, h: &mut Harness) -> ApprovalId {
        loop {
            if let AgentOutput::Event { body, .. } = h.next().await {
                if matches!(&body, EventBody::AgentOther { kind, .. } if kind == "test/request") {
                    continue;
                }
                let id = if let EventBody::ApprovalRequested { approval_id, .. } = &body {
                    Some(approval_id.clone())
                } else {
                    None
                };
                self.push(Some(RUN), body);
                if let Some(id) = id {
                    return id;
                }
            }
        }
    }

    async fn collect_turn(&mut self, h: &mut Harness) -> Vec<EventBody> {
        let mut seen = Vec::new();
        loop {
            if let AgentOutput::Event { body, .. } = h.next().await {
                if matches!(&body, EventBody::AgentOther { kind, .. } if kind == "test/request") {
                    continue;
                }
                let done = (matches!(&body, EventBody::AgentState { state: AgentRunState::Idle })
                    && seen.iter().any(|item| matches!(item, EventBody::TurnEnded { .. })))
                    || (matches!(&body, EventBody::AgentState { state: AgentRunState::RequiresAction })
                        && seen.iter().any(|item| {
                            matches!(item, EventBody::ApprovalRequested { request: Some(r), .. } if matches!(r.asks, Asks::Plan { .. }))
                        }));
                seen.push(body.clone());
                self.push(Some(RUN), body);
                if done {
                    return seen;
                }
            }
        }
    }
}

fn plain_wakeup_script() -> Value {
    let cmd_args = json!({"CommandLine": "cargo test", "toolSummary": "Unit test execution", "WaitMsBeforeAsync": 500});
    let completed_out = json!({"exit_code": 0, "output": "Command exited with code 0.\n\nCommand output:\nok\n"});
    json!([
        {
            "on": "session/prompt", "at": 1,
            "actions": [
                user_chunk("session-1", "run tests"),
                tool_event("session-1", "tool_call", "bg-1", "in_progress", &cmd_args, None),
                msg_chunk("session-1", "Running in the background."),
                {"result": {"stopReason": "end_turn"}}
            ]
        },
        {
            "on": "session/prompt", "at": 2,
            "actions": [
                tool_event("session-1", "tool_call_update", "bg-1", "completed", &cmd_args, Some(completed_out)),
                thought_chunk("session-1", "Tests finished."),
                msg_chunk("session-1", "All tests passed."),
                user_chunk("session-1", "Continue")
            ]
        },
        {"on": "session/cancel", "actions": [{"id": 7, "jsonrpc": "2.0", "result": {"stopReason": "cancelled"}}]}
    ])
}

#[tokio::test]
async fn background_command_lists_task_drains_wakeup_into_own_turn_cancels_before_extra_call_and_obeys_laws() {
    let mut h = Harness::new(&plain_wakeup_script(), PermissionMode::FullAccess, Start::New { conversation: None });
    let agent = h.started().await;
    let mut ledger = Ledger::new();
    let p1 = ledger.push(None, EventBody::UserPrompt { prompt: Prompt::new("run tests"), client_msg_id: None });

    agent.send_prompt(p1, &Prompt::new("run tests")).await.unwrap();
    let turn1 = ledger.collect_turn(&mut h).await;
    let expected_task = AgentTask {
        id: TaskId::from("bg-1"),
        description: Some("Unit test execution".into()),
        kind: TaskKind::Command,
        call: Some(ToolUseId::from("bg-1")),
        subagent: None,
    };
    assert!(turn1.contains(&EventBody::AgentTasks { tasks: vec![expected_task.clone()], stopped: Vec::new() }));

    append_transcript(
        h.room.path(),
        "session-1",
        &[
            json!({"source": "USER_EXPLICIT", "type": "USER_INPUT", "status": "DONE"}),
            planner_step(&[]),
            sys_step(),
            planner_step(&["run_command"]),
        ],
    );
    // In FullAccess (`--dangerously-skip-permissions`), a wakeup command runs unasked past `UNGATED_PARK_GRACE`.
    tokio::time::sleep(Duration::from_millis(520)).await;
    let premature = std::fs::read_to_string(h.room.path().join("native-requests.jsonl")).unwrap();
    assert_eq!(premature.lines().filter(|l| l.contains("\"session/prompt\"")).count(), 1);

    append_transcript(h.room.path(), "session-1", &[generic_step(), planner_step(&[])]);
    let wakeup_events = ledger.collect_turn(&mut h).await;
    agent.stop().await;
    ledger.push(
        Some(RUN),
        EventBody::AgentExited { code: Some(0), signal: None, stderr_tail: Vec::new(), reason: ExitReason::Requested },
    );

    let claims = super::super::claims::claims();
    assert_eq!(laws::check(&ledger.events, |_| &claims), []);
    assert!(matches!(
        &wakeup_events[0],
        EventBody::AgentUser { blocks, .. }
            if matches!(&blocks[0], ContentBlock::CallResult { tool_use_id, output: CallOutput::Command(CommandOutput { output, exit_code: Some(0), ended: Ended::Exited, .. }), .. } if tool_use_id.as_str() == "bg-1" && output == "ok\n")
    ));
    assert_eq!(wakeup_events[1], EventBody::AgentTasks { tasks: Vec::new(), stopped: Vec::new() });
    assert_eq!(wakeup_events[2], EventBody::AgentState { state: AgentRunState::Running });
    assert_eq!(
        wakeup_events[3],
        EventBody::OwnTurn(OwnTurn::TaskEnded {
            ended: vec![EndedTask { task: expected_task, outcome: TaskOutcome::Finished, wait: None }],
        })
    );
    assert!(wakeup_events.iter().any(|e| {
        matches!(e, EventBody::TurnEnded { outcome: TurnOutcome::Success, result: Some(r), .. } if r == "All tests passed.")
    }));
    let asked = std::fs::read_to_string(h.room.path().join("native-requests.jsonl")).unwrap();
    assert!(asked.contains("session/cancel"), "plain wakeup drain must cancel on user_message_chunk: {asked}");
}

#[tokio::test]
async fn mixed_ungated_and_gated_wakeup_batch_drains_and_holds_plan_in_plan_mode() {
    let step2_args = json!({"CommandLine": "echo done", "toolSummary": "Second step", "WaitMsBeforeAsync": 5000});
    let perm = json!({
        "jsonrpc": "2.0", "id": "perm-2", "method": "session/request_permission",
        "params": {
            "sessionId": "session-1",
            "toolCall": {"toolCallId": "step-2", "kind": "execute", "name": "run_command", "status": "pending", "args": step2_args},
            "options": [{"optionId": "allow_once", "kind": "allow_once", "name": "Allow"}]
        }
    });
    let plan_script = json!([
        {"on": "session/prompt", "at": 1, "actions": [tool_event("session-1", "tool_call", "step-2", "pending", &step2_args, None), perm]},
        {"on": "", "at": 1, "actions": [tool_event("session-1", "tool_call_update", "step-2", "completed", &step2_args, Some(json!({"exit_code": 0, "output": "done\n"}))), user_chunk("session-1", "Continue"), msg_chunk("session-1", "Plan ready."), {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}]}
    ]);
    let mut plan_h = Harness::new(&plan_script, PermissionMode::Plan, Start::New { conversation: None });
    let plan_agent = plan_h.started().await;
    let mut plan_ledger = Ledger::new();
    wait_init(&mut plan_h).await;
    append_transcript(
        plan_h.room.path(),
        "session-1",
        &[sys_step(), planner_step(&["read_file", "read_file", "run_command"]), generic_step()],
    );
    let approval = plan_ledger.collect_until_approval(&mut plan_h).await;
    plan_ledger.push(
        None,
        EventBody::ApprovalResolved {
            approval_id: approval.clone(),
            outcome: ApprovalOutcome::Allowed,
            answers_by_text: None,
            answers_by_position: None,
            by_setting: false,
        },
    );
    let allow_once = Decision::Allow { edited: None, remember: Remember::Once };
    plan_agent.control(&Control::Decide { approval, decision: allow_once }).await.unwrap();
    let plan_wakeup = plan_ledger.collect_turn(&mut plan_h).await;
    plan_agent.stop().await;
    let claims = super::super::claims::claims();
    assert_eq!(laws::check(&plan_ledger.events, |_| &claims), []);
    assert!(plan_wakeup.iter().any(|e| matches!(e, EventBody::ApprovalRequested { request: Some(r), .. } if matches!(&r.asks, Asks::Plan { plan, .. } if plan == "Plan ready."))));
}

#[tokio::test]
async fn mixed_ungated_and_question_wakeup_batch_drains_in_full_access() {
    let question = json!({
        "jsonrpc": "2.0", "id": "elic-w", "method": "elicitation/create",
        "params": {
            "mode": "form", "message": "Pick",
            "requestedSchema": {"type": "object", "properties": {"q": {"type": "string", "description": "Choice?", "oneOf": [{"const": "1", "title": "1. Yes"}]}}}
        }
    });
    let q_script = json!([
        {"on": "session/prompt", "at": 1, "actions": [question]},
        {"on": "", "at": 1, "actions": [user_chunk("session-1", "Continue"), msg_chunk("session-1", "Answered."), {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}]}
    ]);
    let mut full_h = Harness::new(&q_script, PermissionMode::FullAccess, Start::New { conversation: None });
    let full_agent = full_h.started().await;
    let mut full_ledger = Ledger::new();
    wait_init(&mut full_h).await;
    append_transcript(
        full_h.room.path(),
        "session-1",
        &[sys_step(), planner_step(&["read_file", "ask_question"]), generic_step()],
    );
    let q_id = full_ledger.collect_until_approval(&mut full_h).await;
    let ans = Decision::Answer { answers: vec![QuestionAnswer { picked: vec![0], typed: None }] };
    full_agent.control(&Control::Decide { approval: q_id, decision: ans }).await.unwrap();
    let full_wakeup = full_ledger.collect_turn(&mut full_h).await;
    full_agent.stop().await;
    assert!(full_wakeup.iter().any(|e| matches!(e, EventBody::TurnEnded { result: Some(r), .. } if r == "Answered.")));
}

#[tokio::test]
async fn resumed_conversation_skips_historical_transcript_wakeups_from_before_launch() {
    let script = json!([
        {"on": "session/resume", "actions": [{"result": {"sessionId": "kept-id", "configOptions": []}}]},
        {"on": "session/prompt", "at": 1, "actions": [msg_chunk("kept-id", "Post-resume wakeup."), user_chunk("kept-id", "Continue")]},
        {"on": "session/cancel", "actions": [{"id": 5, "jsonrpc": "2.0", "result": {"stopReason": "cancelled"}}]}
    ]);
    let wakeup_steps = [sys_step(), planner_step(&[])];
    let mut h = Harness::seeded(
        &script,
        PermissionMode::FullAccess,
        Start::Resume { conversation: AgentSessionId::from("kept-id") },
        |room| {
            let kept = room.join(".gemini").join("antigravity-cli").join("conversations");
            std::fs::create_dir_all(&kept).unwrap();
            std::fs::write(kept.join("kept-id.db"), b"kept").unwrap();
            append_transcript(room, "kept-id", &wakeup_steps);
        },
    );
    let agent = h.started().await;
    wait_init(&mut h).await;

    append_transcript(h.room.path(), "kept-id", &wakeup_steps);
    let result = loop {
        if let AgentOutput::Event { body: EventBody::TurnEnded { outcome, result, .. }, .. } = h.next().await {
            break (outcome, result);
        }
    };
    agent.stop().await;

    assert_eq!(result, (TurnOutcome::Success, Some("Post-resume wakeup.".into())));
    let asked = std::fs::read_to_string(h.room.path().join("native-requests.jsonl")).unwrap();
    assert_eq!(asked.lines().filter(|l| l.contains("\"session/prompt\"")).count(), 1);
}

#[tokio::test]
async fn mid_turn_system_message_written_before_prompt_finishes_does_not_trigger_extra_drain() {
    let script = json!([
        {"on": "session/prompt", "at": 1, "actions": [user_chunk("session-1", "turn 1")]},
        {"on": "session/set_config_option", "at": 3, "actions": [{"result": {}}, msg_chunk("session-1", "Handled mid-turn."), {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}]},
        {"on": "session/prompt", "at": 2, "actions": [msg_chunk("session-1", "Between-turn wakeup."), user_chunk("session-1", "Continue")]},
        {"on": "session/cancel", "actions": [{"id": 8, "jsonrpc": "2.0", "result": {"stopReason": "cancelled"}}]}
    ]);
    let mut h = Harness::new(&script, PermissionMode::FullAccess, Start::New { conversation: None });
    let agent = h.started().await;
    let mut ledger = Ledger::new();
    let p1 = ledger.push(None, EventBody::UserPrompt { prompt: Prompt::new("turn 1"), client_msg_id: None });
    agent.send_prompt(p1, &Prompt::new("turn 1")).await.unwrap();
    loop {
        if matches!(h.next().await, AgentOutput::Event { body: EventBody::PromptTaken { .. }, .. }) {
            break;
        }
    }
    append_transcript(h.room.path(), "session-1", &[sys_step(), planner_step(&[])]);
    let low =
        protocol::model::SessionSettingsChange { effort: Some(protocol::model::Effort::Low), ..Default::default() };
    agent.configure(&low).await.unwrap();
    let _ = ledger.collect_turn(&mut h).await;

    append_transcript(h.room.path(), "session-1", &[sys_step(), planner_step(&[])]);
    let wakeup_turn = ledger.collect_turn(&mut h).await;
    agent.stop().await;
    assert!(
        wakeup_turn
            .iter()
            .any(|e| matches!(e, EventBody::TurnEnded { result: Some(r), .. } if r == "Between-turn wakeup."))
    );
}
