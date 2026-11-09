//! A workspace's setup script.

#![cfg(test)]

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use client_core::ClientError;
use protocol::client::ErrorCode;
use protocol::event::{ContentBlock, EventBody};

use crate::support::{
    Stack, assistant_text, commit_settings, git, parent_of, read_all, signal, wait_for_setup, wait_for_turn_end,
    wait_until,
};

#[test]
fn a_new_workspace_gets_its_local_files_and_runs_the_setup_script_with_conductors_variables() {
    let stack = Stack::start();
    std::fs::write(stack.repo().join(".gitignore"), ".env*\n").unwrap();
    std::fs::write(stack.repo().join(".env.local"), "SECRET=1\n").unwrap();
    git(&stack.repo(), &["add", ".gitignore"]);
    git(&stack.repo(), &["commit", "-qm", "ignore"]);
    commit_settings(
        &stack,
        r#"[scripts]
setup = """
test -f .env.local && echo found-env
echo "$CONDUCTOR_WORKSPACE_NAME $CONDUCTOR_PORT $CONDUCTOR_ROOT_PATH" > setup-ran.txt
"""
"#,
    );
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    assert_eq!(wait_for_setup(&client, &workspace.id), protocol::model::SetupStatus::Succeeded);

    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    assert_eq!(std::fs::read_to_string(worktree.join(".env.local")).unwrap(), "SECRET=1\n");
    let ran = std::fs::read_to_string(worktree.join("setup-ran.txt")).unwrap();
    assert!(ran.starts_with(&format!("{} 55000 ", workspace.name)), "{ran}");
    assert!(ran.trim_end().ends_with("repo"), "{ran}");
    let log = client.workspace_setup_log(&workspace.id).unwrap();
    assert!(log.contains("Copied .env.local") && log.contains("found-env"), "{log}");
}

#[test]
fn a_prompt_sent_during_setup_is_recorded_at_once_and_delivered_when_it_finishes() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 1; touch ready.txt\"\n");
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let (seq, _) = client.session_prompt(&session.id, "tool:test -f ready.txt && echo was-ready", None).unwrap();
    assert_eq!(seq, 1);
    assert!(!read_all(&client, &session.id).iter().any(|e| matches!(e.body, EventBody::AgentStarted { .. })));
    let events = wait_for_turn_end(&client, &session.id, 1);
    let saw_it = events.iter().any(|e| match &e.body {
        EventBody::AgentUser { blocks, .. } => {
            blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { text, .. } if text.contains("was-ready")))
        }
        _ => false,
    });
    assert!(saw_it, "{events:#?}");
}

#[test]
fn a_failed_setup_warns_once_and_leaves_the_workspace_usable() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"echo nope; exit 7\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    assert_eq!(wait_for_setup(&client, &workspace.id), protocol::model::SetupStatus::Failed { code: Some(7) });
    assert!(client.workspace_setup_log(&workspace.id).unwrap().contains("nope"));

    client.session_prompt(&session.id, "say:one", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    client.session_prompt(&session.id, "say:two", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(assistant_text(&events).ends_with("two"));
    let warnings = events
        .iter()
        .filter(|e| matches!(&e.body, EventBody::Notice { message, .. } if message.contains("setup")))
        .count();
    assert_eq!(warnings, 1);
}

/// The app won't replace a hub that says this, since restarting would cut the script off.
#[test]
fn a_hub_reports_a_setup_for_as_long_as_it_runs() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"touch started.txt; sleep 60\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    wait_until("the script to start", || worktree.join("started.txt").exists());

    assert_eq!(client.hub_status().unwrap().running_setups, 1);

    client.session_cancel(&session.id).unwrap();
    wait_until("the hub to stop reporting it", || client.hub_status().unwrap().running_setups == 0);
}

/// Archiving carries on after its client has gone, so the app won't replace a hub that says this.
#[test]
fn a_hub_reports_a_request_for_as_long_as_it_works_on_it() {
    let stack = Stack::start();
    let marker = stack.path("archive-started");
    commit_settings(&stack, &format!("[scripts]\narchive = \"touch {}; sleep 2\"\n", marker.display()));
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    assert_eq!(client.hub_status().unwrap().requests, 1, "only this request");

    let archiving = stack.client();
    let id = workspace.id.clone();
    let archive = std::thread::spawn(move || archiving.workspace_archive(&id, true));
    wait_until("the archive script to start", || marker.exists());

    assert_eq!(client.hub_status().unwrap().requests, 2);
    archive.join().unwrap().unwrap();
    assert_eq!(client.hub_status().unwrap().requests, 1);
}

#[test]
fn stopping_interrupts_a_running_setup_and_it_can_be_run_again() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"touch started.txt; sleep 60\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    wait_until("the script to start", || worktree.join("started.txt").exists());
    client.session_prompt(&session.id, "say:hello", None).unwrap();

    client.session_cancel(&session.id).unwrap();
    assert_eq!(wait_for_setup(&client, &workspace.id), protocol::model::SetupStatus::Failed { code: None });
    // The waiting prompt goes through, with the warning.
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(assistant_text(&events).contains("hello"));

    std::fs::remove_file(worktree.join("started.txt")).unwrap();
    client.workspace_rerun_setup(&workspace.id).unwrap();
    wait_until("the script to start again", || worktree.join("started.txt").exists());
    let err = client.workspace_rerun_setup(&workspace.id).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict), "already running");
}

/// Stopped as soon as the workspace exists, which is before the worker has started the script.
#[test]
fn stopping_setup_before_its_script_has_started_still_stops_it() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 60\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);

    client.session_cancel(&session.id).unwrap();

    assert_eq!(wait_for_setup(&client, &workspace.id), protocol::model::SetupStatus::Failed { code: None });
}

/// A process running a command line containing this, or none.
fn pid_running(pattern: &str) -> Option<u32> {
    let out = Command::new("pgrep").args(["-f", pattern]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).split_whitespace().next().and_then(|p| p.parse().ok())
}

#[test]
fn a_setup_script_stops_with_its_hub_and_with_a_killed_worker() {
    let mut stack = Stack::start();
    // Unique to this test, so it can only find what this test started.
    let seconds = 7000 + std::process::id() % 1000;
    commit_settings(&stack, &format!("[scripts]\nsetup = \"sleep {seconds}\"\n"));
    let needle = format!("sleep {seconds}");
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    wait_until("the script to start", || pid_running(&needle).is_some());

    // A clean stop, promptly, and not after the hub gives up waiting on its worker.
    let stopping = Instant::now();
    client.hub_shutdown().unwrap();
    let mut hub = stack.hub.take().unwrap();
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
    assert!(stopping.elapsed() < Duration::from_secs(8), "took {:?}", stopping.elapsed());
    wait_until("the script to stop with it", || pid_running(&needle).is_none());
    drop(client);

    // A killed worker, which can't clean up after itself.
    stack.start_hub();
    let client = stack.client();
    assert_eq!(
        client.workspace_list(false).unwrap().into_iter().find(|w| w.id == workspace.id).unwrap().setup,
        protocol::model::SetupStatus::Failed { code: None },
        "it never finished"
    );
    client.workspace_rerun_setup(&workspace.id).unwrap();
    wait_until("the script to start again", || pid_running(&needle).is_some());
    let script = pid_running(&needle).unwrap();
    // SAFETY: kill(2) has no memory effects. This is the script's parent, the worker, by the chain below.
    let group_leader = parent_of(script);
    signal(parent_of(group_leader), libc::SIGKILL);
    wait_until("the script to be stopped", || pid_running(&needle).is_none());
}

#[test]
fn archiving_runs_the_archive_script_and_a_failure_blocks_unless_forced() {
    let stack = Stack::start();
    let marker = stack.path("archived-marker");
    commit_settings(&stack, &format!("[scripts]\narchive = \"touch {} && exit 1\"\n", marker.display()));
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    wait_for_setup(&client, &workspace.id);

    let err = client.workspace_archive(&workspace.id, false).unwrap_err();
    assert!(matches!(&err, ClientError::Api(e) if e.code == ErrorCode::NeedsForce), "{err}");
    assert!(err.to_string().contains("archive script"), "{err}");
    assert!(marker.exists());
    assert!(PathBuf::from(workspace.path.clone().unwrap()).exists(), "a failed script leaves the workspace alone");

    client.workspace_archive(&workspace.id, true).unwrap();
    assert!(!PathBuf::from(workspace.path.unwrap()).exists());
}

#[test]
fn restoring_runs_setup_again_because_what_it_made_went_with_the_worktree() {
    let stack = Stack::start();
    std::fs::write(stack.repo().join(".gitignore"), "made.txt\n").unwrap();
    git(&stack.repo(), &["add", "."]);
    git(&stack.repo(), &["commit", "-qm", "ignore"]);
    commit_settings(&stack, "[scripts]\nsetup = \"touch made.txt\"\n");
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    wait_for_setup(&client, &workspace.id);
    assert!(worktree.join("made.txt").exists());

    client.workspace_archive(&workspace.id, false).unwrap();
    assert!(!worktree.exists());
    client.workspace_restore(&workspace.id).unwrap();
    wait_until("setup to run again", || worktree.join("made.txt").exists());
    assert_eq!(wait_for_setup(&client, &workspace.id), protocol::model::SetupStatus::Succeeded);
}

#[test]
fn the_agent_sees_the_workspace_variables_and_each_workspace_has_its_own_ports() {
    let stack = Stack::start();
    let client = stack.client();
    let (first, session) = stack.workspace(&client);
    let (second, _) = stack.workspace(&client);
    assert_eq!(second.port_base, first.port_base + 10);
    client
        .session_prompt(
            &session.id,
            "env:CONDUCTOR_PORT; env:CONDUCTOR_SESSION_ID; env:CONDUCTOR_WORKSPACE_PATH; env:CONDUCTOR_IS_LOCAL",
            None,
        )
        .unwrap();
    let text = assistant_text(&wait_for_turn_end(&client, &session.id, 1));
    assert!(text.contains(&format!("CONDUCTOR_PORT={}", first.port_base)), "{text}");
    assert!(text.contains(&format!("CONDUCTOR_SESSION_ID={}", session.id)), "{text}");
    assert!(text.contains(&format!("CONDUCTOR_WORKSPACE_PATH={}", first.path.unwrap())), "{text}");
    assert!(text.contains("CONDUCTOR_IS_LOCAL=1"), "{text}");
}
