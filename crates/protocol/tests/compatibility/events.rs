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

/// A hub stores what an agent says its subagent's conversation holds as this and under this kind, so both have to stay
/// as they are once one is recorded. A subagent's end recorded before ends counted says nothing of it, and one that
/// counts none is stored as those were.
#[test]
fn what_a_subagents_conversation_holds_keeps_the_form_and_the_kind_its_stored_under() {
    let stored = r#"{"type":"subagent_context","call":"toolu_1","tokens":37386}"#;
    let before = r#"{"type":"subagent_ended","call":"toolu_1","outcome":"finished","report":"Done."}"#;

    let holds: EventBody = serde_json::from_str(stored).unwrap();
    let ended: EventBody = serde_json::from_str(before).unwrap();

    assert_eq!(holds, EventBody::SubagentContext { call: "toolu_1".into(), tokens: 37_386 });
    assert_eq!(holds.kind(), "subagent_context");
    assert_eq!(serde_json::to_string(&holds).unwrap(), stored);
    let EventBody::SubagentEnded { context_tokens: None, .. } = &ended else { panic!("{ended:?}") };
    assert_eq!(serde_json::to_string(&ended).unwrap(), before);
    let counted = before.replace('}', r#","context_tokens":41020}"#);
    let EventBody::SubagentEnded { context_tokens: Some(41_020), .. } = serde_json::from_str(&counted).unwrap() else {
        panic!("{counted}");
    };
}
