//! Keeping the Mac awake while there's work. Only a Mac has what the hub does it with.

#![cfg(test)]

use std::process::Command;

use protocol::model::SessionStatus;

use crate::hub_settings::keep_awake;
use crate::support::{Stack, wait_for_setup, wait_for_status, wait_until};

/// What macOS's own program for staying awake runs as when it's this hub's, set to stop when the hub goes.
fn held_for(hub_pid: u32) -> String {
    format!("caffeinate -i -w {hub_pid}")
}

fn kept_awake_for(hub_pid: u32) -> bool {
    let processes = Command::new("ps").args(["-axo", "command="]).output().unwrap();
    let held_for = held_for(hub_pid);
    String::from_utf8_lossy(&processes.stdout).lines().any(|command| command.trim_end().ends_with(&held_for))
}

#[test]
fn the_mac_is_kept_awake_while_theres_work_and_never_past_its_hub() {
    let mut stack = Stack::start();
    let client = stack.client();
    let hub_pid = stack.hub_pid();
    assert!(!kept_awake_for(hub_pid), "nothing is at work yet");

    let (workspace, session) = stack.workspace(&client);
    wait_until("setting up a workspace to keep the Mac awake", || kept_awake_for(hub_pid));

    // It's kept awake for a while after the work stops, which would hide whether an agent's work keeps it awake too.
    // With this stopped and nothing at work, only work starting has it started again.
    wait_for_setup(&client, &workspace.id);
    Command::new("pkill").args(["-9", "-P", &hub_pid.to_string(), "caffeinate"]).status().unwrap();
    wait_until("the Mac to be let go", || !kept_awake_for(hub_pid));
    client.session_prompt(&session.id, &format!("background:{}", stack.path("never").display()), None).unwrap();
    wait_for_status(&client, &session.id, SessionStatus::Background);
    wait_until("an agent at work to keep the Mac awake", || kept_awake_for(hub_pid));
    wait_until("the hub to say it's keeping the Mac awake", || client.hub_status().unwrap().keeping_awake);

    // Someone who'd rather it slept has it let go at once, with the agent still at work.
    client.hub_configure(keep_awake(false)).unwrap();
    wait_until("the Mac to be let go once that's not wanted", || !kept_awake_for(hub_pid));
    wait_until("the hub to say it has let go", || !client.hub_status().unwrap().keeping_awake);
    client.hub_configure(keep_awake(true)).unwrap();
    wait_until("the Mac to be kept awake again once it's wanted", || kept_awake_for(hub_pid));

    // A hub that's killed can't let go itself.
    drop(client);
    stack.kill_hub();
    wait_until("the Mac to be let go once its hub is gone", || !kept_awake_for(hub_pid));
}
