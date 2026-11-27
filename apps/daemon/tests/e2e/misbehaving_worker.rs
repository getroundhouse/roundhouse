//! A worker binary that misbehaves once its hub has admitted it. `misbehaving_worker` stands in for it, as the stack's
//! `MISBEHAVING_WORKER` says.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use client_core::Connection;
use hub::Timers;
use process::ProcessRecord;

use crate::support::{Stack, TIMEOUT, assistant_text, patient_with_workers, wait_for_turn_end, wait_until};

/// The stack's workers, once a test has put the stand-in in place, misbehave as `how` says.
fn misbehaving(how: &str) -> (&'static str, String) {
    ("MISBEHAVING_WORKER", how.to_string())
}

/// Runs the hub from a copy of its binary, so a test can replace what it starts next.
fn hub_from_copy(stack: &mut Stack) -> PathBuf {
    stack.kill_hub();
    let exe = stack.path(&format!("roundhoused-copy{}", std::env::consts::EXE_SUFFIX));
    process::testing::install_program(crate::support::daemon(), &exe).unwrap();
    stack.hub = Some(stack.command_at(&exe).spawn().unwrap().into());
    wait_until("the hub", || Connection::connect(&stack.socket(), "test").is_ok());
    exe
}

fn restore_real_worker(exe: &Path) {
    process::testing::install_program(crate::support::daemon(), exe).unwrap();
}

fn replace_with_misbehaving(exe: &Path) {
    process::testing::install_program(crate::support::misbehaving_worker(), exe).unwrap();
}

/// The worker running on each of the stack's instances, as it recorded itself once it had claimed it.
fn workers(stack: &Stack) -> Vec<ProcessRecord> {
    let instances = std::fs::read_dir(stack.paths().instances_dir).unwrap().flatten();
    let recorded = instances
        .filter_map(|instance| ProcessRecord::read(&instance.path().join(protocol::paths::WORKER_RECORD_FILE)));
    recorded.filter(ProcessRecord::is_running).collect()
}

/// Kills each of the stack's workers, and waits until the hub has noticed, which it does once a worker's connection
/// has gone quiet.
fn kill_workers(stack: &Stack, client: &Connection) {
    for worker in workers(stack) {
        process::signal_group(worker.pid, process::Signal::Kill);
    }
    wait_until("the hub to notice", || client.hub_status().unwrap().workers == 0);
}

/// What `call` gives, or `None` if it hasn't within [`TIMEOUT`], which only one waiting for something that never comes
/// runs out.
fn answer<T: Send + 'static>(call: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(call());
    });
    rx.recv_timeout(TIMEOUT).ok()
}

#[test]
fn a_worker_speaking_another_version_only_blocks_until_it_is_replaced() {
    let mut stack = Stack::start_with(vec![misbehaving("another-version")]);
    let exe = hub_from_copy(&mut stack);
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.workspace_status(&workspace.id).unwrap();

    replace_with_misbehaving(&exe);
    kill_workers(&stack, &client);

    let err = client.workspace_status(&workspace.id).unwrap_err();
    assert!(err.to_string().contains("another version"), "{err}");
    let err = client.workspace_status(&workspace.id).unwrap_err();
    assert!(err.to_string().contains("another version"), "the same error, not a stuck instance: {err}");

    restore_real_worker(&exe);
    client.workspace_status(&workspace.id).unwrap();
    client.session_prompt(&session.id, "say:recovered", None).unwrap();
    assert!(assistant_text(&wait_for_turn_end(&client, &session.id, 1)).ends_with("recovered"));
}

/// The workers' hello has a limit far longer than the test waits, so whatever waited for one fails the test, however
/// slow the machine. That the limit is reached is the next test's.
#[test]
fn a_worker_that_never_answers_holds_up_neither_the_hub_nor_another_workspace() {
    let patient = Timers { hello_within: Duration::from_mins(10), ..patient_with_workers() };
    let mut stack = Stack::start_with(vec![patient.env(), misbehaving("silent")]);
    let exe = hub_from_copy(&mut stack);
    let client = stack.client();
    let (hung, _) = stack.workspace(&client);
    let (also_hung, _) = stack.workspace(&client);
    let (fine, _) = stack.workspace(&client);
    kill_workers(&stack, &client);
    client.workspace_status(&fine.id).unwrap();

    // The next workers to start never say hello.
    replace_with_misbehaving(&exe);
    for id in [hung.id.clone(), also_hung.id.clone()] {
        let socket = stack.socket();
        std::thread::spawn(move || Connection::connect(&socket, "test").unwrap().workspace_status(&id).map(drop));
    }
    // Both starts are under way at once, so neither waits for the other.
    wait_until("both of them to be starting a worker, beside the one that's fine", || workers(&stack).len() == 3);

    let socket = stack.socket();
    let status = answer(move || Connection::connect(&socket, "test").unwrap().hub_status().unwrap());
    assert!(status.is_some(), "hub status waited on one workspace's worker");
    let (socket, fine_id) = (stack.socket(), fine.id.clone());
    let status = answer(move || Connection::connect(&socket, "test").unwrap().workspace_status(&fine_id).unwrap());
    assert!(status.is_some(), "another workspace waited on it");
    // So the workers that never answer don't outlive the test.
    kill_workers(&stack, &client);
}

#[test]
fn a_worker_that_never_answers_its_hello_is_given_up_on_and_ended() {
    // Long enough for every real worker this test starts, the last of them the first run of a binary just copied into
    // place, which macOS checks first.
    let hello = Duration::from_secs(3);
    let mut stack =
        Stack::start_with(vec![Timers { hello_within: hello, ..patient_with_workers() }.env(), misbehaving("silent")]);
    let exe = hub_from_copy(&mut stack);
    let client = stack.client();
    let (hung, _) = stack.workspace(&client);
    kill_workers(&stack, &client);

    // The next worker to start never says hello.
    replace_with_misbehaving(&exe);
    let (socket, hung_id) = (stack.socket(), hung.id.clone());
    let status = answer(move || Connection::connect(&socket, "test").unwrap().workspace_status(&hung_id).map(drop));

    let err = status.expect("its start never gave up on it").unwrap_err();
    assert!(err.to_string().contains("didn't answer within"), "{err}");
    // It's ended as its start gives up, long before the hub would end it as a worker that didn't stop.
    wait_until("the worker that never answered to be gone", || workers(&stack).is_empty());

    restore_real_worker(&exe);
    client.workspace_status(&hung.id).unwrap();
}
