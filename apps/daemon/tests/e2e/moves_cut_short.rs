//! A hub that goes while it's archiving or restoring a workspace, and what the next one records of it: what spans
//! processes, the next hub settling each move as it starts, by asking the worker the last one left at it. What it
//! records from what that worker says is the hub's, and its table in `hub/workspaces/tests/making.rs` proves each
//! outcome on a stand-in for the worker.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::thread;

use client_core::Connection;
use process::ProcessRecord;
use protocol::model::{SetupStatus, Workspace, WorkspaceId, WorkspaceStatus};
use protocol::paths::WORKER_RECORD_FILE;

use crate::support::{Gate, Stack, commit_settings, wait_for_setup, wait_for_turn_end};
use crate::support::{assistant_text, wait_until, workspace};

/// A shell line that says it has started, by making `started`, and then waits for `go` to be opened.
fn held(started: &Path, go: &Gate) -> String {
    format!("touch {}; {}", started.display(), go.shell_wait())
}

/// A ready workspace whose archive script waits to be let go, the file that says it has started, and what lets it go.
fn workspace_with_a_held_archive(stack: &Stack, client: &Connection) -> (Workspace, PathBuf, Gate) {
    let (started, go) = (stack.path("archive-started"), Gate::new(stack, "archive-go"));
    commit_settings(stack, &format!("[scripts]\narchive = \"{}\"\n", held(&started, &go)));
    let (made, _) = stack.workspace(client);
    wait_for_setup(client, &made.id);
    (made, started, go)
}

/// Asks for the workspace to be archived, on a thread of its own, since the hub goes before it answers. Returns once
/// the archive script is running, with the record saying the workspace is being archived.
fn archive_until_the_script_runs(stack: &Stack, id: &WorkspaceId, started: &Path) {
    let (asking, archived) = (stack.client(), id.clone());
    thread::spawn(move || {
        let _ = asking.workspace_archive(&archived, false);
    });
    wait_until("the archive script to start", || started.exists());
    assert_eq!(workspace(&stack.client(), id).status, WorkspaceStatus::Archiving);
}

fn wait_for_workspace(client: &Connection, id: &WorkspaceId, status: WorkspaceStatus) {
    wait_until(&format!("the workspace to be {status:?}"), || workspace(client, id).status == status);
}

/// The PID of the worker serving the workspace's instance, as it recorded itself.
fn worker_of(stack: &Stack, client: &Connection, id: &WorkspaceId) -> u32 {
    let instance = stack.paths().instances_dir.join(workspace(client, id).instance_id.to_string());
    ProcessRecord::read(&instance.join(WORKER_RECORD_FILE)).expect("the worker's record").pid
}

#[test]
fn an_archive_cut_short_with_the_worktree_still_there_leaves_the_workspace_ready() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (made, started, go) = workspace_with_a_held_archive(&stack, &client);
    archive_until_the_script_runs(&stack, &made.id, &started);
    let archiving = worker_of(&stack, &client, &made.id);

    stack.restart_hub();
    let client = stack.client();
    wait_for_workspace(&client, &made.id, WorkspaceStatus::Ready);
    // Only the worker that was archiving it can say how far it got while it's still at it, so the next hub keeps it to
    // ask, rather than stopping it and starting another.
    assert_eq!(worker_of(&stack, &client, &made.id), archiving, "settled by the worker that was archiving it");

    // It's a workspace like any other: an agent runs in it, and it can be archived.
    go.open();
    let session = client.session_list(&made.id).unwrap().remove(0);
    client.session_prompt(&session.id, "say:still here", None).unwrap();
    assert_eq!(assistant_text(&wait_for_turn_end(&client, &session.id, 1)), "still here");
    assert_eq!(client.workspace_archive(&made.id, false).unwrap().status, WorkspaceStatus::Archived);
}

#[test]
fn a_restore_cut_short_is_settled_once_the_worker_has_finished_putting_the_worktree_back() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (made, _) = stack.workspace(&client);
    wait_for_setup(&client, &made.id);
    client.workspace_archive(&made.id, false).unwrap();
    let worktree = PathBuf::from(made.path.clone().unwrap());

    // Git runs this as it checks the worktree out, so the worker is still putting it back when the hub goes.
    let (started, go) = (stack.path("checkout-started"), Gate::new(&stack, "checkout-go"));
    let hook = stack.repo().join(".git/hooks/post-checkout");
    process::testing::write_program(&hook, format!("#!/bin/sh\n{}\n", held(&started, &go))).unwrap();
    let (asking, restored) = (stack.client(), made.id.clone());
    thread::spawn(move || {
        let _ = asking.workspace_restore(&restored);
    });
    wait_until("the checkout to start", || started.exists());
    assert_eq!(workspace(&client, &made.id).status, WorkspaceStatus::Restoring);
    let restoring = worker_of(&stack, &client, &made.id);

    stack.restart_hub();
    let client = stack.client();
    // The worktree's folder is there already, but the worker hasn't finished with it, so the hub isn't told yet.
    assert!(worktree.exists());
    assert_eq!(workspace(&client, &made.id).status, WorkspaceStatus::Restoring);

    go.open();
    wait_for_workspace(&client, &made.id, WorkspaceStatus::Ready);
    // By the worker that was putting the worktree back, which answers once it has finished, rather than one started
    // since, which would find the folder there at once. The hub's own tests have it take a worker's answer over that.
    assert_eq!(worker_of(&stack, &client, &made.id), restoring, "settled by the worker that was restoring it");
    // As a restore that wasn't cut short ends: setup runs again, and an agent can work in it.
    assert_eq!(wait_for_setup(&client, &made.id), SetupStatus::Succeeded);
    assert!(workspace(&client, &made.id).archived_at_ms.is_none());
}
