//! Agent adapters: supervised CLI processes, translated into the capability contracts and recorded events.

#![warn(clippy::wildcard_enum_match_arm)]

mod binary;
pub mod claude;
pub mod codex;
#[cfg(feature = "testing")]
pub mod conformance;
mod error;
mod interface;
mod model_offers;
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
