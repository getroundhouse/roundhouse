//! What a subscription that has fallen behind the feed is sent.

use protocol::client::listed::{Listed, SessionRecords};
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{CLIENT_PROTOCOL_VERSION, HubFrame, HubNotice};
use protocol::model::{Level, Rev, Subagent};
use tokio::sync::{broadcast, mpsc};

use super::super::notices::forward;
use super::given::{ok, start};
use crate::hub::Followed;
use crate::hub::tests::{hub_in, store_of};
use crate::server::Closer;
use crate::server::tests::{Connection, WAIT};

/// More notices of one level than a connection's queue, the room in its stream and the feed hold together.
const ROOM_ON_A_CONNECTION: u64 = 4_000;

/// A subscription that falls behind the feed is sent where its level stands again, in place of the notices it missed,
/// and then each notice after, rather than `Resync`.
#[tokio::test]
async fn a_subscription_that_falls_behind_is_sent_where_its_level_stands_again() {
    let (feed, behind) = broadcast::channel(2);
    let (frames, mut sent) = mpsc::channel(16);
    let notice = |rev| HubNotice::Subagent(Subagent { rev: Rev::new(rev), ..protocol::fixtures::subagent("c") });
    let records = SessionRecords {
        session_id: "s".into(),
        subagents: Vec::new(),
        proposals: Vec::new(),
        replies_heard: Vec::new(),
        observations: Vec::new(),
        rev: Rev::new(5),
    };
    // The feed holds 2, so it's left with 5 and 6 when it first reads.
    for rev in 1..=6 {
        feed.send(std::sync::Arc::new(notice(rev))).unwrap();
    }
    let listed = Listed::Session(records);
    let list = || std::future::ready(Ok(listed.clone()));

    let level = Level::Session { id: "s".into() };
    drop(feed);
    forward(behind.into(), &level, 7, &frames, &Closer::default(), list).await;
    drop(frames);

    let mut frames = Vec::new();
    while let Some(queued) = sent.recv().await {
        frames.push(queued.frame);
    }
    let notice = |rev| HubFrame::Notice { subscription_id: 7, notice: notice(rev) };
    let expected = [HubFrame::Listed { subscription_id: 7, listed }, notice(5), notice(6)];
    assert_eq!(frames, expected);
}

/// A subscription that falls behind and whose level can't be listed again, as a session deleted meanwhile can't, has
/// its connection closed, since no frame tells a client one subscription has ended, and the connection's other
/// subscriptions end with it, so its client follows everything again.
#[tokio::test]
async fn a_subscription_that_cant_be_listed_again_has_its_connection_closed() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut one = Connection::greeted(&hub, CLIENT_PROTOCOL_VERSION).await;
    let follows = vec![start(1, Follow::Hub), start(2, Follow::Session { id: "s".into(), from: None })];
    ok(&mut one, 1, Subscribe { stop: Vec::new(), start: follows }).await;
    let following = |followed: &Followed| hub.followed(followed);
    assert!(following(&Followed::Hub) && following(&Followed::Session("s".into())));

    // Unread, so the session's subscription waits for room on the connection, and falls behind the feed meanwhile.
    for rev in 0..ROOM_ON_A_CONNECTION {
        let subagent = Subagent { rev: Rev::new(1_000 + rev), ..protocol::fixtures::subagent("c") };
        hub.notify(HubNotice::Subagent(subagent));
    }
    store_of(&hub).delete_unused_session(&"s".into()).unwrap().expect("s, with nothing sent in it");
    one.until_closed().await;

    waiting::task::until(WAIT, "the connection's subscriptions to end", || {
        !following(&Followed::Hub) && !following(&Followed::Session("s".into()))
    })
    .await;
}
