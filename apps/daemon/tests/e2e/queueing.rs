//! Prompts queued while the agent works, which the hub sends one turn at a time once it has finished. One example,
//! through a client, a hub, a worker and an agent, and what spans a restart of the hub, or a client's sending: the
//! hub's own tests, in its `hub/queues`, have what the queue takes and refuses, and when the hub sends what's in it.

#![cfg(test)]

use client_core::{Client, FileToSend, Written};
use protocol::attachment::MediaType;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, QueuedPrompt, Seq, SessionId, SessionStatus};
use protocol::prompt::Prompt;

use crate::steering::wait_for_tool;
use crate::support::{Gate, Stack, assistant_text, wait_for_status, wait_for_turn_end, wait_until};
use protocol::prompt::PromptFrom;

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
    let tool = Gate::new(&stack, "tool");
    client.session_prompt(&session.id, &format!("hang-until:{tool}; say:working"), None).unwrap();
    wait_for_tool(&client, &session.id);

    queue(&client, &session.id, "q1", "say:first queued");
    let queued = queue(&client, &session.id, "q2", "say:second queued");

    assert_eq!(queued.iter().map(|q| q.client_msg_id.as_str()).collect::<Vec<_>>(), ["q1", "q2"]);
    tool.open();
    let events = wait_for_turn_end(&client, &session.id, 3);
    let ends = turn_ends(&events);
    assert_eq!(ends.len(), 3, "each queued prompt is a turn of its own");
    let (first, second) = (sent_at(&events, "q1").unwrap(), sent_at(&events, "q2").unwrap());
    assert!(ends[0] < first && first < ends[1] && ends[1] < second, "{ends:?}, {first}, {second}");
    assert_eq!(assistant_text(&events), "workingfirst queuedsecond queued");
    wait_until("the queue to empty", || client.session_get(&session.id).unwrap().queued.is_empty());
}

#[test]
fn prompts_are_queued_in_the_order_they_were_written_though_the_first_is_still_adding_its_file() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang", None).unwrap();
    wait_for_tool(&client, &session.id);
    // Big enough to take far longer to add than a prompt with nothing to add takes to queue.
    let big = FileToSend::checked("big.log", MediaType::Text, vec![b'a'; 40 << 20]).unwrap();
    let written =
        |id: &str, files| Written { client_msg_id: id.into(), prompt: Prompt::carrying(format!("say:{id}"), files) };

    let first = client.start_queueing(&session.id, written("q1", vec![big]));
    let second = client.start_queueing(&session.id, written("q2", Vec::new()));

    first.wait().unwrap();
    let queued = second.wait().unwrap().queued;
    assert_eq!(queued.iter().map(|q| q.client_msg_id.as_str()).collect::<Vec<_>>(), ["q1", "q2"]);
}

/// Sends the second of two queued prompts, as the person does, and checks it went first, once the first has gone after
/// it. A hub that was going to send the queue itself would have sent the first before this reached the session, since
/// it decides as it takes the session on, ahead of anything a client asks of it.
fn send_the_second_and_see_the_first_was_held(client: &Client, session_id: &SessionId, turns: usize) {
    client.session_send_queued(session_id, "q2".into()).unwrap();
    let events = wait_for_turn_end(client, session_id, turns);
    assert!(sent_at(&events, "q2").unwrap() < sent_at(&events, "q1").unwrap(), "held until the person sent from it");
    assert_eq!(assistant_text(&events), "sent from the queuefirst in line");
    let sent = client.session_get(session_id).unwrap();
    assert!(!sent.queue_held && sent.queued.is_empty());
}

#[test]
fn a_stopped_queue_stays_held_through_a_restart_until_the_person_sends_from_it() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang", None).unwrap();
    wait_for_tool(&client, &session.id);
    queue(&client, &session.id, "q1", "say:first in line");
    queue(&client, &session.id, "q2", "say:sent from the queue");
    client.session_cancel(&session.id).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Idle);
    assert!(client.session_get(&session.id).unwrap().queue_held);

    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    let found = client.session_get(&session.id).unwrap();
    assert!(found.queue_held && found.queued.len() == 2);

    // The stopped turn, then the one the person sent, and the one sent after it.
    send_the_second_and_see_the_first_was_held(&client, &session.id, 3);
}

#[test]
fn a_queue_whose_agent_went_away_mid_turn_is_held_and_sending_from_it_resumes_the_conversation() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang", None).unwrap();
    wait_for_tool(&client, &session.id);
    queue(&client, &session.id, "q1", "say:first in line");
    queue(&client, &session.id, "q2", "say:sent from the queue");

    stack.kill_hub_and_workers();
    stack.start_hub();
    let client = stack.client();
    let found = client.session_get(&session.id).unwrap();
    assert_eq!(found.status, SessionStatus::Interrupted);
    assert!(found.queue_held);

    // The turn that went away never ended.
    send_the_second_and_see_the_first_was_held(&client, &session.id, 2);
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
        let prompt = QueuedPrompt { client_msg_id: id.into(), prompt: Prompt::new(text), from: PromptFrom::Person };
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
