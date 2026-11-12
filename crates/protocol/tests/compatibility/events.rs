//! Events a hub kept in its log in the form an older hub wrote them, which every later hub and client has to read.

use protocol::event::EventBody;

/// A hub stores events in the form it sends, so an agent's start recorded before hubs said which build it ran on has to
/// read as saying nothing of it, which a chat takes as not having been updated.
#[test]
fn an_agent_s_start_from_before_builds_were_recorded_says_nothing_of_them() {
    let before: EventBody = serde_json::from_str(r#"{"type":"agent_started","pid":7,"resumed":true}"#).unwrap();
    let EventBody::AgentStarted { pid: 7, resumed: true, build: None, previous_build: None } = before else {
        panic!("{before:?}");
    };
    let updated = EventBody::AgentStarted {
        pid: 7,
        resumed: true,
        build: Some("1.0.0".into()),
        previous_build: Some("0.9.0".into()),
    };
    assert_eq!(
        serde_json::to_string(&updated).unwrap(),
        r#"{"type":"agent_started","pid":7,"resumed":true,"build":"1.0.0","previous_build":"0.9.0"}"#
    );
}
