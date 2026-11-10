//! One binary that runs as a hub or a worker. A hub starts its workers by running this same binary,
//! so on a Mac they're separate processes from day one.

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = protocol::paths::DAEMON_NAME, version = protocol::BUILD_VERSION, about = "Runs as a hub or a worker")]
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
    Worker {
        /// Speak the worker protocol on stdin and stdout.
        #[arg(long)]
        stdio: bool,
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
    unblock_signals();
    // Logs always go to stderr, because a worker's stdout is its protocol stream.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env(protocol::paths::ENV_LOG).unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let args = Args::parse();
    let claude_bin = std::env::var_os(worker::ENV_CLAUDE_BIN).filter(|bin| !bin.is_empty()).map(PathBuf::from);
    if matches!(args.role, Role::Worker { .. }) {
        forget_hub_variables();
    }
    let runtime = || tokio::runtime::Builder::new_multi_thread().enable_all().build();
    match args.role {
        Role::Hub { on_demand } => {
            let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).map_err(anyhow::Error::msg)?;
            // Follows symlinks, so moving an install's `current` link can't change what later workers run.
            let worker_exe = std::env::current_exe()
                .and_then(std::fs::canonicalize)
                .context("finding this binary to run workers")?;
            let shell = std::env::var("SHELL").ok().filter(|shell| !shell.is_empty());
            let path = std::env::var("PATH").ok();
            runtime()?.block_on(hub::run(hub::HubConfig { paths, worker_exe, shell, path, on_demand }))?;
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
        Role::Worker { stdio, workspaces_root, state_dir } => {
            anyhow::ensure!(stdio, "--stdio is the only worker transport so far");
            let exe = std::env::current_exe().context("finding this binary to delete worktrees with")?;
            runtime()?.block_on(worker::serve(
                tokio::io::stdin(),
                tokio::io::stdout(),
                worker::Config {
                    workspaces_root,
                    state_dir,
                    home: std::env::var_os("HOME").map(PathBuf::from),
                    path: std::env::var_os("PATH"),
                    claude_bin,
                    scratch_dir: std::env::temp_dir(),
                    deleter: worker::Deleter::new(exe, ["delete", "--"]),
                },
            ));
            Ok(())
        }
    }
}
