//! The hub's own settings, which it keeps and tells every client.

#![cfg(test)]

use client_core::RecordsFeed;
use protocol::model::{HubSettingsChange, SessionStatus};

use crate::support::{Stack, wait_for_status, wait_until};

pub(crate) fn keep_awake(on: bool) -> HubSettingsChange {
    HubSettingsChange { keep_awake: Some(on), ..HubSettingsChange::default() }
}

#[test]
fn a_change_reaches_every_client_and_outlives_the_hub() {
    let mut stack = Stack::start();
    let (client, other) = (stack.client(), stack.client());
    let following = RecordsFeed::start(&other);
    wait_until("the other client to hear what the hub starts out doing", || {
        following.snapshot().hub_settings().is_some_and(|settings| settings.keep_awake)
    });

    let off = client.hub_configure(keep_awake(false)).unwrap();

    assert!(!off.keep_awake);
    wait_until("the other client to hear", || following.snapshot().hub_settings() == Some(&off));
    assert_eq!(client.hub_configure(HubSettingsChange::default()).unwrap(), off, "changing nothing changes nothing");
    drop((client, other));
    stack.kill_hub();
    stack.start_hub();
    assert_eq!(stack.client().records_list().unwrap().hub_settings, Some(off));
}

#[test]
fn a_hub_set_not_to_keep_its_machine_awake_doesnt_once_it_starts_again() {
    let mut stack = Stack::start();
    stack.client().hub_configure(keep_awake(false)).unwrap();
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    client.session_prompt(&session.id, &format!("background:{}", stack.path("never").display()), None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Background);

    // The hub looks at what's at work each time a session's status changes, which it now has.
    assert!(!client.hub_status().unwrap().keeping_awake);
}
