//! A hub that's asked to stop, and what it stops with it.

#![cfg(test)]

use std::time::{Duration, Instant};

use protocol::event::EventBody;
use protocol::model::SessionStatus;

use crate::support::{
    Stack, TIMEOUT, assistant_text, is_alive, latest_started_pid, parent_of, read_all, signal, wait_for_turn_end,
    wait_until,
};

#[test]
fn a_clean_hub_stop_stops_workers_and_agents_and_the_next_hub_resumes() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    let agent_pid = events
        .iter()
        .find_map(|e| match &e.body {
            EventBody::AgentStarted { pid, .. } => Some(*pid),
            _ => None,
        })
        .unwrap();

    let stopping = Instant::now();
    client.hub_shutdown().unwrap();
    let mut hub = stack.hub.take().unwrap();
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
    assert!(!is_alive(agent_pid));
    // Well under the 15 s the hub waits for a stuck worker, so this was a clean stop.
    assert!(stopping.elapsed() < Duration::from_secs(8), "took {:?}", stopping.elapsed());
    assert!(!stack.socket().exists(), "the socket should be removed");

    stack.start_hub();
    let client = stack.client();
    // It was idle when stopped, so there's nothing to report as interrupted.
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Idle);
    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert!(assistant_text(&events).contains("say:first"));
}

#[test]
fn stopping_a_hub_with_several_workers_is_quick() {
    let mut stack = Stack::start();
    let client = stack.client();
    for _ in 0..4 {
        let (workspace, _) = stack.workspace(&client);
        client.workspace_status(&workspace.id).unwrap();
    }
    assert_eq!(client.hub_status().unwrap().workers, 4);
    let stopping = Instant::now();
    client.hub_shutdown().unwrap();
    let mut hub = stack.hub.take().unwrap();
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
    assert!(stopping.elapsed() < Duration::from_secs(8), "took {:?}", stopping.elapsed());
}

#[test]
fn a_hub_stop_ends_the_agents_of_a_worker_that_doesnt_stop() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    // Long enough to outlast the test. An agent whose input closes finishes its turn first.
    client.session_prompt(&session.id, "hang:120000", None).unwrap();
    let mut agent = None;
    wait_until("the agent to start", || {
        agent = latest_started_pid(&read_all(&client, &session.id));
        agent.is_some()
    });
    let agent = agent.unwrap();
    // So it can't stop its agents when it's asked to.
    signal(parent_of(agent), libc::SIGSTOP);

    client.hub_shutdown().unwrap();

    let mut hub = stack.hub.take().unwrap();
    // The hub gives a stuck worker 15 s.
    let deadline = Instant::now() + Duration::from_secs(15) + TIMEOUT;
    while matches!(hub.try_wait(), Ok(None)) {
        assert!(Instant::now() < deadline, "the hub didn't exit");
        std::thread::sleep(Duration::from_millis(50));
    }
    wait_until("the agent to stop", || !is_alive(agent));
}
