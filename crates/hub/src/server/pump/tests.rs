//! What a subscription is sent of the tail.

use std::time::Duration;

use protocol::model::RunId;

use super::*;
use crate::hub::tests::{carried_on, hub_in, stand_in_worker, writing};
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
