use protocol::model::SessionStatus::{Idle, Running, WaitingApproval};
use protocol::model::{Session, SessionStatus};

use super::*;
use crate::store::tests::{notice, store_with_session};

/// Appends an event that changes the session's status.
fn change(store: &Store, status: SessionStatus, turn: TurnEdge) -> SessionEvent {
    let change = Some(StatusChange { status, turn, background_tasks: None }.into());
    store.append(&"s".into(), None, &notice("x"), None, None, change).unwrap().event
}

fn session(store: &Store) -> Session {
    store.get_session(&"s".into()).unwrap().session
}

#[test]
fn a_turn_starts_when_the_event_that_starts_it_was_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert_eq!(session(&store).turn_started_at_ms, None, "a new session hasn't had a turn");

    let prompt = change(&store, Running, TurnEdge::Starts);
    change(&store, WaitingApproval, TurnEdge::Neither);
    change(&store, Running, TurnEdge::Neither);
    change(&store, Idle, TurnEdge::Ends);

    assert_eq!(session(&store).turn_started_at_ms, Some(prompt.ts_ms), "it stays the newest turn's once it's over");
}

#[test]
fn the_next_turn_moves_it_and_its_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    change(&store, Running, TurnEdge::Starts);
    change(&store, Idle, TurnEdge::Ends);
    // Earlier than any clock could say, so the next turn can't land on the same moment.
    lock(&store.writer).execute("UPDATE sessions SET turn_started_at_ms = 1", []).unwrap();
    let before = session(&store);

    let prompt = change(&store, Running, TurnEdge::Starts);

    let after = session(&store);
    assert_eq!(after.turn_started_at_ms, Some(prompt.ts_ms));
    assert!(after.rev > before.rev, "a client only takes a newer copy");
}
