//! A worker does the work: it runs agents, git and terminals for one workspace, and reports to its hub.

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
mod found_login;
mod gh;
pub mod git;
mod git_credential;
mod images;
mod instructions;
mod linked_images;
mod serve;
pub mod setup;
mod terminals;
mod timers;
mod tools;
mod worktree_file;

pub use agents::{ENV_ANTIGRAVITY_BIN, ENV_CLAUDE_BIN, ENV_CODEX_BIN};
#[cfg(unix)]
pub use attach::attach;
pub use claim::{ClaimError, InstanceClaim, claim_instance};
#[cfg(unix)]
pub use claim::{claim_to_serve, instance_claimed};
pub use deleting::{Deleter, delete_trees};
pub use git_credential::{CredentialHelper, answer_credential_request};
#[cfg(feature = "testing")]
pub use serve::TEST_BUILD;
#[cfg(unix)]
pub use serve::listen;
pub use serve::{Config, serve};
pub use timers::Timers;
