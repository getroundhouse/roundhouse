#![cfg(test)]
use std::time::Duration;

use protocol::capability::conversation::Start;
use protocol::event::{AgentRunState, EventBody, TurnOutcome};
use protocol::model::Seq;
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::harness::{Harness, append_transcript, append_transcript_raw, msg_chunk, update, user_chunk};
use crate::testing::Asking;
use crate::{AgentOutput, AgentProcess};

fn tool_call(name: &str, session_update: &str, id: &str, status: &str, args: &Value) -> Value {
    update(&json!({
        "sessionUpdate": session_update,
        "toolCallId": id,
        "name": name,
        "status": status,
        "args": args,
        "rawInput": args,
    }))
}

fn subagent_tool(session_update: &str, id: &str, status: &str) -> Value {
    let args = json!({"TypeName": "self", "Role": "Helper", "Prompt": "do work"});
    tool_call("invoke_subagent", session_update, id, status, &args)
}

async fn collect_turn_result(h: &mut Harness) -> (TurnOutcome, Option<String>) {
    let mut ended = None;
    loop {
        if let AgentOutput::Event { body, .. } = h.next().await {
            if let EventBody::TurnEnded { outcome, result, .. } = body {
                ended = Some((outcome, result));
            } else if matches!(body, EventBody::AgentState { state: AgentRunState::Idle })
                && let Some(done) = ended
            {
                return done;
            }
        }
    }
}

/// Exercises the `agy` 1.5.9 `user_message_chunk` cancel-drain path once a spawned subagent goes idle.
#[tokio::test]
async fn subagent_completion_clears_async_latch_so_wakeup_drain_cancels_cleanly() {
    let script = json!([
        {
            "on": "session/prompt", "at": 1,
            "actions": [
                user_chunk("delegate"),
                subagent_tool("tool_call", "sub-call-1", "in_progress"),
                subagent_tool("tool_call_update", "sub-call-1", "completed"),
                msg_chunk("Delegated to subagent."),
                {"result": {"stopReason": "end_turn"}}
            ]
        },
        {
            "on": "session/prompt", "at": 2,
            "actions": [
                msg_chunk("Parent heard subagent result."),
                user_chunk("Continue")
            ]
        },
        {"on": "session/cancel", "actions": [{"id": 7, "jsonrpc": "2.0", "result": {"stopReason": "cancelled"}}]}
    ]);
    let mut h = Harness::new(&script, super::super::claims::full_access(), Start::New { conversation: None });
    let agent = h.started().await;

    agent.prompt(Seq::FIRST, &Prompt::new("delegate")).await.unwrap();
    let turn1 = collect_turn_result(&mut h).await;
    assert_eq!(turn1, (TurnOutcome::Success, Some("Delegated to subagent.".into())));

    append_transcript(
        h.room.path(),
        &[
            json!({
                "source": "SYSTEM",
                "type": "SYSTEM_MESSAGE",
                "status": "DONE",
                "content": "[Message] sender=system content=Subagent child-1 has gone idle. This was the subagent's most recent response: ok"
            }),
            json!({"source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "tool_calls": []}),
        ],
    );
    let wakeup = collect_turn_result(&mut h).await;
    agent.stop().await;

    assert_eq!(wakeup, (TurnOutcome::Success, Some("Parent heard subagent result.".into())));
    let asked = std::fs::read_to_string(h.room.path().join("native-requests.jsonl")).unwrap();
    assert!(asked.contains("session/cancel"), "wakeup drain must cancel once the subagent went idle: {asked}");
}

#[tokio::test]
async fn turn_transcript_scan_preserves_partial_trailing_line_and_does_not_replay_idle_after_reopen() {
    let msg_args = json!({"Recipient": "child-1", "Message": "keep going"});
    let script = json!([
        {
            "on": "session/prompt", "at": 1,
            "actions": [
                user_chunk("spawn"),
                subagent_tool("tool_call", "sub-1", "in_progress"),
                subagent_tool("tool_call_update", "sub-1", "completed"),
                {"result": {"stopReason": "end_turn"}}
            ]
        },
        {
            "on": "session/prompt", "at": 2,
            "actions": [
                user_chunk("reopen"),
                tool_call("send_message", "tool_call", "msg-1", "in_progress", &msg_args),
                tool_call("send_message", "tool_call_update", "msg-1", "completed", &msg_args),
                {"result": {"stopReason": "end_turn"}}
            ]
        }
    ]);
    let mut h = Harness::new(&script, super::super::claims::full_access(), Start::New { conversation: None });
    let agent = h.started().await;

    // Seed a complete `Created` line plus an incomplete trailing line before Turn 1 finishes so `read_turn_transcript`
    // consumes only through the newline and leaves the partial line intact for the next write. `wakeup::watch` may read
    // the `Created` line before the turn starts, and the spawn's completion that follows is then that subagent too.
    let created = json!({
        "type": "GENERIC",
        "content": "Created the following subagent:\n{\"conversationId\": \"child-1\"}"
    });
    let idle_step = json!({
        "source": "SYSTEM",
        "type": "SYSTEM_MESSAGE",
        "status": "DONE",
        "content": "Subagent child-1 has gone idle."
    })
    .to_string();
    let split = idle_step.len() / 2;
    append_transcript_raw(h.room.path(), &format!("{created}\n{}", &idle_step[..split]));

    agent.prompt(Seq::FIRST, &Prompt::new("spawn")).await.unwrap();
    let _ = collect_turn_result(&mut h).await;
    assert!(agent.shared.state.lock().async_agents.has_live_subagents());

    // Finish the previously partial idle line (without a `PLANNER_RESPONSE`, so no wakeup drain fires); `wakeup::watch`
    // must still see it and clear `child-1` while advancing the shared `async_agents_offset`.
    append_transcript_raw(h.room.path(), &format!("{}\n", &idle_step[split..]));
    waiting::task::until(Duration::from_secs(5), "wakeup::watch should consume completed partial idle line", || {
        !agent.shared.state.lock().async_agents.has_live_subagents()
    })
    .await;

    // Turn 2 reopens `child-1` via `send_message`; `read_turn_transcript` must not replay the old idle line.
    agent.prompt(Seq::new(2), &Prompt::new("reopen")).await.unwrap();
    let _ = collect_turn_result(&mut h).await;
    assert!(
        agent.shared.state.lock().async_agents.has_live_subagents(),
        "reopened subagent must stay live and not be cleared by replaying an already-consumed idle line"
    );
    agent.stop().await;
}
