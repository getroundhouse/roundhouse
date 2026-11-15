//! How what names a subagent is written, which stays as it was stored, whatever the field is called in Rust.

use protocol::event::EventBody;
use protocol::model::SubagentId;
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
