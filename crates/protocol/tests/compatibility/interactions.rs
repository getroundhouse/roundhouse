//! How the person is talking with a chat's agent, as its record says and each event was recorded under, which a hub
//! keeps in the form it sends.

use protocol::event::SessionEvent;
use protocol::model::{ClientDevice, Interaction, InteractionMode, Session};

const RECORD: &str = r#"{"id":"s","workspace_id":"w","agent":"claude","model":"opus","mode":{"unasked":"everything","beyond":"asks"},"effort":"high","status":"idle","last_seq":4,"created_at_ms":1,"updated_at_ms":2"#;

fn record(more: &str) -> String {
    format!("{RECORD}{more}}}")
}

/// Conversation mode keeps its form on the record, with its device. Typing, as nearly every chat is, is left out, as a
/// hub from before says, and a mode this build doesn't know reads as one, rather than failing the whole record.
#[test]
fn a_records_interaction_keeps_its_form_and_typing_is_left_out() {
    let form = r#"{"mode":"conversation","device":{"id":"d1","name":"Mac Studio"}}"#;
    let session: Session = serde_json::from_str(&record(&format!(r#","interaction":{form}"#))).unwrap();
    let device = ClientDevice { id: "d1".into(), name: "Mac Studio".into() };
    assert_eq!(session.interaction, Interaction::Conversation { device: Box::new(device) });
    assert_eq!(serde_json::to_string(&session.interaction).unwrap(), form);
    assert!(serde_json::to_string(&session).unwrap().contains(&format!(r#""interaction":{form}"#)));

    let typing: Session = serde_json::from_str(&record("")).unwrap();
    assert_eq!(typing.interaction, Interaction::Typing);
    assert!(!serde_json::to_string(&typing).unwrap().contains("interaction"));

    let newer: Session = serde_json::from_str(&record(r#","interaction":{"mode":"meeting","room":"r"}"#)).unwrap();
    assert_eq!(newer.interaction, Interaction::Unknown);
}

/// Each event says the mode it was recorded under, and typing is left out, as every event from before says. A mode this
/// build doesn't know reads as one.
#[test]
fn an_events_mode_keeps_its_form_and_typing_is_left_out() {
    let event = |more: &str| {
        format!(
            r#"{{"session_id":"s","seq":4,"ts_ms":1,"place":3{more},"body":{{"type":"tool_call_dropped","tool_use_id":"toolu_1"}}}}"#
        )
    };
    let said = event(r#","interaction":"conversation""#);
    let spoken: SessionEvent = serde_json::from_str(&said).unwrap();
    assert_eq!(spoken.interaction, InteractionMode::Conversation);
    assert_eq!(serde_json::to_string(&spoken).unwrap(), said);

    let typed = event("");
    let before: SessionEvent = serde_json::from_str(&typed).unwrap();
    assert_eq!(before.interaction, InteractionMode::Typing);
    assert_eq!(serde_json::to_string(&before).unwrap(), typed);

    let newer: SessionEvent = serde_json::from_str(&event(r#","interaction":"meeting""#)).unwrap();
    assert_eq!(newer.interaction, InteractionMode::Unknown);
}
