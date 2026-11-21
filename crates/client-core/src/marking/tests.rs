use protocol::client::{ApiError, ClientRequest, ErrorCode, ReplyOutcome};
use protocol::fixtures::{session, workspace};
use protocol::model::{Seq, Session, SessionStatus, WorkspaceStatus};

use super::{Mark, Reading, needs_a_look, waiting_on_you, workspace_marking};
use crate::stand_in_hub::{self, Does};

/// A chat whose newest reply is at `reply`, read through `read`, last worked in at `active_ms`.
fn chat(id: &str, reply: u64, read: u64, active_ms: i64) -> Session {
    let mut chat = session(id);
    (chat.reply_seq, chat.read_seq, chat.user_active_at_ms) = (Seq::new(reply), Seq::new(read), Some(active_ms));
    chat
}

fn ids(chats: &[Session]) -> Vec<&str> {
    chats.iter().map(|chat| chat.id.as_str()).collect()
}

#[test]
fn a_workspace_with_unread_replies_marks_each_of_those_chats_read() {
    let chats = [chat("a", 4, 4, 30), chat("b", 6, 2, 10), chat("c", 3, 1, 20)];
    let (mark, marked) = workspace_marking(&chats, Session::unread).unwrap();
    assert_eq!((mark, ids(&marked)), (Mark::Read, vec!["b", "c"]));
}

#[test]
fn a_workspace_with_everything_read_marks_the_last_worked_in_reply_unread() {
    let chats = [chat("a", 4, 4, 30), chat("b", 6, 6, 50), chat("c", 0, 0, 90)];
    let (mark, marked) = workspace_marking(&chats, Session::unread).unwrap();
    assert_eq!((mark, ids(&marked)), (Mark::Unread, vec!["b"]), "c has no reply");
}

#[test]
fn closed_chats_and_a_workspace_with_no_reply_are_left_alone() {
    let mut closed = chat("a", 6, 2, 10);
    closed.closed_at_ms = Some(20);
    assert_eq!(workspace_marking(&[closed, chat("b", 0, 0, 30)], Session::unread), None);
}

#[test]
fn what_the_client_shows_as_read_is_read() {
    let chats = [chat("a", 6, 2, 10)];
    let (mark, _) = workspace_marking(&chats, |_| false).unwrap();
    assert_eq!(mark, Mark::Unread, "a reply being marked read shows as read already");
}

#[test]
fn a_chat_shown_with_an_unread_reply_is_read_once_and_again_for_each_reply_after() {
    let mut reading = Reading::default();
    assert!(reading.read(Some(&chat("a", 4, 2, 0))), "shown with a reply nobody has read");
    assert!(!reading.read(Some(&chat("a", 4, 2, 0))), "the same reply, before the hub says it's read");
    assert!(reading.read(Some(&chat("a", 6, 4, 0))), "a reply that came while it showed");
}

#[test]
fn a_chat_marked_unread_while_it_shows_stays_unread_until_its_shown_again() {
    let mut reading = Reading::default();
    assert!(reading.read(Some(&chat("a", 4, 2, 0))));
    assert!(!reading.read(Some(&chat("a", 4, 3, 0))), "marked unread again, still showing");
    assert!(!reading.read(None), "gone elsewhere");
    assert!(reading.read(Some(&chat("a", 4, 3, 0))), "shown again");
}

#[test]
fn without_a_hub_a_chat_no_longer_in_front_is_read_again_once_it_is() {
    let mut reading = Reading::default();
    let a = chat("a", 4, 2, 0);
    assert!(reading.read(Some(&a)));
    reading.forget_unless(Some(&a.id));
    assert!(!reading.read(Some(&a)), "still in front");
    reading.forget_unless(None);
    assert!(reading.read(Some(&a)), "gone and back");
}

/// Marks a chat each way on a hub that answers every mark with `answer`.
fn marked_on(answer: impl Fn() -> ReplyOutcome + Send + 'static) -> [crate::Result<()>; 2] {
    let dir = tempfile::tempdir().unwrap();
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), move |request| {
        let marks = matches!(request, ClientRequest::SessionMarkRead(_) | ClientRequest::SessionMarkUnread(_));
        if marks { Does::Answers(Box::new(answer())) } else { Does::Nothing }
    });
    let chat = chat("a", 4, 2, 0);
    [Mark::Read.chat(&client, &chat), Mark::Unread.chat(&client, &chat)]
}

#[test]
fn a_hub_too_old_to_keep_replies_has_none_to_mark_and_any_other_refusal_fails() {
    let older = || ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "an older hub"));
    assert!(marked_on(older).iter().all(Result::is_ok), "an older hub keeps no replies");

    let refused = || ReplyOutcome::Error(ApiError::new(ErrorCode::Internal, "something else"));
    assert!(marked_on(refused).iter().all(Result::is_err), "any other refusal is said");
}

#[test]
fn an_open_chat_waiting_on_the_person_needs_a_look() {
    let waiting = Session { status: SessionStatus::WaitingApproval, ..chat("a", 0, 0, 0) };
    let running = Session { status: SessionStatus::Running, ..chat("b", 0, 0, 0) };
    assert!(waiting_on_you(&[running.clone(), waiting.clone()]));
    assert!(!waiting_on_you(std::slice::from_ref(&running)));
    let closed = Session { closed_at_ms: Some(1), ..waiting };
    assert!(!waiting_on_you(&[closed]), "a closed chat doesn't count");
}

#[test]
fn a_workspace_needs_a_look_for_an_open_chat_unread_or_waiting_but_not_one_archived_with_replies_unread() {
    let ready = workspace("w");
    let archived = protocol::model::Workspace { status: WorkspaceStatus::Archived, ..workspace("w") };
    let unread = chat("a", 4, 2, 0);
    let closed = Session { closed_at_ms: Some(1), ..unread.clone() };
    let waiting = Session { status: SessionStatus::WaitingApproval, ..chat("b", 0, 0, 0) };

    assert!(needs_a_look(&ready, std::slice::from_ref(&unread), Session::unread));
    assert!(!needs_a_look(&ready, std::slice::from_ref(&unread), |_| false), "shown as read by the client");
    assert!(!needs_a_look(&ready, &[closed], Session::unread), "a closed chat doesn't count");
    assert!(!needs_a_look(&archived, &[unread], Session::unread), "archived, until it's back");
    assert!(needs_a_look(&archived, &[waiting], Session::unread), "waiting on the person, wherever it is");
}
