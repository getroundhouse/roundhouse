//! How each reply said out loud ended: what the device that read it out says of it, the list a client reads of a
//! session's, and the notice that says one changed, which every peer from now on reads as they were first written.

use protocol::client::conversation::{SessionReplyHeard, SessionSpeaking};
use protocol::client::{ClientRequest, HubNotice};
use protocol::model::{Ending, RecordKey, ReplyHeard, ReplyPlace, Rev, SettledBy, SpokenThrough};
use protocol::tail::Tail;

use super::keeps_its_form;

const PLACE: ReplyPlace = ReplyPlace { block: 2, through: 140 };

fn heard(ending: Ending) -> ReplyHeard {
    ReplyHeard {
        session_id: "s".into(),
        message: "msg_1".into(),
        ending,
        settled_by: SettledBy::Device,
        rev: Rev::new(7),
    }
}

/// Each ending keeps its form, and one this build doesn't know reads as one, rather than failing the whole record.
#[test]
fn each_ending_keeps_its_form() {
    let place = r#""through":{"block":2,"through":140}"#;
    keeps_its_form(&Ending::ToTheEnd, r#"{"how":"to_the_end"}"#);
    keeps_its_form(&Ending::TalkedOver { through: PLACE }, &format!(r#"{{"how":"talked_over",{place}}}"#));
    keeps_its_form(&Ending::Stopped { through: PLACE }, &format!(r#"{{"how":"stopped",{place}}}"#));
    keeps_its_form(&Ending::NotReadOut, r#"{"how":"not_read_out"}"#);
    let newer: Ending = serde_json::from_str(r#"{"how":"faded","after_ms":300}"#).unwrap();
    assert_eq!(newer, Ending::Unknown);
}

#[test]
fn what_a_device_says_of_a_reply_and_the_list_of_a_sessions_keep_their_form() {
    let said =
        SessionReplyHeard { id: "s".into(), message: "msg_1".into(), ending: Ending::Stopped { through: PLACE } };
    keeps_its_form(
        &ClientRequest::SessionReplyHeard(said),
        r#"{"method":"session_reply_heard","id":"s","message":"msg_1","ending":{"how":"stopped","through":{"block":2,"through":140}}}"#,
    );
}

/// The feed's notice, and the key a record of one is held by, which no removal names yet.
#[test]
fn the_notice_of_a_reply_heard_keeps_its_form() {
    keeps_its_form(
        &HubNotice::ReplyHeard(heard(Ending::NotReadOut)),
        r#"{"notice":"reply_heard","session_id":"s","message":"msg_1","ending":{"how":"not_read_out"},"rev":7}"#,
    );
    keeps_its_form(
        &RecordKey::ReplyHeard { session_id: "s".into(), message: "msg_1".into() },
        r#"{"kind":"reply_heard","session_id":"s","message":"msg_1"}"#,
    );
}

/// What the hub settled says so, and what the device said leaves it out, as every record from before it was said does.
/// One this build doesn't know reads as one.
#[test]
fn who_settled_a_reply_keeps_its_form_and_the_device_is_left_out() {
    let settled = ReplyHeard { settled_by: SettledBy::Hub, ..heard(Ending::NotReadOut) };
    keeps_its_form(
        &settled,
        r#"{"session_id":"s","message":"msg_1","ending":{"how":"not_read_out"},"settled_by":"hub","rev":7}"#,
    );
    let newer: ReplyHeard = serde_json::from_str(
        r#"{"session_id":"s","message":"msg_1","ending":{"how":"to_the_end"},"settled_by":"relay","rev":7}"#,
    )
    .unwrap();
    assert_eq!(newer.settled_by, SettledBy::Unknown);
}

/// How far a reply has been read out, as a device says it and as the tail carries it.
#[test]
fn how_far_a_reply_has_been_read_out_keeps_its_form() {
    let through = SpokenThrough { message: "msg_1".into(), through: PLACE };
    keeps_its_form(
        &ClientRequest::SessionSpeaking(SessionSpeaking { id: "s".into(), through: through.clone() }),
        r#"{"method":"session_speaking","id":"s","through":{"message":"msg_1","through":{"block":2,"through":140}}}"#,
    );
    keeps_its_form(
        &Tail { speaking: Some(through), ..Tail::default() },
        r#"{"messages":[],"speaking":{"message":"msg_1","through":{"block":2,"through":140}}}"#,
    );
}
