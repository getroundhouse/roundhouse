use super::outliving;
use crate::runtime::run_to_end;
use anyhow::Context;
use std::path::PathBuf;

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
    pub build: protocol::Build,
    pub workspaces_root: PathBuf,
    pub state_dir: Option<PathBuf>,
    pub claude_bin: Option<PathBuf>,
    pub codex_bin: Option<PathBuf>,
}

pub(crate) fn run(transport: Transport, given: Given) -> anyhow::Result<()> {
    match transport {
        Transport::Stdio => on_stdio(given),
        Transport::Attach => outliving::attach(&given),
        Transport::Listen => outliving::listen(given),
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

pub(super) fn config(given: Given, refused: Option<String>) -> anyhow::Result<worker::Config> {
    let exe = std::env::current_exe().context("finding this binary to delete worktrees with")?;
    Ok(worker::Config {
        build: given.build,
        workspaces_root: given.workspaces_root,
        state_dir: given.state_dir,
        home: std::env::var_os(protocol::paths::HOME_VAR).map(PathBuf::from),
        path: std::env::var_os("PATH"),
        claude_bin: given.claude_bin,
        codex_bin: given.codex_bin,
        scratch_dir: std::env::temp_dir(),
        deleter: worker::Deleter::new(exe, ["delete", "--"]),
        refused,
    })
}
