//! The copies of the daemon that stamped builds start their hubs from.

#![cfg(test)]

use std::fs::File;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use client_core::{Client, Daemon};
use protocol::StampedBuild;

use crate::support::{Stack, install_executable, wait_until};

/// Every update leaves a copy behind, so a client that starts a hub removes those nothing runs from, once they're old
/// enough. A worker that outlived the hub of its build runs from that build's copy, which stays.
#[test]
fn starting_a_hub_removes_the_copies_nothing_runs_from_but_not_a_worker_s() {
    let mut stack = Stack::start();
    stack.kill_hub();
    let app = stack.path("app");
    std::fs::create_dir_all(&app).unwrap();
    install_executable(Path::new(env!("CARGO_BIN_EXE_roundhoused")), &app.join("roundhoused"));
    // The version this daemon gives, which a copy is checked against.
    let build = |number| StampedBuild::new(protocol::this_build!().version(), number).unwrap();
    let paths = stack.paths();
    // Builds 1 and 2 made their copies two days ago, and 3 an hour ago, and a worker of build 1 has outlived its hub.
    for (number, ago) in [(1, 48), (2, 48), (3, 1)] {
        let copy = paths.hub_copy(&build(number));
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        install_executable(Path::new(env!("CARGO_BIN_EXE_roundhoused")), &copy);
        let made = SystemTime::now() - Duration::from_hours(ago);
        File::options().write(true).open(&copy).unwrap().set_modified(made).unwrap();
    }
    let state_dir = stack.path("old-worker");
    let mut worker = Command::new(paths.hub_copy(&build(1)))
        .args(["worker", "--listen", "--workspaces-root"])
        .arg(stack.path("workspaces"))
        .arg("--state-dir")
        .arg(&state_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_until("the worker to listen", || state_dir.join(protocol::paths::WORKER_SOCKET_FILE).exists());
    let daemon = Daemon { path: app.join("roundhoused"), build: Some(build(4)) };

    let started = Client::connect_or_start(&paths, &daemon, "test");
    let _ = client_core::stop_hub(&stack.socket());
    worker.kill().unwrap();
    worker.wait().unwrap();

    started.unwrap();
    let left: Vec<_> = (1..=4).map(|number| paths.hub_copy(&build(number)).exists()).collect();
    assert_eq!(left, [true, false, true, true], "the copies of builds 1 to 4");
}
