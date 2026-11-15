//! Agent adapters: supervised CLI processes, translated into the capability contracts and recorded events.

#![warn(clippy::wildcard_enum_match_arm)]

mod acp;
pub mod antigravity;
mod attachments;
mod binary;
pub mod claude;
pub mod codex;
mod command_parts;
#[cfg(feature = "testing")]
pub mod conformance;
mod error;
mod interface;
mod jsonrpc;
mod model_offers;
mod options;
mod output;
mod registry;
mod starting;
mod stdio;
mod tap;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
mod title;

pub use error::{AgentError, CallFailed, NoAccount, NoTitle};
pub use interface::AgentProcess;
pub use options::{AgentOptions, BinaryLookup, Instructions, ToolServer};
pub use output::AgentOutput;
pub use protocol::paths::{ENV_ANTIGRAVITY_BIN, ENV_CLAUDE_BIN, ENV_CODEX_BIN};
pub use registry::{account, descriptions, spawn, title_for};

/// Overrides the output-drain deadline in test builds.
#[cfg(feature = "testing")]
pub const ENV_DRAIN_GRACE_MS: &str = "ROUNDHOUSE_DRAIN_GRACE_MS";
