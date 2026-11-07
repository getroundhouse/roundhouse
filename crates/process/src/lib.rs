//! What `agents`, `providers`, `worker`, `hub` and `client-core` share about processes: signalling a
//! process or its group, and telling whether a recorded PID is still the same process. Those crates
//! can't depend on each other, and `protocol` has no IO, so this is its own small crate. It's also where
//! the `unsafe` for signalling lives.

#![warn(clippy::wildcard_enum_match_arm)]

mod group;
mod record;

pub use group::{Signal, signal_group, signal_process};
pub use record::{ProcessRecord, start_time};
