//! What `agents`, `providers` and `worker` share about the processes they start: signalling a process
//! group, and telling whether a recorded PID is still the same process. Those crates can't depend on
//! each other, and `protocol` has no IO, so this is its own small crate. It's also where the `unsafe`
//! for signalling lives.

#![warn(clippy::wildcard_enum_match_arm)]

mod group;
mod record;

pub use group::{Signal, signal_group};
pub use record::{ProcessRecord, start_time};
