#![cfg(test)]
use protocol::capability::{
    Control,
    conversation::Start,
    settings::{Effort, Settings},
};
use protocol::event::{AgentRunState, EventBody, NoticeLevel, TurnOutcome};
use protocol::model::{AgentSessionId, PermissionMode, Seq};
use protocol::prompt::Prompt;
use serde_json::json;

use super::harness::Harness;
use crate::antigravity::state::Ending;
use crate::{AgentOutput, AgentProcess};

#[tokio::test]
async fn async_error_on_last_prompt_drops_it_once_ends_turn_with_error_and_keeps_process_alive() {
    let mut h = Harness::new(
        &json!([
            {
                "on": "session/prompt",
                "at": 1,
                "actions": [{"error": {"code": -32603, "message": "upstream quota exceeded"}}]
            },
            {
                "on": "session/prompt",
                "at": 2,
                "actions": [
                    {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": {"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": "retry"}}}},
                    {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "recovered"}}}},
                    {"result": {"stopReason": "end_turn"}}
                ]
            }
        ]),
        PermissionMode::FullAccess,
        Start::New { conversation: None },
    );
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("first")).await.unwrap();

    let mut dropped = Vec::new();
    let mut errors = Vec::new();
    loop {
        if let AgentOutput::Event { body, .. } = h.next().await {
            if let EventBody::PromptDropped { prompt } = body {
                dropped.push(prompt);
            } else if let EventBody::TurnEnded { outcome: TurnOutcome::Error, errors: e, .. } = body {
                errors = e;
            } else if matches!(body, EventBody::AgentState { state: AgentRunState::Idle }) && !errors.is_empty() {
                break;
            }
        }
    }
    assert_eq!(dropped, [Seq::FIRST]);
    assert!(errors.iter().any(|e| e.contains("upstream quota exceeded")), "{errors:?}");

    agent.send_prompt(Seq::new(2), &Prompt::new("retry")).await.unwrap();
    loop {
        if let AgentOutput::Event { body: EventBody::TurnEnded { outcome, result, .. }, .. } = h.next().await {
            assert_eq!(outcome, TurnOutcome::Success);
            assert_eq!(result.as_deref(), Some("recovered"));
            break;
        }
    }
    agent.stop().await;
}

#[tokio::test]
async fn mid_turn_steer_superseded_ok_completes_once_after_steer_finishes() {
    let mut h = Harness::new(
        &json!([
            {
                "on": "session/prompt",
                "at": 1,
                "actions": [
                    {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": {"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": "first"}}}}
                ]
            },
            {
                "on": "session/prompt",
                "at": 2,
                "actions": [
                    {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}},
                    {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": {"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": "steer"}}}},
                    {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "steered reply"}}}},
                    {"id": 7, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}
                ]
            }
        ]),
        PermissionMode::FullAccess,
        Start::New { conversation: None },
    );
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("first")).await.unwrap();
    let mut taken = Vec::new();
    loop {
        if let AgentOutput::Event { body: EventBody::PromptTaken { prompt }, .. } = h.next().await {
            taken.push(prompt);
            break;
        }
    }
    agent.send_prompt(Seq::new(2), &Prompt::new("steer")).await.unwrap();

    let mut endings = Vec::new();
    let mut dropped = Vec::new();
    loop {
        if let AgentOutput::Event { body, .. } = h.next().await {
            if let EventBody::PromptTaken { prompt } = body {
                taken.push(prompt);
            } else if let EventBody::PromptDropped { prompt } = body {
                dropped.push(prompt);
            } else if let EventBody::TurnEnded { outcome, result, .. } = body {
                endings.push((outcome, result));
                break;
            }
        }
    }
    agent.stop().await;
    while let Some(output) = h.rx.recv().await {
        if let AgentOutput::Event { body: EventBody::TurnEnded { outcome, result, .. }, .. } = output {
            endings.push((outcome, result));
        }
    }
    assert_eq!(taken, [Seq::FIRST, Seq::new(2)]);
    assert!(dropped.is_empty());
    assert_eq!(endings, vec![(TurnOutcome::Success, Some("steered reply".into()))]);
}

#[tokio::test]
async fn cancelling_two_untaken_prompts_drops_both_and_marks_neither_taken() {
    let mut h = Harness::new(
        &json!([
            {"on": "session/prompt", "at": 1, "actions": []},
            {"on": "session/prompt", "at": 2, "actions": []},
            {
                "on": "session/cancel",
                "actions": [
                    {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}},
                    {"id": 7, "jsonrpc": "2.0", "result": {"stopReason": "cancelled"}}
                ]
            }
        ]),
        PermissionMode::FullAccess,
        Start::New { conversation: None },
    );
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("first")).await.unwrap();
    agent.send_prompt(Seq::new(2), &Prompt::new("second")).await.unwrap();
    let _ = h.request("session/prompt").await;
    let _ = h.request("session/prompt").await;
    agent.interrupt().await.unwrap();

    let mut taken = Vec::new();
    let mut dropped = Vec::new();
    loop {
        if let AgentOutput::Event { body, .. } = h.next().await {
            if let EventBody::PromptTaken { prompt } = body {
                taken.push(prompt);
            } else if let EventBody::PromptDropped { prompt } = body {
                dropped.push(prompt);
            } else if matches!(body, EventBody::TurnEnded { outcome: TurnOutcome::Cancelled, .. }) {
                break;
            }
        }
    }
    agent.stop().await;
    assert!(taken.is_empty(), "untaken cancelled prompts must not be marked taken: {taken:?}");
    assert_eq!(dropped, [Seq::FIRST, Seq::new(2)]);
}

#[tokio::test]
async fn mid_turn_steer_error_drops_only_the_steer_and_keeps_original_turn_alive() {
    let mut h = Harness::new(
        &json!([
            {
                "on": "session/prompt",
                "at": 1,
                "actions": [
                    {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": {"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": "first"}}}}
                ]
            },
            {
                "on": "session/prompt",
                "at": 2,
                "actions": [
                    {"id": 7, "jsonrpc": "2.0", "error": {"code": -32602, "message": "invalid steer payload"}},
                    {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "session-1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "original finished"}}}},
                    {"id": 6, "jsonrpc": "2.0", "result": {"stopReason": "end_turn"}}
                ]
            }
        ]),
        PermissionMode::FullAccess,
        Start::New { conversation: None },
    );
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("first")).await.unwrap();
    loop {
        if matches!(h.next().await, AgentOutput::Event { body: EventBody::PromptTaken { prompt }, .. } if prompt == Seq::FIRST)
        {
            break;
        }
    }
    agent.send_prompt(Seq::new(2), &Prompt::new("steer")).await.unwrap();

    let mut dropped = Vec::new();
    let mut notices = Vec::new();
    let final_end = loop {
        if let AgentOutput::Event { body, .. } = h.next().await {
            if let EventBody::PromptDropped { prompt } = body {
                dropped.push(prompt);
            } else if let EventBody::Notice { level: NoticeLevel::Error, message, .. } = body {
                notices.push(message);
            } else if let EventBody::TurnEnded { outcome, result, .. } = body {
                break (outcome, result);
            }
        }
    };
    agent.stop().await;
    assert_eq!(dropped, [Seq::new(2)]);
    assert!(notices.iter().any(|m| m.contains("invalid steer payload")), "{notices:?}");
    assert_eq!(final_end, (TurnOutcome::Success, Some("original finished".into())));
}

#[tokio::test]
async fn stale_config_option_update_from_set_mode_preserves_model_and_effort_choices() {
    let mut h = Harness::new(
        &json!([
            {
                "on": "session/set_config_option",
                "at": 1,
                "actions": [
                    {
                        "result": {
                            "configOptions": [
                                {"id": "model", "currentValue": "gemini-4-carbon", "options": [{"value": "gemini-4-carbon", "name": "Gemini 4 Carbon"}]},
                                {"id": "thought_level", "currentValue": "high", "options": [{"value": "low", "name": "Low"}, {"value": "medium", "name": "Medium"}, {"value": "high", "name": "High"}]}
                            ]
                        }
                    },
                    {
                        "jsonrpc": "2.0",
                        "method": "session/update",
                        "params": {
                            "sessionId": "session-1",
                            "update": {
                                "sessionUpdate": "config_option_update",
                                "configOptions": [
                                    {"id": "model", "currentValue": "gemini-3.1-pro", "options": [{"value": "gemini-3.1-pro", "name": "Gemini 3.1 Pro"}]},
                                    {"id": "thought_level", "currentValue": "low", "options": [{"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]}
                                ]
                            }
                        }
                    }
                ]
            }
        ]),
        PermissionMode::Plan,
        Start::New { conversation: None },
    );
    let agent = h.started().await;
    let init_model = loop {
        if let AgentOutput::Event { body: EventBody::AgentInit { model, .. }, .. } = h.next().await {
            break model;
        }
    };
    assert_eq!(init_model.as_deref(), Some("gemini-4-carbon"));
    agent
        .control(&Control::Change(Settings { effort: Some(Effort::Medium), ..Settings::default() }))
        .await
        .expect("stale gemini-3.1-pro config_option_update must not hide medium effort on gemini-4-carbon");
    agent.stop().await;
}

/// What `agy` 1.5.9 answers a resume it can't read, whether the conversation's file is gone, unreadable or corrupt.
fn failed_to_read(conversation: &str) -> serde_json::Value {
    json!({"code": -32603, "message": format!("Failed to read session {conversation}; retry, or load it with session/load")})
}

#[tokio::test]
async fn resuming_a_conversation_whose_file_is_gone_falls_back_to_session_new() {
    let mut h = Harness::new(
        &json!([
            {"on": "session/resume", "actions": [{"error": failed_to_read("missing-id")}]},
            {"on": "session/new", "actions": [{"result": {"sessionId": "fresh-session", "configOptions": []}}]}
        ]),
        PermissionMode::FullAccess,
        Start::Resume { conversation: AgentSessionId::from("missing-id") },
    );
    let agent = h.started().await;
    let resume = h.request("session/resume").await;
    assert_eq!(resume["params"]["sessionId"], "missing-id");
    let fresh = h.request("session/new").await;
    assert_eq!(fresh["params"]["_meta"]["antigravity"]["systemPrompt"]["append"], "Roundhouse instructions");
    loop {
        if let AgentOutput::Event { body: EventBody::AgentInit { agent_session_id, .. }, .. } = h.next().await {
            assert_eq!(agent_session_id, Some(AgentSessionId::from("fresh-session")));
            break;
        }
    }
    agent.stop().await;
}

#[tokio::test]
async fn a_conversation_agy_cant_read_but_still_has_is_kept_rather_than_started_again() {
    let mut h = Harness::keeping(
        &json!([
            {"on": "session/resume", "actions": [{"error": failed_to_read("kept-id")}]},
            {"on": "session/new", "actions": [{"result": {"sessionId": "fresh-session", "configOptions": []}}]}
        ]),
        PermissionMode::FullAccess,
        Start::Resume { conversation: AgentSessionId::from("kept-id") },
        "kept-id",
    );

    let refused = h.refused().await;

    assert!(matches!(refused, crate::AgentError::Failed(_)), "{refused:?}");
    let asked = std::fs::read_to_string(h.room.path().join("native-requests.jsonl")).unwrap();
    assert!(asked.contains("session/resume") && !asked.contains("session/new"), "{asked}");
}

#[tokio::test]
async fn a_resume_that_fails_for_another_reason_fails_the_launch_even_with_the_conversations_file_gone() {
    let mut h = Harness::new(
        &json!([
            {"on": "session/resume", "actions": [{"error": {"code": -32603, "message": "Model provider unavailable"}}]},
            {"on": "session/new", "actions": [{"result": {"sessionId": "fresh-session", "configOptions": []}}]}
        ]),
        PermissionMode::FullAccess,
        Start::Resume { conversation: AgentSessionId::from("gone-id") },
    );

    let refused = h.refused().await;

    assert!(matches!(refused, crate::AgentError::Failed(_)), "{refused:?}");
    let asked = std::fs::read_to_string(h.room.path().join("native-requests.jsonl")).unwrap();
    assert!(asked.contains("session/resume") && !asked.contains("session/new"), "{asked}");
}

#[tokio::test]
async fn an_interrupt_while_the_turn_is_recorded_as_ended_waits_for_it_rather_than_ending_it_again() {
    let mut h = Harness::new(&json!([]), PermissionMode::FullAccess, Start::New { conversation: None });
    let agent = h.started().await;
    // Where the reader leaves a turn whose last prompt has finished, while its end is recorded. Nothing in the adapter
    // can be held there, so the test puts it there.
    agent.shared.state.lock().ending = Ending::Recording;
    let watching = agent.shared.turn_done.receiver_count();

    let interrupting = tokio::spawn({
        let agent = agent.clone();
        async move { agent.interrupt().await }
    });
    // Until the interrupt waits on the count of ends, or has finished without waiting.
    let failure = "the interrupt neither waited on the count of ends nor finished";
    waiting::task::until(std::time::Duration::from_secs(10), failure, || {
        interrupting.is_finished() || agent.shared.turn_done.receiver_count() != watching
    })
    .await;
    assert!(!interrupting.is_finished(), "the interrupt waits for the end");
    agent.shared.count_end(&mut agent.shared.state.lock(), Ending::Not);
    let interrupted = tokio::time::timeout(std::time::Duration::from_secs(10), interrupting).await;
    interrupted.expect("the interrupt never finished once the turn's end was counted").unwrap().unwrap();

    while let Ok(output) = h.rx.try_recv() {
        if let AgentOutput::Event { body: EventBody::TurnEnded { outcome, .. }, .. } = output {
            panic!("the interrupt ended the turn again, as {outcome:?}");
        }
    }
    let asked = std::fs::read_to_string(h.room.path().join("native-requests.jsonl")).unwrap();
    assert!(!asked.contains("session/cancel"), "{asked}");
    agent.stop().await;
}
