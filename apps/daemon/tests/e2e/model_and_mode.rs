//! Model and approval mode: full access, unless the agent is planning.

#![cfg(test)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use protocol::event::{ApprovalDecision, EventBody};
use protocol::model::PermissionMode::{FullAccess, Plan, Unknown};
use protocol::model::{
    AgentKind, DEFAULT_EFFORT, DEFAULT_MODEL, ModelName, PermissionMode, SessionSettings, SessionSettingsChange,
};

use crate::support::{
    Stack, assistant_text, commit_settings, in_mode, read_all, wait_for_approval, wait_for_turn_end, wait_until,
};

/// The changes of mode in the log.
fn mode_changes(client: &client_core::Client, session: &protocol::model::SessionId) -> Vec<PermissionMode> {
    read_all(client, session)
        .into_iter()
        .filter_map(|e| match e.body {
            EventBody::AgentMode { permission_mode } => Some(permission_mode),
            _ => None,
        })
        .collect()
}

/// Settings that set only the model.
fn on_model(model: &'static str) -> SessionSettingsChange {
    SessionSettingsChange { model: Some(ModelName::named(model)), ..SessionSettingsChange::default() }
}

#[test]
fn a_change_reaches_the_running_agent_and_the_next_launch() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_planning(&client);
    client.session_prompt(&session.id, "tool:echo first > first.txt", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);
    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    client.session_approve(&session.id, &approval, allow).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let changed = client
        .session_configure(
            &session.id,
            SessionSettingsChange { model: Some(ModelName::named("sonnet")), ..in_mode(FullAccess) },
        )
        .unwrap();
    assert_eq!(changed.permission_mode, FullAccess);
    assert_eq!(changed.model, ModelName::named("sonnet"));

    // The same process, and it no longer asks.
    client.session_prompt(&session.id, "tool:echo second > second.txt", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    // The agent reports its new mode after taking it, so the report can come after the reply. It comes before what the
    // next turn says, though, so it's in the log by the turn's end.
    assert_eq!(mode_changes(&client, &session.id), [FullAccess], "reported by the agent, and only by it");
    let path = PathBuf::from(workspace.path.clone().unwrap());
    assert!(path.join("second.txt").exists());
    assert_eq!(events.iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count(), 1);
    assert_eq!(events.iter().filter(|e| matches!(e.body, EventBody::ApprovalRequested { .. })).count(), 1);
    let last_init = events.iter().rev().find_map(|e| match &e.body {
        EventBody::AgentInit { model, permission_mode, .. } => Some((model.clone(), *permission_mode)),
        _ => None,
    });
    assert_eq!(last_init, Some((Some("sonnet".to_string()), Some(FullAccess))));

    // A new process starts with what was chosen, from the hub's own record.
    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.session_get(&session.id).unwrap().permission_mode, FullAccess);
    client.session_prompt(&session.id, "tool:echo third > third.txt", None).unwrap();
    wait_until("the third file", || path.join("third.txt").exists());
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--permission-mode bypassPermissions") && args.contains("--model sonnet"), "{args}");
}

/// No agent will report the change of mode, so the hub records it, and the log says when plan mode began.
#[test]
fn a_change_with_no_agent_running_is_saved_and_a_change_of_mode_goes_in_the_log() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let changed = client.session_configure(&session.id, in_mode(Plan)).unwrap();
    assert_eq!(changed.permission_mode, Plan);
    assert_eq!(changed.model, DEFAULT_MODEL, "None leaves a setting as it is");
    let changed = client.session_configure(&session.id, on_model("haiku")).unwrap();
    assert_eq!(changed.permission_mode, Plan, "None leaves a setting as it is");
    client.session_configure(&session.id, in_mode(Plan)).unwrap();
    let saved = client.session_get(&session.id).unwrap();
    assert_eq!((saved.model, saved.permission_mode), (ModelName::named("haiku"), Plan));
    assert_eq!(mode_changes(&client, &session.id), [Plan], "once, since the others changed no mode");

    client.session_prompt(&session.id, "say:planning", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    assert_eq!(mode_changes(&client, &session.id), [Plan], "an agent that starts in plan mode doesn't say so again");
}

#[test]
fn a_chat_made_with_no_model_or_effort_names_the_defaults_to_its_agent() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    assert_eq!(session.settings(), SessionSettings::default());

    client.session_prompt(&session.id, "say:ready", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    let launched = format!("--model {} --effort {}", DEFAULT_MODEL.as_str(), DEFAULT_EFFORT.as_str().unwrap());
    assert!(args.contains(&launched), "{args}");
}

/// Every agent has full access unless it's planning, so the hub refuses any other mode, for a new chat or a change.
#[test]
fn only_full_access_and_plan_mode_are_taken() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);

    let refused = client.session_create(&workspace.id, AgentKind::Claude, in_mode(Unknown), None).unwrap_err();
    assert!(refused.to_string().contains("full access or in plan mode"), "{refused}");
    assert_eq!(client.session_list(&workspace.id).unwrap().len(), 1, "no chat was made");
    client.session_configure(&session.id, in_mode(Unknown)).unwrap_err();
    assert_eq!(client.session_get(&session.id).unwrap().permission_mode, FullAccess);

    for mode in [Plan, FullAccess] {
        let changed = client.session_configure(&session.id, in_mode(mode)).unwrap();
        assert_eq!(changed.permission_mode, mode);
    }
}

#[test]
fn a_change_the_agent_refuses_changes_nothing() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:ready", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let before = client.session_get(&session.id).unwrap();

    let refused = client.session_configure(&session.id, on_model("not-a-model")).unwrap_err();
    assert!(refused.to_string().contains("not found"), "{refused}");
    let after = client.session_get(&session.id).unwrap();
    assert_eq!((after.model, after.permission_mode), (before.model, before.permission_mode));

    client.session_prompt(&session.id, "say:still fine", None).unwrap();
    assert!(assistant_text(&wait_for_turn_end(&client, &session.id, 2)).contains("still fine"));
}

#[test]
fn approving_a_plan_leaves_plan_mode_and_the_next_launch_agrees() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_planning(&client);
    client.session_prompt(&session.id, "plan:Do the thing", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);
    let asked = read_all(&client, &session.id).into_iter().find_map(|e| match e.body {
        EventBody::ApprovalRequested { tool_name, input, .. } => Some((tool_name, input)),
        _ => None,
    });
    let (tool_name, input) = asked.unwrap();
    assert_eq!((tool_name.as_str(), input["plan"].as_str()), ("ExitPlanMode", Some("Do the thing")));
    assert_eq!(client.session_get(&session.id).unwrap().permission_mode, Plan);

    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    client.session_approve(&session.id, &approval, allow).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    assert_eq!(client.session_get(&session.id).unwrap().permission_mode, FullAccess);

    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "say:back", None).unwrap();
    assert!(assistant_text(&wait_for_turn_end(&client, &session.id, 2)).contains("back"));
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--permission-mode bypassPermissions"), "{args}");
}

/// The agent goes into plan mode once it's running, because the CLI leaves a plan for the mode it was entered from,
/// and a launch straight into plan mode would come back asking before every call.
#[test]
fn an_agent_that_starts_in_plan_mode_has_full_access_once_its_plan_is_approved() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_planning(&client);
    client.session_prompt(&session.id, "plan:Make the file", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--permission-mode bypassPermissions"), "{args}");

    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    client.session_approve(&session.id, &approval, allow).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    client.session_prompt(&session.id, "tool:echo made > made.txt", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);

    assert!(PathBuf::from(workspace.path.unwrap()).join("made.txt").exists());
    assert_eq!(events.iter().filter(|e| matches!(e.body, EventBody::ApprovalRequested { .. })).count(), 1);
    assert_eq!(client.session_get(&session.id).unwrap().permission_mode, FullAccess);
}

#[test]
fn a_chat_with_no_mode_chosen_has_full_access() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);

    client.session_prompt(&session.id, "tool:echo made > made.txt", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);

    assert!(PathBuf::from(workspace.path.unwrap()).join("made.txt").exists());
    assert!(!events.iter().any(|e| matches!(e.body, EventBody::ApprovalRequested { .. })));
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--permission-mode bypassPermissions"), "{args}");
}

#[test]
fn a_mode_chosen_during_setup_applies_to_the_prompt_waiting_on_it() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 2\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace_planning(&client);
    client.session_prompt(&session.id, "tool:echo done > done.txt", None).unwrap();

    let started = Instant::now();
    client.session_configure(&session.id, in_mode(FullAccess)).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1), "waited for setup: {:?}", started.elapsed());
    assert_eq!(client.workspace_list(false).unwrap()[0].setup, protocol::model::SetupStatus::Running);

    let path = PathBuf::from(workspace.path.unwrap());
    wait_until("the tool to run without asking", || path.join("done.txt").exists());
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--permission-mode bypassPermissions"), "{args}");
}
