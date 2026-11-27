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

/// In the worker's state directory, where the socket is named relative to it, to a worker an earlier build left
/// listening there. It starts none, so with none there it fails, and the hub ends whatever it finds on the instance.
/// It goes at the next break of the worker protocol, as `docs/decisions/hub-speaks-previous-version.md` says.
pub(crate) fn attach(given: &Given) -> anyhow::Result<()> {
    move_into(&given.state_dir)?;
    worker::attach(Path::new(protocol::paths::WORKER_SOCKET_FILE))?;
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
