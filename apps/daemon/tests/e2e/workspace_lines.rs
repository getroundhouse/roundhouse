//! Each workspace's lines added and removed, which the hub reads from the workspace's worker as a turn ends there, and
//! sends every client that follows the hub, and the files it changes, read at the same time, which it sends every
//! client that follows the workspace. The hub's own tests cover when it reads them, and what it keeps of a refusal.

#![cfg(test)]

use std::time::SystemTime;

use client_core::RecordsFeed;
use protocol::client::subscriptions::{Follow, Subscribe, Subscription};
use protocol::client::{ClientRequest, ClientResponse, HUB_SPEAKS, Hello, HubFrame, HubNotice, ReplyOutcome};
use protocol::model::{ChangeStatus, ChangeSummary, LineCounts, Observation, Observed};

use crate::support::{RawConnection, Stack, wait_for_turn_end, wait_until};

/// A turn that changes a tracked file and adds an untracked one, in a workspace no client has open, has a client that
/// follows only the hub hold the lines they add and remove, as of when the hub read them.
#[test]
fn a_turn_that_changes_files_has_a_client_that_follows_the_hub_hold_its_workspace_s_lines() {
    let stack = Stack::start();
    let client = stack.client();
    // On a connection of its own, so only the feed, and not the replies it would take in, can bring them.
    let feed = RecordsFeed::start(&stack.client());
    let (workspace, session) = stack.workspace(&client);
    let before = now_ms();

    client.session_prompt(&session.id, "tool:echo changed > a.txt && echo new > new.txt", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let changed = LineCounts { added: 2, removed: 1 };
    wait_until(
        "the copy to hold the lines",
        || matches!(feed.snapshot().workspace_lines(&workspace.id), Observed::AsOf { value, .. } if *value == changed),
    );
    let Observed::AsOf { as_of_ms, .. } = *feed.snapshot().workspace_lines(&workspace.id) else { unreachable!() };
    assert!((before..=now_ms()).contains(&as_of_ms), "read at {as_of_ms}, after {before}");
}

/// A turn that changes a tracked file and adds an untracked one has a client that follows the workspace hold the files
/// it changes, as of when the hub read them, without asking, and a client that follows only the hub is sent the lines
/// and none of the files.
#[test]
fn a_turn_that_changes_files_has_a_client_that_follows_the_workspace_hold_its_files() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let mut both = following(&stack, vec![Follow::Hub, Follow::Workspace { id: workspace.id.clone() }]);
    let mut hub_only = following(&stack, vec![Follow::Hub]);
    let before = now_ms();

    client.session_prompt(&session.id, "tool:echo changed > a.txt && echo new > new.txt", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    // The files of the turn, after those of the read the worker's first word of its branches had the hub make.
    let of_the_turn =
        |changes: &&Observed<ChangeSummary>| matches!(changes, Observed::AsOf { value, .. } if !value.files.is_empty());
    both.until("the files", |sent| sent.iter().filter_map(changes_in).any(|changes| of_the_turn(&changes)));
    let changes = both.sent.iter().filter_map(changes_in).find(of_the_turn).unwrap();
    let Observed::AsOf { value: summary, as_of_ms } = changes else { unreachable!() };
    let files: Vec<_> = summary.files.iter().map(|file| (file.path.as_str(), file.status, file.lines)).collect();
    let line = |added, removed| Some(LineCounts { added, removed });
    assert_eq!(
        files,
        [("a.txt", ChangeStatus::Modified, line(1, 1)), ("new.txt", ChangeStatus::Untracked, line(1, 0))]
    );
    assert_eq!(summary.lines, LineCounts { added: 2, removed: 1 });
    assert!((before..=now_ms()).contains(as_of_ms), "read at {as_of_ms}, after {before}");
    // At the hub's level, after the files, so once the client that follows only the hub has it, it has every notice of
    // the read.
    client.workspace_rename(&workspace.id, "Renamed").unwrap();
    hub_only.until("the rename", |sent| sent.iter().any(renamed));
    assert_eq!(hub_only.sent.iter().rev().find_map(lines_in), Some(summary.lines), "the lines");
    assert!(!hub_only.sent.iter().any(|frame| changes_in(frame).is_some()), "{:#?}", hub_only.sent);
}

/// A connection that has said hello at the newest version this hub speaks, and follows each of `follows` through one
/// `Subscribe`, and sends nothing after.
fn following(stack: &Stack, follows: Vec<Follow>) -> RawConnection {
    let mut connection = RawConnection::open(stack);
    let hello = Hello {
        protocol_version: HUB_SPEAKS.newest,
        client: "following".into(),
        oldest_protocol_version: None,
        device: None,
    };
    let greeted = connection.call(0, &ClientRequest::Hello(hello));
    assert!(matches!(greeted, ReplyOutcome::Response(ClientResponse::Hello(_))), "{greeted:?}");
    let start = (1..).zip(follows).map(|(subscription_id, follow)| Subscription { subscription_id, follow }).collect();
    let subscribed = connection.call(1, &ClientRequest::Subscribe(Subscribe { stop: Vec::new(), start }));
    assert!(matches!(subscribed, ReplyOutcome::Response(ClientResponse::Subscribed(_))), "{subscribed:?}");
    connection
}

/// The changed files `frame` tells of, if it tells of any.
fn changes_in(frame: &HubFrame) -> Option<&Observed<ChangeSummary>> {
    let HubFrame::Notice { notice: HubNotice::Observation(Observation::WorkspaceChanges { changes, .. }), .. } = frame
    else {
        return None;
    };
    Some(changes)
}

/// The lines `frame` tells of, if it tells of them as of a read.
fn lines_in(frame: &HubFrame) -> Option<LineCounts> {
    let HubFrame::Notice { notice: HubNotice::Observation(Observation::WorkspaceLines { lines, .. }), .. } = frame
    else {
        return None;
    };
    let Observed::AsOf { value, .. } = lines else { return None };
    Some(*value)
}

fn renamed(frame: &HubFrame) -> bool {
    let HubFrame::Notice { notice: HubNotice::Workspace(workspace), .. } = frame else { return false };
    workspace.title.as_ref().map(protocol::model::Title::as_str) == Some("Renamed")
}

/// The time by this machine's clock, which is the hub's, in milliseconds since the Unix epoch.
fn now_ms() -> i64 {
    let since = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    i64::try_from(since.as_millis()).unwrap()
}
