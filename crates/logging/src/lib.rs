//! How every process logs, as `docs/principles/observability.md` says: its `main` starts a [`Log`], and everything it
//! builds logs with `tracing`.

#![warn(clippy::wildcard_enum_match_arm)]

mod json;
mod kept;
mod levels;
mod log;
mod panics;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
#[cfg(test)]
mod tests;

pub use kept::move_aside_if_long;
pub use levels::{Levels, OURS};
pub use log::{AlreadyStarted, Log};
