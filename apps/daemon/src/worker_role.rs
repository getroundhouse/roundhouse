//! The daemon as a worker: on its hub's pipes, or as one that can outlive its hub, which listens on a socket in its
//! instance directory, and as the command a hub runs to reach that one.

#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
#[cfg(unix)]
use std::time::Duration;

use anyhow::Context;

use crate::runtime::run_to_end;

/// How a worker reaches its hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transport {
    /// The worker protocol on stdin and stdout, so the worker stops when its hub goes.
    Stdio,
    /// Connects stdin and stdout to the instance's worker, and starts it first if none is serving it, so it can
    /// outlive its hub. Not on Windows.
    Attach,
    /// Serves the instance on a socket in its directory, as `Attach` starts it.
    Listen,
}

/// What a worker is given, whichever way it reaches its hub.
pub(crate) struct Given {
    pub workspaces_root: PathBuf,
    pub state_dir: Option<PathBuf>,
    pub claude_bin: Option<PathBuf>,
}

/// How long a worker that can outlive its hub goes on without one before it stops its agents, as "When a process dies"
/// in `docs/architecture.md` says. Longer than a hub takes to restart, a migration included.
#[cfg(unix)]
const WITHOUT_A_HUB: Duration = Duration::from_mins(10);

pub(crate) fn run(transport: Transport, given: Given) -> anyhow::Result<()> {
    match transport {
        Transport::Stdio => on_stdio(given),
        #[cfg(unix)]
        Transport::Attach => attach(&given),
        #[cfg(unix)]
        Transport::Listen => listen(given),
        #[cfg(not(unix))]
        Transport::Attach | Transport::Listen => {
            anyhow::bail!("a worker can outlive its hub only where there are Unix sockets")
        }
    }
}

fn on_stdio(given: Given) -> anyhow::Result<()> {
    // Held until this process exits, since a provider's `connect` can be asked more than once.
    let (_claim, refused) = match given.state_dir.as_deref().map(worker::claim_instance).transpose() {
        Ok(claim) => (claim, None),
        Err(e @ worker::ClaimError::Taken(_)) => (None, Some(e.to_string())),
        Err(e) => return Err(e.into()),
    };
    let config = config(given, refused)?;
    run_to_end(worker::serve(tokio::io::stdin(), tokio::io::stdout(), config))?;
    Ok(())
}

/// In the instance's directory, where the socket is named relative to it. A worker that's stopping has removed its
/// socket but still holds the instance, and this fails rather than waiting for it, since it may be stuck: the hub ends
/// it, as it does whatever it finds on an instance that it doesn't know of.
#[cfg(unix)]
fn attach(given: &Given) -> anyhow::Result<()> {
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
#[cfg(unix)]
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
    process::own_session(&mut command);
    let mut started = command.spawn()?;
    // Should it stop while this command runs. Once this command has gone, the system reaps it.
    std::thread::spawn(move || started.wait());
    Ok(())
}

#[cfg(unix)]
fn listen(given: Given) -> anyhow::Result<()> {
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
#[cfg(unix)]
fn in_state_dir(given: &Given) -> anyhow::Result<PathBuf> {
    let state_dir = given.state_dir.clone().context("a worker that can outlive its hub needs --state-dir")?;
    std::fs::create_dir_all(&state_dir).with_context(|| format!("creating {}", state_dir.display()))?;
    std::env::set_current_dir(&state_dir).with_context(|| format!("moving into {}", state_dir.display()))?;
    Ok(state_dir)
}

fn config(given: Given, refused: Option<String>) -> anyhow::Result<worker::Config> {
    let exe = std::env::current_exe().context("finding this binary to delete worktrees with")?;
    Ok(worker::Config {
        workspaces_root: given.workspaces_root,
        state_dir: given.state_dir,
        home: std::env::var_os(protocol::paths::HOME_VAR).map(PathBuf::from),
        path: std::env::var_os("PATH"),
        claude_bin: given.claude_bin,
        scratch_dir: std::env::temp_dir(),
        deleter: worker::Deleter::new(exe, ["delete", "--"]),
        refused,
    })
}
