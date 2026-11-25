//! A hub that's asked to stop, and what it stops with it.

#![cfg(test)]

use std::process::{Command, Stdio};
use std::time::Duration;

use hub::Timers;
use protocol::event::EventBody;
use protocol::model::SessionStatus;

use crate::support::{
    Stack, TIMEOUT, assistant_text, is_alive, latest_started_pid, parent_of, patient_with_workers, read_all, signal,
    wait_for_turn_end, wait_until,
};

#[test]
fn a_clean_hub_stop_stops_workers_and_agents_and_the_next_hub_resumes() {
    let mut stack = Stack::start_with(vec![patient_with_workers().env()]);
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

    client.hub_shutdown().unwrap();
    let mut hub = stack.hub.take().unwrap();
    // Long before the hub would give up on a stuck worker, so this was a clean stop.
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
    assert!(!is_alive(agent_pid));
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
    let mut stack = Stack::start_with(vec![patient_with_workers().env()]);
    let client = stack.client();
    for _ in 0..4 {
        let (workspace, _) = stack.workspace(&client);
        client.workspace_status(&workspace.id).unwrap();
    }
    assert_eq!(client.hub_status().unwrap().workers, 4);

    client.hub_shutdown().unwrap();

    // Long before the hub would give up on any of them, so none of them held it up.
    let mut hub = stack.hub.take().unwrap();
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
}

#[test]
fn a_hub_stop_ends_the_agents_of_a_worker_that_doesnt_stop() {
    let grace = Duration::from_secs(1);
    let mut stack = Stack::start_with(vec![Timers { shutdown_grace: grace, ..Timers::FOR_TESTS }.env()]);
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    // Until the turn is stopped. An agent whose input closes finishes its turn first, so it never exits by itself.
    client.session_prompt(&session.id, "hang", None).unwrap();
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
    waiting::thread::until(grace + TIMEOUT, "the hub didn't exit", || !matches!(hub.try_wait(), Ok(None)));
    wait_until("the agent to stop", || !is_alive(agent));
}

#[test]
fn a_tests_hub_stops_once_the_test_has_gone() {
    // Standing in for the test, which can be killed before it stops its hub. It runs until it's killed.
    let mut test = Command::new("cat").stdin(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
    let mut stack = Stack::start_with(vec![(protocol::testing::ENV_TEST_PROCESS, test.id().to_string())]);
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    client.workspace_status(&workspace.id).unwrap();
    assert_eq!(client.hub_status().unwrap().workers, 1);

    test.kill().unwrap();
    test.wait().unwrap();

    let mut hub = stack.hub.take().unwrap();
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
    assert!(hub.try_wait().unwrap().unwrap().success(), "it stopped, rather than failing");
    let log = std::fs::read_to_string(stack.hub_log()).unwrap();
    assert!(log.contains("hub.stops_with_gone"), "{log}");
    assert!(log.contains("hub.stopped"), "{log}");
}

/// A test's hub that's killed leaves its workers running for a next hub, as any hub does. A run that's killed before
/// the test starts that hub, or stops them, leaves them with neither, so a test's worker stops with its agents once the
/// test has gone.
#[test]
fn a_tests_worker_left_without_a_hub_stops_once_the_test_has_gone() {
    // Standing in for the test, as above.
    let mut test = Command::new("cat").stdin(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
    let mut stack = Stack::start_with(vec![(protocol::testing::ENV_TEST_PROCESS, test.id().to_string())]);
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    // Until the turn is stopped. An agent whose input closes finishes its turn first, so it never exits by itself.
    client.session_prompt(&session.id, "hang", None).unwrap();
    let mut agent = None;
    wait_until("the agent to start", || {
        agent = latest_started_pid(&read_all(&client, &session.id));
        agent.is_some()
    });
    let agent = agent.unwrap();
    let worker = parent_of(agent);
    stack.kill_hub();
    assert!(is_alive(worker), "the worker carries on for a next hub");

    test.kill().unwrap();
    test.wait().unwrap();

    wait_until("the worker to stop", || !is_alive(worker));
    wait_until("the agent to stop", || !is_alive(agent));
}
