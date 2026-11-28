//! Whether a worker a hub left is still running, as its provider says, which a hub that has just started asks for the
//! workers that haven't dialed it.

#![cfg(test)]

use std::path::PathBuf;
use std::time::Duration;

use hub::Timers;
use protocol::event::EventBody;

use crate::support::{Stack, assert_gapless, assistant_text, is_alive, wait_for_turn_end, wait_until};

/// The hub waits ten minutes for a worker the last hub left to dial it, far longer than the test waits for it to serve, so
/// a hub that waited for this worker, which can't dial, fails the wait however fast or slow the machine. The local
/// provider says a worker is gone once the system has let go of its claim, which is proof.
#[test]
fn a_worker_that_died_while_its_hub_was_gone_is_known_gone_and_the_next_prompt_gets_a_new_one() {
    let patient = Timers { before_serving: Duration::from_mins(10), ..Timers::FOR_TESTS };
    let mut stack = Stack::start_with(vec![patient.env()]);
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let worker_pid = stack.worker_pid();
    drop(client);

    stack.kill_hub();
    Stack::kill_worker(worker_pid);
    wait_until("the worker to be gone", || !is_alive(worker_pid));
    stack.start_hub();

    let client = stack.client();
    client.session_prompt(&session.id, "say:second", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert_gapless(&events);
    let started = events.iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 2, "the second prompt should have got a new worker, and so a new agent: {events:#?}");
    assert!(assistant_text(&events).ends_with("second"));
}

/// What the hub's provider says of a worker a hub that crashed left, and of the same one once it's killed.
#[test]
fn the_local_provider_says_a_worker_the_last_hub_left_is_running_until_it_is_killed() {
    use providers::local::{LocalConfig, LocalProvider};
    use providers::{Provider as _, WorkerLiveness};

    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let worker = stack.worker_pid();
    drop(client);
    stack.kill_hub();
    let paths = stack.paths();
    let provider = LocalProvider::new(LocalConfig {
        worker_exe: PathBuf::new(),
        instances_dir: paths.instances_dir,
        workspaces_root: stack.path("workspaces"),
        clones_dir: paths.clones_dir,
        worker_logs_dir: paths.worker_logs_dir,
        environment: process::Environment::NONE,
        dials: true,
        hub_address_file: None,
    });
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let state = || runtime.block_on(provider.worker_state(&workspace.instance_id)).unwrap();

    let left = state();
    Stack::kill_worker(worker);
    wait_until("the system to let go of the worker's claim", || state() == WorkerLiveness::Gone);

    assert_eq!(left, WorkerLiveness::Running);
}
