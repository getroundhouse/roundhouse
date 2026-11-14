use protocol::capability::{Capabilities, Control, conversation::Start};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::AgentKind;
use protocol::worker::{AgentControl, AgentSpec, WorkerRequest};
use serde_json::{Value, json};

#[test]
fn a_chosen_conversation_starts_without_a_placeholder_and_resumes_the_reported_id() {
    let wire = json!({"session_id":"s","run":4,"agent":"codex","cwd":"/w","start":"new",
        "model":"gpt-6-luna","permission_mode":"bypassPermissions","effort":"low","env":[]});
    let mut spec: AgentSpec = serde_json::from_value(wire).unwrap();

    assert_eq!(spec.agent, AgentKind::Codex);
    assert_eq!(spec.start, Start::New { conversation: None });
    let sent = serde_json::to_value(&spec).unwrap();
    assert!(sent.get("agent_session_id").is_none());
    assert_eq!(serde_json::from_value::<AgentSpec>(sent).unwrap(), spec);

    spec.start = Start::Resume { conversation: "chosen-by-cli".into() };
    let sent = serde_json::to_value(&spec).unwrap();
    assert_eq!(sent["agent_session_id"], "chosen-by-cli");
    assert_eq!(serde_json::from_value::<AgentSpec>(sent).unwrap(), spec);
}

#[test]
fn legacy_initialization_has_no_claims_and_new_claims_survive_the_log_shape() {
    let old: EventBody = serde_json::from_value(json!({"type":"agent_init","agent_session_id":"a"})).unwrap();
    let EventBody::AgentInit { capabilities: None, .. } = old else { panic!("{old:?}") };
    let new: EventBody = serde_json::from_value(json!({"type":"agent_init","agent_session_id":"a",
        "capabilities":{"prompts":{"steer":{}}}}))
    .unwrap();
    let EventBody::AgentInit { capabilities: Some(ref claims), .. } = new else { panic!("{new:?}") };
    assert!(claims.prompts.steer.is_some());
    assert_eq!(serde_json::from_value::<EventBody>(serde_json::to_value(&new).unwrap()).unwrap(), new);
}

#[test]
fn a_control_has_a_new_envelope_and_keeps_its_capability_requirements() {
    let request = WorkerRequest::AgentControl(AgentControl { session_id: "s".into(), control: Control::Cancel });
    let sent = serde_json::to_value(&request).unwrap();

    assert_eq!(sent, json!({"method":"agent_control","session_id":"s","control":"cancel"}));
    assert_eq!(serde_json::from_value::<WorkerRequest>(sent).unwrap(), request);
    assert!(!Control::Cancel.allowed(&Capabilities::default()));
}

#[test]
fn current_calls_keep_typed_input_once_and_old_calls_stay_readable() {
    let call: ContentBlock = serde_json::from_value(json!({"type":"call","id":"c","call":
        {"kind":"write","path":"a.txt","content":"only once"}}))
    .unwrap();
    let sent = serde_json::to_string(&call).unwrap();

    assert_eq!(sent.matches("only once").count(), 1);
    assert_eq!(serde_json::from_str::<ContentBlock>(&sent).unwrap(), call);
    let old = json!({"type":"tool_use","id":"c","name":"Write","input":{"file_path":"a.txt","content":"old"}});
    let decoded: ContentBlock = serde_json::from_value(old).unwrap();
    assert!(matches!(decoded, ContentBlock::ToolUse { .. }));
    let unknown: ContentBlock = serde_json::from_value(json!({"type":"future_block","input":Value::Null})).unwrap();
    assert_eq!(unknown, ContentBlock::Unknown);
}

/// Older clients know the event variant but require its legacy fields, even when a typed adapter has no native input.
#[test]
fn typed_approvals_keep_the_fields_an_older_reader_requires() {
    #[derive(serde::Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum BeforeTypedRequests {
        ApprovalRequested { tool_name: String, input: Value },
    }

    let current: EventBody = serde_json::from_value(json!({
        "type": "approval_requested", "approval_id": "public",
        "request": {"asks": {"asks": "plan", "plan": "Read the file"}},
        "tool_name": "", "input": null
    }))
    .unwrap();
    let sent = serde_json::to_value(&current).unwrap();
    let old: BeforeTypedRequests = serde_json::from_value(sent.clone()).unwrap();
    let BeforeTypedRequests::ApprovalRequested { tool_name, input } = old;
    assert_eq!(tool_name, "");
    assert!(input.is_null());
    assert_eq!(sent.get("input"), Some(&Value::Null));
    assert_eq!(serde_json::from_value::<EventBody>(sent).unwrap(), current);
}
