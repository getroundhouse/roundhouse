//! Unix's, where the socket is a Unix socket.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;

use crate::runtime::run_to_end;
use crate::worker_role::{Given, config};

/// How long a worker that can outlive its hub goes on without one before it stops its agents, as "When a process dies"
/// in `docs/architecture.md` says. Longer than a hub takes to restart, a migration included.
const WITHOUT_A_HUB: Duration = Duration::from_mins(10);

/// In the instance's directory, where the socket is named relative to it. A worker that's stopping has removed its
/// socket but still holds the instance, and this fails rather than waiting for it, since it may be stuck: the hub ends
/// it, as it does whatever it finds on an instance that it doesn't know of.
pub(crate) fn attach(given: &Given) -> anyhow::Result<()> {
    let state_dir = in_state_dir(given)?;
    let exe = std::env::current_exe().context("finding this binary to start a worker with")?;
    let socket = Path::new(protocol::paths::WORKER_SOCKET_FILE);
    worker::attach(socket, || {
        if worker::instance_claimed(&state_dir)? {
            return Err(std::io::Error::other("a worker that's stopping still holds the instance"));
        }
        start_listening(&exe, &given.workspaces_root, &state_dir)
    })?;
    Ok(())
}

/// In a session of its own, so nothing that ends its hub or this command reaches it. What it logs goes where this
/// command's does, which is the hub's log for the workspace.
fn start_listening(exe: &Path, workspaces_root: &Path, state_dir: &Path) -> std::io::Result<()> {
    let mut command = std::process::Command::new(exe);
    command
        .args(["worker", "--listen", "--workspaces-root"])
        .arg(workspaces_root)
        .arg("--state-dir")
        .arg(state_dir)
        .current_dir(state_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    process::detach(&mut command);
    let mut started = command.spawn()?;
    // Should it stop while this command runs. Once this command has gone, the system reaps it.
    std::thread::spawn(move || started.wait());
    Ok(())
}

pub(crate) fn listen(given: Given) -> anyhow::Result<()> {
    let state_dir = in_state_dir(&given)?;
    let socket = Path::new(protocol::paths::WORKER_SOCKET_FILE);
    // Held until this process exits.
    let Some(_claim) = worker::claim_to_serve(&state_dir)? else {
        tracing::info!("another worker, started at the same time, has the instance");
        return Ok(());
    };
    let config = config(given, None)?;
    run_to_end(worker::listen(socket, config, WITHOUT_A_HUB))??;
    Ok(())
}

/// Moves this process into the instance's directory, which a worker that can outlive its hub needs, making it first if
/// need be, and returns it.
fn in_state_dir(given: &Given) -> anyhow::Result<PathBuf> {
    let state_dir = given.state_dir.clone().context("a worker that can outlive its hub needs --state-dir")?;
    std::fs::create_dir_all(&state_dir).with_context(|| format!("creating {}", state_dir.display()))?;
    std::env::set_current_dir(&state_dir).with_context(|| format!("moving into {}", state_dir.display()))?;
    Ok(state_dir)
}
