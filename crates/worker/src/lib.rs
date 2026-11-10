//! A worker does the work: it runs agents and git for one workspace, and reports to its hub.

#![warn(clippy::wildcard_enum_match_arm)]

pub mod attachments;
mod deleting;
pub mod git;
mod images;
mod serve;
pub mod setup;

pub use agents::ENV_CLAUDE_BIN;
pub use deleting::{Deleter, delete_trees};
pub use serve::{Config, serve};
