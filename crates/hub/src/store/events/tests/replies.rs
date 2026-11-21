//! How the session's newest turn ended, which its record says with its reply.

use protocol::event::TurnOutcome;
use protocol::model::SessionStatus::{Idle, Running};

use super::{change, session};
use crate::store::TurnEdge;
use crate::store::tests::store_with_session;

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
