//! What a subscription that has fallen behind the feed is sent.

use protocol::client::github::GithubUpdate;
use protocol::client::listed::{Listed, SessionRecords};
use protocol::client::{HubFrame, HubNotice};
use protocol::model::{Level, Rev, Subagent};
use tokio::sync::{broadcast, mpsc};

use super::super::hub_level;
use super::super::notices::forward;
use crate::hub::tests::{hub_in, store_of};
use crate::server::sends::Sends;
use crate::server::tests::WAIT;
use crate::store::WorkspacePlacement;

/// How many notices the feed holds for a receiver that hasn't taken them, as `CAPACITY` in `crate::hub::feed` has it.
const FEED_HOLDS: usize = 256;

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
    let sends = Sends::at(level, false);
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

/// A subscription at the hub's level that falls behind is listed again, and from then on is sent what GitHub says of
/// each workspace that list has, such as one made while it was behind, whose notice it missed.
#[tokio::test]
async fn the_hub_s_level_listed_again_shows_each_workspace_it_lists() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    // Room for its first list alone, so it waits to send anything after, and falls behind the feed meanwhile.
    let (frames, mut sent) = mpsc::channel(1);
    let feeding = hub_level::feeding(&hub, Sends::at(Level::Hub, false), 7, frames).await.unwrap();
    let fed = tokio::spawn(feeding);
    let placement = WorkspacePlacement::local_checkout("/repo");
    store_of(&hub).insert_workspace(&protocol::fixtures::workspace("v"), &placement, None).unwrap();

    for _ in 0..=FEED_HOLDS {
        hub.notify(HubNotice::Subagent(protocol::fixtures::subagent("c")));
    }

    let of_v = |frame: &HubFrame| matches!(frame, HubFrame::Github { update: GithubUpdate::PullRequest { workspace, .. }, .. } if *workspace == "v".into());
    let shown_v = async {
        let mut frames = Vec::new();
        while !frames.iter().any(of_v) {
            frames.push(sent.recv().await.unwrap().frame);
        }
        frames
    };
    let frames = tokio::time::timeout(WAIT, shown_v).await.expect("v's pull request, once listed again");
    let lists = frames.iter().filter(|frame| matches!(frame, HubFrame::Listed { listed: Listed::Hub(_), .. })).count();
    assert_eq!(lists, 2, "listed first and again");
    fed.abort();
}
