//! How the session's newest turn ended, which its record says with its reply.

use protocol::event::{EventBody, TurnOutcome};
use protocol::model::SessionStatus::{Idle, Running};

use super::{change, session};
use crate::store::tests::{notice, store_with_session};
use crate::store::{TurnEdge, older, schema};

#[test]
fn the_reply_says_how_its_turn_ended_until_the_next_turn_ends() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = session(&store).rev;

    change(&store, Running, TurnEdge::Starts);
    let failed = change(&store, Idle, TurnEdge::Ends(Some(TurnOutcome::Error)));

    let ended = session(&store);
    assert_eq!((ended.reply_seq, ended.reply_outcome), (failed.seq, Some(TurnOutcome::Error)));
    assert!(ended.rev > before);
    assert!(ended.turn_failed());
    change(&store, Running, TurnEdge::Starts);
    let started = session(&store);
    assert_eq!(started.reply_outcome, Some(TurnOutcome::Error), "the reply is still the failed turn's");
    assert!(!started.turn_failed(), "but a turn has started since");
    change(&store, Idle, TurnEdge::Ends(None));
    assert_eq!(session(&store).reply_outcome, None, "a turn that ended without saying how");
}

/// A session from before the record said is filled in from the event its reply is, and only one whose reply is a
/// turn's end moves, so a client takes it.
#[test]
fn a_session_from_before_says_how_its_reply_ended_from_its_log() {
    let dir = tempfile::tempdir().unwrap();
    let ended = protocol::fixtures::turn_ended;
    let session = |id: &str, reply_seq: u64| {
        format!(
            "INSERT INTO sessions (id, workspace_id, agent, model, permission_mode, effort, agent_session_id, status, \
             last_seq, created_at_ms, updated_at_ms, reply_seq) \
             VALUES ('{id}', 'w', 'claude', 'm', 'bypassPermissions', 'high', '{id}', 'idle', 2, 0, 0, {reply_seq});"
        )
    };
    let event = |id: &str, seq: u64, body: &EventBody| {
        format!(
            "INSERT INTO events (session_id, seq, ts_ms, kind, place, body) VALUES ('{id}', {seq}, 0, '{}', {seq}, '{}');",
            body.kind(),
            serde_json::to_string(body).unwrap()
        )
    };
    let rows = [
        session("failed", 2),
        event("failed", 1, &ended(TurnOutcome::Success)),
        event("failed", 2, &ended(TurnOutcome::Error)),
        session("done", 1),
        event("done", 1, &ended(TurnOutcome::Success)),
        session("said-idle", 1),
        event("said-idle", 1, &notice("idle")),
        session("new", 0),
    ]
    .concat();

    let store = older::store_from_before(dir.path(), schema::BEFORE_REPLY_OUTCOME, &rows);

    let read = |id: &str| store.get_session(&id.into()).unwrap().session;
    assert_eq!(read("failed").reply_outcome, Some(TurnOutcome::Error));
    assert_eq!(read("done").reply_outcome, Some(TurnOutcome::Success));
    assert_eq!((read("said-idle").reply_outcome, read("new").reply_outcome), (None, None));
    let unfilled = read("said-idle").rev.max(read("new").rev);
    assert!(read("failed").rev > unfilled && read("done").rev > unfilled, "only those filled in move");
}
