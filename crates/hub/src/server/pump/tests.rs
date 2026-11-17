//! What a subscription is sent of the tail.

use std::time::Duration;

use protocol::event::{ContentBlock, EventBody};
use protocol::model::{Place, RunId, SessionId, SubagentId};

use super::*;
use crate::hub::tests::{carried_on, flushed, hub_in, stand_in_worker, writing};
use crate::session::Item;

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
