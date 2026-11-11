//! Several chats in one workspace.

#![cfg(test)]

use std::path::PathBuf;

use client_core::ClientError;
use protocol::client::{ErrorCode, HubNotice};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{AgentKind, RecordKey};

use crate::support::{
    Stack, assistant_text, commit_settings, in_mode, is_alive, next_notice, read_all, started_pid, wait_for_turn_end,
    wait_until,
};

#[test]
fn chats_in_a_workspace_run_side_by_side_and_keep_to_themselves() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, first) = stack.workspace(&client);
    let second = client.session_create(&workspace.id, AgentKind::Claude, in_mode("bypassPermissions"), None).unwrap();
    client.session_prompt(&first.id, "slow:15:from the first", None).unwrap();
    client.session_prompt(&second.id, "slow:15:from the second", None).unwrap();
    let first_events = wait_for_turn_end(&client, &first.id, 1);
    let second_events = wait_for_turn_end(&client, &second.id, 1);
    assert!(assistant_text(&first_events).contains("first") && !assistant_text(&first_events).contains("second"));
    assert!(assistant_text(&second_events).contains("second") && !assistant_text(&second_events).contains("first"));
    assert_ne!(started_pid(&first_events), started_pid(&second_events));
    let when = |events: &[SessionEvent], wanted: fn(&EventBody) -> bool| {
        events.iter().find(|e| wanted(&e.body)).unwrap().ts_ms
    };
    let started: fn(&EventBody) -> bool = |body| matches!(body, EventBody::AgentStarted { .. });
    let ended: fn(&EventBody) -> bool = |body| matches!(body, EventBody::TurnEnded { .. });
    assert!(when(&first_events, started) < when(&second_events, ended), "one waited for the other");
    assert!(when(&second_events, started) < when(&first_events, ended), "one waited for the other");

    // The first prompt names a chat, and later ones don't rename it.
    client.session_prompt(&first.id, "say:again", None).unwrap();
    wait_for_turn_end(&client, &first.id, 2);
    assert_eq!(client.session_get(&first.id).unwrap().title.as_deref(), Some("slow:15:from the first"));
}

#[test]
fn closing_a_chat_stops_only_its_agent_and_reopening_continues_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, closing) = stack.workspace(&client);
    let staying = client.session_create(&workspace.id, AgentKind::Claude, in_mode("bypassPermissions"), None).unwrap();
    client.session_prompt(&closing.id, "say:remember me", None).unwrap();
    client.session_prompt(&staying.id, "say:here", None).unwrap();
    let closing_pid = started_pid(&wait_for_turn_end(&client, &closing.id, 1)).unwrap();
    let staying_pid = started_pid(&wait_for_turn_end(&client, &staying.id, 1)).unwrap();

    let closed = client.session_close(&closing.id).unwrap();
    assert!(closed.closed_at_ms.is_some());
    wait_until("its agent to exit", || !is_alive(closing_pid));
    assert!(is_alive(staying_pid), "the other chat's agent went too");

    let refused = client.session_prompt(&closing.id, "say:hello?", None).unwrap_err();
    assert!(matches!(refused, ClientError::Api(e) if e.code == ErrorCode::Conflict));
    let listed = client.session_list(&workspace.id).unwrap();
    assert_eq!(listed.iter().find(|s| s.id == closing.id).unwrap().closed_at_ms, closed.closed_at_ms);

    assert_eq!(client.session_reopen(&closing.id).unwrap().closed_at_ms, None);
    client.session_prompt(&closing.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &closing.id, 2);
    assert!(assistant_text(&events).contains("remember me"), "{}", assistant_text(&events));
}

#[test]
fn closing_a_chat_with_a_prompt_waiting_on_setup_never_starts_its_agent() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 1\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "tool:echo no > never.txt", None).unwrap();
    client.session_close(&session.id).unwrap();
    wait_until("setup to finish", || {
        client.workspace_list(false).unwrap()[0].setup != protocol::model::SetupStatus::Running
    });
    // Commands run one at a time, so once this is refused, the first prompt has been dealt with.
    assert!(client.session_prompt(&session.id, "say:too late", None).is_err());
    assert!(!PathBuf::from(workspace.path.unwrap()).join("never.txt").exists());
    assert!(!read_all(&client, &session.id).iter().any(|e| matches!(e.body, EventBody::AgentStarted { .. })));
}

#[test]
fn closing_a_chat_nothing_was_sent_in_deletes_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, used) = stack.workspace(&client);
    client.session_prompt(&used.id, "say:keep me", None).unwrap();
    wait_for_turn_end(&client, &used.id, 1);
    let unused = client.session_create(&workspace.id, AgentKind::Claude, in_mode("bypassPermissions"), None).unwrap();
    let feed = client.feed_subscribe().unwrap();

    let closed = client.session_close(&unused.id).unwrap();

    assert_eq!(closed.closed_at_ms, None, "the reply is the chat as it was");
    let removed = loop {
        if let HubNotice::Removed { record, rev } = next_notice(&feed, "the chat's removal") {
            break (record, rev);
        }
    };
    assert_eq!(removed.0, RecordKey::Session { id: unused.id.clone() });
    assert!(removed.1 > closed.rev);
    let listed: Vec<_> = client.session_list(&workspace.id).unwrap().into_iter().map(|s| s.id).collect();
    assert_eq!(listed, std::slice::from_ref(&used.id));
    let gone = |e: ClientError| matches!(e, ClientError::Api(e) if e.code == ErrorCode::NotFound);
    assert!(gone(client.session_close(&unused.id).unwrap_err()), "closing it again");
    assert!(gone(client.session_prompt(&unused.id, "say:hello?", None).unwrap_err()));
}
