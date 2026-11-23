//! Each workspace's lines added and removed, which the hub reads from the workspace's worker as a turn ends there, and
//! sends every client that follows it. The hub's own tests cover when it reads them, and what it keeps of a refusal.

#![cfg(test)]

use std::time::SystemTime;

use client_core::RecordsFeed;
use protocol::model::{LineCounts, Observed};

use crate::support::{Stack, wait_for_turn_end, wait_until};

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

/// The time by this machine's clock, which is the hub's, in milliseconds since the Unix epoch.
fn now_ms() -> i64 {
    let since = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    i64::try_from(since.as_millis()).unwrap()
}
