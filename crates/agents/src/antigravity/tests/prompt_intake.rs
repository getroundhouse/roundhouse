#![cfg(test)]
use protocol::capability::conversation::Start;
use protocol::event::{AgentRunState, ContentBlock, EventBody, TurnOutcome};
use protocol::model::Seq;
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::harness::{Harness, msg_chunk, thought_chunk, update};
use crate::testing::Asking;
use crate::{AgentOutput, AgentProcess};

fn tool_event(kind: &str, id: &str, status: &str, out: Option<Value>) -> Value {
    let args = json!({"CommandLine": "echo hi", "toolSummary": "Echo", "WaitMsBeforeAsync": 5000});
    let mut body = json!({"sessionUpdate": kind, "toolCallId": id, "name": "run_command", "status": status, "args": args, "rawInput": args});
    if let Some(raw_output) = out {
        body["rawOutput"] = raw_output;
    }
    update(&body)
}

#[derive(Debug, PartialEq, Eq)]
enum Order {
    Taken(Seq),
    Running,
    Delta,
    Thought(String),
    Text(String),
    Call(String),
    Result(String),
    Ended(TurnOutcome, Option<String>),
}

async fn collect_order(h: &mut Harness) -> Vec<Order> {
    let mut seen = Vec::new();
    loop {
        match h.next().await {
            AgentOutput::Delta(_) => seen.push(Order::Delta),
            AgentOutput::Event { body: EventBody::PromptTaken { prompt }, .. } => seen.push(Order::Taken(prompt)),
            AgentOutput::Event { body: EventBody::AgentState { state: AgentRunState::Running }, .. } => {
                seen.push(Order::Running);
            }
            AgentOutput::Event { body: EventBody::Assistant { blocks, .. }, .. } => match blocks.into_iter().next() {
                Some(ContentBlock::Thinking { text }) => seen.push(Order::Thought(text)),
                Some(ContentBlock::Text { text, .. }) => seen.push(Order::Text(text)),
                Some(ContentBlock::Call { id, .. }) => seen.push(Order::Call(id.to_string())),
                Some(ContentBlock::CallResult { .. } | ContentBlock::Image { .. } | ContentBlock::Unknown) | None => {}
            },
            AgentOutput::Event { body: EventBody::AgentUser { blocks, .. }, .. } => {
                if let Some(ContentBlock::CallResult { tool_use_id, .. }) = blocks.into_iter().next() {
                    seen.push(Order::Result(tool_use_id.to_string()));
                }
            }
            AgentOutput::Event { body: EventBody::TurnEnded { outcome, result, .. }, .. } => {
                seen.push(Order::Ended(outcome, result));
                return seen;
            }
            AgentOutput::Event { .. }
            | AgentOutput::Retrying(_)
            | AgentOutput::Report(_)
            | AgentOutput::Exited { .. } => {}
        }
    }
}

#[tokio::test]
async fn plain_turn_without_user_message_chunk_takes_prompt_before_assistant_output() {
    let script = json!([{
        "on": "session/prompt", "at": 1,
        "actions": [
            thought_chunk("Thinking."),
            tool_event("tool_call", "c-1", "in_progress", None),
            tool_event("tool_call_update", "c-1", "completed", Some(json!({"exit_code": 0, "output": "hi\n"}))),
            msg_chunk("Done."),
            {"result": {"stopReason": "end_turn"}}
        ]
    }]);
    let mut h = Harness::new(&script, super::super::claims::full_access(), Start::New { conversation: None });
    let agent = h.started().await;

    agent.prompt(Seq::FIRST, &Prompt::new("say hi")).await.unwrap();
    let seen = collect_order(&mut h).await;
    agent.stop().await;

    assert_eq!(seen[0], Order::Taken(Seq::FIRST));
    assert_eq!(seen[1], Order::Running);
    assert_eq!(seen[2], Order::Delta);
    assert!(seen.contains(&Order::Thought("Thinking.".into())));
    assert!(seen.contains(&Order::Call("c-1".into())));
    assert!(seen.contains(&Order::Result("c-1".into())));
    assert!(seen.contains(&Order::Text("Done.".into())));
    assert_eq!(seen.last(), Some(&Order::Ended(TurnOutcome::Success, Some("Done.".into()))));
}

#[tokio::test]
async fn mid_turn_steer_without_user_message_chunk_takes_steer_after_prior_tool_completes() {
    let script = json!([
        {
            "on": "session/prompt", "at": 1,
            "actions": [tool_event("tool_call", "c-1", "in_progress", None)]
        },
        {
            "on": "session/prompt", "at": 2,
            "actions": [
                {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}},
                tool_event("tool_call_update", "c-1", "completed", Some(json!({"exit_code": 0, "output": "hi\n"}))),
                msg_chunk("Steered."),
                {"id": 7, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}
            ]
        }
    ]);
    let mut h = Harness::new(&script, super::super::claims::full_access(), Start::New { conversation: None });
    let agent = h.started().await;

    agent.prompt(Seq::FIRST, &Prompt::new("start")).await.unwrap();
    let mut seen = Vec::new();
    loop {
        if let AgentOutput::Event { body, .. } = h.next().await {
            if let EventBody::PromptTaken { prompt } = body {
                seen.push(Order::Taken(prompt));
            } else if matches!(body, EventBody::AgentState { state: AgentRunState::Running }) {
                seen.push(Order::Running);
            } else if let EventBody::Assistant { blocks, .. } = body
                && let Some(ContentBlock::Call { id, .. }) = blocks.into_iter().next()
            {
                seen.push(Order::Call(id.to_string()));
                break;
            }
        }
    }

    agent.prompt(Seq::new(2), &Prompt::new("steer")).await.unwrap();
    seen.extend(collect_order(&mut h).await);
    agent.stop().await;

    let taken_1 = seen.iter().position(|e| *e == Order::Taken(Seq::FIRST)).unwrap();
    let call_1 = seen.iter().position(|e| *e == Order::Call("c-1".into())).unwrap();
    let result_1 = seen.iter().position(|e| *e == Order::Result("c-1".into())).unwrap();
    let taken_2 = seen.iter().position(|e| *e == Order::Taken(Seq::new(2))).unwrap();
    let text_2 = seen.iter().position(|e| *e == Order::Text("Steered.".into())).unwrap();
    assert!(taken_1 < call_1 && call_1 < result_1 && result_1 < taken_2 && taken_2 < text_2, "{seen:?}");
    assert_eq!(seen.last(), Some(&Order::Ended(TurnOutcome::Success, Some("Steered.".into()))));
}

#[test]
fn drain_cancels_when_idle_and_stays_open_while_background_tasks_or_subagents_remain() {
    use super::super::state::{Conversation, InFlightPrompt, OpenCall, PromptOrigin, UserChunk};
    use protocol::capability::{calls::Call, settings};
    use protocol::model::{AgentSessionId, ModelName, ToolUseId};

    let drain_prompt = |rpc_id| InFlightPrompt {
        rpc_id,
        origin: PromptOrigin::WakeupDrain { keep_open: false, started_own_turn: false, self_cancelled: false },
        user_chunks_left: 1,
        taken: false,
    };
    let mut conv = Conversation::new(
        ModelName::named("gemini-4-carbon"),
        super::super::claims::full_access(),
        settings::Effort::Medium,
        true,
        true,
    );
    conv.session_id = Some(AgentSessionId::from("s-1"));

    conv.prompts.push_back(drain_prompt(1));
    assert_eq!(conv.accept_user_chunk(), UserChunk::CancelDrain { session_id: AgentSessionId::from("s-1") });
    conv.prompts.clear();

    let cmd = Call::Other { tool: "run_command".into(), input: Value::Null };
    conv.calls.insert(ToolUseId::from("bg-2"), OpenCall { call: cmd, order: 1, outlives_turn: true });
    conv.prompts.push_back(drain_prompt(2));
    assert_eq!(conv.accept_user_chunk(), UserChunk::ContinueDrain);
    conv.calls.clear();
    conv.prompts.clear();

    let subagent_wire = crate::acp::ToolCallWire {
        tool_call_id: "sub-call".into(),
        status: Some(crate::acp::ToolStatusWire::Completed),
        name: Some("invoke_subagent".into()),
        args: Some(json!({ "Prompt": "work" })),
        ..crate::acp::ToolCallWire::default()
    };
    conv.async_agents.on_tool_completed(&super::super::calls::translate_call(&subagent_wire), &subagent_wire);
    conv.async_agents.on_turn_settled();
    conv.prompts.push_back(drain_prompt(3));
    assert_eq!(conv.accept_user_chunk(), UserChunk::ContinueDrain);
    conv.prompts.clear();

    conv.async_agents.on_transcript_step(&json!({
        "type": "SYSTEM_MESSAGE",
        "content": "Subagent child-1 has gone idle."
    }));
    conv.prompts.push_back(drain_prompt(4));
    assert_eq!(conv.accept_user_chunk(), UserChunk::CancelDrain { session_id: AgentSessionId::from("s-1") });
    conv.prompts.clear();

    conv.async_agents.on_tool_completed(&super::super::calls::translate_call(&subagent_wire), &subagent_wire);
    let _ = conv.take_replaced_child_tasks();
    conv.prompts.push_back(drain_prompt(5));
    assert_eq!(conv.accept_user_chunk(), UserChunk::CancelDrain { session_id: AgentSessionId::from("s-1") });
}

#[tokio::test]
async fn prompt_sent_during_recording_waits_and_runs_pending_mode_swap_before_prompting() {
    use std::time::Duration;

    use super::super::state::Ending;

    let script = json!([{
        "on": "session/prompt", "at": 1,
        "actions": [msg_chunk("In plan mode."), {"result": {"stopReason": "end_turn"}}]
    }]);
    let mut h = Harness::new(&script, super::super::claims::full_access(), Start::New { conversation: None });
    let agent = h.started().await;
    let first_pid = agent.pid();
    {
        let mut state = agent.shared.state.lock();
        state.mode = super::super::claims::plan();
        state.ending = Ending::Recording;
    }
    let watching = agent.shared.turn_done.receiver_count();

    let sending = tokio::spawn({
        let agent = agent.clone();
        async move { agent.prompt(Seq::FIRST, &Prompt::new("plan now")).await }
    });
    waiting::task::until(Duration::from_secs(5), "prompt should wait on Ending::Recording", || {
        sending.is_finished() || agent.shared.turn_done.receiver_count() != watching
    })
    .await;
    assert!(!sending.is_finished(), "send_prompt_rpc must wait out Ending::Recording before ensure_mode_process");
    agent.shared.count_end(&mut agent.shared.state.lock(), Ending::Not);
    tokio::time::timeout(Duration::from_secs(5), sending).await.unwrap().unwrap().unwrap();

    assert_ne!(agent.pid(), first_pid, "pending mode swap must run before sending the prompt");
    agent.stop().await;
}
