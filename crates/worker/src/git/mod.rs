//! Worktrees and git state, by running the `git` binary. Local config, credentials and hooks then
//! behave exactly as they do for the user.

#[cfg(test)]
use protocol::client::{ApiError, ErrorCode};
#[cfg(test)]
use protocol::model::{BranchName, CommitId};
#[cfg(test)]
use protocol::worker::WorkspaceReady;
#[cfg(test)]
use std::path::{Path, PathBuf};

pub(crate) mod blob;
pub(crate) mod branches;
mod children;
pub(crate) mod files;
mod names;
mod pull_request;
mod removal;
#[cfg(test)]
pub(crate) mod tests;

pub(crate) use children::add_child_worktree;
pub use names::names_git_has;
use names::no_branch_in_the_way;
pub use pull_request::{pull_request, rate_limit};
pub use removal::remove_workspace;

mod commands;

pub(crate) use commands::list_nul_separated;
use commands::{
    GH_TIMEOUT, GitOutput, command, commit_of, fetch_if_remote, git, git_error, git_ok, make_parent, off_thread,
    ref_exists, reject_option_like, run, valid_branch_name,
};
pub use commands::{diff, prepare_workspace, restore_workspace, status, worktree_there};
