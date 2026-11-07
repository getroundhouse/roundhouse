//! Workspaces.

#![cfg(test)]

use std::path::PathBuf;

use client_core::{Client, ClientError};
use protocol::client::ErrorCode;
use protocol::event::EventBody;
use protocol::model::{DiffSide, SessionStatus, WorkspaceStatus};

use crate::support::{
    Stack, assistant_text, git, install_executable, is_alive, read_all, wait_for_turn_end, wait_until,
};

#[test]
fn archiving_protects_uncommitted_work_keeps_the_branch_and_restoring_brings_it_all_back() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    client.session_prompt(&session.id, "tool:echo work > work.txt; say:done", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let err = client.workspace_archive(&workspace.id, false).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::NeedsForce));
    assert!(worktree.join("work.txt").exists());
    client.session_prompt(&session.id, "say:the agent was left running by the refused archive", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    let started = events.iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1);

    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-qm", "the agent's work"]);
    let archived = client.workspace_archive(&workspace.id, false).unwrap();
    assert_eq!(archived.status, WorkspaceStatus::Archived);
    assert!(!worktree.exists());
    assert_eq!(client.workspace_list(false).unwrap().len(), 0);
    assert_eq!(client.workspace_list(true).unwrap().len(), 1);
    assert!(!git(&stack.repo(), &["branch", "--list", &workspace.branch]).is_empty());

    // Archiving it again can't work, and force wouldn't change that.
    let err = client.workspace_archive(&workspace.id, false).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));

    // An archived workspace takes no prompts, and its history stays readable.
    let err = client.session_prompt(&session.id, "say:no", None).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));
    let before = read_all(&client, &session.id);
    assert!(before.len() > 3);

    let restored = client.workspace_restore(&workspace.id).unwrap();
    assert_eq!(restored.status, WorkspaceStatus::Ready);
    assert_eq!(std::fs::read_to_string(worktree.join("work.txt")).unwrap(), "work\n");
    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 3);
    assert!(assistant_text(&events[before.len()..]).contains("tool:echo work"));
}

#[test]
fn archiving_keeps_a_branch_the_agent_renamed_and_restoring_finds_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "tool:git branch -m fix-the-login-bug; say:renamed", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let archived = client.workspace_archive(&workspace.id, false).unwrap();
    assert_eq!(archived.branch, "fix-the-login-bug");
    let restored = client.workspace_restore(&workspace.id).unwrap();
    let worktree = PathBuf::from(restored.path.unwrap());
    assert_eq!(git(&worktree, &["branch", "--show-current"]), "fix-the-login-bug");
}

#[test]
fn a_forced_archive_stops_a_running_agent() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    let mut agent_pid = None;
    wait_until("the tool to start", || {
        let events = read_all(&client, &session.id);
        agent_pid = events.iter().find_map(|e| match &e.body {
            EventBody::AgentStarted { pid, .. } => Some(*pid),
            _ => None,
        });
        agent_pid.is_some() && events.iter().any(|e| matches!(e.body, EventBody::Assistant { .. }))
    });
    std::fs::write(PathBuf::from(workspace.path.clone().unwrap()).join("dirty.txt"), "x").unwrap();
    client.workspace_archive(&workspace.id, true).unwrap();
    assert!(!is_alive(agent_pid.unwrap()));
    assert!(!PathBuf::from(workspace.path.unwrap()).exists());
    let session = client.session_get(&session.id).unwrap();
    assert_eq!(session.status, SessionStatus::Interrupted);
}

#[test]
fn status_and_diff_come_from_the_worker() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "tool:echo changed >> a.txt && echo new > new.txt", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let status = client.workspace_status(&workspace.id).unwrap();
    assert_eq!(status.branch.as_deref(), Some(workspace.branch.as_str()));
    let file = |path: &str| status.files.iter().find(|f| f.path == path).unwrap().status.clone();
    assert_eq!(file("a.txt"), ".M");
    assert_eq!(file("new.txt"), "??");

    let diff = client.workspace_diff(&workspace.id).unwrap();
    assert!(diff.patch.contains("+changed"), "{}", diff.patch);
    assert!(diff.patch.contains("+new"), "{}", diff.patch);
}

#[test]
fn comments_are_kept_by_the_hub_and_survive_a_restart() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let added = client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "Rename this").unwrap();
    assert!(client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "  ").is_err(), "an empty comment");
    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.comment_list(&workspace.id).unwrap(), vec![added.clone()]);
    client.comment_delete(&added.id).unwrap();
    assert_eq!(client.comment_list(&workspace.id).unwrap(), vec![]);
}

#[test]
fn a_workspace_with_no_pull_request_reports_none_rather_than_failing() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    assert_eq!(client.workspace_pull_request(&workspace.id).unwrap(), None);
}

#[test]
fn a_workspace_reports_its_pull_request_from_gh() {
    let mut stack = Stack::start();
    stack.kill_hub();
    // No login shell, so this PATH is the one used, with a stand-in `gh` at the front of it.
    let bin = stack.path("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let source = stack.path("gh.source");
    std::fs::write(
        &source,
        "#!/bin/sh\nprintf '{\"isDraft\":false,\"number\":7,\"state\":\"OPEN\",\"title\":\"T\",\"url\":\"https://example.com/7\"}'\n",
    )
    .unwrap();
    install_executable(&source, &bin.join("gh"));
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let mut command = stack.command();
    command.env("PATH", path).env("SHELL", "/usr/bin/false");
    stack.hub = Some(command.spawn().unwrap());
    wait_until("the hub", || Client::connect(&stack.socket(), "test").is_ok());

    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let pr = client.workspace_pull_request(&workspace.id).unwrap().unwrap();
    assert_eq!((pr.number, pr.state.as_str(), pr.url.as_str()), (7, "OPEN", "https://example.com/7"));
}

#[test]
fn workspace_names_are_checked_and_collisions_are_handled() {
    let stack = Stack::start();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();

    let err = client.workspace_create(&repo.id, Some("../escape".into()), None).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::InvalidRequest));

    let first = client.workspace_create(&repo.id, Some("mine".into()), None).unwrap();
    let err = client.workspace_create(&repo.id, Some("mine".into()), None).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::NameTaken));

    // A branch that exists outside the hub also blocks a chosen name, and leaves nothing behind.
    git(&stack.repo(), &["branch", "taken"]);
    let err = client.workspace_create(&repo.id, Some("taken".into()), None).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::NameTaken));
    assert_eq!(client.workspace_list(true).unwrap().len(), 1);
    assert_eq!(client.workspace_list(true).unwrap()[0].id, first.id);

    let generated = client.workspace_create(&repo.id, None, None).unwrap();
    assert_ne!(generated.name, first.name);
}
