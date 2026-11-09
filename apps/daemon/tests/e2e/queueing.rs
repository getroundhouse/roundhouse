//! Prompts queued while the agent works, which the hub sends one turn at a time once it has finished.

#![cfg(test)]

use client_core::{Client, ClientError};
use protocol::client::ErrorCode;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, QueuedPrompt, Seq, SessionId, SessionStatus};

use crate::steering::wait_for_tool;
use crate::support::{Stack, assistant_text, read_all, wait_for_status, wait_for_turn_end, wait_until};

/// Where the prompt queued under `id` was recorded, once it has been.
fn sent_at(events: &[SessionEvent], id: &str) -> Option<Seq> {
    events.iter().find_map(|e| match &e.body {
        EventBody::UserPrompt { client_msg_id: Some(sent), .. } if sent.as_str() == id => Some(e.seq),
        _ => None,
    })
}

fn turn_ends(events: &[SessionEvent]) -> Vec<Seq> {
    events.iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).map(|e| e.seq).collect()
}

fn queue(client: &Client, session_id: &SessionId, id: &str, text: &str) -> Vec<QueuedPrompt> {
    client.session_queue(session_id, ClientMessageId::from(id), text).unwrap().queued
}

#[test]
fn queued_prompts_are_sent_in_order_each_as_a_turn_once_the_agent_has_finished() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:1000; say:working", None).unwrap();
    wait_for_tool(&client, &session.id);

    queue(&client, &session.id, "q1", "say:first queued");
    let queued = queue(&client, &session.id, "q2", "say:second queued");

    assert_eq!(queued.iter().map(|q| q.client_msg_id.as_str()).collect::<Vec<_>>(), ["q1", "q2"]);
    let events = wait_for_turn_end(&client, &session.id, 3);
    let ends = turn_ends(&events);
    assert_eq!(ends.len(), 3, "each queued prompt is a turn of its own");
    let (first, second) = (sent_at(&events, "q1").unwrap(), sent_at(&events, "q2").unwrap());
    assert!(ends[0] < first && first < ends[1] && ends[1] < second, "{ends:?}, {first}, {second}");
    assert_eq!(assistant_text(&events), "workingfirst queuedsecond queued");
    wait_until("the queue to empty", || client.session_get(&session.id).unwrap().queued.is_empty());
}

#[test]
fn a_queued_prompt_changes_or_goes_until_its_sent_and_not_after() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:1000", None).unwrap();
    wait_for_tool(&client, &session.id);
    queue(&client, &session.id, "q1", "say:before");
    queue(&client, &session.id, "q2", "say:taken out");

    let changed = queue(&client, &session.id, "q1", "say:after");
    let left = client.session_unqueue(&session.id, "q2".into()).unwrap().queued;

    assert_eq!(changed[0].text, "say:after", "a change keeps its place");
    assert_eq!(left.iter().map(|q| q.client_msg_id.as_str()).collect::<Vec<_>>(), ["q1"]);
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert_eq!(assistant_text(&events), "after");
    let late = client.session_queue(&session.id, "q1".into(), "say:too late");
    assert!(matches!(late, Err(ClientError::Api(e)) if e.code == ErrorCode::Conflict), "sent, so it can't change");
    assert!(client.session_get(&session.id).unwrap().queued.is_empty());
}

#[test]
fn a_prompt_queued_while_the_agent_is_idle_is_sent_at_once() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    queue(&client, &session.id, "q1", "say:straight away");

    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(sent_at(&events, "q1").is_some());
    assert_eq!(assistant_text(&events), "straight away");
}

#[test]
fn a_stopped_turn_holds_the_queue_until_another_turn_ends() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:20000", None).unwrap();
    wait_for_tool(&client, &session.id);
    queue(&client, &session.id, "q1", "say:held");

    client.session_cancel(&session.id).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Idle);
    let (next, _) = client.session_prompt(&session.id, "say:next", None).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 3);
    let ends = turn_ends(&events);
    let held = sent_at(&events, "q1").unwrap();
    assert!(
        next < ends[1] && ends[1] < held,
        "held past the stop, and sent after the next turn: {ends:?} {next} {held}"
    );
    assert!(read_all(&client, &session.id).iter().all(|e| !matches!(&e.body, EventBody::PromptDropped { .. })));
    assert!(!client.session_get(&session.id).unwrap().queue_held);
}

/// Long enough for a hub that was going to send a queue to have sent it.
fn not_sent_within_a_while(client: &Client, session_id: &SessionId, id: &str) {
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(sent_at(&read_all(client, session_id), id).is_none(), "held, so not sent");
}

#[test]
fn a_stopped_queue_stays_held_through_a_restart_until_the_person_sends_from_it() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:20000", None).unwrap();
    wait_for_tool(&client, &session.id);
    queue(&client, &session.id, "q1", "say:held");
    client.session_cancel(&session.id).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Idle);
    assert!(client.session_get(&session.id).unwrap().queue_held);

    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    let found = client.session_get(&session.id).unwrap();
    assert!(found.queue_held && found.queued.len() == 1);
    not_sent_within_a_while(&client, &session.id, "q1");

    client.session_send_queued(&session.id, "q1".into()).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(sent_at(&events, "q1").is_some());
    assert_eq!(assistant_text(&events), "held");
    let sent = client.session_get(&session.id).unwrap();
    assert!(!sent.queue_held && sent.queued.is_empty());
}

#[test]
fn a_queue_whose_agent_went_away_mid_turn_is_held_and_sending_from_it_resumes_the_conversation() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:20000", None).unwrap();
    wait_for_tool(&client, &session.id);
    queue(&client, &session.id, "q1", "say:after the restart");

    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    let found = client.session_get(&session.id).unwrap();
    assert_eq!(found.status, SessionStatus::Interrupted);
    assert!(found.queue_held);
    not_sent_within_a_while(&client, &session.id, "q1");

    client.session_send_queued(&session.id, "q1".into()).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(sent_at(&events, "q1").is_some());
    assert_eq!(assistant_text(&events), "after the restart");
}

#[test]
fn a_new_hub_sends_a_queue_the_last_one_left_ready() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    stack.kill_hub();
    // As a hub that went away with the agent idle, and before the queue went, leaves it.
    let store = hub::store::Store::open(&stack.paths().database, &stack.paths().backups_dir).unwrap();
    for (id, text) in [("q1", "say:first"), ("q2", "say:second")] {
        let prompt = QueuedPrompt { client_msg_id: id.into(), text: text.into() };
        assert!(matches!(store.queue_prompt(&session.id, &prompt).unwrap(), hub::store::Queued::Done(_)));
    }
    drop(store);

    stack.start_hub();

    let client = stack.client();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(sent_at(&events, "q1").unwrap() < sent_at(&events, "q2").unwrap(), "in order");
    assert_eq!(assistant_text(&events), "firstsecond");
    wait_until("the queue to empty", || client.session_get(&session.id).unwrap().queued.is_empty());
}

#[test]
fn a_queued_prompt_sent_now_steers_the_turn_and_the_rest_wait_their_turn() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:1000; say:working", None).unwrap();
    wait_for_tool(&client, &session.id);
    queue(&client, &session.id, "q1", "say:later");
    queue(&client, &session.id, "q2", "say:steered");

    let left = client.session_send_queued(&session.id, "q2".into()).unwrap().queued;
    let again = client.session_send_queued(&session.id, "q2".into()).unwrap().queued;

    assert_eq!(left.iter().map(|q| q.client_msg_id.as_str()).collect::<Vec<_>>(), ["q1"]);
    assert_eq!(again, left, "sent already, so nothing changes");
    let events = wait_for_turn_end(&client, &session.id, 2);
    let ends = turn_ends(&events);
    let (steered, later) = (sent_at(&events, "q2").unwrap(), sent_at(&events, "q1").unwrap());
    assert!(steered < ends[0] && ends[0] < later, "into the running turn, and q1 after it: {ends:?} {steered} {later}");
    assert_eq!(assistant_text(&events), "workingsteeredlater");
}
