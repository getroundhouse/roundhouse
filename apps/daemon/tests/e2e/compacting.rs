//! Asking the agent to compact its conversation.

#![cfg(test)]

use client_core::ClientError;
use protocol::client::ErrorCode;
use protocol::event::{CompactedBy, CompactionOutcome, EventBody};
use protocol::model::SessionStatus;

use crate::support::{Stack, read_all, wait_for_turn_end, wait_until};

#[test]
fn a_compaction_asked_for_is_recorded_and_answered_and_its_turn_says_it_was_only_that() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    client.session_compact(&session.id, Some("Keep the greeting")).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 2);
    let asked = events
        .iter()
        .find(|e| matches!(&e.body, EventBody::CompactionAsked { keeping: Some(k) } if k == "Keep the greeting"))
        .map(|e| e.seq)
        .expect("the request, recorded");
    assert!(events.iter().any(|e| matches!(e.body, EventBody::PromptTaken { prompt } if prompt == asked)));
    let compacted = events.iter().find_map(|e| match &e.body {
        EventBody::AgentCompacted { asked: Some(named), keeping, outcome } if *named == asked => {
            Some((keeping.clone(), outcome.clone()))
        }
        _ => None,
    });
    let (keeping, outcome) = compacted.expect("the compaction, naming the request");
    assert_eq!(keeping.as_deref(), Some("Keep the greeting"));
    let CompactionOutcome::Compacted { by, summary, .. } = outcome else { panic!("{outcome:?}") };
    assert_eq!(by, CompactedBy::Asked);
    assert!(summary.is_some_and(|summary| summary.contains("Keep the greeting")), "the agent was told what to keep");
    let ends: Vec<_> = events
        .iter()
        .filter_map(|e| match e.body {
            EventBody::TurnEnded { only_compacted, .. } => Some(only_compacted),
            _ => None,
        })
        .collect();
    let kinds: Vec<_> = events.iter().map(|e| e.body.kind()).collect();
    assert_eq!(ends, [false, true], "{kinds:?}");
    wait_until("the agent to be idle", || client.session_get(&session.id).unwrap().status == SessionStatus::Idle);
}

#[test]
fn asking_to_compact_before_any_conversation_or_while_the_agent_is_on_a_turn_is_refused_and_records_nothing() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let conflict = |refused: &ClientError| matches!(refused, ClientError::Api(e) if e.code == ErrorCode::Conflict);

    let before_any = client.session_compact(&session.id, None).unwrap_err();
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    wait_until("the agent to be working", || client.session_get(&session.id).unwrap().status == SessionStatus::Running);
    let on_a_turn = client.session_compact(&session.id, None).unwrap_err();

    assert!(conflict(&before_any), "{before_any:?}");
    assert!(conflict(&on_a_turn), "{on_a_turn:?}");
    let asked = read_all(&client, &session.id).iter().any(|e| matches!(e.body, EventBody::CompactionAsked { .. }));
    assert!(!asked);
    client.session_cancel(&session.id).unwrap();
}
