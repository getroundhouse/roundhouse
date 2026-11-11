//! Cancelling.

#![cfg(test)]

use protocol::event::{ApprovalOutcome, EventBody, TurnOutcome};
use protocol::model::SessionStatus;

use crate::support::{
    Stack, assistant_text, pending_approval, read_all, wait_for_approval, wait_for_turn_end, wait_until,
};

#[test]
fn cancelling_stops_the_turn_and_keeps_the_session_usable() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    wait_until("the tool to start", || {
        read_all(&client, &session.id).iter().any(|e| matches!(e.body, EventBody::Assistant { .. }))
    });
    client.session_cancel(&session.id).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(events.iter().any(|e| matches!(e.body, EventBody::TurnEnded { outcome: TurnOutcome::Cancelled, .. })));

    client.session_prompt(&session.id, "say:after", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(assistant_text(&events).ends_with("after"));
}

#[test]
fn cancelling_closes_an_open_approval() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_planning(&client);
    client.session_prompt(&session.id, "tool:echo x", None).unwrap();
    let approval_id = wait_for_approval(&client, &session.id);
    client.session_cancel(&session.id).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(events.iter().any(|e| matches!(
        &e.body,
        EventBody::ApprovalResolved { approval_id: id, outcome: ApprovalOutcome::Cancelled } if *id == approval_id
    )));
    assert_eq!(pending_approval(&events), None);
}

#[test]
fn cancelling_with_nothing_running_is_harmless() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_cancel(&session.id).unwrap();
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);
}
