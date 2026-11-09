//! What anyone on the network can cost a hub without a code or a token: a few connections for a few seconds, and
//! none of the hub's own room.

#![cfg(test)]

use std::path::{Path, PathBuf};

use client_core::{Client, ClientError};

use crate::support::{Stack, commit_settings, install_executable, pair_phone, wait_for_setup, wait_until};

#[test]
fn only_a_few_connections_can_wait_to_authenticate_and_none_count_as_clients() {
    let stack = Stack::start();
    let client = stack.client();
    let paired = pair_phone(&client, "phone");
    let unauthenticated = || Client::open_remote(&paired.hosts, &paired.fingerprint);
    // Before the phone, so it's long since been served by the time the clients are counted.
    let mut waiting = vec![unauthenticated().unwrap()];
    let phone = Client::connect_remote(&paired, "phone").unwrap();

    waiting.extend((1..hub::MAX_UNAUTHENTICATED).map(|_| unauthenticated().unwrap()));
    let over = unauthenticated().map(drop);

    assert!(matches!(over, Err(ClientError::Unreachable { .. })), "{over:?}");
    assert!(phone.repo_list().is_ok(), "a device that has authenticated carries on");
    wait_until("only this client and the phone to count", || client.hub_status().unwrap().clients == 2);
    drop(waiting);
    wait_until("room for a device to connect again", || Client::connect_remote(&paired, "phone").is_ok());
}

/// A hub holds a descriptor for each client, worker and connection from the network.
#[test]
fn a_hub_raises_its_limit_on_open_files() {
    let mut stack = Stack::start();
    stack.kill_hub();
    let few = stack.path("roundhoused-with-few-files");
    let script = few.with_extension("source");
    let exe = Path::new(env!("CARGO_BIN_EXE_roundhoused")).display();
    std::fs::write(&script, format!("#!/bin/sh\nulimit -Sn 64\nexec '{exe}' \"$@\"\n")).unwrap();
    install_executable(&script, &few);
    stack.hub = Some(stack.command_at(&few).spawn().unwrap());
    wait_until("the hub", || Client::connect(&stack.socket(), "test").is_ok());
    commit_settings(&stack, "[scripts]\nsetup = \"ulimit -Sn > open-files.txt\"\n");
    let client = stack.client();

    // What the hub's workers inherit, which is what the hub has.
    let (workspace, _) = stack.workspace(&client);
    wait_for_setup(&client, &workspace.id);

    let worktree = PathBuf::from(workspace.path.unwrap());
    let limit = std::fs::read_to_string(worktree.join("open-files.txt")).unwrap();
    assert!(limit.trim().parse::<u64>().is_ok_and(|limit| limit > 64), "{limit}");
}
