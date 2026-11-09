//! Effort: how hard a session's agent thinks.

#![cfg(test)]

use client_core::ClientError;
use protocol::client::ErrorCode;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{AgentKind, Effort, SessionOptions};

use crate::support::{Stack, assistant_text, in_mode, wait_for_turn_end};

/// Settings that set only the effort.
fn at(effort: Effort) -> SessionOptions {
    SessionOptions { effort: Some(effort), ..SessionOptions::default() }
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
    assert_eq!(last_reply(&wait_for_turn_end(&client, &session.id, 1)), "effort=high", "the user's own, to start");

    let changed = client.session_configure(&session.id, at(Effort::Xhigh)).unwrap();
    assert_eq!(changed.effort, Some(Effort::Xhigh));

    // The same process, now at the new level.
    client.session_prompt(&session.id, "effort", None).unwrap();
    assert_eq!(last_reply(&wait_for_turn_end(&client, &session.id, 2)), "effort=xhigh");

    // A new process starts with it, from the hub's own record.
    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.session_get(&session.id).unwrap().effort, Some(Effort::Xhigh));
    client.session_prompt(&session.id, "effort", None).unwrap();
    assert_eq!(last_reply(&wait_for_turn_end(&client, &session.id, 3)), "effort=xhigh");
    let args = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    assert!(args.contains("--effort xhigh"), "{args}");
}

#[test]
fn a_new_chat_starts_with_the_effort_it_was_made_with() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);

    let options = SessionOptions { effort: Some(Effort::Max), ..in_mode("bypassPermissions") };

    let made = client.session_create(&workspace.id, AgentKind::Claude, options, None).unwrap();
    assert_eq!(made.effort, Some(Effort::Max));

    client.session_prompt(&made.id, "effort", None).unwrap();
    assert_eq!(last_reply(&wait_for_turn_end(&client, &made.id, 1)), "effort=max");
}

#[test]
fn an_effort_with_no_agent_running_is_saved_and_leaves_the_rest() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    let changed = client.session_configure(&session.id, at(Effort::Low)).unwrap();
    assert_eq!((changed.effort, changed.permission_mode.as_deref()), (Some(Effort::Low), Some("bypassPermissions")));
    let haiku = SessionOptions { model: Some("haiku".into()), ..SessionOptions::default() };
    let changed = client.session_configure(&session.id, haiku).unwrap();
    assert_eq!(changed.effort, Some(Effort::Low), "None leaves it as it is");
    assert_eq!(client.session_get(&session.id).unwrap().effort, Some(Effort::Low));

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
    assert!(refused.to_string().contains("kept its effort at high"), "{refused}");
    assert_eq!(client.session_get(&session.id).unwrap().effort, None);
    client.session_prompt(&session.id, "effort", None).unwrap();
    assert_eq!(last_reply(&wait_for_turn_end(&client, &session.id, 2)), "effort=high");
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

    assert_eq!(client.session_get(&session.id).unwrap().effort, None);
    assert_eq!(client.session_list(&workspace.id).unwrap().len(), 1);
}
