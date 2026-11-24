//! A turn that fails on a request to the model: the chat stops short on it, its queue waits for the person, and the
//! next prompt carries on. The agent picking up by itself into the same failure is said to repeat it.

#![cfg(test)]

use protocol::event::{EventBody, FailureKind, SessionEvent, TurnOutcome};
use protocol::model::{ClientMessageId, SessionStatus};

use crate::steering::wait_for_tool;
use crate::support::{Gate, Stack, assistant_text, wait_for_status, wait_for_turn_end};

#[test]
fn a_turn_that_fails_on_a_request_stops_the_chat_short_and_holds_its_queue_until_the_person_sends_something() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let tool = Gate::new(&stack, "tool");
    client.session_prompt(&session.id, &format!("hang-until:{tool}; fail:rate_limit"), None).unwrap();
    wait_for_tool(&client, &session.id);
    client.session_queue(&session.id, ClientMessageId::from("q1"), "say:queued").unwrap();
    // Queued with the hub, which holds it until the turn ends, so it never reaches the agent meanwhile.
    tool.open();

    let events = wait_for_turn_end(&client, &session.id, 1);
    wait_for_status(&client, &session.id, SessionStatus::Idle);

    let failures: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.body {
            EventBody::RequestFailed { subagent: None, failure, .. } => Some((failure.kind, failure.resets_at_ms)),
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

/// A request the agent tries again reaches a client in the session's tail, which says which try and why, and nothing of
/// it is recorded. Its answer coming ends it, for the hub and the client alike.
#[test]
fn a_request_being_tried_again_is_in_the_tail_until_the_agent_answers() {
    use client_core::SubscriptionItem;
    use protocol::model::Seq;
    use protocol::tail::{Retrying, Tail};

    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let subscription = client.follow_session(&session.id, Seq::new(1)).unwrap();
    let answer = Gate::new(&stack, "answer");
    client.session_prompt(&session.id, &format!("retry-until:{answer}; say:answered"), None).unwrap();

    // What a client holds of the tail, as it follows the session, until `until` says so of it.
    let follow = |tail: &mut Tail, until: fn(&Tail) -> bool| {
        while !until(tail) {
            match subscription.items.recv_timeout(crate::support::TIMEOUT).expect("more of the session") {
                SubscriptionItem::Event(event) => tail.on_event(event.seq, event.run, &event.body),
                SubscriptionItem::Delta(delta) => tail.apply(&delta),
                SubscriptionItem::Tail(snapshot) => *tail = snapshot,
                SubscriptionItem::Notice(_)
                | SubscriptionItem::Listed(_)
                | SubscriptionItem::Terminal(_)
                | SubscriptionItem::TerminalPicture(_) => {}
            }
        }
    };
    let mut tail = Tail::default();
    follow(&mut tail, |tail| tail.retrying.is_some());
    let retrying = tail.retrying.unwrap();
    assert_eq!(
        Retrying { run: None, ..retrying },
        Retrying { run: None, attempt: 2, of: Some(10), status: Some(503), delay_ms: Some(1_204) }
    );
    assert!(retrying.run.is_some(), "the hub says whose");
    answer.open();
    follow(&mut tail, |tail| tail.retrying.is_none());

    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_eq!(assistant_text(&events), "answered");
    let kept = |e: &SessionEvent| matches!(&e.body, EventBody::AgentOther { kind, .. } if kind == "system/api_retry");
    assert!(!events.iter().any(kept), "nothing of it is recorded");
}
