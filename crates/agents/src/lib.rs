//! Agent adapters. Each one runs an agent as a child process and turns its output into normalized
//! events, while keeping every raw line so those events can be rebuilt later.

#![warn(clippy::wildcard_enum_match_arm)]

mod binary;
pub mod claude;
#[cfg(feature = "testing")]
pub mod conformance;
mod error;
mod interface;
mod options;
mod output;
mod registry;
mod stdio;
mod tap;

pub use error::{AgentError, CallFailed, NoAccount, NoTitle};
pub use interface::AgentProcess;
pub use options::{AgentOptions, BinaryLookup, ToolServer};
pub use output::AgentOutput;
pub use protocol::paths::ENV_CLAUDE_BIN;
pub use registry::{account, descriptions, spawn, title_for};

/// Overrides the output-drain deadline in test builds.
#[cfg(feature = "testing")]
pub const ENV_DRAIN_GRACE_MS: &str = "ROUNDHOUSE_DRAIN_GRACE_MS";
