//! A worker whose connection to a running hub ends with no goodbye: it's waited for, not ended, and dials again.

#![cfg(test)]

use protocol::event::{EventBody, ExitReason};
use protocol::model::SessionStatus;

use crate::support::{
    Stack, assert_gapless, assistant_text, is_alive, read_all, started_pid, wait_for_status, wait_for_turn_end,
    wait_until,
};

#[cfg(unix)]
fn hub_logged(stack: &Stack, event: &str) -> usize {
    let log = std::fs::read_to_string(stack.hub_log()).unwrap_or_default();
    log.lines().filter(|line| line.contains(&format!("\"event\":\"{event}\""))).count()
}

/// A worker that's killed leaves no goodbye, but its provider can say it's gone, which the local one does once the
/// system lets go of its claim. So its workspace is recovered as it was when the hub learnt of a crash at once.
#[test]
fn a_worker_killed_while_the_hub_runs_is_found_gone_and_the_next_prompt_gets_a_new_one() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first; hang", None).unwrap();
    wait_until("the agent to start", || started_pid(&read_all(&client, &session.id)).is_some());
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();

    Stack::kill_worker(stack.worker_pid());

    // Once the hub has lost it, a request for a worker isn't refused for want of its provider having been asked: the worker
    // is gone, and a new one is made.
    wait_until("the hub to have lost the worker", || client.hub_status().unwrap().workers == 0);
    client.workspace_status(&workspace.id).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Interrupted);
    let events = read_all(&client, &session.id);
    let lost = |e: &protocol::event::SessionEvent| {
        matches!(e.body, EventBody::AgentExited { reason: ExitReason::WorkerLost, .. })
    };
    assert!(events.iter().any(lost), "{events:#?}");
    wait_until("the agent to stop with its worker", || !is_alive(agent));
    client.session_prompt(&session.id, "say:back", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_gapless(&events);
    assert!(assistant_text(&events).ends_with("back"));
}

/// The worker's own timer says its connection has dropped before the hub's does: the hub is stalled, so it says
/// nothing for longer than the 3 s a worker in a test waits, and the worker dials again. Neither stops, so the agent's
/// turn goes on over the new connection, with nothing recorded twice or lost. Where the hub can be stopped by a signal.
#[cfg(unix)]
#[test]
fn a_connection_the_worker_drops_is_replaced_by_the_one_it_dials_and_the_agents_turn_finishes() {
    use std::time::Duration;

    use crate::carrying_on::assert_one_agent_said_it_all_once;
    use crate::support::{Gate, signal};

    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let tool = Gate::new(&stack, "tool");
    client.session_prompt(&session.id, &format!("say:before; hang-until:{tool}; say:after"), None).unwrap();
    wait_until("the agent to start its tool", || assistant_text(&read_all(&client, &session.id)) == "before");
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();

    // Neither the hub nor the worker stops: the hub is held still, and let go once the worker has given up on it.
    signal(stack.hub_pid(), libc::SIGSTOP);
    waiting::thread::for_what_mustnt_happen(Duration::from_secs(5));
    signal(stack.hub_pid(), libc::SIGCONT);

    wait_until("the hub to have lost the connection", || hub_logged(&stack, "worker.disconnected") > 0);
    wait_until("the worker to be carried on with", || hub_logged(&stack, "recovery.worker_connected") > 0);
    assert!(is_alive(agent), "the connection dropping ended the agent");
    tool.open();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_one_agent_said_it_all_once(&events, "beforeafter");
}
