//! Letting go of a chat that's been deleted, and of what was typed in answer to a question that's been answered,
//! against a hub that never starts a worker, since a chat nothing was sent in never has one.

use std::time::{Duration, Instant};

use protocol::model::AnswerSoFar;
use tokio::sync::broadcast::error::RecvError;

use super::*;
use crate::hub::tests::hub_in;
use crate::session::Origin;
use crate::store::cancelled_approval;

const TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
async fn closing_an_unused_chat_lets_go_of_it_and_of_everyone_following_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let live = hub.live_session(&"s".into()).await.unwrap();
    let held = Arc::downgrade(&live);
    // Holding the chat while it waits, as the server does for each client following it.
    let mut following = live.subscribe();
    let follower = tokio::spawn(async move {
        let _holding = live;
        following.recv().await
    });

    hub.session_close("s".into()).await.unwrap();

    let heard = tokio::time::timeout(TIMEOUT, follower).await.expect("the follower was never let go").unwrap();
    assert!(matches!(heard, Err(RecvError::Closed)), "{heard:?}");
    let deadline = Instant::now() + TIMEOUT;
    while held.upgrade().is_some() {
        assert!(Instant::now() < deadline, "something still holds the chat");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(hub.sessions.lock().live.is_empty());
}

#[test]
fn a_session_read_before_it_was_deleted_isnt_kept_once_it_has_been() {
    let mut sessions = Sessions::default();

    assert!(sessions.forget(&"s".into()).is_none(), "nothing of it was in memory");

    let kept = sessions.keep(&"s".into(), || panic!("a deleted session was started"));
    assert!(matches!(kept, Err(e) if e.code == ErrorCode::NotFound));
    assert!(sessions.live.is_empty());
}

/// The chat's status doesn't change, since the question wasn't one its agent is waiting on, but what was typed in
/// answer to it goes, and the feed says so.
#[tokio::test]
async fn a_question_answered_takes_what_was_typed_in_answer_and_the_feed_tells_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let typed = AnswerDraft {
        approval_id: "q".into(),
        answers: vec![AnswerSoFar { typed: "half".into(), ..AnswerSoFar::default() }],
    };
    hub.session_answer_draft("s".into(), Some(typed)).await.unwrap();
    let mut feed = hub.subscribe_feed();
    let live = hub.live_session(&"s".into()).await.unwrap();

    live.record(Origin::Hub, cancelled_approval("q".into()), None, None).await.unwrap();

    let told = tokio::time::timeout(TIMEOUT, async {
        loop {
            if let Ok(HubNotice::Session(session)) = feed.recv().await {
                return session;
            }
        }
    });
    let session = told.await.expect("the feed never told of the chat");
    assert_eq!(session.answer_draft, None);
}
