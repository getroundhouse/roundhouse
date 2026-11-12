//! A worker does the work: it runs agents and git for one workspace, and reports to its hub.

#![warn(clippy::wildcard_enum_match_arm)]

mod asks;
#[cfg(unix)]
mod attach;
pub mod attachments;
mod branch_watch;
mod claim;
mod context_dir;
mod deleting;
mod files;
pub mod git;
mod images;
mod instructions;
mod linked_images;
mod serve;
pub mod setup;
mod tools;
mod worktree_file;

pub use agents::ENV_CLAUDE_BIN;
#[cfg(unix)]
pub use attach::attach;
pub use claim::{ClaimError, InstanceClaim, claim_instance};
#[cfg(unix)]
pub use claim::{claim_to_serve, instance_claimed};
pub use deleting::{Deleter, delete_trees};
#[cfg(feature = "testing")]
pub use serve::TEST_BUILD;
#[cfg(unix)]
pub use serve::listen;
pub use serve::{Config, serve};
