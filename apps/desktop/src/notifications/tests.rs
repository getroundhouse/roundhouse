use protocol::event::TurnOutcome;
use protocol::model::{Seq, Session, SessionId, SessionStatus};

use super::{Changes, Watch, Why};

fn chat(id: &str, status: SessionStatus, reply_seq: u64, read_seq: u64) -> Session {
    Session { status, reply_seq: Seq::new(reply_seq), read_seq: Seq::new(read_seq), ..protocol::fixtures::session(id) }
}

fn working(id: &str) -> Session {
    chat(id, SessionStatus::Running, 0, 0)
}

/// What came, as each chat's ID and why.
fn came(changes: &Changes) -> Vec<(&str, Why)> {
    changes.came.iter().map(|came| (came.chat.as_str(), came.why)).collect()
}

/// A watch that has seen these, as the app has once it's open.
fn having_seen(sessions: &[Session]) -> Watch {
    let mut watch = Watch::default();
    watch.look(sessions, None);
    watch
}

#[test]
fn the_first_look_tells_of_nothing_already_waiting() {
    let mut watch = Watch::default();

    let changes = watch.look(
        &[
            chat("a", SessionStatus::Idle, 5, 0),
            chat("b", SessionStatus::WaitingApproval, 0, 0),
            chat("c", SessionStatus::Interrupted, 0, 0),
        ],
        None,
    );

    assert_eq!(changes, Changes::default());
}

#[test]
fn a_reply_nobody_has_read_is_told_of_once() {
    let mut watch = having_seen(&[working("a")]);
    let replied = chat("a", SessionStatus::Idle, 4, 0);

    let first = watch.look(std::slice::from_ref(&replied), None);
    let again = watch.look(&[replied], None);

    assert_eq!(came(&first), [("a", Why::Finished)]);
    assert_eq!(again, Changes::default());
}

#[test]
fn a_reply_read_as_it_came_isnt_told_of() {
    let mut watch = having_seen(&[working("a")]);

    let changes = watch.look(&[chat("a", SessionStatus::Idle, 4, 4)], None);

    assert_eq!(changes, Changes::default());
}

#[test]
fn waiting_on_the_person_and_stopping_short_are_told_of() {
    let mut watch = having_seen(&[working("a"), working("b")]);

    let changes = watch
        .look(&[chat("a", SessionStatus::WaitingApproval, 0, 0), chat("b", SessionStatus::Interrupted, 0, 0)], None);

    assert_eq!(came(&changes), [("a", Why::WaitingOnYou), ("b", Why::StoppedShort)]);
}

#[test]
fn a_chat_that_comes_to_several_at_once_says_the_most_pressing() {
    let mut watch = having_seen(&[working("a")]);

    let changes = watch.look(&[chat("a", SessionStatus::WaitingApproval, 3, 0)], None);

    assert_eq!(came(&changes), [("a", Why::WaitingOnYou)]);
}

#[test]
fn the_chat_being_read_is_never_told_of() {
    let mut watch = having_seen(&[working("a")]);
    let reading = SessionId::from("a");

    let changes = watch.look(&[chat("a", SessionStatus::WaitingApproval, 3, 0)], Some(&reading));
    let after = watch.look(&[chat("a", SessionStatus::WaitingApproval, 3, 0)], None);

    assert_eq!(changes, Changes::default());
    assert_eq!(after, Changes::default(), "it was in view as it came, so leaving it tells of nothing");
}

#[test]
fn what_was_told_of_goes_once_the_chat_no_longer_needs_the_person() {
    let mut watch = having_seen(&[working("read"), working("answered"), working("closed"), working("gone")]);
    watch.look(
        &[
            chat("read", SessionStatus::Idle, 2, 0),
            chat("answered", SessionStatus::WaitingApproval, 0, 0),
            chat("closed", SessionStatus::Idle, 2, 0),
            chat("gone", SessionStatus::Idle, 2, 0),
        ],
        None,
    );
    let closed = Session { closed_at_ms: Some(1), ..chat("closed", SessionStatus::Idle, 2, 0) };

    let changes = watch.look(&[chat("read", SessionStatus::Idle, 2, 2), working("answered"), closed], None);

    let mut went: Vec<&str> = changes.went.iter().map(SessionId::as_str).collect();
    went.sort_unstable();
    assert_eq!(went, ["answered", "closed", "gone", "read"]);
    assert_eq!(watch.look(&[], None), Changes::default(), "each goes once");
}

#[test]
fn what_was_told_of_goes_once_the_chat_is_being_read() {
    let mut watch = having_seen(&[working("a")]);
    let waiting = chat("a", SessionStatus::WaitingApproval, 0, 0);
    watch.look(std::slice::from_ref(&waiting), None);

    let changes = watch.look(&[waiting], Some(&SessionId::from("a")));

    assert_eq!(changes.went, [SessionId::from("a")]);
}

#[test]
fn a_chat_first_seen_after_the_first_look_only_takes_note() {
    let mut watch = having_seen(&[]);

    let first = watch.look(&[chat("new", SessionStatus::Idle, 2, 0)], None);
    let next = watch.look(&[chat("new", SessionStatus::Idle, 3, 0)], None);

    assert_eq!(first, Changes::default());
    assert_eq!(came(&next), [("new", Why::Finished)]);
}

#[test]
fn a_reply_nobody_has_read_stays_told_of_while_its_agent_works_again() {
    let mut watch = having_seen(&[working("a")]);
    watch.look(&[chat("a", SessionStatus::Idle, 4, 0)], None);

    let working_again = watch.look(&[chat("a", SessionStatus::Running, 4, 0)], None);
    let read = watch.look(&[chat("a", SessionStatus::Running, 4, 4)], None);

    assert_eq!(working_again, Changes::default(), "it's still unread, as the sidebar shows it");
    assert_eq!(read.went, [SessionId::from("a")]);
}

/// A turn that failed is a reply, but one the agent stopped short on, so that's what's told of. Failing again, with
/// the turn between unseen, as across a reconnect, is told of again.
#[test]
fn a_turn_that_failed_is_told_of_as_stopping_short_each_time() {
    let failed =
        |reply_seq| Session { reply_outcome: Some(TurnOutcome::Error), ..chat("a", SessionStatus::Idle, reply_seq, 0) };
    let mut watch = having_seen(&[working("a")]);

    let first = watch.look(&[failed(4)], None);
    let again = watch.look(&[failed(4)], None);
    let failed_again = watch.look(&[failed(9)], None);

    assert_eq!(came(&first), [("a", Why::StoppedShort)]);
    assert_eq!(again, Changes::default());
    assert_eq!(came(&failed_again), [("a", Why::StoppedShort)]);
}
