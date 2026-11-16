//! The daemon as a hub: what it's told from the environment and this build, and running it until it stops.

use anyhow::Context;

use super::BUILD;
use crate::runtime::run_to_end;

/// Runs a hub until it's asked to stop, saying whether a client started it because it needed one.
pub(crate) fn run(on_demand: bool) -> anyhow::Result<()> {
    let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).map_err(anyhow::Error::msg)?;
    // Follows symlinks, so moving an install's `current` link can't change what later workers run.
    let worker_exe =
        std::env::current_exe().and_then(std::fs::canonicalize).context("finding this binary to run workers")?;
    let shell = std::env::var("SHELL").ok().filter(|shell| !shell.is_empty());
    let path = std::env::var("PATH").ok();
    // The hub doesn't depend on the adapters, so it's told what each kind of agent offers from here.
    let agents = agents::descriptions();
    // A build without `testing`, as the product's is, takes the defaults whatever the environment says.
    let timers = hub::Timers::from_env(|name| std::env::var(name).ok());
    // GitHub's own, whatever the environment says, in a build without `testing`, as the product's is.
    let github = hub::GithubConfig::from_env(|name| std::env::var(name).ok());
    let config = hub::HubConfig { build: BUILD, paths, worker_exe, shell, path, on_demand, agents, timers, github };
    run_to_end(hub::run(config))??;
    Ok(())
}
