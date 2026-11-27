//! Model, approval mode and effort, through to a real agent: full access, unless the agent is planning. What the hub
//! saves, logs and refuses of a change, and what it gives the next launch, are its own tests', in
//! `hub/sessions/tests/settings.rs`, on stand-ins for its workers.

#![cfg(test)]

use std::path::PathBuf;

use client_core::ClientError;
use protocol::capability::approvals::{ApprovalMode, Asks};
use protocol::capability::settings::Effort::{self, Medium};
use protocol::capability::settings::Settings;
use protocol::client::ErrorCode;
use protocol::event::EventBody;
use protocol::fixtures::{full_access, plan_mode};
use protocol::model::ModelName;

use crate::support::{
    Stack, approve_plan, assistant_text, in_mode, read_all, wait_for_approval, wait_for_turn_end, wait_until,
};

/// The changes of mode in the log.
fn mode_changes(client: &client_core::Connection, session: &protocol::model::SessionId) -> Vec<ApprovalMode> {
    read_all(client, session)
        .into_iter()
        .filter_map(|e| match e.body {
            EventBody::AgentMode { mode } => Some(mode),
            _ => None,
        })
        .collect()
}

/// Settings that set only the model.
fn on_model(model: &'static str) -> Settings {
    Settings { model: Some(ModelName::named(model)), ..Settings::default() }
}

/// Settings that set only the effort.
fn at(effort: Effort) -> Settings {
    Settings { effort: Some(effort), ..Settings::default() }
}

/// What the fake agent's CLI was last launched with.
fn last_args(stack: &Stack) -> String {
    std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap()
}

/// The first launch names what the agent's description gives a new chat. A change then reaches the agent that's
/// running, which takes it without being started again, and one started after the hub restarts, from the hub's own
/// record.
#[test]
fn a_change_of_model_mode_and_effort_reaches_the_running_agent_and_the_next_launch() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_planning(&client);
    let defaults = agents::claude::description().defaults();
    assert_eq!(session.settings(), Settings { mode: Some(plan_mode()), ..defaults.clone() });
    client.session_prompt(&session.id, "tool:echo first > first.txt", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);
    client.session_decide(&session.id, &approval, protocol::fixtures::allow_once()).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let (model, effort) = (defaults.model.unwrap(), defaults.effort.unwrap());
    let launched = format!("--model {} --effort {}", model.as_str(), effort.as_str().unwrap());
    assert!(last_args(&stack).contains(&launched), "the description's: {}", last_args(&stack));

    let change = Settings { model: Some(ModelName::named("sonnet")), effort: Some(Medium), ..in_mode(full_access()) };
    let changed = client.session_configure(&session.id, change).unwrap();
    assert_eq!((changed.model, changed.mode, changed.effort), (ModelName::named("sonnet"), full_access(), Medium));

    // The same process, and it no longer asks, at the new level.
    client.session_prompt(&session.id, "tool:echo second > second.txt;effort", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    // The agent reports its new mode after taking it, so the report can come after the reply. It comes before what the
    // next turn says, though, so it's in the log by the turn's end.
    assert_eq!(mode_changes(&client, &session.id), [full_access()], "reported by the agent, and only by it");
    let path = PathBuf::from(workspace.path.clone().unwrap());
    assert!(path.join("second.txt").exists());
    assert!(assistant_text(&events).ends_with("effort=medium"), "{}", assistant_text(&events));
    assert_eq!(events.iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count(), 1);
    assert_eq!(events.iter().filter(|e| matches!(e.body, EventBody::ApprovalRequested { .. })).count(), 1);
    let last_init = events.iter().rev().find_map(|e| match &e.body {
        EventBody::AgentInit { model, mode, .. } => Some((model.clone(), *mode)),
        _ => None,
    });
    assert_eq!(last_init, Some((Some("sonnet".to_string()), Some(full_access()))));

    // A new process starts with what was chosen, from the hub's own record.
    drop(client);
    stack.stop_hub();
    stack.start_hub();
    let client = stack.client();
    let kept = client.session_get(&session.id).unwrap();
    assert_eq!((kept.mode, kept.effort), (full_access(), Medium));
    client.session_prompt(&session.id, "tool:echo third > third.txt;effort", None).unwrap();
    wait_until("the third file", || path.join("third.txt").exists());
    assert!(assistant_text(&wait_for_turn_end(&client, &session.id, 3)).ends_with("effort=medium"));
    let args = last_args(&stack);
    assert!(args.contains("--permission-mode bypassPermissions") && args.contains("--model sonnet"), "{args}");
    assert!(args.contains("--effort medium"), "{args}");
}

/// The agent's own refusals, which reach a client as a worker's error, so only the message says which this is: a model
/// its CLI doesn't know, and an effort it says it took and didn't, which only reading the level back shows. That a
/// refused change saves nothing is the hub's, and in its own tests.
#[test]
fn a_change_the_agent_refuses_or_doesnt_take_is_refused_and_changes_nothing() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "keep-effort", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let before = client.session_get(&session.id).unwrap();

    let unknown_model = client.session_configure(&session.id, on_model("not-a-model")).unwrap_err();
    let after_the_model = client.session_get(&session.id).unwrap();
    let effort_not_taken = client.session_configure(&session.id, at(Effort::Max)).unwrap_err();
    let after_the_effort = client.session_get(&session.id).unwrap();

    assert!(unknown_model.to_string().contains("not found"), "a model: {unknown_model}");
    assert_eq!(after_the_model.settings(), before.settings(), "a model: nothing changed");
    assert!(matches!(&effort_not_taken, ClientError::Api(e) if e.code == ErrorCode::Worker), "{effort_not_taken:?}");
    assert!(effort_not_taken.to_string().contains("kept its effort at xhigh"), "an effort: {effort_not_taken}");
    assert_eq!(after_the_effort.settings(), before.settings(), "an effort: nothing changed");
    assert_eq!(before.effort, Effort::Xhigh);
    client.session_prompt(&session.id, "say:still fine;effort", None).unwrap();
    let text = assistant_text(&wait_for_turn_end(&client, &session.id, 2));
    assert!(text.contains("still fine") && text.ends_with("effort=xhigh"), "the agent works on as it was: {text}");
}

#[test]
fn approving_a_plan_leaves_plan_mode_and_the_next_launch_agrees() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_planning(&client);
    client.session_prompt(&session.id, "plan:Do the thing", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);
    let asked = read_all(&client, &session.id).into_iter().find_map(|e| match e.body {
        EventBody::ApprovalRequested { request, .. } => Some(request.asks),
        _ => None,
    });
    assert!(matches!(&asked, Some(Asks::Plan { plan, .. }) if plan == "Do the thing"), "{asked:?}");
    assert_eq!(client.session_get(&session.id).unwrap().mode, plan_mode());

    approve_plan(&client, &session.id, &approval);
    wait_for_turn_end(&client, &session.id, 1);
    assert_eq!(client.session_get(&session.id).unwrap().mode, full_access());

    drop(client);
    stack.stop_hub();
    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "say:back", None).unwrap();
    assert!(assistant_text(&wait_for_turn_end(&client, &session.id, 2)).contains("back"));
    let args = last_args(&stack);
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
    let args = last_args(&stack);
    assert!(args.contains("--permission-mode bypassPermissions"), "{args}");

    approve_plan(&client, &session.id, &approval);
    wait_for_turn_end(&client, &session.id, 1);
    client.session_prompt(&session.id, "tool:echo made > made.txt", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);

    assert!(PathBuf::from(workspace.path.unwrap()).join("made.txt").exists());
    assert_eq!(events.iter().filter(|e| matches!(e.body, EventBody::ApprovalRequested { .. })).count(), 1);
    assert_eq!(client.session_get(&session.id).unwrap().mode, full_access());
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
    let args = last_args(&stack);
    assert!(args.contains("--permission-mode bypassPermissions"), "{args}");
}
