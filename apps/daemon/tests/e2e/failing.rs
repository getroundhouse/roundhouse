//! A turn that fails on a request to the model: the chat stops short on it, its queue waits for the person, and the
//! next prompt carries on. The agent picking up by itself into the same failure is said to repeat it.

#![cfg(test)]

use protocol::event::{Again, EventBody, FailureKind, SessionEvent, TurnOutcome};
use protocol::model::{ClientMessageId, SessionStatus};

use crate::steering::wait_for_tool;
use crate::support::{Stack, assistant_text, read_all, wait_for_status, wait_for_turn_end, wait_until};

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
            EventBody::RequestFailed { parent_tool_use_id: None, failure, .. } => {
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

/// The CLI goes on picking up by itself as its tasks in the background end, and while the limit holds, each turn is
/// refused the same way, with what the CLI sends as it starts a turn by itself between: the hub says each repeats the
/// one before.
#[test]
fn the_agent_picking_up_by_itself_into_the_same_failure_repeats_the_one_before() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let (first, second) = (stack.path("first"), stack.path("second"));
    let script = format!("background:{}; background:{}; fail:rate_limit", first.display(), second.display());
    // With work still going, the chat isn't idle between them.
    let ended = |count: usize| {
        wait_until(&format!("{count} turn(s) to end"), || {
            let events = read_all(&client, &session.id);
            events.iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).count() >= count
        });
    };
    client.session_prompt(&session.id, &script, None).unwrap();
    ended(1);

    // Moved into place whole, since the agent reads what it says as soon as it's there.
    let refuse = |task: &std::path::Path| {
        let script = task.with_extension("next");
        std::fs::write(&script, "fail:rate_limit").unwrap();
        std::fs::rename(&script, task).unwrap();
    };
    refuse(&first);
    ended(2);
    refuse(&second);
    let events = wait_for_turn_end(&client, &session.id, 3);

    let failed: Vec<_> = events
        .iter()
        .filter_map(|e| {
            let EventBody::RequestFailed { parent_tool_use_id: None, again, .. } = &e.body else { return None };
            Some((e.seq, *again))
        })
        .collect();
    let [(once, None), (twice, Some(again)), (_, Some(third))] = failed[..] else { panic!("{failed:?}") };
    assert_eq!(again, Again { previous: once, times: 2 });
    assert_eq!(third, Again { previous: twice, times: 3 });
    assert!(events.iter().any(|e| matches!(e.body, EventBody::OwnTurn(_))), "picked up by itself: {events:#?}");
}
