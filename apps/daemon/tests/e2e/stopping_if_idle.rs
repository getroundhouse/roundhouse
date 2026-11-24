//! Asking a hub to stop only if that cuts nothing off, which is how the app replaces an older one without stopping
//! anything. The hub decides, since a prompt can be sent between a client seeing it idle and stopping it, which the
//! hub's own tests, in its `hub/sessions`, have it refuse.

#![cfg(test)]

use client_core::{IdleStop, stop_hub_if_idle};

use crate::support::{Stack, wait_until};

#[test]
fn a_hub_with_nothing_on_it_stops_when_asked_to_if_idle() {
    let mut stack = Stack::start();
    let client = stack.client();
    // So nothing asked for as the stack started is still under way.
    wait_until("the hub to be idle", || client.hub_status().unwrap().is_idle());

    let stopped = stop_hub_if_idle(&client).unwrap();

    assert_eq!(stopped, IdleStop::Stopped);
    assert!(stack.hub.take().unwrap().wait().unwrap().success());
}

/// Another client that's only connected loses nothing, since it connects again to the next hub, so it doesn't keep
/// this one, and the update waiting on it, for as long as it stays open, as an app on another device can.
#[test]
fn a_hub_stops_if_idle_while_another_client_is_connected() {
    let mut stack = Stack::start();
    let client = stack.client();
    wait_until("the hub to be idle", || client.hub_status().unwrap().is_idle());
    let _other = stack.client();
    wait_until("the hub to count the other client", || client.hub_status().unwrap().clients == 2);

    assert_eq!(stop_hub_if_idle(&client).unwrap(), IdleStop::Stopped);
    assert!(stack.hub.take().unwrap().wait().unwrap().success());
}
