//! One binary that runs as a hub or a worker. A hub starts its workers by running this same binary,
//! so on a Mac they're separate processes from day one.

mod runtime;
mod worker_role;

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use runtime::run_to_end;
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
        #[arg(long)]
        on_demand: bool,
    },
    /// Deletes the worktrees a worker moved aside when archiving, in a process of its own so it can outlive the
    /// worker. Started by a worker.
    #[command(hide = true)]
    Delete {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Runs agents and git for a workspace. Started by a hub's provider.
    #[command(group(clap::ArgGroup::new("transport").required(true)))]
    Worker {
        /// Speak the worker protocol on stdin and stdout, so the worker stops when its hub goes.
        #[arg(long, group = "transport")]
        stdio: bool,
        /// Connect stdin and stdout to the instance's worker, starting one if none is serving it, so it can outlive
        /// its hub. Not on Windows.
        #[arg(long, group = "transport", requires = "state_dir")]
        attach: bool,
        /// Serve the instance on a socket in its directory, as `--attach` starts it.
        #[arg(long, group = "transport", requires = "state_dir", hide = true)]
        listen: bool,
        /// Where worktrees go.
        #[arg(long)]
        workspaces_root: PathBuf,
        /// Where to record running agents, so they can be stopped if this process is killed.
        #[arg(long)]
        state_dir: Option<PathBuf>,
    },
}

/// Blocked signals carry across fork and exec, Rust's `Command` no longer resets them, and the desktop
/// app starts a hub from a background thread that has them all blocked. This binary needs them: tokio
/// only learns that a child exited from `SIGCHLD`, so without it the first `git` never returns, and
/// `SIGTERM` is how it's asked to stop. Everything this starts inherits the result, agents included.
#[cfg(unix)]
fn unblock_signals() {
    let mut none = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
    // SAFETY: `sigemptyset` initializes the set it's given, and this runs on the main thread before any
    // other exists, so every thread started later inherits the empty mask.
    unsafe {
        libc::sigemptyset(none.as_mut_ptr());
        libc::pthread_sigmask(libc::SIG_SETMASK, none.as_ptr(), std::ptr::null_mut());
    }
}

/// Everything a worker starts runs the code in a workspace, which can be another build of Roundhouse, and these
/// would point it at this hub. That includes git hooks, so it has to cover every command rather than a few.
fn forget_hub_variables() {
    for name in protocol::paths::HUB_ONLY_ENV {
        // SAFETY: this runs on the main thread before any other exists, so nothing can be reading the environment.
        unsafe { std::env::remove_var(name) };
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(unix)]
    unblock_signals();
    // Before anything is started. A worker's input and output are its hub's pipes, and the hub learns that the worker
    // has exited when its output ends.
    process::keep_stdio_from_children();
    // Logs always go to stderr, because a worker's stdout is its protocol stream.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env(protocol::paths::ENV_LOG).unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let args = Args::parse();
    let claude_bin = std::env::var_os(worker::ENV_CLAUDE_BIN).filter(|bin| !bin.is_empty()).map(PathBuf::from);
    // The command that reaches a worker runs nothing in a workspace, and passes them on to the worker it starts, which
    // forgets them itself.
    if matches!(args.role, Role::Worker { attach: false, .. }) {
        forget_hub_variables();
    }
    match args.role {
        Role::Hub { on_demand } => {
            let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).map_err(anyhow::Error::msg)?;
            // Follows symlinks, so moving an install's `current` link can't change what later workers run.
            let worker_exe = std::env::current_exe()
                .and_then(std::fs::canonicalize)
                .context("finding this binary to run workers")?;
            let shell = std::env::var("SHELL").ok().filter(|shell| !shell.is_empty());
            let path = std::env::var("PATH").ok();
            // The hub doesn't depend on the adapters, so it's told what each kind of agent offers from here.
            let agents = agents::descriptions();
            let config = hub::HubConfig { build: BUILD, paths, worker_exe, shell, path, on_demand, agents };
            run_to_end(hub::run(config))??;
            Ok(())
        }
        Role::Delete { paths } => {
            let failed = worker::delete_trees(&paths);
            for (path, e) in &failed {
                tracing::error!(path = %path.display(), "couldn't delete a worktree moved aside: {e}");
            }
            anyhow::ensure!(failed.is_empty(), "{} of {} weren't deleted", failed.len(), paths.len());
            Ok(())
        }
        Role::Worker { stdio: _, attach, listen, workspaces_root, state_dir } => {
            let transport = match (attach, listen) {
                (true, _) => Transport::Attach,
                (_, true) => Transport::Listen,
                _ => Transport::Stdio,
            };
            worker_role::run(transport, worker_role::Given { build: BUILD, workspaces_root, state_dir, claude_bin })
        }
    }
}
