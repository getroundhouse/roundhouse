//! A turn that fails on a request to the model: the chat stops short on it, its queue waits for the person, and the
//! next prompt carries on.

#![cfg(test)]

use protocol::event::{EventBody, FailureKind, SessionEvent, TurnOutcome};
use protocol::model::{ClientMessageId, SessionStatus};

use crate::steering::wait_for_tool;
use crate::support::{Stack, assistant_text, wait_for_status, wait_for_turn_end};

#[test]
fn a_turn_that_fails_on_a_request_stops_the_chat_short_and_holds_its_queue_until_the_person_sends_something() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:1000; fail:rate_limit", None).unwrap();
    wait_for_tool(&client, &session.id);
    client.session_queue(&session.id, ClientMessageId::from("q1"), "say:queued").unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    wait_for_status(&client, &session.id, SessionStatus::Idle);

    let failures: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.body {
            EventBody::RequestFailed { parent_tool_use_id: None, failure } => {
                Some((failure.kind, failure.resets_at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(failures, [(FailureKind::Limit, Some(1_700_000_000_000))]);
    let ended_on_it = |e: &SessionEvent| {
        matches!(e.body, EventBody::TurnEnded { outcome: TurnOutcome::Error, request_failed: true, .. })
    };
    assert!(events.iter().any(ended_on_it), "{events:#?}");
    assert_eq!(assistant_text(&events), "", "what the CLI said isn't the agent's words");
    let failed = client.session_get(&session.id).unwrap();
    assert!(failed.turn_failed() && failed.stopped_short(), "{failed:?}");
    assert!(failed.queue_held && failed.queued.len() == 1, "the queue waits for the person: {failed:?}");

    client.session_prompt(&session.id, "say:carrying on", None).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 3);
    // Held until then, since one that wasn't would have been sent before it.
    assert_eq!(assistant_text(&events), "carrying onqueued");
    let carried_on = client.session_get(&session.id).unwrap();
    assert!(!carried_on.stopped_short() && carried_on.reply_outcome == Some(TurnOutcome::Success), "{carried_on:?}");
}
