//! Effort: how hard a session's agent thinks.

#![cfg(test)]

use client_core::ClientError;
use protocol::client::ErrorCode;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{AgentKind, DEFAULT_EFFORT, Effort, ModelName, PermissionMode, SessionSettingsChange};

use crate::support::{Stack, assistant_text, wait_for_turn_end};

/// Settings that set only the effort.
fn at(effort: Effort) -> SessionSettingsChange {
    SessionSettingsChange { effort: Some(effort), ..SessionSettingsChange::default() }
}

/// What the agent said in its newest turn, which a script of `effort` makes the level it's using.
fn last_reply(events: &[SessionEvent]) -> String {
    let last_prompt = events.iter().rposition(|e| matches!(e.body, EventBody::UserPrompt { .. }));
    assistant_text(&events[last_prompt.map_or(0, |at| at + 1)..])
}

#[test]
fn an_effort_reaches_the_running_agent_and_the_next_launch() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "effort", None).unwrap();
    assert_eq!(last_reply(&wait_for_turn_end(&client, &session.id, 1)), "effort=xhigh", "the hub's default, to start");

    let changed = client.session_configure(&session.id, at(Effort::Medium)).unwrap();
    assert_eq!(changed.effort, Effort::Medium);

    // The same process, now at the new level.
    client.session_prompt(&session.id, "effort", None).unwrap();
    assert_eq!(last_reply(&wait_for_turn_end(&client, &session.id, 2)), "effort=medium");

    // A new process starts with it, from the hub's own record.
    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.session_get(&session.id).unwrap().effort, Effort::Medium);
    client.session_prompt(&session.id, "effort", None).unwrap();
    assert_eq!(last_reply(&wait_for_turn_end(&client, &session.id, 3)), "effort=medium");
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--effort medium"), "{args}");
}

#[test]
fn a_new_chat_starts_with_the_effort_it_was_made_with() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);

    let options = SessionSettingsChange { effort: Some(Effort::Max), ..SessionSettingsChange::default() };

    let made = client.session_create(&workspace.id, AgentKind::Claude, options, None).unwrap();
    assert_eq!(made.effort, Effort::Max);

    client.session_prompt(&made.id, "effort", None).unwrap();
    assert_eq!(last_reply(&wait_for_turn_end(&client, &made.id, 1)), "effort=max");
}

#[test]
fn an_effort_with_no_agent_running_is_saved_and_leaves_the_rest() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    let changed = client.session_configure(&session.id, at(Effort::Low)).unwrap();
    assert_eq!((changed.effort, changed.permission_mode), (Effort::Low, PermissionMode::FullAccess));
    let haiku = SessionSettingsChange { model: Some(ModelName::named("haiku")), ..SessionSettingsChange::default() };
    let changed = client.session_configure(&session.id, haiku).unwrap();
    assert_eq!(changed.effort, Effort::Low, "None leaves it as it is");
    assert_eq!(client.session_get(&session.id).unwrap().effort, Effort::Low);

    // The first agent launches with what the hub holds for the session, not only what it saved.
    client.session_prompt(&session.id, "effort", None).unwrap();

    assert_eq!(last_reply(&wait_for_turn_end(&client, &session.id, 1)), "effort=low");
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--effort low") && args.contains("--model haiku"), "{args}");
}

#[test]
fn an_effort_the_agent_doesnt_take_is_refused_and_not_saved() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "keep-effort", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let refused = client.session_configure(&session.id, at(Effort::Max)).unwrap_err();

    // Every refusal from an agent reaches a client as a worker error, so only the message says which this is.
    assert!(matches!(&refused, ClientError::Api(e) if e.code == ErrorCode::Worker), "{refused:?}");
    assert!(refused.to_string().contains("kept its effort at xhigh"), "{refused}");
    assert_eq!(client.session_get(&session.id).unwrap().effort, Effort::Xhigh);
    client.session_prompt(&session.id, "effort", None).unwrap();
    assert_eq!(last_reply(&wait_for_turn_end(&client, &session.id, 2)), "effort=xhigh");
}

#[test]
fn an_effort_this_hub_doesnt_know_is_refused() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);

    let refused = client.session_configure(&session.id, at(Effort::Unknown)).unwrap_err();
    assert!(matches!(refused, ClientError::Api(e) if e.code == ErrorCode::InvalidRequest));
    let refused = client.session_create(&workspace.id, AgentKind::Claude, at(Effort::Unknown), None).unwrap_err();
    assert!(matches!(refused, ClientError::Api(e) if e.code == ErrorCode::InvalidRequest));

    assert_eq!(client.session_get(&session.id).unwrap().effort, DEFAULT_EFFORT);
    assert_eq!(client.session_list(&workspace.id).unwrap().len(), 1);
}
