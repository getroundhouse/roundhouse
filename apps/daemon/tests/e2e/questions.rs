//! Questions from the agent.

#![cfg(test)]

use std::collections::BTreeMap;

use protocol::event::{ApprovalDecision, ApprovalOutcome, ContentBlock, EventBody};
use protocol::model::SessionStatus;

use crate::support::{Stack, wait_for_approval, wait_for_turn_end};

#[test]
fn an_answer_gets_back_to_the_agent_in_every_mode() {
    let stack = Stack::start();
    let client = stack.client();
    // It asks even with full access, because only a person can answer it.
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "ask:Red or blue?=Red/Blue", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);

    let answers = std::iter::once(("Red or blue?".to_string(), "Blue".to_string())).collect();
    client.session_approve(&session.id, &approval, ApprovalDecision::Answer { answers }).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    let told = events.iter().any(|e| match &e.body {
        EventBody::AgentUser { blocks, .. } => blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolResult { text, .. } if text.contains(r#""Red or blue?"=Blue"#))),
        _ => false,
    });
    assert!(told, "{events:?}");
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);
}

/// So the transcript can say what was decided, whatever the agent was told.
#[test]
fn what_was_answered_is_kept_on_the_record() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "ask:*Which days?=Monday/Friday|Why?=Because", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);

    let answers: BTreeMap<String, String> = [("Which days?".to_string(), "Monday, Friday".to_string())].into();
    client.session_approve(&session.id, &approval, ApprovalDecision::Answer { answers: answers.clone() }).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    let recorded = events.iter().find_map(|e| match &e.body {
        EventBody::ApprovalResolved { approval_id, outcome: ApprovalOutcome::Allowed, answers }
            if *approval_id == approval =>
        {
            Some(answers.clone())
        }
        _ => None,
    });
    assert_eq!(recorded, Some(Some(answers)), "{events:?}");
}
