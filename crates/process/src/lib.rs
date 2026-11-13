//! What `agents`, `providers`, `worker`, the hub, `client-core` and the daemon share about the processes they start:
//! starting one as the leader of a process group, or apart from this process, signalling that group, telling whether a
//! recorded PID is still the same process, keeping a process's own input and output from what it starts, and telling
//! whether any process runs from a program's file. None of those crates is one all the others may depend on, and
//! `protocol` has no IO, so this is its own small crate. It's also where the `unsafe` for all of that lives.

#![warn(clippy::wildcard_enum_match_arm)]

mod group;
mod record;
mod running_from;
mod stdio;
#[cfg(feature = "testing")]
pub mod testing;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as sys;
#[cfg(windows)]
use windows as sys;

pub use group::{Signal, detach, own_group, signal_group};
pub use record::{ProcessRecord, start_time};
pub use running_from::{Claimed, RunningFrom, claim_unused};
pub use stdio::keep_stdio_from_children;
