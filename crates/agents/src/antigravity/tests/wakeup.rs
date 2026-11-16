#![cfg(test)]
use std::io::Write as _;
use std::path::Path;

use protocol::capability::{
    calls::{
        CallOutput,
        output::{CommandOutput, Ended},
    },
    conversation::Start,
    laws,
    turns::OwnTurn,
};
use protocol::event::{
    AgentRunState, AgentTask, ContentBlock, EndedTask, EventBody, ExitReason, SessionEvent, TaskKind, TaskOutcome,
    TurnOutcome,
};
use protocol::fixtures;
use protocol::model::{AgentSessionId, PermissionMode, RunId, Seq, TaskId, ToolUseId};
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

    async fn collect_turn(&mut self, h: &mut Harness) -> Vec<EventBody> {
        let mut seen = Vec::new();
        loop {
            if let AgentOutput::Event { body, .. } = h.next().await {
                if matches!(&body, EventBody::AgentOther { kind, .. } if kind == "test/request") {
                    continue;
                }
                let idle_after_end = matches!(&body, EventBody::AgentState { state: AgentRunState::Idle })
                    && seen.iter().any(|item| matches!(item, EventBody::TurnEnded { .. }));
                seen.push(body.clone());
                self.push(Some(RUN), body);
                if idle_after_end {
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
    };
    assert!(turn1.contains(&EventBody::AgentTasks { tasks: vec![expected_task.clone()], stopped: Vec::new() }));

    append_transcript(
        h.room.path(),
        "session-1",
        &[
            json!({"source": "USER_EXPLICIT", "type": "USER_INPUT", "status": "DONE"}),
            json!({"source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "tool_calls": []}),
            json!({"source": "SYSTEM", "type": "SYSTEM_MESSAGE", "status": "DONE"}),
            json!({"source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "tool_calls": []}),
        ],
    );

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

fn gated_wakeup_script() -> Value {
    let bg_args = json!({"CommandLine": "sleep 10", "toolSummary": "First step", "WaitMsBeforeAsync": 500});
    let step2_args = json!({"CommandLine": "echo done", "toolSummary": "Second step", "WaitMsBeforeAsync": 5000});
    let perm = json!({
        "jsonrpc": "2.0", "id": "perm-2", "method": "session/request_permission",
        "params": {
            "sessionId": "session-1",
            "toolCall": {"toolCallId": "step-2", "kind": "execute", "name": "run_command", "status": "pending", "args": step2_args},
            "options": [{"optionId": "allow_once", "kind": "allow_once", "name": "Allow"}]
        }
    });
    json!([
        {
            "on": "session/prompt", "at": 1,
            "actions": [
                user_chunk("session-1", "go"),
                tool_event("session-1", "tool_call", "bg-1", "in_progress", &bg_args, None),
                {"result": {"stopReason": "end_turn"}}
            ]
        },
        {
            "on": "session/prompt", "at": 2,
            "actions": [
                tool_event("session-1", "tool_call_update", "bg-1", "completed", &bg_args, Some(json!({"exit_code": 0, "output": ""}))),
                tool_event("session-1", "tool_call", "step-2", "pending", &step2_args, None),
                perm
            ]
        },
        {
            "on": "", "at": 1,
            "actions": [
                tool_event("session-1", "tool_call_update", "step-2", "completed", &step2_args, Some(json!({"exit_code": 0, "output": "done\n"}))),
                user_chunk("session-1", "Continue"),
                msg_chunk("session-1", "Finished both steps."),
                {"id": 7, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}
            ]
        }
    ])
}

#[tokio::test]
async fn wakeup_parked_on_permission_gated_command_keeps_drain_open_and_finishes_follow_up_step() {
    let mut h = Harness::new(&gated_wakeup_script(), PermissionMode::FullAccess, Start::New { conversation: None });
    let agent = h.started().await;
    let mut ledger = Ledger::new();
    let p1 = ledger.push(None, EventBody::UserPrompt { prompt: Prompt::new("go"), client_msg_id: None });
    agent.send_prompt(p1, &Prompt::new("go")).await.unwrap();
    let _ = ledger.collect_turn(&mut h).await;

    append_transcript(
        h.room.path(),
        "session-1",
        &[
            json!({"source": "SYSTEM", "type": "SYSTEM_MESSAGE", "status": "DONE"}),
            json!({"source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "tool_calls": [{"name": "run_command"}]}),
        ],
    );

    let wakeup_events = ledger.collect_turn(&mut h).await;
    agent.stop().await;
    ledger.push(
        Some(RUN),
        EventBody::AgentExited { code: Some(0), signal: None, stderr_tail: Vec::new(), reason: ExitReason::Requested },
    );

    let claims = super::super::claims::claims();
    assert_eq!(laws::check(&ledger.events, |_| &claims), []);
    assert!(wakeup_events.iter().any(|e| {
        matches!(e, EventBody::TurnEnded { outcome: TurnOutcome::Success, result: Some(r), .. } if r == "Finished both steps.")
    }));
    let asked = std::fs::read_to_string(h.room.path().join("native-requests.jsonl")).unwrap();
    assert!(!asked.contains("session/cancel"), "gated-tool drain must stay open until end_turn: {asked}");
}

#[tokio::test]
async fn resumed_conversation_skips_historical_transcript_wakeups_from_before_launch() {
    let script = json!([
        {"on": "session/resume", "actions": [{"result": {"sessionId": "kept-id", "configOptions": []}}]},
        {
            "on": "session/prompt", "at": 1,
            "actions": [msg_chunk("kept-id", "Post-resume wakeup."), user_chunk("kept-id", "Continue")]
        },
        {"on": "session/cancel", "actions": [{"id": 5, "jsonrpc": "2.0", "result": {"stopReason": "cancelled"}}]}
    ]);
    let wakeup_steps = [
        json!({"source": "SYSTEM", "type": "SYSTEM_MESSAGE", "status": "DONE"}),
        json!({"source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "tool_calls": []}),
    ];
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
    loop {
        if matches!(h.next().await, AgentOutput::Event { body: EventBody::AgentInit { .. }, .. }) {
            break;
        }
    }

    append_transcript(h.room.path(), "kept-id", &wakeup_steps);
    let result = loop {
        if let AgentOutput::Event { body: EventBody::TurnEnded { outcome, result, .. }, .. } = h.next().await {
            break (outcome, result);
        }
    };
    agent.stop().await;

    assert_eq!(result, (TurnOutcome::Success, Some("Post-resume wakeup.".into())));
    let prompt_count = std::fs::read_to_string(h.room.path().join("native-requests.jsonl"))
        .unwrap()
        .lines()
        .filter(|line| line.contains("\"session/prompt\""))
        .count();
    assert_eq!(prompt_count, 1, "historical wakeup before launch must not trigger a second drain prompt");
}

#[tokio::test]
async fn mid_turn_system_message_written_before_prompt_finishes_does_not_trigger_extra_drain() {
    let script = json!([
        {
            "on": "session/prompt", "at": 1,
            "actions": [user_chunk("session-1", "turn 1")]
        },
        {
            "on": "session/set_config_option", "at": 3,
            "actions": [
                {"result": {}},
                msg_chunk("session-1", "Handled mid-turn."),
                {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}
            ]
        },
        {
            "on": "session/prompt", "at": 2,
            "actions": [msg_chunk("session-1", "Between-turn wakeup."), user_chunk("session-1", "Continue")]
        },
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
    // Written while Turn 1's `session/prompt` is still open; advancing the floor at `prompt_finished` skips it.
    append_transcript(
        h.room.path(),
        "session-1",
        &[
            json!({"source": "SYSTEM", "type": "SYSTEM_MESSAGE", "status": "DONE"}),
            json!({"source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "tool_calls": []}),
        ],
    );
    agent
        .configure(&protocol::model::SessionSettingsChange {
            model: None,
            effort: Some(protocol::model::Effort::Low),
            permission_mode: None,
        })
        .await
        .unwrap();
    let _ = ledger.collect_turn(&mut h).await;

    // Now write an actual between-turn wakeup; it should be prompt #2 (`at: 2`), proving the mid-turn one was skipped.
    append_transcript(
        h.room.path(),
        "session-1",
        &[
            json!({"source": "SYSTEM", "type": "SYSTEM_MESSAGE", "status": "DONE"}),
            json!({"source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "tool_calls": []}),
        ],
    );
    let wakeup_turn = ledger.collect_turn(&mut h).await;
    agent.stop().await;

    assert!(wakeup_turn.iter().any(|e| {
        matches!(e, EventBody::TurnEnded { outcome: TurnOutcome::Success, result: Some(r), .. } if r == "Between-turn wakeup.")
    }));
}
