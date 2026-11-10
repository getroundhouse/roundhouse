//! Reading and following the agent's own events apart from a subagent's.

#![cfg(test)]

use client_core::Client;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Place, SessionId, ToolUseId};

use crate::support::{Follower, Stack, read_all, wait_for_turn_end, wait_until};

/// The call that started the subagent, once it's in the log.
fn subagent_call(client: &Client, session_id: &SessionId) -> ToolUseId {
    let mut call = None;
    wait_until("the subagent's call", || {
        call = read_all(client, session_id).iter().find_map(|event| match &event.body {
            EventBody::Assistant { blocks, .. } => blocks.iter().find_map(|block| match block {
                ContentBlock::ToolUse { id, name, .. } if name == "Agent" => Some(ToolUseId::from(id.clone())),
                _ => None,
            }),
            _ => None,
        });
        call.is_some()
    });
    call.unwrap()
}

/// Checks that `events` are all from `subagent`, or the agent's own, in place order from the first.
fn assert_placed(events: &[SessionEvent], subagent: Option<&ToolUseId>) {
    for (place, event) in (1..).zip(events) {
        assert_eq!((event.subagent.as_ref(), event.place), (subagent, Place::new(place)), "{event:?}");
    }
}

fn reported(follower: &Follower) -> bool {
    follower.events.iter().any(|event| {
        matches!(&event.body, EventBody::Assistant { blocks, .. }
            if matches!(blocks.as_slice(), [ContentBlock::Text { text }] if text == "There's a README here."))
    })
}

#[test]
fn the_agents_own_events_and_a_subagents_are_read_and_followed_apart() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "agent:Look around", None).unwrap();
    let call = subagent_call(&client, &session.id);
    let (own, subagent) = (client.session_subscribe_places(&session.id, None, Place::new(1)).unwrap(), {
        client.session_subscribe_places(&session.id, Some(&call), Place::new(1)).unwrap()
    });
    let (mut agent, mut work) = (Follower::default(), Follower::default());

    work.drain_until(&subagent, "the subagent to write", |work| work.tail_text().contains("There's"));
    work.drain_until(&subagent, "the subagent's report", reported);
    wait_for_turn_end(&client, &session.id, 1);
    let (read, last) = client.session_read_places(&session.id, None, Place::new(1), 1000).unwrap();
    agent.drain_until(&own, "the agent's own events", |agent| {
        assert!(!agent.tail_text().contains("README"), "the agent's tail holds only what it writes");
        agent.events.len() >= read.len()
    });

    assert_placed(&work.events, Some(&call));
    assert_eq!(work.events.len(), 3, "its call, the call's result, and its report");
    assert_placed(&agent.events, None);
    assert!(agent.events.iter().any(|event| matches!(event.body, EventBody::TurnEnded { .. })));
    assert_eq!((&agent.events[..read.len()], last), (read.as_slice(), Place::new(u64::try_from(read.len()).unwrap())));
    let (rest, last) = client.session_read_places(&session.id, Some(&call), Place::new(2), 1000).unwrap();
    assert_eq!((rest.as_slice(), last), (&work.events[1..], Place::new(3)));
}

#[test]
fn a_subagent_cut_off_as_it_writes_leaves_its_follower_nothing_half_written() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "agent:Look around", None).unwrap();
    let call = subagent_call(&client, &session.id);
    let subagent = client.session_subscribe_places(&session.id, Some(&call), Place::new(1)).unwrap();
    let mut work = Follower::default();
    work.drain_until(&subagent, "the subagent to write", |work| work.tail_text().contains("There's"));

    client.session_cancel(&session.id).unwrap();

    // The end of the turn ends what the subagent was writing, though its follower never sees that event.
    work.drain_until(&subagent, "what the subagent was writing to go", |work| work.tail.is_empty());
    wait_for_turn_end(&client, &session.id, 1);
    assert!(!reported(&work));
    assert_placed(&work.events, Some(&call));
}
