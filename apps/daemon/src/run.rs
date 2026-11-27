//! The command line, and running the role it names.

use std::io::IsTerminal as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory as _, FromArgMatches as _, Parser, Subcommand};
use protocol::client::HubStart;

use crate::worker_reach::WorkerReach;
use crate::worker_role;
use crate::{Config, hub_role, preflight, service_log, signals};

#[derive(Parser)]
#[command(name = protocol::paths::DAEMON_NAME, about = "Runs as a hub or a worker")]
struct Args {
    #[command(subcommand)]
    role: Role,
}

#[derive(Subcommand)]
enum Role {
    /// Owns state and serves clients on a local socket.
    Hub {
        /// Say that a client started this because it needed one, so a client may restart it.
        #[arg(long, conflicts_with = "service")]
        on_demand: bool,
        /// Say that the machine's service manager started this, as `roundhouse hub install` has it do.
        #[arg(long)]
        service: bool,
    },
    /// Deletes the worktrees a worker moved aside when archiving, in a process of its own so it can outlive the
    /// worker. Started by a worker.
    #[command(hide = true)]
    Delete {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Answers git as its credential helper for github.com, with what the worker whose files are in `dir` was given.
    /// Started by git, in what a worker config.
    #[command(hide = true)]
    GitCredential {
        dir: PathBuf,
        /// What git asks: `get`, `store` or `erase`.
        action: String,
    },
    /// Prints this build's two floors, as JSON, which only the build can say: the versions of the worker protocol its
    /// hub speaks, and the oldest release it updates from, once it says one, as `update_feed::Floors` has them. The
    /// release workflow writes them in the update feed, as `tools/package/publish.py` says. It's named for the first,
    /// as are the file the workflow keeps it in and the publisher's flag that reads it, until #2275 renames them.
    #[command(hide = true)]
    WorkerVersions,
    /// Prints the environment this was started in, as `process::print_environment` says. Run by the login shell a hub
    /// asks for the environment the profile gives, after it has read the profile, so reading it needs nothing of the
    /// machine's own, such as its `env`. A parent Claude session's variables are forgotten first, as for every role, so
    /// a profile can't give them back.
    #[command(hide = true, name = process::PRINTS_ENVIRONMENT)]
    PrintEnvironment,
    /// Answer one question about the prospective launch and exit.
    #[command(hide = true)]
    Preflight {
        #[arg(value_parser = preflight::parse_agent)]
        agent: protocol::model::AgentKind,
        checkout: PathBuf,
    },
    /// Ends the worker whose state directory is `state_dir`, if one is running, and whatever it left there, such as its
    /// agents, giving them a few seconds to stop by themselves, or none with `--now`. Run by a provider's adapter on
    /// the machine the worker runs on.
    #[command(hide = true)]
    EndWorker {
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        now: bool,
    },
    /// Runs agents and git for a workspace. Started by a hub's provider.
    #[command(group(clap::ArgGroup::new("transport").required(true)))]
    Worker {
        #[command(flatten)]
        reach: WorkerReach,
        /// Where the worker writes its log, in place of stderr, added to the end.
        #[arg(long)]
        log: Option<PathBuf>,
        /// Start the worker in a session of its own, and return once it has started, so it outlives whatever ran this,
        /// such as a command run in a VM, whose session ends with it. It logs to `--log`.
        #[arg(long, requires_all = ["ticket", "log"])]
        detach: bool,
        /// Where worktrees go.
        #[arg(long)]
        workspaces_root: PathBuf,
        /// The worker's state directory, where it records what it runs, so it can be stopped if this process is killed,
        /// and keeps the setup log and each run's files.
        #[arg(long)]
        state_dir: PathBuf,
        /// Where the worker keeps its clones of the repositories it's told of by their address, which every worker its
        /// provider runs on this machine shares.
        #[arg(long)]
        clones_dir: PathBuf,
    },
}

/// Everything a worker starts runs the code in a workspace, which can be another build of Roundhouse, and these
/// would point it at this hub. That includes git hooks, so it has to cover every command rather than a few.
fn forget_hub_variables(also: &[&str]) {
    forget(&protocol::paths::HUB_ONLY_ENV);
    forget(also);
}

/// A hub can be started from inside a Claude session, such as by an agent, and every program the daemon starts inherits
/// what it was started with: a worker, an agent, a terminal's shell, a setup or run script, and git. A Claude CLI among
/// them, however far down, would take itself for part of that session.
fn forget_a_parent_claude_session() {
    forget(agents::claude::PARENT_SESSION_ENV);
}

/// Removes `names` from this process's environment. Only [`run`] calls it, before it starts a thread.
fn forget(names: &[&str]) {
    for name in names {
        // SAFETY: `run`, which is all a `main` does, calls this on the main thread before any other exists, so nothing can be reading the
        // environment.
        unsafe { std::env::remove_var(name) };
    }
}

fn say_worker_versions() -> anyhow::Result<()> {
    println!("{}", serde_json::to_string(&update_feed::FLOORS)?);
    Ok(())
}

/// Logs to stderr, whatever this runs as, because a worker's stdout is its protocol stream. A hub's and a worker's is
/// their log's file, which a worker's delete writes to as well, unless they're run from a terminal. Anything else's is
/// read by whoever ran it.
fn start_log(role: &Role) -> anyhow::Result<()> {
    let directives = std::env::var(protocol::paths::ENV_LOG).ok();
    let log = logging::Log::new(logging::Levels::new(directives.as_deref()));
    // A worker given a file of its own, as one that dials its hub from another machine is, which no hub keeps.
    if let Role::Worker { log: Some(file), .. } = role {
        return Ok(log.json_to(std::sync::Arc::new(open_log(file)?)).start()?);
    }
    let has_a_file = match role {
        Role::Hub { .. } | Role::Worker { .. } | Role::Delete { .. } => !std::io::stderr().is_terminal(),
        Role::GitCredential { .. }
        | Role::WorkerVersions
        | Role::PrintEnvironment
        | Role::Preflight { .. }
        | Role::EndWorker { .. } => false,
    };
    Ok(if has_a_file { log.json_to(std::io::stderr) } else { log.text_to(std::io::stderr) }.start()?)
}

/// The file at `path`, to add to the end of, making its directory first, which only this user can read.
fn open_log(path: &std::path::Path) -> anyhow::Result<std::fs::File> {
    use anyhow::Context as _;
    if let Some(dir) = path.parent() {
        private_files::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    private_files::file_options()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))
}

/// Deletes worktrees moved aside, and logs each it couldn't.
fn delete(paths: &[PathBuf]) -> anyhow::Result<()> {
    let failed = worker::delete_trees(paths);
    for (path, e) in &failed {
        tracing::error!(
            name: "worktree.undeleted",
            path = %path.display(),
            error = %e,
            "Couldn't delete a worktree moved aside"
        );
    }
    anyhow::ensure!(failed.is_empty(), "{} of {} weren't deleted", failed.len(), paths.len());
    Ok(())
}

/// The command line, of the program that's `build`.
fn command(build: protocol::Build) -> clap::Command {
    Args::command().version(build.version())
}

fn parse(build: protocol::Build) -> Args {
    Args::from_arg_matches(&command(build).get_matches()).unwrap_or_else(|e| e.exit())
}

/// What started a hub, as its flags say. Clap refuses both at once.
fn hub_start(on_demand: bool, service: bool) -> HubStart {
    match (on_demand, service) {
        (true, _) => HubStart::OnDemand,
        (_, true) => HubStart::AsService,
        _ => HubStart::ByHand,
    }
}

/// Runs the role the process's arguments name, given `config`.
///
/// # Errors
///
/// As the role fails.
pub fn run(config: Config) -> anyhow::Result<ExitCode> {
    // First, whatever this runs as, so nothing it starts has them.
    forget_a_parent_claude_session();
    signals::unblock();
    // Before anything is started. A worker's input and output are its hub's pipes, and the hub learns that the worker
    // has exited when its output ends.
    process::keep_stdio_from_children();
    // Whatever this runs as, and for as long as it runs, so a client removing the copies of the daemon that nothing
    // runs from leaves the one this runs from. One that can't be marked is being removed just then, which can stop it
    // as it starts, so it goes on unmarked and leaves the rest to that.
    let _running_from = std::env::current_exe().and_then(|program| process::RunningFrom::mark(&program)).ok();
    let args = parse(config.build);
    if let Role::Hub { service: true, .. } = args.role
        && let Ok(paths) = protocol::paths::resolve(|name| std::env::var(name).ok())
    {
        service_log::begin_anew_if_long(&paths.log);
    }
    start_log(&args.role)?;
    let codex_bin = std::env::var_os(worker::ENV_CODEX_BIN).filter(|bin| !bin.is_empty()).map(PathBuf::from);
    let claude_bin = std::env::var_os(worker::ENV_CLAUDE_BIN).filter(|bin| !bin.is_empty()).map(PathBuf::from);
    let antigravity_bin =
        std::env::var_os(worker::ENV_ANTIGRAVITY_BIN).filter(|bin| !bin.is_empty()).map(PathBuf::from);
    // A worker that detaches runs nothing in a workspace, and passes them on to the worker it starts, which forgets them
    // itself.
    let detaches = matches!(args.role, Role::Worker { detach: true, .. });
    if matches!(args.role, Role::Worker { .. } | Role::Preflight { .. }) && !detaches {
        forget_hub_variables(config.also_forgotten);
    }
    // Only a hub says something by how it exits.
    let ran = match args.role {
        Role::Hub { on_demand, service } => return hub_role::run(hub_start(on_demand, service), config),
        Role::GitCredential { dir, action } => Ok(worker::answer_credential_request(
            &dir,
            &action,
            std::io::stdin().lock(),
            std::io::stdout(),
            std::io::stderr(),
        )?),
        Role::Preflight { agent, checkout } => preflight::run(agent, &checkout, claude_bin, codex_bin, antigravity_bin),
        Role::WorkerVersions => say_worker_versions(),
        Role::EndWorker { state_dir, now } => {
            Ok(crate::runtime::run_to_end(providers::local::end_worker_in(&state_dir, now))?)
        }
        Role::PrintEnvironment => Ok(process::print_environment(std::env::vars_os(), &mut std::io::stdout().lock())?),
        Role::Delete { paths } => delete(&paths),
        Role::Worker { reach, log: _, detach, workspaces_root, state_dir, clones_dir } => worker_role::run(
            reach.transport(detach)?,
            worker_role::Given {
                workspaces_root,
                state_dir,
                clones_dir,
                claude_bin,
                codex_bin,
                antigravity_bin,
                config,
            },
        ),
    };
    ran.map(|()| ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a client checks a daemon is before it runs it, and what an older hub reads of a newer release to update to
    /// it, as `docs/principles/versions.md` says.
    #[test]
    fn the_version_the_daemon_says_is_the_line_its_checked_by() {
        let said = command(protocol::Build::UNSTAMPED).render_version();

        assert_eq!(said.trim_end(), protocol::paths::daemon_version_line(protocol::Build::UNSTAMPED.version()));
    }
}
