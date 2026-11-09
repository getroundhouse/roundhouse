//! A worker does the work: it runs agents and git for one workspace, and reports to its hub.

#![warn(clippy::wildcard_enum_match_arm)]

pub mod attachments;
pub mod git;
mod serve;
pub mod setup;

pub use agents::ENV_CLAUDE_BIN;
pub use serve::{Config, serve};
