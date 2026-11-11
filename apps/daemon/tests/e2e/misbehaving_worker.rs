//! A worker binary that misbehaves.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use client_core::Client;

use crate::support::{Stack, assistant_text, install_executable, signal, wait_for_turn_end, wait_until};

/// Runs the hub from a copy of its binary, so a test can replace what it starts next.
fn hub_from_copy(stack: &mut Stack) -> PathBuf {
    stack.kill_hub();
    let exe = stack.path("roundhoused-copy");
    install_executable(Path::new(env!("CARGO_BIN_EXE_roundhoused")), &exe);
    stack.hub = Some(stack.command_at(&exe).spawn().unwrap());
    wait_until("the hub", || Client::connect(&stack.socket(), "test").is_ok());
    exe
}

fn restore_real_worker(exe: &Path) {
    install_executable(Path::new(env!("CARGO_BIN_EXE_roundhoused")), exe);
}

fn replace_with_script(exe: &Path, body: &str) {
    let source = exe.with_extension("source");
    std::fs::write(&source, format!("#!/bin/sh\n{body}\n")).unwrap();
    install_executable(&source, exe);
}

/// Kills every worker of this hub, and waits until the hub has noticed.
fn children_of(parent: u32) -> Vec<u32> {
    let out = Command::new("pgrep").args(["-P", &parent.to_string()]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).split_whitespace().map(|pid| pid.parse().unwrap()).collect()
}

fn kill_workers(stack: &Stack, client: &Client) {
    for pid in children_of(stack.hub_pid()) {
        signal(pid, libc::SIGKILL);
    }
    wait_until("the hub to notice", || client.hub_status().unwrap().workers == 0);
}

fn answered_within<T: Send + 'static>(limit: Duration, call: impl FnOnce() -> T + Send + 'static) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(call());
    });
    rx.recv_timeout(limit).is_ok()
}

#[test]
fn a_worker_speaking_another_version_only_blocks_until_it_is_replaced() {
    let mut stack = Stack::start();
    let exe = hub_from_copy(&mut stack);
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.workspace_status(&workspace.id).unwrap();

    replace_with_script(
        &exe,
        r#"read line
printf '%s\n' '{"frame":"reply","id":1,"error":{"code":"version_mismatch","message":"stub speaks another version"}}'
cat > /dev/null"#,
    );
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

#[test]
fn a_worker_that_never_answers_only_affects_its_own_workspace_and_times_out() {
    let mut stack = Stack::start();
    let exe = hub_from_copy(&mut stack);
    let client = stack.client();
    let (hung, _) = stack.workspace(&client);
    let (also_hung, _) = stack.workspace(&client);
    let (fine, _) = stack.workspace(&client);
    kill_workers(&stack, &client);
    client.workspace_status(&fine.id).unwrap();

    // The next workers to start never say hello.
    replace_with_script(&exe, "exec sleep 1000");
    let started = Instant::now();
    let blocked: Vec<_> = [hung.id.clone(), also_hung.id.clone()]
        .into_iter()
        .map(|id| {
            let socket = stack.socket();
            std::thread::spawn(move || {
                Client::connect(&socket, "test").unwrap().workspace_status(&id).map(|_| ()).map_err(|e| e.to_string())
            })
        })
        .collect();
    wait_until("both of them to be starting a worker, beside the one that's fine", || {
        children_of(stack.hub_pid()).len() == 3
    });

    let socket = stack.socket();
    assert!(
        answered_within(Duration::from_secs(3), move || Client::connect(&socket, "test")
            .unwrap()
            .hub_status()
            .unwrap()),
        "hub status must not wait on one workspace's worker"
    );
    let socket = stack.socket();
    let fine_id = fine.id.clone();
    assert!(
        answered_within(Duration::from_secs(3), move || Client::connect(&socket, "test")
            .unwrap()
            .workspace_status(&fine_id)
            .unwrap()),
        "other workspaces must not wait on it either"
    );

    for attempt in blocked {
        assert!(attempt.join().unwrap().is_err(), "a worker that never answers is an error");
    }
    // Each times out on its own 10 s. If one start had to wait for the other, this would be 20 s.
    assert!(started.elapsed() < Duration::from_secs(15), "took {:?}", started.elapsed());
    // Each is ended as its start gives up, rather than left until the 10 s a stopping worker gets.
    let given_up = Instant::now();
    wait_until("the workers that never answered to be gone", || children_of(stack.hub_pid()).len() == 1);
    assert!(given_up.elapsed() < Duration::from_secs(5), "took {:?}", given_up.elapsed());

    restore_real_worker(&exe);
    client.workspace_status(&hung.id).unwrap();
}
