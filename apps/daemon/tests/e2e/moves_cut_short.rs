//! A hub that goes while it's archiving or restoring a workspace, and what the next one records of it.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use client_core::Client;
use protocol::model::{SetupStatus, Workspace, WorkspaceId, WorkspaceStatus};

use crate::support::{Stack, commit_settings, git, install_executable, wait_for_setup, wait_for_turn_end, wait_until};
use crate::support::{assistant_text, workspace};

/// A shell line that says it has started, by making `started`, and then waits for `go` to be there.
fn held(started: &Path, go: &Path) -> String {
    format!("touch {}; until [ -e {} ]; do sleep 0.05; done", started.display(), go.display())
}

/// A ready workspace whose archive script waits to be let go, and the files that say it has started and let it go.
fn workspace_with_a_held_archive(stack: &Stack, client: &Client) -> (Workspace, PathBuf, PathBuf) {
    let (started, go) = (stack.path("archive-started"), stack.path("archive-go"));
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

fn wait_for_workspace(client: &Client, id: &WorkspaceId, status: WorkspaceStatus) {
    wait_until(&format!("the workspace to be {status:?}"), || workspace(client, id).status == status);
}

#[test]
fn an_archive_cut_short_with_the_worktree_still_there_leaves_the_workspace_ready() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (made, started, go) = workspace_with_a_held_archive(&stack, &client);
    archive_until_the_script_runs(&stack, &made.id, &started);

    stack.restart_hub();
    let client = stack.client();
    wait_for_workspace(&client, &made.id, WorkspaceStatus::Ready);

    // It's a workspace like any other: an agent runs in it, and it can be archived.
    std::fs::write(&go, "").unwrap();
    let session = client.session_list(&made.id).unwrap().remove(0);
    client.session_prompt(&session.id, "say:still here", None).unwrap();
    assert_eq!(assistant_text(&wait_for_turn_end(&client, &session.id, 1)), "still here");
    assert_eq!(client.workspace_archive(&made.id, false).unwrap().status, WorkspaceStatus::Archived);
}

#[test]
fn an_archive_cut_short_after_the_worktree_went_leaves_the_workspace_archived() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (made, started, go) = workspace_with_a_held_archive(&stack, &client);
    let worktree = PathBuf::from(made.path.clone().unwrap());
    git(&worktree, &["branch", "-m", "fix-the-login-bug"]);
    wait_until("the hub to record the branch", || {
        let branches = workspace(&client, &made.id).branches;
        branches.and_then(|b| b.checked_out().map(|c| c.name.to_string())).as_deref() == Some("fix-the-login-bug")
    });
    archive_until_the_script_runs(&stack, &made.id, &started);

    // As far as the hub got before it went: the worktree removed, and nothing recorded.
    stack.kill_hub();
    git(&stack.repo(), &["worktree", "remove", "--force", worktree.to_str().unwrap()]);
    std::fs::write(&go, "").unwrap();
    stack.start_hub();
    let client = stack.client();
    wait_for_workspace(&client, &made.id, WorkspaceStatus::Archived);

    // On the branch the worktree was last seen on, so it can come back.
    let archived = workspace(&client, &made.id);
    assert_eq!(archived.branch.as_str(), "fix-the-login-bug");
    assert!(archived.archived_at_ms.is_some());
    let restored = client.workspace_restore(&made.id).unwrap();
    assert_eq!(restored.status, WorkspaceStatus::Ready);
    assert_eq!(git(&worktree, &["branch", "--show-current"]), "fix-the-login-bug");
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
    let (started, go) = (stack.path("checkout-started"), stack.path("checkout-go"));
    let hook = stack.path("post-checkout");
    std::fs::write(&hook, format!("#!/bin/sh\n{}\n", held(&started, &go))).unwrap();
    install_executable(&hook, &stack.repo().join(".git/hooks/post-checkout"));
    let (asking, restored) = (stack.client(), made.id.clone());
    thread::spawn(move || {
        let _ = asking.workspace_restore(&restored);
    });
    wait_until("the checkout to start", || started.exists());
    assert_eq!(workspace(&client, &made.id).status, WorkspaceStatus::Restoring);

    stack.restart_hub();
    let client = stack.client();
    // The worktree's folder is there already, but the worker hasn't finished with it, so the hub isn't told yet. Long
    // enough for a hub that took the folder for the answer to have recorded it.
    assert!(worktree.exists());
    thread::sleep(Duration::from_millis(500));
    assert_eq!(workspace(&client, &made.id).status, WorkspaceStatus::Restoring);

    std::fs::write(&go, "").unwrap();
    wait_for_workspace(&client, &made.id, WorkspaceStatus::Ready);
    // As a restore that wasn't cut short ends: setup runs again, and an agent can work in it.
    assert_eq!(wait_for_setup(&client, &made.id), SetupStatus::Succeeded);
    assert!(workspace(&client, &made.id).archived_at_ms.is_none());
}
