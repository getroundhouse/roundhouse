#![cfg(test)]
use protocol::capability::asking::QuestionAnswer;
use protocol::capability::{
    Control,
    approvals::{AfterDenial, Asks, Decision},
    asking::PlanContext,
    calls::{CallOutput, output::Ended},
    conversation::Start,
};
use protocol::event::{ApprovalOutcome, ContentBlock, Denied, EventBody, TurnOutcome};
use protocol::model::{PermissionMode, Seq, ToolUseId};
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::harness::Harness;
use crate::{AgentOutput, AgentProcess};

fn permission_request(id: &str, call_id: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/request_permission",
        "params": {
            "sessionId": "session-1",
            "toolCall": {
                "toolCallId": call_id,
                "title": "Run command",
                "kind": "execute",
                "rawInput": {"CommandLine": "rm -f marker"}
            },
            "options": [
                {"optionId": "allow_once", "kind": "allow_once", "name": "Allow once"},
                {"optionId": "reject_once", "kind": "reject_once", "name": "Reject"}
            ]
        }
    })
}

fn command_tool_update(call_id: &str, status: &str, raw_output: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "session-1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": call_id,
                "kind": "execute",
                "status": status,
                "rawInput": {"CommandLine": "rm -f marker"},
                "rawOutput": raw_output
            }
        }
    })
}

fn elicitation_fixture() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": "elic-1",
        "method": "elicitation/create",
        "params": {
            "mode": "form",
            "message": "Clarify options",
            "requestedSchema": {
                "type": "object",
                "properties": {
                    "question_0": {
                        "type": "string",
                        "description": "Which target?",
                        "oneOf": [{"const": "1", "title": "1. Mac"}, {"const": "2", "title": "2. Linux"}]
                    },
                    "question_1": {
                        "type": "array",
                        "description": "Which extras?",
                        "items": {"anyOf": [{"const": "1", "title": "1. Docs"}, {"const": "2", "title": "2. Tests"}]}
                    },
                    "question_1_other": {"type": "string", "description": "Other answer"}
                }
            }
        }
    })
}

#[tokio::test]
async fn full_access_auto_allows_permission_requests_and_parses_command_envelope() {
    let script = json!([
        {"on": "session/prompt", "actions": [permission_request("perm-1", "call-1")]},
        {
            "on": "",
            "actions": [
                command_tool_update("call-1", "completed", "Command exited with code 0.\n\nCommand output:\ndone\n"),
                {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}
            ]
        }
    ]);
    let mut full = Harness::new(&script, PermissionMode::FullAccess, Start::New { conversation: None });
    let agent = full.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("clean up")).await.unwrap();
    let mut selected = None;
    let mut command_out = None;
    loop {
        if let AgentOutput::Event { body, raw, .. } = full.next().await {
            assert!(!matches!(body, EventBody::ApprovalRequested { .. }), "FullAccess must not ask");
            if let EventBody::AgentOther { kind, .. } = &body
                && kind == "test/request"
                && let Some(raw) = &raw
                && let Ok(val) = serde_json::from_str::<Value>(raw)
                && val["params"]["id"] == "perm-1"
                && val["params"].get("method").is_none()
            {
                selected = val["params"]["result"]["outcome"]["optionId"].as_str().map(str::to_owned);
            }
            if let EventBody::AgentUser { blocks, .. } = &body
                && let [ContentBlock::CallResult { output: CallOutput::Command(out), .. }] = blocks.as_slice()
            {
                command_out = Some(out.clone());
            }
            if matches!(body, EventBody::TurnEnded { outcome: TurnOutcome::Success, .. }) {
                break;
            }
        }
    }
    assert_eq!(selected.as_deref(), Some("allow_once"));
    let out = command_out.expect("expected command output");
    assert_eq!(out.output, "done\n");
    assert_eq!(out.exit_code, Some(0));
    assert_eq!(out.ended, Ended::Exited);
    agent.stop().await;
}

#[tokio::test]
async fn plan_mode_asks_permission_and_marks_denied_call_results() {
    let script = json!([
        {"on": "session/prompt", "actions": [permission_request("perm-2", "call-denied")]},
        {
            "on": "session/cancel",
            "actions": [
                command_tool_update("call-denied", "failed", "Permission rejected by user"),
                {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "cancelled"}}
            ]
        }
    ]);
    let mut plan = Harness::new(&script, PermissionMode::Plan, Start::New { conversation: None });
    let agent = plan.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("plan it")).await.unwrap();
    let approval_id = loop {
        if let AgentOutput::Event {
            body: EventBody::ApprovalRequested { approval_id, request, tool_use_id, .. }, ..
        } = plan.next().await
        {
            assert!(matches!(request.as_deref().map(|r| &r.asks), Some(Asks::Call { .. })));
            assert_eq!(tool_use_id, Some(ToolUseId::from("call-denied")), "the request names the call it would make");
            break approval_id;
        }
    };
    let deny = Decision::Deny { words: None, then: AfterDenial::Stop, keep: None };
    agent.control(&Control::Decide { approval: approval_id, decision: deny }).await.unwrap();

    let mut saw_denied = false;
    loop {
        if let AgentOutput::Event { body, .. } = plan.next().await {
            if let EventBody::AgentUser { blocks, .. } = &body
                && let [ContentBlock::CallResult { is_error, denied, .. }] = blocks.as_slice()
            {
                assert!(*is_error);
                assert_eq!(*denied, Some(Denied { said: None }));
                saw_denied = true;
            }
            if matches!(body, EventBody::TurnEnded { outcome: TurnOutcome::Cancelled, .. }) {
                break;
            }
        }
    }
    assert!(saw_denied, "rejected call result must carry Denied");
    agent.stop().await;
}

#[tokio::test]
async fn plan_mode_holds_only_post_tool_plan_text_and_preserves_tokens_when_cancelled() {
    let read_update = json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "session-1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "read-1",
                "kind": "read",
                "name": "read_file",
                "args": {"Path": "README.md"},
                "status": "completed",
                "content": [{"type": "content", "content": {"type": "text", "text": "Showing lines 1-1 of 1\n1: hello"}}]
            }
        }
    });
    let script = json!([{
        "on": "session/prompt",
        "actions": [
            {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Let me inspect first."}}}},
            read_update,
            {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "1. Update README.md"}}}},
            {"result": {"stopReason": "end_turn", "usage": {"inputTokens": 80, "outputTokens": 20, "thoughtTokens": 5, "cachedReadTokens": 0, "cachedWriteTokens": 0, "totalTokens": 100}}}
        ]
    }]);
    let mut h = Harness::new(&script, PermissionMode::Plan, Start::New { conversation: None });
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("make a plan")).await.unwrap();
    let held_id = loop {
        if let AgentOutput::Event { body: EventBody::ApprovalRequested { approval_id, request, .. }, .. } =
            h.next().await
        {
            let Some(req) = request else { panic!("expected plan request") };
            assert_eq!(req.asks, Asks::Plan { plan: "1. Update README.md".into(), file: None });
            break approval_id;
        }
    };
    agent.interrupt().await.unwrap();
    let mut resolved = false;
    let tokens = loop {
        if let AgentOutput::Event { body, .. } = h.next().await {
            if let EventBody::ApprovalResolved { approval_id, outcome: ApprovalOutcome::Cancelled, .. } = &body
                && *approval_id == held_id
            {
                resolved = true;
            }
            if let EventBody::TurnEnded { outcome: TurnOutcome::Cancelled, tokens, .. } = body {
                break tokens;
            }
        }
    };
    assert!(resolved);
    let usage = tokens.expect("held plan tokens should report on cancel");
    assert_eq!(usage.input_tokens, 80);
    assert_eq!(usage.output_tokens, 20);
    agent.stop().await;
}

#[tokio::test]
async fn an_approved_plans_turn_counts_what_the_plan_took_with_what_carrying_it_out_did() {
    let said = |text: &str| json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}}});
    let ended = |input: u64, output: u64| json!({"result": {"stopReason": "end_turn", "usage": {"inputTokens": input, "outputTokens": output, "cachedReadTokens": 4, "cachedWriteTokens": 0}}});
    let script = json!([
        {"on": "session/prompt", "at": 1, "actions": [said("1. Update README.md"), ended(80, 20)]},
        {"on": "session/prompt", "at": 2, "actions": [said("Done."), ended(200, 30)]}
    ]);
    let mut h = Harness::new(&script, PermissionMode::Plan, Start::New { conversation: None });
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("make a plan")).await.unwrap();
    let plan = loop {
        if let AgentOutput::Event { body: EventBody::ApprovalRequested { approval_id, .. }, .. } = h.next().await {
            break approval_id;
        }
    };

    let approve = Decision::ApprovePlan { then: super::super::claims::full_access(), context: PlanContext::Kept };
    agent.control(&Control::Decide { approval: plan, decision: approve }).await.unwrap();

    let (outcome, result, tokens) = loop {
        if let AgentOutput::Event { body: EventBody::TurnEnded { outcome, result, tokens, .. }, .. } = h.next().await {
            break (outcome, result, tokens);
        }
    };
    assert_eq!((outcome, result.as_deref()), (TurnOutcome::Success, Some("Done.")));
    let tokens = tokens.expect("both prompts said what they took");
    assert_eq!((tokens.input_tokens, tokens.output_tokens, tokens.cache_read_tokens), (280, 50, 8));
    let launches: Vec<Value> = std::fs::read_to_string(h.room.path().join("native-requests.jsonl"))
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|entry| entry.get("launch_args").is_some())
        .map(|entry| entry["launch_args"].clone())
        .collect();
    assert_eq!(
        launches,
        vec![
            json!(["acp", "--disable-telemetry"]),
            json!(["acp", "--disable-telemetry", "--dangerously-skip-permissions"]),
        ]
    );
    agent.stop().await;
}

#[tokio::test]
async fn mode_switches_swap_skip_permissions_flag_stop_background_tasks_and_keep_active_turn_mode() {
    let bg_call = json!({
        "jsonrpc": "2.0", "method": "session/update",
        "params": {"sessionId": "session-1", "update": {"sessionUpdate": "tool_call", "toolCallId": "bg-swap", "name": "run_command", "status": "in_progress", "args": {"CommandLine": "sleep 30", "WaitMsBeforeAsync": 500}}}
    });
    let reply = |text: &str| json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}}});
    let script = json!([
        {"on": "session/prompt", "at": 1, "actions": [bg_call, {"result": {"stopReason": "end_turn"}}]},
        {"on": "session/prompt", "at": 2, "actions": [reply("1. Plan step")]},
        {"on": "session/set_config_option", "at": 5, "actions": [{"result": {}}, {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}]}
    ]);
    let mut h = Harness::new(&script, PermissionMode::FullAccess, Start::New { conversation: None });
    let agent = h.started().await;
    let first_pid = agent.pid();
    agent.send_prompt(Seq::FIRST, &Prompt::new("start bg")).await.unwrap();
    while !matches!(h.next().await, AgentOutput::Event { body: EventBody::TurnEnded { .. }, .. }) {}

    let to_plan =
        protocol::model::SessionSettingsChange { permission_mode: Some(PermissionMode::Plan), ..Default::default() };
    agent.configure(&to_plan).await.unwrap();
    assert_ne!(agent.pid(), first_pid);
    let (mut saw_dropped, mut saw_stopped) = (false, false);
    loop {
        if let AgentOutput::Event { body, .. } = h.next().await {
            saw_dropped |=
                matches!(&body, EventBody::ToolCallDropped { tool_use_id } if tool_use_id.as_str() == "bg-swap");
            saw_stopped |=
                matches!(&body, EventBody::AgentTasks { tasks, stopped } if tasks.is_empty() && stopped.len() == 1);
            if matches!(body, EventBody::AgentMode { permission_mode: PermissionMode::Plan }) {
                break;
            }
        }
    }
    assert!(saw_dropped && saw_stopped);

    agent.send_prompt(Seq::new(2), &Prompt::new("write plan")).await.unwrap();
    while !matches!(h.next().await, AgentOutput::Delta(_)) {}
    let mid = protocol::model::SessionSettingsChange {
        effort: Some(protocol::model::Effort::Low),
        permission_mode: Some(PermissionMode::FullAccess),
        model: None,
    };
    agent.configure(&mid).await.unwrap();
    let held = loop {
        if let AgentOutput::Event { body: EventBody::ApprovalRequested { request: Some(req), .. }, .. } = h.next().await
        {
            break req.asks;
        }
    };
    assert_eq!(held, Asks::Plan { plan: "1. Plan step".into(), file: None });
    agent.stop().await;
}

#[tokio::test]
async fn elicitation_questions_round_trip_answers() {
    let mut h = Harness::new(
        &json!([{"on": "session/prompt", "actions": [elicitation_fixture()]}]),
        PermissionMode::FullAccess,
        Start::New { conversation: None },
    );
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("ask me")).await.unwrap();
    let approval_id = loop {
        if let AgentOutput::Event { body: EventBody::ApprovalRequested { approval_id, request, .. }, .. } =
            h.next().await
        {
            let Some(req) = request else { panic!("expected typed request") };
            let Asks::Questions { questions } = &req.asks else { panic!("expected questions") };
            assert_eq!(questions.len(), 2);
            break approval_id;
        }
    };
    let decision = Decision::Answer {
        answers: vec![
            QuestionAnswer { picked: vec![1], typed: None },
            QuestionAnswer { picked: vec![0, 1], typed: Some("Benchmarks".into()) },
        ],
    };
    agent.control(&Control::Decide { approval: approval_id, decision }).await.unwrap();
    let reply = h.reply_to(&json!("elic-1")).await;
    assert_eq!(reply["result"]["action"], "accept");
    assert_eq!(reply["result"]["content"]["question_0"], "2");
    assert_eq!(reply["result"]["content"]["question_1"], json!(["1", "2"]));
    assert_eq!(reply["result"]["content"]["question_1_other"], "Benchmarks");
    agent.stop().await;
}
