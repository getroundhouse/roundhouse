//! Unix's, where the socket is a Unix socket.

use std::path::Path;

use anyhow::Context;

use crate::worker_role::Given;

/// In the worker's state directory, where the socket is named relative to it, to a worker an earlier build left
/// listening there. It starts none, so with none there it fails, and the hub ends whatever it finds on the instance.
/// It goes at the next break of the worker protocol, as `docs/decisions/hub-speaks-previous-version.md` says.
pub(crate) fn attach(given: &Given) -> anyhow::Result<()> {
    let state_dir = &given.state_dir;
    std::env::set_current_dir(state_dir).with_context(|| format!("moving into {}", state_dir.display()))?;
    worker::attach(Path::new(protocol::paths::WORKER_SOCKET_FILE))?;
    Ok(())
}
