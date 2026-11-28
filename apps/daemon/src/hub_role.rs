//! The daemon as a hub: what it's told from the environment and this build, and running it until it stops.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context;
use protocol::client::HubStart;

use crate::Config;
use crate::runtime::run_to_end;

/// What a hub exits with once it has left its workers for the next hub, as for an update: `EX_TEMPFAIL`, a failure the
/// service a hub can be installed as starts it again after, as it doesn't after a success. A client that starts a hub
/// starts the next one itself, and reads nothing from how the last one exited.
const RESTARTING: u8 = 75;

/// Runs a hub until it's asked to stop, saying what started it, and says by how it exits whether it stopped or left
/// its workers for the next hub.
pub(crate) fn run(start: HubStart, given: Config) -> anyhow::Result<ExitCode> {
    let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).map_err(anyhow::Error::msg)?;
    if let (HubStart::AsService, Some(build)) = (start, given.build.stamped()) {
        crate::service_builds::remove_unused(&paths, &build);
    }
    // Follows symlinks, so moving an install's `current` link can't change what later workers run.
    let running =
        std::env::current_exe().and_then(std::fs::canonicalize).context("finding this binary to run workers")?;
    let (worker_exe, _marked) = workers_from(&paths, given.build.stamped(), running);
    // The user's own home and login shell when whatever started the hub didn't say, as a service manager or a
    // container may not.
    let users = process::home_and_shell();
    let shell = std::env::var("SHELL").ok().filter(|shell| !shell.is_empty()).or(users.shell);
    let home = std::env::var_os(protocol::paths::HOME_VAR).filter(|home| !home.is_empty()).map(PathBuf::from);
    let home = home.or(users.home);
    let path = std::env::var("PATH").ok();
    // The hub doesn't depend on the adapters, so it's told what each kind of agent offers from here.
    let agents = agents::descriptions();
    let network_address =
        protocol::paths::listen_address(|name| std::env::var(name).ok()).map_err(anyhow::Error::msg)?;
    // Only a hub installed as a service updates as a client says, so only that one reads the feed.
    let update_feed = if start == HubStart::AsService { hub::UpdateFeed::published() } else { None };
    let config = hub::HubConfig {
        network_address,
        relays: given.relays,
        endpoint_ports: given.endpoint_ports,
        build: given.build,
        paths,
        worker_exe,
        shell,
        home,
        path,
        start,
        agents,
        timers: given.hub_timers,
        github: given.github,
        update_feed,
    };
    Ok(match run_to_end(hub::run(config, given.stops_with))?? {
        hub::Ended::Stopped => ExitCode::SUCCESS,
        hub::Ended::Restarting => ExitCode::from(RESTARTING),
    })
}

/// What this hub's workers run from: `build`'s copy, if it runs from [`Paths::fixed_hub`], as a Windows hub a client
/// started does, or else `running`, the file it runs from. The fixed path can have another build by the time a worker
/// starts, and Windows goes on giving a program the path it was started from after it's been moved. The copy is marked
/// as run from too, since what was marked as this process started can have been another build's by then. A copy
/// that's gone is still what the workers run from, so they fail to start rather than run another build.
///
/// [`Paths::fixed_hub`]: protocol::paths::Paths::fixed_hub
fn workers_from(
    paths: &protocol::paths::Paths,
    build: Option<protocol::StampedBuild>,
    running: PathBuf,
) -> (PathBuf, Option<process::RunningFrom>) {
    let at_fixed = std::fs::canonicalize(paths.fixed_hub()).is_ok_and(|fixed| fixed == running);
    match build {
        Some(build) if at_fixed => {
            let copy = paths.hub_copy(&build);
            let copy = std::fs::canonicalize(&copy).unwrap_or(copy);
            let marked = process::RunningFrom::mark(&copy).ok();
            (copy, marked)
        }
        _ => (running, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::StampedBuild;
    use protocol::paths::Paths;
    use std::path::Path;

    fn paths_in(dir: &Path) -> Paths {
        let home = dir.join("home");
        let data = dir.join("data");
        let env = |name: &str| match name {
            protocol::paths::HOME_VAR => Some(home.display().to_string()),
            protocol::paths::ENV_HOME => Some(data.display().to_string()),
            _ => None,
        };
        protocol::paths::resolve(env).unwrap()
    }

    /// The build's copy, and the fixed path linked to it, as a client that starts a hub on Windows leaves them.
    fn placed(paths: &Paths, build: &StampedBuild) -> PathBuf {
        let copy = paths.hub_copy(build);
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        std::fs::write(&copy, "hub").unwrap();
        std::fs::hard_link(&copy, paths.fixed_hub()).unwrap();
        std::fs::canonicalize(copy).unwrap()
    }

    #[test]
    fn a_hub_at_the_fixed_path_starts_its_workers_from_its_builds_copy() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        let build = StampedBuild::new("0.1.7", 7).unwrap();
        let copy = placed(&paths, &build);
        let running = std::fs::canonicalize(paths.fixed_hub()).unwrap();

        let (workers, marked) = workers_from(&paths, Some(build), running);

        assert_eq!(workers, copy);
        assert!(marked.is_some());
    }

    #[test]
    fn a_hub_anywhere_else_starts_its_workers_from_where_it_runs() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        let build = StampedBuild::new("0.1.7", 7).unwrap();
        placed(&paths, &build);
        // A daemon started where it is, such as beside an install.
        let elsewhere = dir.path().join("install").join("roundhoused");
        std::fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
        std::fs::write(&elsewhere, "hub").unwrap();
        let elsewhere = std::fs::canonicalize(elsewhere).unwrap();
        let unstamped_at_fixed = std::fs::canonicalize(paths.fixed_hub()).unwrap();

        let (stamped, stamped_marked) = workers_from(&paths, Some(build), elsewhere.clone());
        let (unstamped, unstamped_marked) = workers_from(&paths, None, unstamped_at_fixed.clone());

        assert_eq!(stamped, elsewhere);
        assert!(stamped_marked.is_none());
        assert_eq!(unstamped, unstamped_at_fixed);
        assert!(unstamped_marked.is_none());
    }

    #[test]
    fn a_hub_at_the_fixed_path_whose_copy_is_gone_still_names_it() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        let build = StampedBuild::new("0.1.7", 7).unwrap();
        placed(&paths, &build);
        std::fs::remove_file(paths.hub_copy(&build)).unwrap();
        let running = std::fs::canonicalize(paths.fixed_hub()).unwrap();

        let (workers, _) = workers_from(&paths, Some(build), running);

        assert_eq!(workers, paths.hub_copy(&build));
    }
}
