//! What a chat does with what it's asked, on a hub whose workspaces have stand-ins for their workers, whose agents say
//! what a test has them say: a prompt sent again, or while the agent is at work, or just before the hub is asked to
//! stop if idle, a stop or a compaction there's nothing to do for, being closed with nothing sent, and when each turn
//! started. What the agent takes of each, and when, is its adapter's to say, as the daemon's `e2e/steering.rs` and
//! `e2e/core_loop.rs` have a real one say it.

use std::sync::atomic::Ordering;

use protocol::client::{ErrorCode, HubNotice};
use protocol::event::{AgentRunState, EventBody, TurnOutcome};
use protocol::fixtures::turn_ended;
use protocol::model::{AgentKind, RecordKey, Seq, Session, SessionId, SessionSettingsChange, SessionStatus};
use protocol::prompt::Prompt;
use tokio::sync::broadcast;

use crate::hub::Hub;
use crate::hub::stand_in_workers::{Asked, TIMEOUT, a_chat, a_turn, hub_with_stand_in_workers, the_agent_says};
use crate::store::now_ms;

fn log(hub: &Hub, chat: &SessionId) -> Vec<protocol::event::SessionEvent> {
    hub.store.read_events(chat, Seq::FIRST, 1000, u64::MAX, false).unwrap()
}

/// Once the agent of `chat` has been given the prompt at `seq`.
async fn given(asked: &Asked, chat: &SessionId, seq: Seq) {
    waiting::task::until(TIMEOUT, "the prompt to be given", || asked.prompts_given(chat).contains(&seq)).await;
}

/// The next copy of `chat`'s record the feed tells of that `wanted` says is the one.
async fn told(
    feed: &mut broadcast::Receiver<HubNotice>,
    chat: &SessionId,
    wanted: impl Fn(&Session) -> bool,
) -> Session {
    let next = async {
        loop {
            if let Ok(HubNotice::Session(told)) = feed.recv().await
                && told.id == *chat
                && wanted(&told)
            {
                return told;
            }
        }
    };
    tokio::time::timeout(TIMEOUT, next).await.expect("the feed never told of it")
}

/// So a client that loses the reply can send it again: the same message is the same prompt, recorded once, in the same
/// place, and given to the agent once.
#[tokio::test]
async fn a_prompt_sent_again_is_the_one_recorded_and_the_agent_is_given_it_once() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    let send = || hub.session_prompt(chat.id.clone(), Some("msg-1".into()), Prompt::new("once"));

    let (first, sent_before) = send().await.unwrap();
    let (again, sent_again) = send().await.unwrap();
    given(&asked, &chat.id, first).await;
    // Commands run in turn, so once the next prompt is given, a second of the first would have been.
    let (next, _) = hub.session_prompt(chat.id.clone(), None, Prompt::new("next")).await.unwrap();
    given(&asked, &chat.id, next).await;

    assert_eq!((first, sent_before), (Seq::new(1), false));
    assert_eq!((again, sent_again), (first, true));
    let prompts = log(&hub, &chat.id).iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).count();
    assert_eq!(prompts, 2, "the first once, and the next");
    assert_eq!(asked.prompts_given(&chat.id), [first, next], "the agent was given it once");
}

/// The record says what the agent has yet to take, which is what a client tells from it whether a compaction can be
/// asked, and the feed tells of the agent taking it, though that changes no status.
#[tokio::test]
async fn the_feed_tells_of_a_prompt_sent_while_the_agent_works_until_the_agent_takes_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    let (working, _) = hub.session_prompt(chat.id.clone(), None, Prompt::new("work")).await.unwrap();
    given(&asked, &chat.id, working).await;
    the_agent_says(&hub, &asked, &chat.id, [EventBody::PromptTaken { prompt: working }]).await;
    let mut feed = hub.subscribe_feed();

    let (steered, _) = hub.session_prompt(chat.id.clone(), None, Prompt::new("steered")).await.unwrap();
    let waiting = told(&mut feed, &chat.id, |told| told.untaken.as_deref() == Some(&[steered][..])).await;
    given(&asked, &chat.id, steered).await;
    the_agent_says(&hub, &asked, &chat.id, [EventBody::PromptTaken { prompt: steered }]).await;
    let taken = told(&mut feed, &chat.id, |told| told.untaken.as_deref() == Some(&[][..])).await;

    assert!(waiting.compaction_waits());
    assert_eq!(taken.status, SessionStatus::Running, "before its turn has ended");
}

/// There's nothing to compact before the agent has had a turn, and an agent on one would compact only once it's done.
/// Either way the request isn't recorded, so nothing is left to say it was asked.
#[tokio::test]
async fn compacting_before_any_conversation_or_while_the_agent_is_on_a_turn_is_refused_and_recorded_nowhere() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;

    let before_any = hub.session_compact(chat.id.clone(), None).await.map_err(|e| e.code);
    let (working, _) = hub.session_prompt(chat.id.clone(), None, Prompt::new("work")).await.unwrap();
    given(&asked, &chat.id, working).await;
    the_agent_says(&hub, &asked, &chat.id, [EventBody::PromptTaken { prompt: working }]).await;
    let on_a_turn = hub.session_compact(chat.id.clone(), None).await.map_err(|e| e.code);

    assert_eq!(before_any.err(), Some(ErrorCode::Conflict));
    assert_eq!(on_a_turn.err(), Some(ErrorCode::Conflict));
    let asked_to = log(&hub, &chat.id).iter().any(|e| matches!(e.body, EventBody::CompactionAsked { .. }));
    assert!(!asked_to, "a refused compaction was recorded");
}

#[tokio::test]
async fn stopping_a_chat_with_nothing_running_is_harmless() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;

    hub.session_cancel(chat.id.clone()).await.unwrap();

    assert_eq!(hub.session_get(chat.id.clone()).await.unwrap().status, SessionStatus::Idle);
}

/// The case it's for, as the app replaces an older hub: the prompt's reply is back, so the request for it no longer
/// counts, and its agent hasn't said anything yet. The agent is at work from when the hub records the prompt, which is
/// before it replies, so the hub doesn't stop, and the turn goes on as it would have.
#[tokio::test]
async fn a_hub_refuses_to_stop_if_idle_straight_after_a_prompt_and_the_turn_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    waiting::task::until(TIMEOUT, "the hub to be idle", || hub.status().is_idle()).await;

    let (seq, _) = hub.session_prompt(chat.id.clone(), None, Prompt::new("still here")).await.unwrap();
    let stopped = hub.shutdown_if_idle().map_err(|e| e.code);
    given(&asked, &chat.id, seq).await;
    let ended = [EventBody::PromptTaken { prompt: seq }, turn_ended(TurnOutcome::Success), idle()];
    the_agent_says(&hub, &asked, &chat.id, ended).await;

    assert_eq!(stopped, Err(ErrorCode::Conflict));
    assert!(!hub.stopping.load(Ordering::SeqCst), "it was asked to stop");
    assert!(hub.status().is_idle(), "it's idle once the turn has ended");
}

fn idle() -> EventBody {
    EventBody::AgentState { state: AgentRunState::Idle }
}

/// A chat nothing was sent in has nothing to keep, so it goes, and every client is told, after the chat as it was,
/// which is what closing it answers. Anything asked of it after that finds nothing.
#[tokio::test]
async fn closing_a_chat_nothing_was_sent_in_deletes_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, used) = a_chat(&hub).await;
    a_turn(&hub, &asked, &used.id, Prompt::new("keep me"), None).await;
    let unchanged = SessionSettingsChange::default();
    let unused = hub.session_create(workspace.id.clone(), AgentKind::Claude, unchanged, None).await.unwrap();
    let mut feed = hub.subscribe_feed();

    let closed = hub.session_close(unused.id.clone()).await.unwrap();

    assert_eq!(closed.closed_at_ms, None, "the reply is the chat as it was");
    let removed = std::iter::from_fn(|| feed.try_recv().ok()).find_map(|notice| {
        let HubNotice::Removed { record, rev } = notice else { return None };
        Some((record, rev))
    });
    let (record, rev) = removed.expect("the feed told of its removal");
    assert_eq!(record, RecordKey::Session { id: unused.id.clone() });
    assert!(rev > closed.rev);
    let listed: Vec<_> = hub.session_list(workspace.id.clone()).await.unwrap().into_iter().map(|s| s.id).collect();
    assert_eq!(listed, [used.id]);
    let again = hub.session_close(unused.id.clone()).await.map_err(|e| e.code);
    let prompted = hub.session_prompt(unused.id.clone(), None, Prompt::new("hello?")).await.map_err(|e| e.code);
    assert_eq!((again.err(), prompted.err()), (Some(ErrorCode::NotFound), Some(ErrorCode::NotFound)));
}

/// By the hub's clock, as the prompt that started it was recorded, whatever the agent said: the chat's record says it of
/// its newest turn, and each turn's end says it of its own.
#[tokio::test]
async fn each_turn_ends_saying_when_it_started_and_the_record_says_when_the_newest_did() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;

    a_turn(&hub, &asked, &chat.id, Prompt::new("hello"), None).await;
    let first = now_ms();
    waiting::task::until(TIMEOUT, "the hub's clock to move on", || now_ms() > first).await;
    a_turn(&hub, &asked, &chat.id, Prompt::new("hello again"), None).await;

    let events = log(&hub, &chat.id);
    let prompted: Vec<_> =
        events.iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).map(|e| Some(e.ts_ms)).collect();
    let ended: Vec<_> = events
        .iter()
        .filter_map(|e| {
            let EventBody::TurnEnded { started_at_ms, .. } = e.body else { return None };
            Some(started_at_ms)
        })
        .collect();
    assert_ne!(prompted[0], prompted[1], "the turns have to start apart to tell them apart");
    assert_eq!(ended, prompted);
    assert_eq!(hub.session_get(chat.id.clone()).await.unwrap().turn_started_at_ms, prompted[1]);
}
