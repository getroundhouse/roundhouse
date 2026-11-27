//! Questions from the agent.

#![cfg(test)]

use client_core::Connection;
use protocol::capability::approvals::Decision;
use protocol::capability::asking::QuestionAnswer;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{ApprovalId, SessionId, SessionStatus};

use crate::support::{Stack, read_all, wait_for_approval, wait_for_turn_end};

/// Whether a tool's result among `events` says `said`.
fn told(events: &[SessionEvent], said: &str) -> bool {
    events.iter().any(|e| match &e.body {
        EventBody::AgentUser { blocks, .. } => {
            blocks.iter().any(|b| matches!(b, ContentBlock::CallResult { output, .. } if client_core::calls::typed::output(output).contains(said)))
        }
        _ => false,
    })
}

/// What the person decided of `approval`, as the hub recorded it.
fn recorded(events: &[SessionEvent], approval: &ApprovalId) -> Option<Decision> {
    events.iter().find_map(|e| match &e.body {
        EventBody::ApprovalResolved { approval_id, decision: Some(decision), by_setting: false }
            if approval_id == approval =>
        {
            Some((**decision).clone())
        }
        _ => None,
    })
}

/// Has the agent ask what `asking` says, in the chat's `turn`th turn, decides it with `decision`, and returns the
/// question, and what that turn recorded.
fn answered(
    client: &Connection,
    chat: &SessionId,
    turn: usize,
    asking: &str,
    decision: Decision,
) -> (ApprovalId, Vec<SessionEvent>) {
    let before = read_all(client, chat).len();
    client.session_prompt(chat, &format!("ask:{asking}"), None).unwrap();
    let approval = wait_for_approval(client, chat);
    client.session_decide(chat, &approval, decision).unwrap();
    let events = wait_for_turn_end(client, chat, turn);
    (approval, events[before..].to_vec())
}

/// Each way a question is answered, one after another in one chat, each from a question of its own: what the agent is
/// told, and what the hub keeps on the record, as it was given, so the transcript can say what was decided, whatever
/// the agent was told.
#[test]
fn an_answer_gets_back_to_the_agent_and_is_kept_as_it_was_given_however_it_was_given() {
    let stack = Stack::start();
    let client = stack.client();
    // It asks even with full access, because only a person can answer it.
    let (_, session) = stack.workspace(&client);
    let both = "*Which days?=Monday/Friday|Why?=Because";

    let picked = Decision::Answer { answers: vec![QuestionAnswer { picked: vec![1], typed: None }] };
    let (_, events) = answered(&client, &session.id, 1, "Red or blue?=Red/Blue", picked);
    assert!(told(&events, r#""Red or blue?"=Blue"#), "by position, in full access: {events:?}");
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);

    let answers = vec![QuestionAnswer { picked: vec![0, 1], typed: Some("Sunday".into()) }, QuestionAnswer::default()];
    let decision = Decision::Answer { answers };
    let (approval, events) = answered(&client, &session.id, 2, both, decision.clone());
    assert_eq!(recorded(&events, &approval), Some(decision), "kept as it was given: {events:?}");
    assert!(told(&events, r#""Which days?"=Monday, Friday, Sunday"#), "picked and typed: {events:?}");
    assert!(!told(&events, r#""Why?"="#), "one left without an answer is left out");

    // None answered, which the agent is told in the adapter's words, since they're addressed to it.
    let skipped = Decision::Answer { answers: vec![QuestionAnswer::default()] };
    let (_, events) = answered(&client, &session.id, 3, "Red or blue?=Red/Blue", skipped);
    assert!(told(&events, "The user chose not to answer."), "left without words: {events:?}");
}
