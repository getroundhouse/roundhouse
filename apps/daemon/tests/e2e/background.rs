//! Work an agent leaves running in the background after its turn ends, through a real adapter, and the agent, its
//! worker or its hub going while it runs. What the hub's record makes of the adapter's lists is the hub's, in
//! `session::tests::status`, `hub::sessions::tests::background` and `hub::sessions::tests::chats`.

#![cfg(test)]

use protocol::event::{EventBody, TaskKind};
use protocol::model::SessionStatus;

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
/// and the turns they start leave the chat idle. It's the fake agent's own test too: each task ends on a thread of its
/// own there, and only a run of it shows that the last list it writes is the list as it is.
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
