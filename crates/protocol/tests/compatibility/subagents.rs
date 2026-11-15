//! How what names a subagent is written, which stays as it was stored, whatever the field is called in Rust.

use protocol::event::EventBody;
use protocol::model::{RecordKey, SessionId, Subagent, SubagentId};
use protocol::tail::{StreamDelta, Tail};

/// Each of the agent's events that can be a subagent's names it under the name it was stored with, from when a
/// subagent was named by the call that started it, so the logs hubs have kept still say whose each event is.
#[test]
fn an_event_names_its_subagent_as_it_was_stored() {
    for stored in [
        r#"{"type":"assistant","message_id":null,"parent_tool_use_id":"toolu_1","model":null,"blocks":[]}"#,
        r#"{"type":"agent_user","parent_tool_use_id":"toolu_1","blocks":[]}"#,
        r#"{"type":"agent_other","kind":"user/synthetic","parent_tool_use_id":"toolu_1"}"#,
        r#"{"type":"notice","level":"info","message":"Hi","parent_tool_use_id":"toolu_1"}"#,
    ] {
        let event: EventBody = serde_json::from_str(stored).unwrap();

        assert_eq!(event.subagent(), Some(&SubagentId::from("toolu_1")), "{stored}");
        assert_eq!(serde_json::to_string(&event).unwrap(), stored);
    }
}

/// A subagent's background task names its subagent, and one from before tasks named theirs says only its call.
#[test]
fn a_subagents_task_names_it_or_only_its_call() {
    let named = r#"{"id":"t1","description":null,"kind":"subagent","call":"toolu_2","subagent":"task_1"}"#;
    let before = r#"{"id":"t1","description":null,"kind":"subagent","call":"toolu_1"}"#;
    for (sent, subagent) in [(named, Some("task_1")), (before, None)] {
        let task: protocol::event::AgentTask = serde_json::from_str(sent).unwrap();

        assert_eq!(task.subagent.as_ref().map(SubagentId::as_str), subagent, "{sent}");
        assert_eq!(serde_json::to_string(&task).unwrap(), sent);
    }
}

/// What a subagent is writing, and which subagents work in the background, name it as the tail was first sent, so a
/// peer from before reads whose each message is.
#[test]
fn what_a_subagent_is_writing_names_it_as_it_was_sent() {
    let sent = r#"{"messages":[{"message_id":"m","parent_tool_use_id":"toolu_1","blocks":[]}],"in_background":[{"call":"toolu_1"}]}"#;
    let delta = r#"{"message_id":"m","parent_tool_use_id":"toolu_1","change":{"type":"message_stop"}}"#;

    let tail: Tail = serde_json::from_str(sent).unwrap();
    let streamed: StreamDelta = serde_json::from_str(delta).unwrap();

    let subagent = Some(SubagentId::from("toolu_1"));
    assert_eq!((&tail.messages[0].subagent, Some(&tail.in_background[0].subagent)), (&subagent, subagent.as_ref()));
    assert_eq!(serde_json::to_string(&tail).unwrap(), sent);
    assert_eq!(streamed.subagent, subagent);
    assert_eq!(serde_json::to_string(&streamed).unwrap(), delta);
}

/// A subagent's start names it by the ID its agent gives it, and one stored before subagents had IDs of their own names
/// only its call, which is then its ID, so the logs hubs have kept still say which subagent each started.
#[test]
fn a_subagent_starts_by_its_own_id_or_by_its_call_as_it_was_stored() {
    let start = |subagent: &str| {
        format!(
            r#"{{"type":"subagent_started","call":"toolu_1",{subagent}"kind":null,"description":null,"prompt":null,"in_background":true}}"#
        )
    };
    for (stored, named) in [(start(r#""subagent":"task_1","#), "task_1"), (start(""), "toolu_1")] {
        let event: EventBody = serde_json::from_str(&stored).unwrap();

        assert_eq!(event.subagent_started(), Some(SubagentId::from(named)), "{stored}");
        assert_eq!(serde_json::to_string(&event).unwrap(), stored);
    }
}

/// A subagent's record says its ID beside its call, and one from a hub from before subagents had IDs of their own says
/// only its call, which is then its ID.
#[test]
fn a_subagents_record_names_it_by_its_id_or_by_its_call() {
    let sent = |id: &str| format!(r#"{{"session_id":"s",{id}"call":"toolu_1","status":"working","started_at_ms":1}}"#);
    for (record, id) in [(sent(r#""id":"task_1","#), "task_1"), (sent(""), "toolu_1")] {
        let subagent: Subagent = serde_json::from_str(&record).unwrap();

        assert_eq!(subagent.id(), SubagentId::from(id), "{record}");
        let written = serde_json::to_value(&subagent).unwrap();
        assert_eq!(written.get("id").and_then(|id| id.as_str()), subagent.id.as_ref().map(SubagentId::as_str));
    }
}

/// A request for approval says which subagent asks, and one recorded before adapters said reads as the agent's own.
#[test]
fn a_request_names_the_subagent_that_asks() {
    let asked = |by: &str| {
        format!(r#"{{"type":"approval_requested","approval_id":"a",{by}"tool_use_id":null,"description":null}}"#)
    };
    for (stored, by) in [(asked(r#""asked_by":"task_1","#), Some("task_1")), (asked(""), None)] {
        let event: EventBody = serde_json::from_str(&stored).unwrap();
        let EventBody::ApprovalRequested { asked_by, .. } = &event else { panic!("{stored}") };

        assert_eq!(asked_by.as_ref().map(SubagentId::as_str), by, "{stored}");
        let written = serde_json::to_value(&event).unwrap();
        assert_eq!(written.get("asked_by").and_then(|by| by.as_str()), by);
    }
}

/// Which record a subagent's is names it as `call`, as it did when a subagent was named by the call that started it,
/// so a peer from then reads it.
#[test]
fn a_subagents_record_key_names_it_as_it_was_first_sent() {
    let key = RecordKey::Subagent { session_id: SessionId::from("s"), subagent: SubagentId::from("task_1") };
    let sent = r#"{"kind":"subagent","session_id":"s","call":"task_1"}"#;

    assert_eq!(serde_json::to_string(&key).unwrap(), sent);
    assert_eq!(serde_json::from_str::<RecordKey>(sent).unwrap(), key);
}
