//! The feed never tells of a copy of a record that's no newer than one it has told of, as two tasks that each announce
//! a record after their own change can finish in either order.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use protocol::client::HubNotice;
use protocol::model::{RecordKey, Rev, Session, Subagent, WorkspaceDraft};
use tokio::sync::broadcast;

use super::Feed;

/// The IDs of the sessions the law sends copies of.
const IDS: [&str; 3] = ["s", "t", "u"];

/// The session `id`'s record at `rev`.
fn session(id: &str, rev: u64) -> Session {
    Session { rev: Rev::new(rev), ..protocol::fixtures::session(id) }
}

/// The subagent `call` in the session `id`, at `rev`.
fn subagent(id: &str, call: &str, rev: u64) -> Subagent {
    Subagent { session_id: id.into(), rev: Rev::new(rev), ..protocol::fixtures::subagent(call) }
}

fn told(session: &Session) -> HubNotice {
    HubNotice::Session(session.clone())
}

/// Everything a client following the feed has heard so far.
fn heard(client: &mut broadcast::Receiver<HubNotice>) -> Vec<HubNotice> {
    std::iter::from_fn(|| client.try_recv().ok()).collect()
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// However the tasks announcing records interleave, a client hears each copy that's newer than every earlier copy
    /// of its record, in the order they were sent, and nothing else.
    #[test]
    fn a_client_hears_each_copy_newer_than_the_last_of_its_record(sent in vec((0..IDS.len(), 1..8u64), 0..40)) {
        let feed = Feed::new();
        let mut client = feed.subscribe();

        for &(record, rev) in &sent {
            feed.send(told(&session(IDS[record], rev)));
        }

        let mut newest = [0; IDS.len()];
        let newer = sent.iter().filter(|&&(record, rev)| {
            let newer = rev > newest[record];
            newest[record] = newest[record].max(rev);
            newer
        });
        let expected: Vec<HubNotice> = newer.map(|&(record, rev)| told(&session(IDS[record], rev))).collect();
        prop_assert_eq!(heard(&mut client), expected);
    }
}

#[test]
fn a_copy_older_than_its_records_removal_isnt_told_of() {
    let feed = Feed::new();
    let mut client = feed.subscribe();
    let removed = HubNotice::Removed { record: RecordKey::Session { id: "s".into() }, rev: Rev::new(3) };

    feed.send(removed.clone());
    feed.send(told(&session("s", 2)));

    assert_eq!(heard(&mut client), [removed]);
}

/// A subagent has no ID of its own, so it's told apart by its call and its session, and from the session itself.
#[test]
fn each_subagents_copies_are_in_order_apart_from_any_others() {
    let feed = Feed::new();
    let mut client = feed.subscribe();
    let copies = [subagent("s", "call-1", 2), subagent("t", "call-1", 2), subagent("s", "call-2", 2)];

    feed.send(told(&session("s", 2)));
    for copy in &copies {
        feed.send(HubNotice::Subagent(copy.clone()));
    }
    feed.send(HubNotice::Subagent(subagent("s", "call-1", 1)));

    let subagents = copies.into_iter().map(HubNotice::Subagent);
    assert_eq!(heard(&mut client), std::iter::once(told(&session("s", 2))).chain(subagents).collect::<Vec<_>>());
}

/// Only a hub that doesn't keep revisions makes a copy without one, and a client takes each, so the feed can't leave
/// any out.
#[test]
fn a_copy_without_a_revision_is_always_told_of() {
    let feed = Feed::new();
    let mut client = feed.subscribe();

    feed.send(told(&session("s", 2)));
    feed.send(told(&session("s", 0)));
    feed.send(told(&session("s", 0)));
    feed.send(told(&session("s", 1)));

    assert_eq!(heard(&mut client), [told(&session("s", 2)), told(&session("s", 0)), told(&session("s", 0))]);
}

/// What's in the new-workspace dialog is one record, with no ID, and its copies are told of the same way.
#[test]
fn the_new_workspace_dialogs_copies_are_in_order() {
    let feed = Feed::new();
    let mut client = feed.subscribe();
    let dialog = |rev| HubNotice::WorkspaceDraft(WorkspaceDraft { rev: Rev::new(rev), ..WorkspaceDraft::default() });

    feed.send(dialog(2));
    feed.send(dialog(1));
    feed.send(dialog(2));
    feed.send(dialog(3));

    assert_eq!(heard(&mut client), [dialog(2), dialog(3)]);
}
