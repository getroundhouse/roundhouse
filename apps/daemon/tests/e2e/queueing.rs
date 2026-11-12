//! Prompts queued while the agent works, which the hub sends one turn at a time once it has finished.

#![cfg(test)]

use client_core::{Client, ClientError, FileToSend, Written};
use protocol::attachment::MediaType;
use protocol::client::{ErrorCode, HubNotice};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, QueuedPrompt, Seq, Session, SessionId, SessionStatus};

use crate::attachments::{png, refused_with};
use crate::steering::wait_for_tool;
use crate::support::{Stack, assistant_text, next_notice, read_all, wait_for_status, wait_for_turn_end, wait_until};

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

fn change(client: &Client, session_id: &SessionId, id: &str, text: &str) -> Result<Session, ClientError> {
    client.session_change_queued(session_id, ClientMessageId::from(id), text)
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

    let changed = change(&client, &session.id, "q1", "say:after").unwrap().queued;
    let left = client.session_unqueue(&session.id, "q2".into()).unwrap().queued;
    let taken_out = change(&client, &session.id, "q2", "say:changed once it was out");

    assert_eq!(changed[0].text, "say:after", "a change keeps its place");
    assert_eq!(left.iter().map(|q| q.client_msg_id.as_str()).collect::<Vec<_>>(), ["q1"]);
    assert_eq!(refused_with(taken_out), ErrorCode::NotFound, "out, so a change doesn't queue it again");
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert_eq!(assistant_text(&events), "after");
    let late = change(&client, &session.id, "q1", "say:too late");
    assert_eq!(refused_with(late), ErrorCode::Conflict, "sent, so it can't change");
    assert!(client.session_get(&session.id).unwrap().queued.is_empty());
}

/// Queuing a prompt is when the person last did something in its chat's workspace, which the feed says then, and the
/// hub sending it later, with nobody there, isn't.
#[test]
fn a_queued_prompt_moves_its_workspace_as_its_queued_and_not_again_as_its_sent() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:1000", None).unwrap();
    wait_for_tool(&client, &session.id);
    let feed = client.feed_subscribe().unwrap();

    let queued = client.session_queue(&session.id, "q1".into(), "say:after").unwrap().user_active_at_ms;

    // Said at once, rather than with its pull request, which the hub reads once the turn the prompt waits on ends.
    let told = loop {
        match next_notice(&feed, "the workspace, as it was worked in") {
            HubNotice::Workspace(w) if w.id == workspace.id => break w,
            HubNotice::Session(s) => assert_eq!(s.status, SessionStatus::Running, "the turn ended first: {s:?}"),
            _ => {}
        }
    };
    assert_eq!(told.user_active_at_ms, queued);
    wait_for_turn_end(&client, &session.id, 2);
    let listed = client.records_list().unwrap().workspaces.into_iter().find(|w| w.id == workspace.id).unwrap();
    assert_eq!(listed.user_active_at_ms, queued);
}

#[test]
fn a_queued_prompt_carries_its_files_to_the_agent_and_a_change_keeps_them() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:1000; say:working", None).unwrap();
    wait_for_tool(&client, &session.id);
    let log = b"error at line 42".to_vec();
    let log = client.attachment_add(&session.id, &"a1".into(), "build.log", MediaType::Text, log).unwrap();

    let queued = client.session_queue_with(&session.id, "q1".into(), "say:before", vec![log.id.clone()]).unwrap();
    let changed = change(&client, &session.id, "q1", "attached").unwrap().queued;

    assert_eq!(queued.queued[0].attachments, std::slice::from_ref(&log));
    assert_eq!(changed[0].attachments, std::slice::from_ref(&log), "a change keeps them");
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert_eq!(assistant_text(&events), "workingattached: build.log: error at line 42");
    let recorded = events.iter().find_map(|e| match &e.body {
        EventBody::UserPrompt { client_msg_id: Some(id), attachments, .. } if id.as_str() == "q1" => Some(attachments),
        _ => None,
    });
    assert_eq!(recorded, Some(&vec![log]));
    let placed = std::path::Path::new(workspace.path.as_deref().unwrap()).join(".context/attachments/build.log");
    assert_eq!(std::fs::read(placed).unwrap(), b"error at line 42");
}

#[test]
fn a_queued_prompt_needs_text_or_files_and_files_the_session_has() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    // Long enough that nothing queued here is sent before the end.
    client.session_prompt(&session.id, "hang:20000", None).unwrap();
    wait_for_tool(&client, &session.id);
    let shot = client.attachment_add(&session.id, &"a1".into(), "shot.png", MediaType::Png, png(64)).unwrap();

    let only_files = client.session_queue_with(&session.id, "q1".into(), "", vec![shot.id.clone()]).unwrap();
    let empty = client.session_queue_with(&session.id, "q2".into(), " ", Vec::new());
    let unknown = client.session_queue_with(&session.id, "q3".into(), "look", vec!["a9".into()]);

    assert_eq!(only_files.queued[0].attachments, [shot]);
    assert_eq!(refused_with(empty), ErrorCode::InvalidRequest);
    assert_eq!(refused_with(unknown), ErrorCode::NotFound);
    assert_eq!(client.session_get(&session.id).unwrap().queued.len(), 1, "neither was queued");
}

#[test]
fn prompts_are_queued_in_the_order_they_were_written_though_the_first_is_still_adding_its_file() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:20000", None).unwrap();
    wait_for_tool(&client, &session.id);
    // Big enough to take far longer to add than a prompt with nothing to add takes to queue.
    let big = FileToSend::checked("big.log", MediaType::Text, vec![b'a'; 40 << 20]).unwrap();
    let written = |id: &str, files| Written { client_msg_id: id.into(), text: format!("say:{id}"), files };

    let first = client.start_queueing(&session.id, written("q1", vec![big]));
    let second = client.start_queueing(&session.id, written("q2", Vec::new()));

    first.wait().unwrap();
    let queued = second.wait().unwrap().queued;
    assert_eq!(queued.iter().map(|q| q.client_msg_id.as_str()).collect::<Vec<_>>(), ["q1", "q2"]);
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
        let prompt = QueuedPrompt { client_msg_id: id.into(), text: text.into(), attachments: Vec::new() };
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
