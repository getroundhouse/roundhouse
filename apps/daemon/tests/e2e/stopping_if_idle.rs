//! Asking a hub to stop only if that cuts nothing off, which is how the app replaces an older one without stopping
//! anything. The hub decides, since a prompt can be sent between a client seeing it idle and stopping it.

#![cfg(test)]

use client_core::{IdleStop, stop_hub_if_idle};
use protocol::model::SessionStatus;

use crate::support::{Stack, wait_for_status, wait_for_turn_end, wait_until};

#[test]
fn a_hub_with_nothing_on_it_stops_when_asked_to_if_idle() {
    let mut stack = Stack::start();
    let client = stack.client();
    let pid = client.hub_status().unwrap().pid;
    // So nothing asked for as the stack started is still under way.
    wait_until("the hub to be idle", || client.hub_status().unwrap().is_idle());

    let stopped = stop_hub_if_idle(&client, &stack.socket(), pid).unwrap();

    assert_eq!(stopped, IdleStop::Stopped);
    assert!(stack.hub.take().unwrap().wait().unwrap().success());
}

/// The case it's for: the prompt's reply is back, so the request for it no longer counts, and its agent hasn't said
/// anything yet. The agent is at work from when the hub records the prompt, which is before it replies.
#[test]
fn a_hub_refuses_to_stop_if_idle_straight_after_a_prompt() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let pid = client.hub_status().unwrap().pid;
    wait_until("the hub to be idle", || client.hub_status().unwrap().is_idle());

    client.session_prompt(&session.id, "say:still here", None).unwrap();
    let stopped = stop_hub_if_idle(&client, &stack.socket(), pid).unwrap();

    assert_eq!(stopped, IdleStop::Busy);
    // It carries on, on the same connection, and the turn ends as it would have.
    wait_for_turn_end(&client, &session.id, 1);
    wait_for_status(&client, &session.id, SessionStatus::Idle);
    assert_eq!(client.hub_status().unwrap().pid, pid);
}

/// Another client that's only connected loses nothing, since it connects again to the next hub, so it doesn't keep
/// this one, and the update waiting on it, for as long as it stays open, as an app on another device can.
#[test]
fn a_hub_stops_if_idle_while_another_client_is_connected() {
    let mut stack = Stack::start();
    let client = stack.client();
    let pid = client.hub_status().unwrap().pid;
    wait_until("the hub to be idle", || client.hub_status().unwrap().is_idle());
    let _other = stack.client();
    wait_until("the hub to count the other client", || client.hub_status().unwrap().clients == 2);

    assert_eq!(stop_hub_if_idle(&client, &stack.socket(), pid).unwrap(), IdleStop::Stopped);
    assert!(stack.hub.take().unwrap().wait().unwrap().success());
}
