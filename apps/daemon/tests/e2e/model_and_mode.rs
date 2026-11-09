//! Model and approval mode.

#![cfg(test)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use protocol::event::{ApprovalDecision, EventBody};
use protocol::model::SessionOptions;

use crate::support::{
    Stack, assistant_text, commit_settings, in_mode, read_all, wait_for_approval, wait_for_turn_end, wait_until,
};

/// Settings that set only the model.
fn on_model(model: &str) -> SessionOptions {
    SessionOptions { model: Some(model.into()), ..SessionOptions::default() }
}

#[test]
fn a_change_reaches_the_running_agent_and_the_next_launch() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "tool:echo first > first.txt", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);
    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    client.session_approve(&session.id, &approval, allow).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let changed = client
        .session_configure(&session.id, SessionOptions { model: Some("opus".into()), ..in_mode("bypassPermissions") })
        .unwrap();
    assert_eq!(changed.permission_mode.as_deref(), Some("bypassPermissions"));
    assert_eq!(changed.model.as_deref(), Some("opus"));

    // The same process, and it no longer asks.
    client.session_prompt(&session.id, "tool:echo second > second.txt", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    let path = PathBuf::from(workspace.path.clone().unwrap());
    assert!(path.join("second.txt").exists());
    assert_eq!(events.iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count(), 1);
    assert_eq!(events.iter().filter(|e| matches!(e.body, EventBody::ApprovalRequested { .. })).count(), 1);
    let last_init = events.iter().rev().find_map(|e| match &e.body {
        EventBody::AgentInit { model, permission_mode, .. } => Some((model.clone(), permission_mode.clone())),
        _ => None,
    });
    assert_eq!(last_init, Some((Some("opus".to_string()), Some("bypassPermissions".to_string()))));

    // A new process starts with what was chosen, from the hub's own record.
    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.session_get(&session.id).unwrap().permission_mode.as_deref(), Some("bypassPermissions"));
    client.session_prompt(&session.id, "tool:echo third > third.txt", None).unwrap();
    wait_until("the third file", || path.join("third.txt").exists());
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--permission-mode bypassPermissions") && args.contains("--model opus"), "{args}");
}

#[test]
fn a_change_with_no_agent_running_just_saves_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_with_mode(&client, None);
    let changed = client.session_configure(&session.id, in_mode("plan")).unwrap();
    assert_eq!(changed.permission_mode.as_deref(), Some("plan"));
    assert_eq!(changed.model, None, "None leaves a setting as it is");
    let changed = client.session_configure(&session.id, on_model("haiku")).unwrap();
    assert_eq!(changed.permission_mode.as_deref(), Some("plan"), "None leaves a setting as it is");
    let saved = client.session_get(&session.id).unwrap();
    assert_eq!((saved.model.as_deref(), saved.permission_mode.as_deref()), (Some("haiku"), Some("plan")));
}

#[test]
fn a_change_the_agent_refuses_changes_nothing() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:ready", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let refused = client.session_configure(&session.id, on_model("not-a-model")).unwrap_err();
    assert!(refused.to_string().contains("not found"), "{refused}");
    client.session_configure(&session.id, in_mode("nonsense")).unwrap_err();
    let after = client.session_get(&session.id).unwrap();
    assert_eq!((after.model, after.permission_mode.as_deref()), (None, Some("bypassPermissions")));

    client.session_prompt(&session.id, "say:still fine", None).unwrap();
    assert!(assistant_text(&wait_for_turn_end(&client, &session.id, 2)).contains("still fine"));
}

#[test]
fn approving_a_plan_leaves_plan_mode_and_the_next_launch_agrees() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_with_mode(&client, Some("plan"));
    client.session_prompt(&session.id, "plan:Do the thing", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);
    let asked = read_all(&client, &session.id).into_iter().find_map(|e| match e.body {
        EventBody::ApprovalRequested { tool_name, input, .. } => Some((tool_name, input)),
        _ => None,
    });
    let (tool_name, input) = asked.unwrap();
    assert_eq!((tool_name.as_str(), input["plan"].as_str()), ("ExitPlanMode", Some("Do the thing")));
    assert_eq!(client.session_get(&session.id).unwrap().permission_mode.as_deref(), Some("plan"));

    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    client.session_approve(&session.id, &approval, allow).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    assert_eq!(client.session_get(&session.id).unwrap().permission_mode.as_deref(), Some("manual"));

    // The CLI calls this mode `default` while running and only takes `manual` on launch.
    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "say:back", None).unwrap();
    assert!(assistant_text(&wait_for_turn_end(&client, &session.id, 2)).contains("back"));
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--permission-mode manual"), "{args}");
}

#[test]
fn a_mode_chosen_during_setup_applies_to_the_prompt_waiting_on_it() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 2\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "tool:echo done > done.txt", None).unwrap();

    let started = Instant::now();
    client.session_configure(&session.id, in_mode("bypassPermissions")).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1), "waited for setup: {:?}", started.elapsed());
    assert_eq!(client.workspace_list(false).unwrap()[0].setup, protocol::model::SetupStatus::Running);

    let path = PathBuf::from(workspace.path.unwrap());
    wait_until("the tool to run without asking", || path.join("done.txt").exists());
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--permission-mode bypassPermissions"), "{args}");
}
