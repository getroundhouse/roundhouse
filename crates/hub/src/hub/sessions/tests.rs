//! Letting go of a chat that's been deleted, and of what was typed in answer to a question that's been answered,
//! waiting for an agent to pick up again after its work in the background, and taking what a worker kept of what an
//! agent is writing, against a hub that never starts a worker, with what an agent would say recorded or sent as if it
//! had.

use std::time::{Duration, Instant};

use protocol::event::{AgentRunState, AgentTask, EventBody, TaskKind, TurnOutcome};
use protocol::model::{AnswerSoFar, RunId, SessionId, SessionStatus};
use protocol::prompt::Prompt;
use tokio::sync::broadcast::error::RecvError;

use super::*;
use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker, writing};
use crate::session::{Item, LiveItem, LiveSession, Origin, PICKS_UP_WITHIN};
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

    live.record(Origin::Hub, cancelled_approval("q".into()), None).await.unwrap();

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

/// The last of its work in the background has gone, between turns.
async fn background_over(live: &LiveSession) {
    live.record(FROM_AGENT, listed(1), None).await.unwrap();
    live.record(FROM_AGENT, listed(0), None).await.unwrap();
}

/// The agent picking up again, as it does by itself once the last of its work in the background has gone.
fn picked_up() -> EventBody {
    EventBody::AgentInit { agent_session_id: None, model: None, cwd: None, permission_mode: None, agent_version: None }
}

fn turn_ended() -> EventBody {
    EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: vec![],
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
        models: Vec::new(),
        tokens: None,
        only_compacted: false,
    }
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
    live.record(FROM_AGENT, picked_up(), None).await.unwrap();
    tokio::time::sleep(PICKS_UP_WITHIN + Duration::from_secs(1)).await;
    flushed(&live).await;

    assert_eq!(live.status(), SessionStatus::Running);
    assert_eq!(states_from_the_hub(&hub), 0);
}

/// The agent picks up again, and its next turn leaves it waiting again just before the first wait's timer is due,
/// which leaves the second wait to its own.
#[tokio::test(start_paused = true)]
async fn the_timer_of_an_earlier_wait_leaves_a_later_one_alone() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let live = hub.live_session(&"s".into()).await.unwrap();
    background_over(&live).await;
    live.record(FROM_AGENT, picked_up(), None).await.unwrap();
    tokio::time::sleep(PICKS_UP_WITHIN.checked_sub(Duration::from_secs(1)).unwrap()).await;
    live.record(FROM_AGENT, listed(1), None).await.unwrap();
    live.record(FROM_AGENT, EventBody::AgentState { state: AgentRunState::Idle }, None).await.unwrap();
    live.record(FROM_AGENT, listed(0), None).await.unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;
    flushed(&live).await;
    assert_eq!(live.status(), SessionStatus::Background, "the first wait's timer ended the second");
    tokio::time::sleep(PICKS_UP_WITHIN).await;
    flushed(&live).await;

    assert_eq!(live.status(), SessionStatus::Idle, "the second wait's own timer didn't end it");
    assert_eq!(states_from_the_hub(&hub), 1);
}

/// The last hub's timer went with it, so the next, carrying on with the agent, waits again.
#[tokio::test(start_paused = true)]
async fn an_agent_carried_on_with_while_it_was_to_pick_up_again_is_taken_to_be_idle_if_it_doesnt() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let id: SessionId = "s".into();
    let last = hub.live_session(&id).await.unwrap();
    background_over(&last).await;
    // As the last hub goes, with its timer.
    last.end().await;
    hub.sessions.lock().live.remove(&id);
    let link = stand_in_worker(&hub);

    let live = carried_on(&hub, &link, RunId::new(1));

    assert_eq!(live.status(), SessionStatus::Background);
    tokio::time::sleep(PICKS_UP_WITHIN + Duration::from_secs(1)).await;
    flushed(&live).await;
    assert_eq!(live.status(), SessionStatus::Idle);
    assert_eq!(states_from_the_hub(&hub), 1);
}

/// A prompt the agent has yet to take holds the wait off, since taking it starts a turn, but the agent can drop it
/// instead.
#[tokio::test(start_paused = true)]
async fn an_agent_that_drops_the_prompt_it_was_to_take_is_taken_to_be_idle_after_all() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let live = hub.live_session(&"s".into()).await.unwrap();
    live.record(FROM_AGENT, picked_up(), None).await.unwrap();
    live.record(FROM_AGENT, listed(1), None).await.unwrap();
    let next = EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("next") };
    let (prompt, _) = live.record(Origin::Hub, next, None).await.unwrap();
    live.record(FROM_AGENT, turn_ended(), None).await.unwrap();
    live.record(FROM_AGENT, listed(0), None).await.unwrap();
    tokio::time::sleep(PICKS_UP_WITHIN + Duration::from_secs(1)).await;
    flushed(&live).await;
    assert_eq!(live.status(), SessionStatus::Background, "taken to be idle with a prompt still to take");

    live.record(FROM_AGENT, EventBody::PromptDropped { prompt: prompt.seq }, None).await.unwrap();

    tokio::time::sleep(PICKS_UP_WITHIN).await;
    flushed(&live).await;
    assert_eq!(live.status(), SessionStatus::Idle);
    assert_eq!(states_from_the_hub(&hub), 1);
}

/// What its worker kept of what the agent was writing, which this hub missed the deltas of, replaces the hub's tail of
/// the run, and every subscriber is told to read the tail again. What it says of a run the session no longer has
/// reaches nobody, as that run's deltas don't.
#[tokio::test]
async fn a_session_carried_on_with_takes_what_its_worker_kept_of_what_the_agent_is_writing() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let live = carried_on(&hub, &stand_in_worker(&hub), RunId::new(2));
    let mut following = live.subscribe();

    live.send_item(Item::Tail { run: RunId::new(1), tail: writing(RunId::new(1), "replaced") }).await;
    live.send_item(Item::Tail { run: RunId::new(2), tail: writing(RunId::new(2), "Reading the") }).await;
    flushed(&live).await;

    assert_eq!(live.snapshot().tail, writing(RunId::new(2), "Reading the"));
    assert!(matches!(following.try_recv().unwrap().item, LiveItem::Rewritten));
    assert!(following.try_recv().is_err(), "the replaced run's reached nobody");
}
