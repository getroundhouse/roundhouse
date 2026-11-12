//! A worker does the work: it runs agents and git for one workspace, and reports to its hub.

#![warn(clippy::wildcard_enum_match_arm)]

mod asks;
#[cfg(unix)]
mod attach;
pub mod attachments;
mod branch_watch;
mod claim;
mod deleting;
mod files;
pub mod git;
mod images;
mod linked_images;
mod serve;
pub mod setup;
mod tools;
mod worktree_file;

pub use agents::ENV_CLAUDE_BIN;
#[cfg(unix)]
pub use attach::attach;
#[cfg(unix)]
pub use claim::claim_to_serve;
pub use claim::{ClaimError, InstanceClaim, claim_instance};
pub use deleting::{Deleter, delete_trees};
#[cfg(unix)]
pub use serve::listen;
pub use serve::{Config, serve};
