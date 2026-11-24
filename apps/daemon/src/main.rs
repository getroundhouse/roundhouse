//! One binary that runs as a hub or a worker. A hub starts its workers by running this same binary,
//! so on a Mac they're separate processes from day one.

mod hub_role;
mod preflight;
mod runtime;
mod service_builds;
mod signals;
mod worker_role;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use protocol::client::HubStart;
use tracing_subscriber::EnvFilter;
use worker_role::Transport;

/// Which build this is, from the stamp it was compiled with, which the hub and the worker are given.
const BUILD: protocol::Build = protocol::this_build!();

#[derive(Parser)]
#[command(name = protocol::paths::DAEMON_NAME, version = BUILD.version(), about = "Runs as a hub or a worker")]
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
    /// Started by git, in what a worker starts.
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
    /// Runs agents and git for a workspace. Started by a hub's provider.
    #[command(group(clap::ArgGroup::new("transport").required(true)))]
    Worker {
        /// Speak the worker protocol on stdin and stdout, so the worker stops when its hub goes.
        #[arg(long, group = "transport")]
        stdio: bool,
        /// Connect stdin and stdout to the instance's worker, starting one if none is serving it, so it can outlive
        /// its hub. Not on Windows.
        #[arg(long, group = "transport")]
        attach: bool,
        /// Serve the instance on a socket in the worker's state directory, as `--attach` starts it.
        #[arg(long, group = "transport", hide = true)]
        listen: bool,
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
fn forget_hub_variables() {
    forget(&protocol::paths::HUB_ONLY_ENV);
}

/// A hub can be started from inside a Claude session, such as by an agent, and every program the daemon starts inherits
/// what it was started with: a worker, an agent, a terminal's shell, a setup or run script, and git. A Claude CLI among
/// them, however far down, would take itself for part of that session.
fn forget_a_parent_claude_session() {
    forget(agents::claude::PARENT_SESSION_ENV);
}

/// Removes `names` from this process's environment. Only `main` calls it, before it starts a thread.
fn forget(names: &[&str]) {
    for name in names {
        // SAFETY: `main` calls this on the main thread before any other exists, so nothing can be reading the
        // environment.
        unsafe { std::env::remove_var(name) };
    }
}

fn say_worker_versions() -> anyhow::Result<()> {
    println!("{}", serde_json::to_string(&update_feed::FLOORS)?);
    Ok(())
}

/// What started a hub, as its flags say. Clap refuses both at once.
fn hub_start(on_demand: bool, service: bool) -> HubStart {
    match (on_demand, service) {
        (true, _) => HubStart::OnDemand,
        (_, true) => HubStart::AsService,
        _ => HubStart::ByHand,
    }
}

fn main() -> anyhow::Result<ExitCode> {
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
    // Logs always go to stderr, because a worker's stdout is its protocol stream.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env(protocol::paths::ENV_LOG).unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let args = Args::parse();
    let codex_bin = std::env::var_os(worker::ENV_CODEX_BIN).filter(|bin| !bin.is_empty()).map(PathBuf::from);
    let claude_bin = std::env::var_os(worker::ENV_CLAUDE_BIN).filter(|bin| !bin.is_empty()).map(PathBuf::from);
    let antigravity_bin =
        std::env::var_os(worker::ENV_ANTIGRAVITY_BIN).filter(|bin| !bin.is_empty()).map(PathBuf::from);
    // Before the hub's variables are forgotten, since a test gives its workers their timers, and the test to stop with,
    // beside its hub's own.
    let for_tests = worker_role::ForTests::from_env();
    // The command that reaches a worker runs nothing in a workspace, and passes them on to the worker it starts, which
    // forgets them itself.
    if matches!(args.role, Role::Worker { attach: false, .. } | Role::Preflight { .. }) {
        forget_hub_variables();
    }
    // Only a hub says something by how it exits.
    let ran = match args.role {
        Role::Hub { on_demand, service } => return hub_role::run(hub_start(on_demand, service)),
        Role::GitCredential { dir, action } => Ok(worker::answer_credential_request(
            &dir,
            &action,
            std::io::stdin().lock(),
            std::io::stdout(),
            std::io::stderr(),
        )?),
        Role::Preflight { agent, checkout } => preflight::run(agent, &checkout, claude_bin, codex_bin, antigravity_bin),
        Role::WorkerVersions => say_worker_versions(),
        Role::PrintEnvironment => Ok(process::print_environment(std::env::vars_os(), &mut std::io::stdout().lock())?),
        Role::Delete { paths } => {
            let failed = worker::delete_trees(&paths);
            for (path, e) in &failed {
                tracing::error!(path = %path.display(), "couldn't delete a worktree moved aside: {e}");
            }
            anyhow::ensure!(failed.is_empty(), "{} of {} weren't deleted", failed.len(), paths.len());
            Ok(())
        }
        Role::Worker { stdio: _, attach, listen, workspaces_root, state_dir, clones_dir } => {
            let transport = match (attach, listen) {
                (true, _) => Transport::Attach,
                (_, true) => Transport::Listen,
                _ => Transport::Stdio,
            };
            worker_role::run(
                transport,
                worker_role::Given {
                    build: BUILD,
                    workspaces_root,
                    state_dir,
                    clones_dir,
                    claude_bin,
                    codex_bin,
                    antigravity_bin,
                    for_tests,
                },
            )
        }
    };
    ran.map(|()| ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory as _;

    use super::*;

    /// What a client checks a daemon is before it runs it, and what an older hub reads of a newer release to update to
    /// it, as `docs/principles/versions.md` says.
    #[test]
    fn the_version_the_daemon_says_is_the_line_its_checked_by() {
        let said = Args::command().render_version();

        assert_eq!(said.trim_end(), protocol::paths::daemon_version_line(BUILD.version()));
    }
}
