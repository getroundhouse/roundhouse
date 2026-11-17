//! What a subscription is sent of the tail.

use std::sync::Arc;
use std::time::Duration;

use protocol::event::{ContentBlock, EventBody, TurnOutcome};
use protocol::fixtures::turn_ended;
use protocol::model::{Place, RunId, SessionId, SubagentId};
use protocol::tail::{DeltaChange, StreamDelta};

use super::*;
use crate::hub::Hub;
use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker, writing};
use crate::session::{Item, LiveSession};

const TIMEOUT: Duration = Duration::from_secs(10);

/// The hub can take what a worker kept of what its agent is writing after a client has started following the chat,
/// so the client is sent the tail again, whole, since no delta says what changed.
#[tokio::test]
async fn a_subscriber_is_sent_the_tail_again_once_the_hub_takes_what_a_worker_kept() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let session = carried_on(&hub, &stand_in_worker(&hub), RunId::new(1));
    let (frames, mut sent) = mpsc::channel(8);
    let live = session.subscribe();
    let following = Following::Log { next: Seq::new(1) };
    tokio::spawn(pump(hub.clone(), session.clone(), "s".into(), 1, following, live, frames));
    let first = tokio::time::timeout(TIMEOUT, sent.recv()).await.unwrap();
    assert!(matches!(first, Some(HubFrame::Tail { ref tail, .. }) if tail.is_empty()), "{first:?}");

    session.send_item(Item::Tail { run: RunId::new(1), tail: writing(RunId::new(1), "Reading the") }).await;

    let next = tokio::time::timeout(TIMEOUT, sent.recv()).await.unwrap();
    let Some(HubFrame::Tail { tail, .. }) = next else { panic!("{next:?}") };
    assert_eq!(tail, writing(RunId::new(1), "Reading the"));
}

/// A client can name a subagent by the call that started it before the agent has said it started, as one that opens it
/// as soon as its call shows does. Once the agent says, it's followed by the ID its agent gives it, as a subscriber
/// that named it after would follow it.
#[tokio::test]
async fn a_subscriber_that_names_a_subagent_by_its_call_before_it_starts_follows_it_once_it_has() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let run = RunId::new(1);
    let session = carried_on(&hub, &stand_in_worker(&hub), run);
    let id: SessionId = "s".into();
    let named = || Following::places(&hub, &id, Some("toolu_1".into()), Place::FIRST);
    let (frames, mut sent) = mpsc::channel(8);
    tokio::spawn(pump(
        hub.clone(),
        session.clone(),
        "s".into(),
        1,
        named().await.unwrap(),
        session.subscribe(),
        frames,
    ));
    let first = tokio::time::timeout(TIMEOUT, sent.recv()).await.unwrap();
    assert!(matches!(first, Some(HubFrame::Tail { .. })), "{first:?}");
    // One whose pump starts only once the start is recorded, so it's before the snapshot.
    let late = named().await.unwrap();

    let started = EventBody::SubagentStarted {
        call: "toolu_1".into(),
        subagent: Some("task_1".into()),
        kind: None,
        description: None,
        prompt: None,
        in_background: true,
    };
    let said = vec![ContentBlock::Text { text: "Found it.".into(), images: Vec::new() }];
    let its_own = EventBody::Assistant {
        message_id: None,
        subagent: Some("task_1".into()),
        model: None,
        blocks: said,
        first_index: None,
    };
    for body in [started, its_own] {
        session
            .send_item(Item::Event { run, body: Box::new(body), raw: None, attachments: Vec::new(), seq: None })
            .await;
    }

    flushed(&session).await;
    let (frames, mut sent_late) = mpsc::channel(8);
    tokio::spawn(pump(hub.clone(), session.clone(), "s".into(), 2, late, session.subscribe(), frames));

    for sent in [&mut sent, &mut sent_late] {
        let event = loop {
            match tokio::time::timeout(TIMEOUT, sent.recv()).await.unwrap() {
                Some(HubFrame::Event { event, .. }) => break event,
                Some(_) => {}
                None => panic!("the pump stopped"),
            }
        };
        assert_eq!((event.subagent, event.place), (Some(SubagentId::from("task_1")), Place::FIRST));
    }
}

/// The subagent `task_1`, which the call `toolu_1` started, in the background or not, as the agent says it started.
fn subagent_started(in_background: bool) -> EventBody {
    EventBody::SubagentStarted {
        call: "toolu_1".into(),
        subagent: Some("task_1".into()),
        kind: None,
        description: None,
        prompt: None,
        in_background,
    }
}

/// What the agent of `run`, or its subagent `from`, has started writing, named for `text`.
fn writes(session: &LiveSession, run: RunId, from: Option<&str>, text: &str) -> impl Future<Output = ()> {
    let change = DeltaChange::Thinking { index: 0, text: text.into() };
    let delta = StreamDelta { run: Some(run), message_id: text.into(), subagent: from.map(Into::into), change };
    session.send_item(Item::Delta { run, delta })
}

/// A subscription to the session `s` of `hub` that follows the agent's own events, or those of the subagent `task_1`,
/// as a pump feeds it.
async fn following(hub: &Arc<Hub>, session: &Arc<LiveSession>, subagent: Option<&str>) -> mpsc::Receiver<HubFrame> {
    let follows = Following::places(hub, &"s".into(), subagent.map(Into::into), Place::FIRST).await.unwrap();
    let (frames, sent) = mpsc::channel(8);
    tokio::spawn(pump(hub.clone(), session.clone(), "s".into(), 1, follows, session.subscribe(), frames));
    sent
}

/// The next tail `sent`, by who is writing each of its messages.
async fn next_tail(sent: &mut mpsc::Receiver<HubFrame>) -> Vec<Option<SubagentId>> {
    loop {
        match tokio::time::timeout(TIMEOUT, sent.recv()).await.expect("the follower was never sent the tail") {
            Some(HubFrame::Tail { tail, .. }) => return tail.messages.into_iter().map(|m| m.subagent).collect(),
            Some(_) => {}
            None => panic!("the pump stopped"),
        }
    }
}

/// A client that starts following the agent, or one subagent, while both are writing, starts from what that one is
/// writing and nothing of the other's.
#[tokio::test]
async fn a_follower_that_joins_as_a_subagent_writes_starts_from_only_what_it_follows() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let run = RunId::new(1);
    let session = carried_on(&hub, &stand_in_worker(&hub), run);
    let started =
        Item::Event { run, body: Box::new(subagent_started(true)), raw: None, attachments: Vec::new(), seq: None };
    session.send_item(started).await;
    writes(&session, run, Some("task_1"), "There's").await;
    writes(&session, run, None, "Waiting").await;
    flushed(&session).await;

    let (mut own, mut work) = (following(&hub, &session, None).await, following(&hub, &session, Some("task_1")).await);

    assert_eq!(next_tail(&mut own).await, [None], "the agent's own");
    assert_eq!(next_tail(&mut work).await, [Some(SubagentId::from("task_1"))], "the subagent's");
}

/// Cancelling a turn ends it, and with it what a subagent in the foreground was writing. Its follower doesn't follow
/// the agent's own events, so it never sees the end, and is sent the tail again, with nothing half written left in it.
#[tokio::test]
async fn the_end_of_a_turn_leaves_a_subagent_in_the_foreground_s_follower_nothing_half_written() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let run = RunId::new(1);
    let session = carried_on(&hub, &stand_in_worker(&hub), run);
    let event = |body| Item::Event { run, body: Box::new(body), raw: None, attachments: Vec::new(), seq: None };
    session.send_item(event(subagent_started(false))).await;
    writes(&session, run, Some("task_1"), "There's").await;
    flushed(&session).await;
    let mut work = following(&hub, &session, Some("task_1")).await;
    assert_eq!(next_tail(&mut work).await, [Some(SubagentId::from("task_1"))], "what it's writing");

    session.send_item(event(turn_ended(TurnOutcome::Cancelled))).await;

    assert_eq!(next_tail(&mut work).await, [], "the tail again, once the turn has ended");
    assert!(work.try_recv().is_err(), "nothing else, since the end is the agent's own: {:?}", work.try_recv());
}
