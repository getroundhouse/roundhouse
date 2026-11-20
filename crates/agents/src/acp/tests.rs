use protocol::capability::asking::QuestionAnswer;
use protocol::capability::asking::{Choose, TypedAnswer};
use protocol::event::{TokenUsage, TurnOutcome};
use serde_json::json;

use super::{
    SessionUpdate, StopReason, ToolKindWire, ToolStatusWire, config_offers_value, elicitation_accept,
    parse_agent_version, parse_elicitation, parse_permission_request, parse_prompt_response, parse_session_response,
    parse_session_update, prompt_blocks,
};

#[test]
fn initialize_response_parses_non_empty_agent_version() {
    assert_eq!(
        parse_agent_version(&json!({ "agentInfo": { "name": "Antigravity ACP Server", "version": "1.5.12" } })),
        Some("1.5.12".into())
    );
    assert_eq!(parse_agent_version(&json!({ "agentInfo": { "name": "Antigravity", "version": "" } })), None);
    assert_eq!(parse_agent_version(&json!({})), None);
}

#[test]
fn session_response_and_config_options_parse_models_and_efforts() {
    let raw = json!({
        "sessionId": "11111111-2222-4333-8444-555555555555",
        "configOptions": [
            {
                "id": "model",
                "category": "model",
                "currentValue": "gemini-4-carbon",
                "options": [
                    { "value": "gemini-4-carbon", "name": "Gemini 4 Carbon", "description": "Balanced" },
                    { "value": "gemini-3.8-flash", "name": "Gemini 3.8 Flash" }
                ]
            },
            {
                "id": "thought_level",
                "category": "thought_level",
                "currentValue": "medium",
                "options": [
                    { "value": "low", "name": "Low" },
                    { "value": "medium", "name": "Medium" },
                    { "value": "xhigh", "name": "Extra High" }
                ]
            }
        ]
    });
    let parsed = parse_session_response(&raw).unwrap();
    assert!(config_offers_value(&parsed.config_options, "thought_level", "xhigh"));
    assert!(!config_offers_value(&parsed.config_options, "thought_level", "ultra"));
    assert_eq!(parsed.config_options[0].current_value.as_deref(), Some("gemini-4-carbon"));
    assert_eq!(parsed.config_options[0].options[0].name, "Gemini 4 Carbon");
    assert_eq!(parsed.config_options[0].options[0].description.as_deref(), Some("Balanced"));
}

#[test]
fn session_updates_and_prompt_response_translate_usage_and_chunks() {
    let thought = parse_session_update(&json!({
        "sessionId": "s",
        "update": { "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "Thinking." } }
    }));
    assert_eq!(thought, SessionUpdate::AgentThoughtChunk { text: "Thinking.".into() });

    let usage = parse_session_update(&json!({
        "sessionId": "s",
        "update": { "sessionUpdate": "usage_update", "used": 180, "size": 1_048_576 }
    }));
    assert_eq!(usage, SessionUpdate::UsageUpdate { used: 180, size: 1_048_576 });

    let tool = parse_session_update(&json!({
        "sessionId": "s",
        "update": {
            "sessionUpdate": "tool_call",
            "toolCallId": "c1",
            "status": "in_progress",
            "kind": "execute",
            "name": "run_command",
            "args": { "CommandLine": "cargo test" },
            "locations": [{ "path": "/work/Cargo.toml" }]
        }
    }));
    let SessionUpdate::ToolCall(wire) = tool else { panic!("expected tool_call") };
    assert_eq!(wire.status, Some(ToolStatusWire::InProgress));
    assert_eq!(wire.kind, Some(ToolKindWire::Execute));
    assert_eq!(wire.name.as_deref(), Some("run_command"));
    assert_eq!(wire.locations, vec!["/work/Cargo.toml"]);

    let prompt = parse_prompt_response(&json!({
        "stopReason": "end_turn",
        "usage": {
            "inputTokens": 120,
            "outputTokens": 45,
            "cachedReadTokens": 300,
            "cachedWriteTokens": 10,
            "thoughtTokens": 15,
            "totalTokens": 475
        }
    }));
    assert_eq!(prompt.stop_reason, StopReason::EndTurn);
    assert_eq!(prompt.stop_reason.outcome(), TurnOutcome::Success);
    assert_eq!(
        prompt.tokens,
        Some(TokenUsage { input_tokens: 120, output_tokens: 45, cache_read_tokens: 300, cache_write_tokens: 10 })
    );
}

#[test]
fn permission_options_resolve_once_always_and_reject() {
    let perm = parse_permission_request(&json!({
        "sessionId": "s",
        "toolCall": { "toolCallId": "w1", "kind": "edit", "name": "write_file" },
        "options": [
            { "optionId": "allow_always", "kind": "allow_always", "name": "Always Allow" },
            { "optionId": "allow", "kind": "allow_once", "name": "Allow" },
            { "optionId": "reject", "kind": "reject_once", "name": "Deny" }
        ]
    }))
    .unwrap();
    assert_eq!(perm.option_for(false, true), Some("allow"));
    assert_eq!(perm.option_for(true, true), Some("allow_always"));
    assert_eq!(perm.option_for(false, false), Some("reject"));
}

#[test]
fn elicitation_requests_round_trip_single_multi_and_other_answers() {
    let elicitation = parse_elicitation(&json!({
        "mode": "form",
        "message": "Pick options",
        "requestedSchema": {
            "type": "object",
            "properties": {
                "q1": {
                    "type": "string",
                    "description": "Which fruit?",
                    "oneOf": [
                        { "const": "1", "title": "Apple" },
                        { "const": "2", "title": "Banana" }
                    ]
                },
                "q1_other": { "type": "string" },
                "q2": {
                    "type": "array",
                    "description": "Toppings?",
                    "items": {
                        "type": "string",
                        "anyOf": [
                            { "const": "1", "title": "Nuts" },
                            { "const": "2", "title": "Sprinkles" },
                            { "const": "3", "title": "Caramel" }
                        ]
                    }
                }
            }
        }
    }))
    .unwrap();
    assert_eq!(elicitation.questions.len(), 2);
    assert_eq!(elicitation.questions[0].choose, Choose::One);
    assert_eq!(elicitation.questions[0].typed, TypedAnswer::Yes);
    assert_eq!(elicitation.questions[1].choose, Choose::Several);
    assert_eq!(elicitation.questions[1].typed, TypedAnswer::No);

    let accepted = elicitation_accept(
        &elicitation.fields,
        &[
            QuestionAnswer { picked: vec![1], typed: Some("Mango".into()) },
            QuestionAnswer { picked: vec![0, 2], typed: None },
        ],
    );
    assert_eq!(
        accepted,
        json!({
            "action": "accept",
            "content": {
                "q1": "2",
                "q1_other": "Mango",
                "q2": ["1", "3"]
            }
        })
    );
}

#[test]
fn empty_elicitation_answers_decline_rather_than_sending_empty_required_fields() {
    let elicitation = parse_elicitation(&json!({
        "mode": "form",
        "requestedSchema": {
            "type": "object",
            "properties": {
                "q1": {
                    "type": "string",
                    "description": "Pick one",
                    "oneOf": [{ "const": "1", "title": "Yes" }]
                }
            }
        }
    }))
    .unwrap();
    assert_eq!(elicitation_accept(&elicitation.fields, &[]), json!({ "action": "decline" }));
    assert_eq!(
        elicitation_accept(&elicitation.fields, &[QuestionAnswer { picked: Vec::new(), typed: None }]),
        json!({ "action": "decline" })
    );
}

#[test]
fn a_spoken_prompt_has_how_to_talk_in_a_block_before_it_and_a_typed_one_doesnt() {
    let texts = |prompt: &protocol::prompt::Prompt<_>| -> Vec<String> {
        prompt_blocks(prompt).iter().map(|block| block["text"].as_str().unwrap().to_string()).collect()
    };

    let spoken = texts(&protocol::prompt::Prompt::new("is it done?").said_out_loud());

    assert!(spoken[0].starts_with("(Roundhouse: the person said this out loud"), "{spoken:?}");
    assert_eq!(spoken[1..], ["is it done?"]);
    assert_eq!(texts(&protocol::prompt::Prompt::new("is it done?")), ["is it done?"]);
}
