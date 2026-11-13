//! Work an agent leaves running in the background after its turn ends.

#![cfg(test)]

use client_core::Client;
use protocol::event::{EventBody, SessionEvent, TaskKind};
use protocol::model::{SessionId, SessionStatus};

use crate::support::{
    Stack, assistant_text, is_alive, read_all, signal, started_pid, wait_for_status, wait_for_turn_end, wait_until,
};

#[test]
fn work_left_in_the_background_keeps_the_agent_at_work_until_it_finishes() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let done = stack.path("done");
    client.session_prompt(&session.id, &format!("background:{}; say:started", done.display()), None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Background);

    // An update waits on it, what it said is a reply, and its record says what's running.
    assert_eq!(client.hub_status().unwrap().running_sessions, 1);
    let waiting = client.session_get(&session.id).unwrap();
    assert!(waiting.has_reply());
    let described: Vec<_> = waiting.background_tasks.iter().map(|t| (t.description.clone(), t.kind)).collect();
    assert_eq!(described, [(Some(format!("Wait for {}", done.display())), TaskKind::Command)]);

    // The turn it starts by itself begins with no state event, and is a turn all the same, with a reply of its own.
    std::fs::write(&done, "").unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(assistant_text(&events).ends_with(&format!("finished {}", done.display())), "{}", assistant_text(&events));
    assert!(events.iter().any(|e| matches!(&e.body, EventBody::AgentTasks { tasks, .. } if tasks.is_empty())));
    let finished = client.session_get(&session.id).unwrap();
    assert!(finished.reply_seq > waiting.reply_seq, "{} after {}", finished.reply_seq, waiting.reply_seq);
    assert_eq!(finished.status, SessionStatus::Idle);
    assert!(finished.background_tasks.is_empty());
    assert_eq!(client.hub_status().unwrap().running_sessions, 0);
}

/// Each list the agent writes is the whole list, so the last one it writes as several tasks end together is empty,
/// and the turns they start leave the chat idle.
#[test]
fn several_tasks_ending_together_leave_the_chat_idle_with_none_listed() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let files = Vec::from_fn(5, |at| stack.path(&format!("done-{at}")));
    let each: Vec<_> = files.iter().map(|file| format!("background:{}", file.display())).collect();
    let now = || client.session_get(&session.id).unwrap();
    client.session_prompt(&session.id, &each.join("; "), None).unwrap();
    wait_until("the turn to end with five left going", || {
        let session = now();
        session.status == SessionStatus::Background && session.background_tasks.len() == 5
    });

    for file in &files {
        std::fs::write(file, "").unwrap();
    }

    wait_until("the chat to go idle with none listed", || {
        let session = now();
        session.status == SessionStatus::Idle && session.background_tasks.is_empty()
    });
}

/// Starts work in the background that finishes once `done` exists, waits until the turn has ended with it going, and
/// then has it finish, with the agent picking up again by itself `pause_ms` after.
fn finish_and_pause(stack: &Stack, client: &Client, session_id: &SessionId, pause_ms: u64) {
    let done = stack.path("done");
    client.session_prompt(session_id, &format!("background:{}", done.display()), None).unwrap();
    wait_for_status(client, session_id, SessionStatus::Background);
    // Moved into place whole, so the agent can't find it before it says how long.
    let pause = stack.path("pause");
    std::fs::write(&pause, pause_ms.to_string()).unwrap();
    std::fs::rename(&pause, &done).unwrap();
    wait_until("the work in the background to finish", || {
        read_all(client, session_id)
            .iter()
            .any(|event| matches!(&event.body, EventBody::AgentTasks { tasks, .. } if tasks.is_empty()))
    });
}

/// Between its work in the background finishing and the turn it starts to take that in, nothing has said the agent
/// is done, so the chat is still at work, and an update still waits for it.
#[test]
fn the_agent_is_at_work_from_its_work_in_the_background_finishing_until_it_picks_up_again() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    finish_and_pause(&stack, &client, &session.id, 1500);

    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Background);
    assert_eq!(client.hub_status().unwrap().running_sessions, 1);
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(assistant_text(&events).contains("finished"), "{}", assistant_text(&events));
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);
}

/// Longer than the hub waits for the agent to pick up again, so it takes the agent to be idle first, and the turn the
/// agent starts after that is a turn all the same.
#[test]
fn an_agent_that_takes_too_long_to_pick_up_again_is_taken_to_be_idle_and_its_turn_is_still_one() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    finish_and_pause(&stack, &client, &session.id, 18_000);
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Background);
    wait_for_status(&client, &session.id, SessionStatus::Idle);

    let from_the_hub = |event: &SessionEvent| event.run.is_none() && matches!(event.body, EventBody::AgentState { .. });
    let idle = read_all(&client, &session.id);
    assert_eq!(idle.iter().filter(|event| from_the_hub(event)).count(), 1, "{idle:#?}");
    assert_eq!(idle.iter().filter(|event| matches!(event.body, EventBody::TurnEnded { .. })).count(), 1);
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(assistant_text(&events).contains("finished"), "{}", assistant_text(&events));
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);
}

/// The agent only says a task is a monitor's a line after it first lists it, so the record has to follow the list
/// said again with the kind, though the same tasks are in it.
#[test]
fn a_task_a_monitor_call_started_is_recorded_as_a_monitor_beside_a_commands() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let (suite, watch) = (stack.path("suite"), stack.path("watch"));
    let script = format!("background:{}; monitor:{}; say:started", suite.display(), watch.display());

    client.session_prompt(&session.id, &script, None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Background);

    let kinds: Vec<_> = client.session_get(&session.id).unwrap().background_tasks.iter().map(|t| t.kind).collect();
    assert_eq!(kinds, [TaskKind::Command, TaskKind::Monitor]);
}

#[test]
fn stopping_the_hub_with_work_in_the_background_interrupts_it() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let never = stack.path("never");
    client.session_prompt(&session.id, &format!("background:{}", never.display()), None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Background);

    client.hub_shutdown().unwrap();
    let mut hub = stack.hub.take().unwrap();
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
    stack.start_hub();

    // The agent said it would carry on when its work finished, and that work went with it.
    let recovered = stack.client().session_get(&session.id).unwrap();
    assert_eq!(recovered.status, SessionStatus::Interrupted);
    assert!(recovered.background_tasks.is_empty());
}

/// As when the machine goes down. A hub that only crashed leaves the agent running with its work.
#[test]
fn an_agent_gone_with_its_hub_with_work_in_the_background_is_interrupted() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, &format!("background:{}", stack.path("never").display()), None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Background);
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    drop(client);

    // Nothing the agent says on its way out is recorded, so the next hub finds it at work.
    stack.kill_hub_and_workers();
    wait_until("the agent to stop", || !is_alive(agent));
    stack.start_hub();

    let recovered = stack.client().session_get(&session.id).unwrap();
    assert_eq!(recovered.status, SessionStatus::Interrupted);
    assert!(recovered.background_tasks.is_empty(), "the agent took its work with it");
}

#[test]
fn an_agent_stopped_from_outside_with_work_in_the_background_is_interrupted() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, &format!("background:{}", stack.path("never").display()), None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Background);
    let pid = started_pid(&read_all(&client, &session.id)).unwrap();

    assert_eq!(signal(pid, libc::SIGTERM), 0);

    // It stops its work and lists none left on the way out, which isn't that work finishing.
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
}
