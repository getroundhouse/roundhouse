//! Questions from the agent.

#![cfg(test)]

use std::collections::BTreeMap;

use client_core::Client;
use protocol::event::{ApprovalDecision, ApprovalOutcome, ContentBlock, EventBody, QuestionAnswer, SessionEvent};
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

/// What the hub recorded of the answers to `approval`, by text and by position.
type Recorded = (Option<BTreeMap<String, String>>, Option<Vec<QuestionAnswer>>);

fn recorded(events: &[SessionEvent], approval: &ApprovalId) -> Option<Recorded> {
    events.iter().find_map(|e| match &e.body {
        EventBody::ApprovalResolved {
            approval_id,
            outcome: ApprovalOutcome::Allowed,
            answers_by_text,
            answers_by_position,
            by_setting: false,
        } if approval_id == approval => Some((answers_by_text.clone(), answers_by_position.clone())),
        _ => None,
    })
}

/// Has the agent ask what `asking` says, in the chat's `turn`th turn, answers it as `answer` says, and returns the
/// question, and what that turn recorded.
fn answered(
    client: &Client,
    chat: &SessionId,
    turn: usize,
    asking: &str,
    answer: ApprovalDecision,
) -> (ApprovalId, Vec<SessionEvent>) {
    let before = read_all(client, chat).len();
    client.session_prompt(chat, &format!("ask:{asking}"), None).unwrap();
    let approval = wait_for_approval(client, chat);
    client.session_approve(chat, &approval, answer).unwrap();
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

    let picked = vec![QuestionAnswer { picked: vec![1], typed: None }];
    let (_, events) = answered(&client, &session.id, 1, "Red or blue?=Red/Blue", by_position(picked));
    assert!(told(&events, r#""Red or blue?"=Blue"#), "by position, in full access: {events:?}");
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);

    let answers = vec![QuestionAnswer { picked: vec![0, 1], typed: Some("Sunday".into()) }, QuestionAnswer::default()];
    let (approval, events) = answered(&client, &session.id, 2, both, by_position(answers.clone()));
    assert_eq!(recorded(&events, &approval), Some((None, Some(answers))), "kept by position: {events:?}");
    assert!(told(&events, r#""Which days?"=Monday, Friday, Sunday"#), "picked and typed: {events:?}");
    assert!(!told(&events, r#""Why?"="#), "one left without an answer is left out");

    // As a client from before answers went by position sends them.
    let answers: BTreeMap<String, String> = [("Which days?".to_string(), "Monday, Friday".to_string())].into();
    let by_text = ApprovalDecision::AnswerByText { answers: answers.clone() };
    let (approval, events) = answered(&client, &session.id, 3, both, by_text);
    assert_eq!(recorded(&events, &approval), Some((Some(answers), None)), "kept by text: {events:?}");
    assert!(told(&events, r#""Which days?"=Monday, Friday"#), "by text: {events:?}");

    // The words are the adapter's, since they're addressed to the agent.
    let skipped = ApprovalDecision::Deny { message: None, interrupt: false };
    let (_, events) = answered(&client, &session.id, 4, "Red or blue?=Red/Blue", skipped);
    assert!(told(&events, "The user chose not to answer."), "left without words: {events:?}");
}

fn by_position(answers: Vec<QuestionAnswer>) -> ApprovalDecision {
    ApprovalDecision::AnswerByPosition { answers }
}
