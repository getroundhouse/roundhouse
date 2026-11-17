#![cfg(test)]
use protocol::attachment::{Attachment, AttachmentFile, MediaType};
use protocol::capability::{
    Control,
    approvals::{AfterDenial, Asks, Decision, Remember},
    asking::PlanContext,
    calls::{
        CallOutput,
        output::{CommandOutput, Ended},
    },
    conversation::Start,
};
use protocol::event::{ContentBlock, EventBody, TurnOutcome};
use protocol::model::{AttachmentId, PermissionMode, Seq};
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::harness::Harness;
use crate::{AgentOutput, AgentProcess};

fn update(payload: &Value) -> Value {
    json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": payload}})
}

fn user_chunk(text: &str) -> Value {
    update(&json!({"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": text}}))
}

fn msg_chunk(text: &str) -> Value {
    update(&json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}))
}

fn thought_chunk(text: &str) -> Value {
    update(&json!({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": text}}))
}

fn tool_event(
    session_update: &str,
    id: &str,
    name: &str,
    status: &str,
    args: &Value,
    raw_output: Option<Value>,
) -> Value {
    let mut body = json!({
        "sessionUpdate": session_update,
        "toolCallId": id,
        "name": name,
        "status": status,
        "args": args,
        "rawInput": args,
    });
    if let Some(out) = raw_output {
        body["rawOutput"] = out;
    }
    update(&body)
}

fn perm_req(rpc_id: &str, call_id: &str, cmd: &str, status: Option<&str>) -> Value {
    let mut call =
        json!({"toolCallId": call_id, "kind": "execute", "name": "run_command", "args": {"CommandLine": cmd}});
    if let Some(status) = status {
        call["status"] = json!(status);
    }
    json!({
        "jsonrpc": "2.0",
        "id": rpc_id,
        "method": "session/request_permission",
        "params": {
            "sessionId": "session-1",
            "toolCall": call,
            "options": [
                {"optionId": "allow_once", "kind": "allow_once", "name": "Allow"},
                {"optionId": "reject_once", "kind": "reject_once", "name": "Reject"}
            ]
        }
    })
}

#[derive(Debug, PartialEq)]
enum Seen {
    Taken(Seq),
    Thought(String),
    Text(String),
    Call(String),
    Result(String, CallOutput),
    Dropped(String),
    PlanAsked(String),
    Ended(TurnOutcome, Option<String>),
}

fn survival_script() -> Value {
    let cmd_args = json!({"CommandLine": "sleep 30", "WaitMsBeforeAsync": 500});
    json!([
        {
            "on": "session/prompt", "at": 1,
            "actions": [
                user_chunk("start bg"),
                tool_event("tool_call", "bg-cmd", "run_command", "in_progress", &cmd_args, None),
                tool_event("tool_call_update", "bg-cmd", "run_command", "pending", &cmd_args, None),
                perm_req("perm-bg", "bg-cmd", "sleep 30", Some("pending")),
            ]
        },
        {
            "on": "", "at": 1,
            "actions": [
                tool_event("tool_call_update", "bg-cmd", "run_command", "in_progress", &cmd_args, None),
                tool_event("tool_call", "bg-sched", "schedule", "in_progress", &json!({"DurationSeconds": 5, "Prompt": "check"}), None),
                tool_event("tool_call", "bg-wf", "run_workflow", "in_progress", &json!({"Description": "fanout"}), None),
                tool_event("tool_call", "fg-read", "read_file", "in_progress", &json!({"Path": "README.md"}), None),
                {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}
            ]
        },
        {
            "on": "session/prompt", "at": 2,
            "actions": [
                user_chunk("unstarted cmd"),
                tool_event("tool_call", "unstarted", "run_command", "in_progress", &json!({"CommandLine": "rm a"}), None),
                perm_req("perm-unstarted", "unstarted", "rm a", None),
            ]
        },
        {"on": "session/cancel", "actions": [{"id": 7, "jsonrpc": "2.0", "result": {"stopReason": "cancelled"}}]},
        {"on": "session/prompt", "at": 3, "actions": [{"error": {"code": -32603, "message": "provider blip"}}]},
        {
            "on": "session/prompt", "at": 4,
            "actions": [
                tool_event("tool_call_update", "bg-cmd", "run_command", "failed", &cmd_args, Some(json!({"output": ""}))),
                tool_event("tool_call_update", "bg-sched", "schedule", "completed", &json!({"DurationSeconds": 5, "Prompt": "check"}), Some(json!("Timer fired"))),
                user_chunk("check results"),
                {"result": {"stopReason": "end_turn"}}
            ]
        }
    ])
}

#[tokio::test]
async fn background_calls_survive_turn_end_cancel_and_async_error_until_settled_or_process_exits() {
    let mut h = Harness::new(&survival_script(), PermissionMode::Plan, Start::New { conversation: None });
    let agent = h.started().await;

    agent.send_prompt(Seq::FIRST, &Prompt::new("start bg")).await.unwrap();
    let allow = Decision::Allow { edited: None, remember: Remember::Once };
    let approval = wait_for_approval(&mut h).await;
    agent.control(&Control::Decide { approval, decision: allow }).await.unwrap();
    let turn1 = collect_until_end(&mut h).await;
    assert_eq!(dropped_ids(&turn1), ["fg-read"], "only non-background in-progress call drops at turn end");

    agent.send_prompt(Seq::new(2), &Prompt::new("unstarted cmd")).await.unwrap();
    let unstarted_approval = wait_for_approval(&mut h).await;
    let deny_stop = Decision::Deny { words: None, then: AfterDenial::Stop, keep: None };
    agent.control(&Control::Decide { approval: unstarted_approval, decision: deny_stop }).await.unwrap();
    let turn2 = collect_until_end(&mut h).await;
    assert_eq!(dropped_ids(&turn2), ["unstarted"], "denied unstarted call drops on cancel while bg calls stay");

    agent.send_prompt(Seq::new(3), &Prompt::new("blip")).await.unwrap();
    let turn3 = collect_until_end(&mut h).await;
    assert!(dropped_ids(&turn3).is_empty(), "async prompt RPC error must keep live bg calls open");

    agent.send_prompt(Seq::new(4), &Prompt::new("check results")).await.unwrap();
    let turn4 = collect_until_end(&mut h).await;
    assert_eq!(
        turn4,
        vec![
            Seen::Result(
                "bg-cmd".into(),
                CallOutput::Command(CommandOutput {
                    output: String::new(),
                    exit_code: None,
                    ended: Ended::Stopped,
                    took_ms: None,
                    truncated: None,
                }),
            ),
            Seen::Result("bg-sched".into(), CallOutput::Words { text: "Timer fired".into() }),
            Seen::Taken(Seq::new(4)),
            Seen::Ended(TurnOutcome::Success, None),
        ]
    );

    agent.stop().await;
    let mut exit_dropped = Vec::new();
    while let Some(output) = h.rx.recv().await {
        if let AgentOutput::Event { body: EventBody::ToolCallDropped { tool_use_id }, .. } = output {
            exit_dropped.push(tool_use_id.to_string());
        }
    }
    assert_eq!(exit_dropped, ["bg-wf"], "process exit drops remaining open background call");
}

async fn wait_for_approval(h: &mut Harness) -> protocol::model::ApprovalId {
    loop {
        if let AgentOutput::Event { body: EventBody::ApprovalRequested { approval_id, .. }, .. } = h.next().await {
            return approval_id;
        }
    }
}

async fn collect_until_end(h: &mut Harness) -> Vec<Seen> {
    let mut out = Vec::new();
    loop {
        if let AgentOutput::Event { body, .. } = h.next().await
            && let Some(seen) = classify(body)
        {
            let done = matches!(seen, Seen::Ended(..) | Seen::PlanAsked(..));
            out.push(seen);
            if done {
                return out;
            }
        }
    }
}

fn dropped_ids(events: &[Seen]) -> Vec<&str> {
    events.iter().filter_map(|e| if let Seen::Dropped(id) = e { Some(id.as_str()) } else { None }).collect()
}

fn classify(body: EventBody) -> Option<Seen> {
    if let EventBody::PromptTaken { prompt } = body {
        Some(Seen::Taken(prompt))
    } else if let EventBody::Assistant { blocks, .. } = body {
        let block = blocks.into_iter().next()?;
        if let ContentBlock::Thinking { text } = block {
            Some(Seen::Thought(text))
        } else if let ContentBlock::Text { text, .. } = block {
            Some(Seen::Text(text))
        } else if let ContentBlock::Call { id, .. } = block {
            Some(Seen::Call(id.to_string()))
        } else {
            None
        }
    } else if let EventBody::AgentUser { blocks, .. } = body {
        let ContentBlock::CallResult { tool_use_id, output, .. } = blocks.into_iter().next()? else { return None };
        Some(Seen::Result(tool_use_id.to_string(), output))
    } else if let EventBody::ToolCallDropped { tool_use_id } = body {
        Some(Seen::Dropped(tool_use_id.to_string()))
    } else if let EventBody::ApprovalRequested { request, .. } = body {
        let Asks::Plan { plan, .. } = request?.asks else { return None };
        Some(Seen::PlanAsked(plan))
    } else if let EventBody::TurnEnded { outcome, result, .. } = body {
        Some(Seen::Ended(outcome, result))
    } else {
        None
    }
}

#[tokio::test]
async fn buffered_wakeup_before_user_message_chunk_stays_separate_from_turn_reply_and_plan_hold() {
    let cmd_args = json!({"CommandLine": "sleep 30", "WaitMsBeforeAsync": 500});
    let script = json!([
        {
            "on": "session/prompt", "at": 1,
            "actions": [
                user_chunk("turn 1"),
                tool_event("tool_call", "bg-1", "run_command", "in_progress", &cmd_args, None),
                {"result": {"stopReason": "end_turn"}}
            ]
        },
        {
            "on": "session/prompt", "at": 2,
            "actions": [
                tool_event("tool_call_update", "bg-1", "run_command", "completed", &cmd_args, Some(json!({"exit_code": 0, "output": "Command exited with code 0.\n\nCommand output:\ndone\n"}))),
                thought_chunk("Wakeup thought."),
                msg_chunk("Wakeup reply."),
                user_chunk("turn 2"),
                thought_chunk("Turn 2 thought."),
                {"result": {"stopReason": "end_turn"}}
            ]
        },
        {
            "on": "session/prompt", "at": 3,
            "actions": [
                msg_chunk("Second wakeup."),
                {"error": {"code": -32603, "message": "blip"}}
            ]
        }
    ]);
    let mut h = Harness::new(&script, PermissionMode::Plan, Start::New { conversation: None });
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("turn 1")).await.unwrap();
    let _ = collect_until_end(&mut h).await;

    agent.send_prompt(Seq::new(2), &Prompt::new("turn 2")).await.unwrap();
    let seen = collect_until_end(&mut h).await;

    agent.send_prompt(Seq::new(3), &Prompt::new("turn 3")).await.unwrap();
    let err_seen = collect_until_end(&mut h).await;
    agent.stop().await;

    assert_eq!(
        seen,
        vec![
            Seen::Result(
                "bg-1".into(),
                CallOutput::Command(CommandOutput {
                    output: "done\n".into(),
                    exit_code: Some(0),
                    ended: Ended::Exited,
                    took_ms: None,
                    truncated: None,
                }),
            ),
            Seen::Thought("Wakeup thought.".into()),
            Seen::Text("Wakeup reply.".into()),
            Seen::Taken(Seq::new(2)),
            Seen::Thought("Turn 2 thought.".into()),
            Seen::Ended(TurnOutcome::Success, None),
        ]
    );
    assert_eq!(err_seen, vec![Seen::Text("Second wakeup.".into()), Seen::Ended(TurnOutcome::Error, None)]);
}

#[tokio::test]
async fn buffered_wakeup_before_approved_plan_continuation_flushes_and_clears_reply() {
    let script = json!([
        {
            "on": "session/prompt", "at": 1,
            "actions": [user_chunk("plan"), msg_chunk("1. Ship it"), {"result": {"stopReason": "end_turn"}}]
        },
        {
            "on": "session/prompt", "at": 2,
            "actions": [
                msg_chunk("Wakeup note."),
                user_chunk("Implement the approved plan."),
                msg_chunk("Implemented."),
                {"result": {"stopReason": "end_turn"}}
            ]
        }
    ]);
    let mut h = Harness::new(&script, PermissionMode::Plan, Start::New { conversation: None });
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("plan")).await.unwrap();
    let plan_id = wait_for_approval(&mut h).await;

    let approve = Decision::ApprovePlan { then: super::super::claims::full_access(), context: PlanContext::Kept };
    agent.control(&Control::Decide { approval: plan_id, decision: approve }).await.unwrap();
    let seen = collect_until_end(&mut h).await;
    agent.stop().await;

    assert_eq!(
        seen,
        vec![
            Seen::Text("Wakeup note.".into()),
            Seen::Text("Implemented.".into()),
            Seen::Ended(TurnOutcome::Success, Some("Implemented.".into())),
        ]
    );
}

#[tokio::test]
async fn multi_block_prompt_with_mid_turn_steer_takes_each_prompt_on_its_own_first_chunk() {
    let script = json!([
        {"on": "session/prompt", "at": 1, "actions": []},
        {
            "on": "session/prompt", "at": 2,
            "actions": [
                user_chunk("attached"),
                user_chunk("prompt 1"),
                msg_chunk("from p1"),
                user_chunk("steer"),
                {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}},
                msg_chunk("from steer"),
                {"id": 7, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}
            ]
        }
    ]);
    let mut h = Harness::new(&script, PermissionMode::FullAccess, Start::New { conversation: None });
    let agent = h.started().await;

    let note_path = h.room.path().join("note.txt");
    std::fs::write(&note_path, "attached").unwrap();
    let attachment = AttachmentFile {
        attachment: Attachment {
            id: AttachmentId::from("att-1"),
            name: "note.txt".into(),
            media_type: MediaType::Text,
            size_bytes: 8,
        },
        path: note_path.to_string_lossy().into_owned(),
    };
    agent
        .send_prompt(Seq::FIRST, &Prompt::<AttachmentFile>::new("prompt 1").with_files(vec![attachment]))
        .await
        .unwrap();
    let _ = h.request("session/prompt").await;
    agent.send_prompt(Seq::new(2), &Prompt::new("steer")).await.unwrap();

    let seen = collect_until_end(&mut h).await;
    agent.stop().await;

    assert_eq!(
        seen,
        vec![
            Seen::Taken(Seq::FIRST),
            Seen::Text("from p1".into()),
            Seen::Taken(Seq::new(2)),
            Seen::Text("from steer".into()),
            Seen::Ended(TurnOutcome::Success, Some("from steer".into())),
        ]
    );
}
