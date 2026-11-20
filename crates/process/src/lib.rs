//! What `agents`, `providers`, `worker`, the hub, `local-hub` and the daemon share about the processes they start:
//! what one is given beyond the environment it inherits, starting one as the leader of a process group, or apart from
//! this process, signalling that group, telling whether a recorded PID is still the same process, keeping a process's
//! own input and output from what it starts, keeping a console window from opening on Windows, telling whether any process runs from a program's file, and which run from
//! programs in a folder. None of those crates is one all the others may depend on, and `protocol` has no IO, so this is
//! its own small crate. It's also where the `unsafe` for all of that lives.

#![warn(clippy::wildcard_enum_match_arm)]

mod environment;
mod group;
mod record;
mod running_from;
mod running_in;
mod stdio;
#[cfg(feature = "testing")]
pub mod testing;
#[cfg(unix)]
mod unix;
mod window;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as sys;
#[cfg(windows)]
use windows as sys;

pub use environment::{Environment, TakesEnvironment};
#[cfg(unix)]
pub use group::group_running;
pub use group::{Signal, detach, own_group, signal_group};
pub use record::{ProcessRecord, start_time};
pub use running_from::{Claimed, RunningFrom, claim_unused};
pub use running_in::{RunningProgram, running_in};
pub use stdio::keep_stdio_from_children;
pub use window::without_window;
