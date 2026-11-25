//! Unix's, where the socket is a Unix socket.

use std::path::Path;
use std::time::Duration;

use anyhow::Context;

use crate::runtime::run_to_end;
use crate::worker_role::{Given, config};

/// How long a worker that can outlive its hub goes on without one before it stops its agents, so nothing runs for good
/// with no hub to record it, as `docs/principles/failure.md` says. Longer than a hub takes to restart, a migration
/// included.
const WITHOUT_A_HUB: Duration = Duration::from_mins(10);

/// In the worker's state directory, where the socket is named relative to it. A worker that's stopping has removed its
/// socket but still holds the instance, and this fails rather than waiting for it, since it may be stuck: the hub ends
/// it, as it does whatever it finds on an instance that it doesn't know of.
pub(crate) fn attach(given: &Given) -> anyhow::Result<()> {
    let state_dir = &given.state_dir;
    move_into(state_dir)?;
    let exe = std::env::current_exe().context("finding this binary to start a worker with")?;
    let socket = Path::new(protocol::paths::WORKER_SOCKET_FILE);
    worker::attach(socket, || {
        if worker::instance_claimed(state_dir)? {
            return Err(std::io::Error::other("a worker that's stopping still holds the instance"));
        }
        start_listening(&exe, given)
    })?;
    Ok(())
}

/// In a session of its own, so nothing that ends its hub or this command reaches it. What it logs goes where this
/// command's does, which is the hub's log for the workspace.
fn start_listening(exe: &Path, given: &Given) -> std::io::Result<()> {
    let mut command = std::process::Command::new(exe);
    command
        .args(["worker", "--listen", "--workspaces-root"])
        .arg(&given.workspaces_root)
        .arg("--state-dir")
        .arg(&given.state_dir)
        .arg("--clones-dir")
        .arg(&given.clones_dir)
        .current_dir(&given.state_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    process::detach(&mut command);
    let mut started = command.spawn()?;
    // Should it stop while this command runs. Once this command has gone, the system reaps it.
    std::thread::spawn(move || started.wait());
    Ok(())
}

pub(crate) fn listen(mut given: Given) -> anyhow::Result<()> {
    move_into(&given.state_dir)?;
    let socket = Path::new(protocol::paths::WORKER_SOCKET_FILE);
    // Held until this process exits.
    let Some(_claim) = worker::claim_to_serve(&given.state_dir)? else {
        tracing::info!(name: "worker.instance_taken", "Another worker, started at the same time, has the instance");
        return Ok(());
    };
    let stops_with = std::mem::replace(&mut given.config.stops_with, Box::pin(std::future::pending()));
    let config = config(given, None)?;
    run_to_end(worker::listen(socket, config, WITHOUT_A_HUB, stops_with))??;
    Ok(())
}

/// Moves this process into the worker's state directory, making it first if need be.
fn move_into(state_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(state_dir).with_context(|| format!("creating {}", state_dir.display()))?;
    std::env::set_current_dir(state_dir).with_context(|| format!("moving into {}", state_dir.display()))
}
