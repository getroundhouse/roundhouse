//! How each reply said out loud ended: what the device that read it out says of it, the list a client reads of a
//! session's, and the notice that says one changed, which every peer from now on reads as they were first written.

use protocol::client::conversation::SessionReplyHeard;
use protocol::client::records::{RepliesHeard, SessionRepliesHeard};
use protocol::client::{ClientRequest, ClientResponse, HubNotice};
use protocol::model::{Ending, RecordKey, ReplyHeard, ReplyPlace, Rev};

use super::keeps_its_form;

const PLACE: ReplyPlace = ReplyPlace { block: 2, through: 140 };

fn heard(ending: Ending) -> ReplyHeard {
    ReplyHeard { session_id: "s".into(), message: "msg_1".into(), ending, rev: Rev::new(7) }
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
    keeps_its_form(
        &ClientRequest::SessionRepliesHeard(SessionRepliesHeard { id: "s".into() }),
        r#"{"method":"session_replies_heard","id":"s"}"#,
    );
    keeps_its_form(
        &ClientResponse::RepliesHeard(RepliesHeard { replies: vec![heard(Ending::ToTheEnd)], rev: Rev::new(9) }),
        r#"{"result":"replies_heard","replies":[{"session_id":"s","message":"msg_1","ending":{"how":"to_the_end"},"rev":7}],"rev":9}"#,
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
