//! A worker an earlier hub left that dials the next hub only after it serves, as one on another machine does that keeps
//! its connection to a hub that was killed until iroh's idle timeout ends it. An adapter's worker in a test has no relay
//! to keep, so it notices a dead hub within 3 s, which the next hub's `before_serving` here is far shorter than.

#![cfg(test)]

use std::time::Duration;

use hub::Timers;
use protocol::event::EventBody;

use crate::carrying_on::{assert_one_agent_said_it_all_once, count};
use crate::support::{Gate, Stack, assistant_text, is_alive, read_all, started_pid, wait_for_turn_end, wait_until};

/// A stack whose workspaces run through an adapter, whose hub serves as soon as it's started rather than wait for a
/// worker.
fn through_an_adapter() -> Stack {
    let quick = Timers { before_serving: Duration::from_millis(100), ..Timers::FOR_TESTS };
    let mut stack = Stack::start_with(vec![quick.env()]);
    stack.make_through_an_adapter();
    stack
}

fn disconnected_workspaces_logged(stack: &Stack) -> usize {
    let log = std::fs::read_to_string(stack.hub_log()).unwrap_or_default();
    log.lines().filter(|line| line.contains("recovery.worker_disconnected")).count()
}

#[test]
fn a_worker_that_dials_after_the_next_hub_serves_carries_on_with_its_agent() {
    let mut stack = through_an_adapter();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let tool = Gate::new(&stack, "tool");
    client.session_prompt(&session.id, &format!("say:before; hang-until:{tool}; say:after"), None).unwrap();
    wait_until("the agent to start its tool", || assistant_text(&read_all(&client, &session.id)) == "before");
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    drop(client);

    stack.kill_hub();
    assert!(is_alive(agent), "the crash left the agent running");
    stack.start_hub();

    let client = stack.client();
    // Following the chat makes it in the hub before its agent's worker has dialed.
    let _following = client.follow_session(&session.id, protocol::model::Seq::new(1)).unwrap();
    // The hub served without the worker, and left its workspace as it was.
    assert_eq!(client.hub_status().unwrap().workers, 0, "the worker dialed within before_serving");
    assert_eq!(disconnected_workspaces_logged(&stack), 1);
    assert!(is_alive(agent), "the next hub ended an agent whose worker hadn't dialed yet");
    wait_until("the worker to dial the next hub", || client.hub_status().unwrap().workers == 1);
    tool.open();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_one_agent_said_it_all_once(&events, "beforeafter");
}

/// A chat sent to a workspace whose worker hasn't dialed yet doesn't start a worker, which would end the agent that's at
/// work there.
#[test]
fn a_prompt_sent_before_the_worker_dials_doesnt_replace_it() {
    let mut stack = through_an_adapter();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    drop(client);

    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.hub_status().unwrap().workers, 0, "the worker dialed within before_serving");
    let _ = client.session_prompt(&session.id, "say:during", None);
    wait_until("the worker to dial the next hub", || client.hub_status().unwrap().workers == 1);
    client.session_prompt(&session.id, "say:after", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);

    assert!(is_alive(agent), "the agent was replaced");
    assert_eq!(count(&events, |b| matches!(b, EventBody::AgentStarted { .. })), 1, "{events:#?}");
}

/// A stop says nothing carries on with a worker, so it ends one the hub has no connection to, through its provider.
#[test]
fn stopping_the_hub_ends_a_worker_that_never_dialed_it() {
    let mut stack = through_an_adapter();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    let worker = stack.worker_pid();
    drop(client);

    stack.kill_hub();
    stack.start_hub();
    assert_eq!(stack.client().hub_status().unwrap().workers, 0, "the worker dialed within before_serving");
    stack.stop_hub();

    wait_until("the worker to be ended", || !is_alive(worker));
    wait_until("its agent to be ended", || !is_alive(agent));
}
