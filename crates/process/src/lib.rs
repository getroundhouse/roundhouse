//! What `agents`, `providers`, `worker`, the hub, `local-hub`, the daemon and the CLI share about the processes they
//! start: what one is given beyond the environment it inherits, how one prints the environment it was started in for
//! the one that started it, the user a process runs as, starting one as the leader of a process group, or apart from
//! this process, signalling that group, telling whether a recorded PID is still the same process, keeping a process's
//! own input and output from what it starts, keeping a console window from opening on Windows, telling whether any
//! process runs from a program's file, removing the builds none runs from, running a program just written, which run
//! from programs in a folder, and the test a test's hub and workers stop with. None of those crates is one all the
//! others may depend on, and `protocol` has no IO, so this is its own small crate. It's also where the `unsafe` for all
//! of that lives.

#![warn(clippy::wildcard_enum_match_arm)]

mod busy;
mod environment;
mod group;
mod os_user;
mod printed_environment;
mod record;
mod running_from;
mod running_in;
mod stdio;
mod test_process;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
#[cfg(unix)]
mod unix;
mod unused_builds;
mod window;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as sys;
#[cfg(windows)]
use windows as sys;

pub use busy::{check_version, run_once_not_busy};
pub use environment::{Environment, TakesEnvironment};
#[cfg(unix)]
pub use group::group_running;
pub use group::{Signal, detach, own_group, signal_group};
#[cfg(unix)]
pub use os_user::OsUser;
pub use os_user::{HomeAndShell, home_and_shell};
pub use printed_environment::{ENVIRONMENT_PRINTER, PRINTS_ENVIRONMENT, print_environment, printed_environment};
pub use record::{ProcessRecord, start_time};
pub use running_from::{Claimed, RunningFrom, claim_unused};
pub use running_in::{RunningProgram, running_in};
pub use stdio::keep_stdio_from_children;
pub use test_process::TestProcess;
pub use unused_builds::{ABANDONED_AFTER, Builds};
pub use window::without_window;
