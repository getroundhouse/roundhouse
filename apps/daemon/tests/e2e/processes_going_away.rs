//! Processes going away.

#![cfg(test)]

use std::process::Command;
use std::time::{Duration, Instant};

use client_core::{Client, ClientError};
use protocol::client::ErrorCode;
use protocol::event::{ApprovalDecision, EventBody, ExitReason, SessionEvent};
use protocol::model::SessionStatus;

use crate::support::{
    Stack, assert_gapless, assistant_text, is_alive, latest_started_pid, parent_of, pending_approval, read_all, signal,
    started_pid, wait_for_approval, wait_for_status, wait_for_turn_end, wait_until,
};

#[test]
fn a_hub_crash_stops_the_agent_and_the_next_hub_resumes_the_conversation() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "say:remember mango; tool:echo x", None).unwrap();
    wait_for_approval(&client, &session.id);
    let events = read_all(&client, &session.id);
    let agent_pid = events
        .iter()
        .find_map(|e| match &e.body {
            EventBody::AgentStarted { pid, .. } => Some(*pid),
            _ => None,
        })
        .unwrap();
    drop(client);

    stack.kill_hub();
    wait_until("the agent to stop after the hub died", || !is_alive(agent_pid));

    stack.start_hub();
    let client = stack.client();
    let recovered = client.session_get(&session.id).unwrap();
    assert_eq!(recovered.status, SessionStatus::Interrupted);
    let events = read_all(&client, &session.id);
    assert_gapless(&events);
    assert_eq!(pending_approval(&events), None, "nobody can answer an approval for a dead agent");
    assert!(events.iter().any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::HubRestart, .. })));

    let before = events.len();
    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_gapless(&events);
    let later = &events[before..];
    assert!(later.iter().any(|e| matches!(e.body, EventBody::AgentStarted { resumed: true, .. })));
    assert!(assistant_text(later).contains("remember mango"), "{}", assistant_text(later));
}

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
fn a_worker_crash_is_reported_and_the_next_prompt_gets_a_new_worker() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    let mut agent_pid = None;
    wait_until("the agent to start", || {
        agent_pid = read_all(&client, &session.id).iter().find_map(|e| match &e.body {
            EventBody::AgentStarted { pid, .. } => Some(*pid),
            _ => None,
        });
        agent_pid.is_some()
    });
    let agent_pid = agent_pid.unwrap();
    let out = Command::new("ps").args(["-o", "ppid=", "-p", &agent_pid.to_string()]).output().unwrap();
    let worker_pid: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
    // SAFETY: kill(2) has no memory effects. This is the worker's PID, read just above.
    unsafe { libc::kill(worker_pid, libc::SIGKILL) };

    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
    let events = read_all(&client, &session.id);
    assert!(events.iter().any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::WorkerLost, .. })));

    wait_until("the agent to stop along with its worker", || !is_alive(agent_pid));

    client.session_prompt(&session.id, "say:back", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_gapless(&events);
    assert!(assistant_text(&events).ends_with("back"));
}

#[test]
fn an_agent_dying_during_an_approval_closes_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace_with_mode(&client, Some("manual"));
    client.session_prompt(&session.id, "tool:echo x", None).unwrap();
    let approval_id = wait_for_approval(&client, &session.id);
    let agent_pid = read_all(&client, &session.id)
        .iter()
        .find_map(|e| match &e.body {
            EventBody::AgentStarted { pid, .. } => Some(*pid),
            _ => None,
        })
        .unwrap();
    signal(agent_pid, libc::SIGKILL);

    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
    let events = read_all(&client, &session.id);
    assert_gapless(&events);
    assert_eq!(pending_approval(&events), None);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::Unexpected, signal: Some(9), .. }))
    );
    let err = client
        .session_approve(
            &session.id,
            &approval_id,
            ApprovalDecision::Allow { updated_input: None, updated_permissions: None },
        )
        .unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));
}

#[test]
fn an_agent_crash_is_reported_and_the_next_prompt_resumes() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:before; crash:3", None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
    let events = read_all(&client, &session.id);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::Unexpected, code: Some(3), .. }))
    );

    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert!(assistant_text(&events).contains("say:before"), "{}", assistant_text(&events));
}

#[test]
fn a_missing_agent_binary_is_reported_in_the_log_rather_than_hanging() {
    let mut stack = Stack::start();
    stack.kill_hub();
    let mut command = stack.command();
    command.env("TRESTLE_CLAUDE_BIN", "/nonexistent/claude");
    stack.hub = Some(command.spawn().unwrap());
    wait_until("the hub", || Client::connect(&stack.socket(), "test").is_ok());

    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hi", None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
    let events = read_all(&client, &session.id);
    let reported = events.iter().any(|e| match &e.body {
        EventBody::AgentExited { reason: ExitReason::StartFailed, stderr_tail, .. } => {
            stderr_tail.iter().any(|line| line.contains("/nonexistent/claude"))
        }
        _ => false,
    });
    assert!(reported, "{events:#?}");
}

#[test]
fn killing_the_hub_and_worker_together_cannot_lead_to_two_agents() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first; hang:60000", None).unwrap();
    let mut agent_pid = 0;
    wait_until("the tool to start", || {
        let events = read_all(&client, &session.id);
        agent_pid = started_pid(&events).unwrap_or(0);
        agent_pid != 0 && assistant_text(&events).contains("first")
    });
    let worker_pid = parent_of(agent_pid);
    drop(client);
    signal(worker_pid, libc::SIGKILL);
    stack.kill_hub();
    assert!(is_alive(agent_pid), "the fake keeps hanging after its stdin closes, like a real long tool");

    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "say:second", None).unwrap();
    wait_until("the second turn to finish", || assistant_text(&read_all(&client, &session.id)).ends_with("second"));
    assert!(!is_alive(agent_pid), "the old agent must be gone before a new one runs on its conversation");
}

#[test]
fn an_exit_reported_late_cannot_be_mistaken_for_the_agent_that_replaced_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    // Something it started keeps its stdout open, so the worker takes seconds to report the exit.
    client.session_prompt(&session.id, "say:first; holdpipe:7; crash:3", None).unwrap();
    let mut first = None;
    wait_until("the agent to start", || {
        first = latest_started_pid(&read_all(&client, &session.id));
        first.is_some()
    });
    let first = first.unwrap();
    wait_until("the agent to die", || !is_alive(first));
    let exits =
        |events: &[SessionEvent]| events.iter().filter(|e| matches!(e.body, EventBody::AgentExited { .. })).count();
    assert_eq!(exits(&read_all(&client, &session.id)), 0, "the exit should still be on its way");

    // A prompt in that window goes to a new agent, rather than failing on the dead one's pipe.
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    let mut second = 0;
    wait_until("a new agent to start", || {
        second = latest_started_pid(&read_all(&client, &session.id)).unwrap_or(first);
        second != first
    });
    wait_until("the old exit to be reported", || exits(&read_all(&client, &session.id)) == 1);
    let events = read_all(&client, &session.id);
    assert_gapless(&events);
    assert!(!events.iter().any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::StartFailed, .. })));

    // It's in the log, but it wasn't about the agent running now.
    assert_eq!(client.session_get(&session.id).unwrap().status, SessionStatus::Running);
    assert!(is_alive(second));

    // The hub still knows about the new agent, so losing its worker is noticed and it gets stopped.
    signal(parent_of(second), libc::SIGKILL);
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
    wait_until("the new agent to stop too", || !is_alive(second));
}

#[test]
fn a_prompt_sent_as_the_agent_dies_is_delivered_to_a_new_one() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let mut seen = 0;
    for trial in 0..3 {
        // A burst leaves output queued behind the process, which keeps its exit from being noticed.
        client.session_prompt(&session.id, "burst:20000; crash:3", None).unwrap();
        // The agent from the last trial takes this prompt, or a new one on the first.
        let mut agent_pid = None;
        wait_until("an agent to be running", || {
            agent_pid = latest_started_pid(&read_all(&client, &session.id));
            agent_pid.is_some()
        });
        wait_until("the agent to die", || !is_alive(agent_pid.unwrap()));
        let marker = format!("after-{trial}");
        client.session_prompt(&session.id, &format!("say:{marker}"), None).unwrap();

        let mut events = Vec::new();
        wait_until("the prompt to be answered", || {
            events = read_all(&client, &session.id);
            assistant_text(&events[seen..]).contains(&marker)
        });
        assert_gapless(&events);
        let failed =
            events.iter().any(|e| matches!(e.body, EventBody::AgentExited { reason: ExitReason::StartFailed, .. }));
        assert!(!failed, "trial {trial} reported a failed start");
        wait_for_status(&client, &session.id, SessionStatus::Idle);
        seen = events.len();
    }

    // The hub must still know about the agent it's on now, so losing it is noticed.
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    wait_until("the tool to start", || read_all(&client, &session.id).len() > seen + 2);
    let agent_pid = latest_started_pid(&read_all(&client, &session.id)).unwrap();
    signal(parent_of(agent_pid), libc::SIGKILL);
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
}
