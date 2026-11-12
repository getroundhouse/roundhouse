//! Letting go of a chat that's been deleted, and of what was typed in answer to a question that's been answered, and
//! waiting for an agent to pick up again after its work in the background, against a hub that never starts a worker,
//! with what an agent would say recorded as if it had.

use std::time::{Duration, Instant};

use protocol::event::{AgentTask, EventBody, TaskKind};
use protocol::model::{AnswerSoFar, RunId, SessionStatus};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::oneshot;

use super::*;
use crate::hub::tests::hub_in;
use crate::session::{Item, Origin, PICKS_UP_WITHIN};
use crate::store::cancelled_approval;

const FROM_AGENT: Origin = Origin::Run { run: RunId::new(1), current: true, numbered: None };

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

/// The list of what the agent runs in the background, with `count` tasks in it.
fn listed(count: usize) -> EventBody {
    let task =
        |i: usize| AgentTask { id: format!("t{i}").into(), description: None, kind: TaskKind::Command, call: None };
    EventBody::AgentTasks { tasks: (0..count).map(task).collect(), stopped: Vec::new() }
}

/// Once everything the session has been sent is recorded.
async fn flushed(live: &LiveSession) {
    let (done, taken) = oneshot::channel();
    live.send_item(Item::Flush(done)).await;
    taken.await.unwrap();
}

/// The last of its work in the background has gone, between turns.
async fn background_over(live: &LiveSession) {
    live.record(FROM_AGENT, listed(1), None, None).await.unwrap();
    live.record(FROM_AGENT, listed(0), None, None).await.unwrap();
}

/// What the hub recorded of its own accord that the agent's state is.
fn states_from_the_hub(hub: &Hub) -> usize {
    let events = hub.store.read_events(&"s".into(), 0, 1000, u64::MAX, false).unwrap();
    events.iter().filter(|event| event.run.is_none() && matches!(event.body, EventBody::AgentState { .. })).count()
}

/// One that never does, as a CLI that changed might not, isn't left at work for ever, keeping the machine awake.
#[tokio::test(start_paused = true)]
async fn an_agent_that_doesnt_pick_up_again_after_its_work_in_the_background_is_taken_to_be_idle() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let live = hub.live_session(&"s".into()).await.unwrap();

    background_over(&live).await;
    assert_eq!(live.status(), SessionStatus::Background, "it's still at work until it picks up again");
    tokio::time::sleep(PICKS_UP_WITHIN.checked_sub(Duration::from_secs(1)).unwrap()).await;
    flushed(&live).await;
    assert_eq!(live.status(), SessionStatus::Background, "it was taken to be idle too soon");
    tokio::time::sleep(Duration::from_secs(2)).await;
    flushed(&live).await;

    assert_eq!(live.status(), SessionStatus::Idle);
    assert_eq!(states_from_the_hub(&hub), 1);
}

#[tokio::test(start_paused = true)]
async fn an_agent_that_picks_up_again_isnt_taken_to_be_idle_later() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let live = hub.live_session(&"s".into()).await.unwrap();

    background_over(&live).await;
    let init = EventBody::AgentInit {
        agent_session_id: None,
        model: None,
        cwd: None,
        permission_mode: None,
        agent_version: None,
    };
    live.record(FROM_AGENT, init, None, None).await.unwrap();
    tokio::time::sleep(PICKS_UP_WITHIN + Duration::from_secs(1)).await;
    flushed(&live).await;

    assert_eq!(live.status(), SessionStatus::Running);
    assert_eq!(states_from_the_hub(&hub), 0);
}
