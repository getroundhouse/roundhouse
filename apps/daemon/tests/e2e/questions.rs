//! Questions from the agent.

#![cfg(test)]

use std::collections::BTreeMap;

use protocol::event::{ApprovalDecision, ApprovalOutcome, ContentBlock, EventBody, QuestionAnswer, SessionEvent};
use protocol::model::{ApprovalId, SessionStatus};

use crate::support::{Stack, wait_for_approval, wait_for_turn_end};

/// Whether a tool's result among `events` says `said`.
fn told(events: &[SessionEvent], said: &str) -> bool {
    events.iter().any(|e| match &e.body {
        EventBody::AgentUser { blocks, .. } => {
            blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { text, .. } if text.contains(said)))
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

#[test]
fn an_answer_gets_back_to_the_agent_in_every_mode() {
    let stack = Stack::start();
    let client = stack.client();
    // It asks even with full access, because only a person can answer it.
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "ask:Red or blue?=Red/Blue", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);

    let answers = vec![QuestionAnswer { picked: vec![1], typed: None }];
    client.session_approve(&session.id, &approval, ApprovalDecision::AnswerByPosition { answers }).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(told(&events, r#""Red or blue?"=Blue"#), "{events:?}");
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);
}

/// So the transcript can say what was decided, whatever the agent was told.
#[test]
fn what_was_answered_is_kept_on_the_record_as_it_was_given() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "ask:*Which days?=Monday/Friday|Why?=Because", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);

    let answers = vec![QuestionAnswer { picked: vec![0, 1], typed: Some("Sunday".into()) }, QuestionAnswer::default()];
    let decision = ApprovalDecision::AnswerByPosition { answers: answers.clone() };
    client.session_approve(&session.id, &approval, decision).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_eq!(recorded(&events, &approval), Some((None, Some(answers))), "{events:?}");
    assert!(told(&events, r#""Which days?"=Monday, Friday, Sunday"#), "{events:?}");
    assert!(!told(&events, r#""Why?"="#), "one left without an answer is left out");
}

/// As a client from before answers went by position sends them.
#[test]
fn answers_by_the_questions_text_still_get_to_the_agent_and_are_kept() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "ask:*Which days?=Monday/Friday|Why?=Because", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);

    let answers: BTreeMap<String, String> = [("Which days?".to_string(), "Monday, Friday".to_string())].into();
    let decision = ApprovalDecision::AnswerByText { answers: answers.clone() };
    client.session_approve(&session.id, &approval, decision).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_eq!(recorded(&events, &approval), Some((Some(answers), None)), "{events:?}");
    assert!(told(&events, r#""Which days?"=Monday, Friday"#), "{events:?}");
}

/// The words are the adapter's, since they're addressed to the agent.
#[test]
fn a_question_left_without_words_tells_the_agent_it_wasnt_answered() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "ask:Red or blue?=Red/Blue", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);

    let skipped = ApprovalDecision::Deny { message: None, interrupt: false };
    client.session_approve(&session.id, &approval, skipped).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(told(&events, "The user chose not to answer."), "{events:?}");
}
