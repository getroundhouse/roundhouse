//! What a subscription that has fallen behind the feed is sent.

use protocol::client::listed::{Listed, SessionRecords};
use protocol::client::{HubFrame, HubNotice};
use protocol::model::{Level, Rev, Subagent};
use tokio::sync::{broadcast, mpsc};

use super::super::notices::forward;
use crate::server::sends::{Sends, Started};

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
        rev: Rev::new(5),
    };
    // The feed holds 2, so it's left with 5 and 6 when it first reads.
    for rev in 1..=6 {
        feed.send(notice(rev)).unwrap();
    }
    let listed = Listed::Session(records);
    let list = || std::future::ready(Ok(listed.clone()));

    let level = Level::Session { id: "s".into() };
    drop(feed);
    let sends = Sends::at(level, Started::Subscribe { github_observed: false });
    forward(behind, &sends, 7, &frames, list, |_| {}).await;
    drop(frames);

    let mut frames = Vec::new();
    while let Some(queued) = sent.recv().await {
        frames.push(queued.frame);
    }
    let notice = |rev| HubFrame::Notice { subscription_id: 7, notice: notice(rev) };
    let expected = [HubFrame::Listed { subscription_id: 7, listed }, notice(5), notice(6)];
    assert_eq!(frames, expected);
}
