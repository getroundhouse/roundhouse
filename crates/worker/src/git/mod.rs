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
mod clones;
pub(crate) mod files;
mod github_remote;
mod making;
mod names;
mod pull_request;
pub(crate) mod remote_pull_requests;
mod removal;
#[cfg(test)]
pub(crate) mod tests;
mod worktrees;

pub(crate) use children::add_child_worktree;
pub(crate) use clones::Clones;
pub use github_remote::github_remote;
pub use making::prepare_workspace;
use making::{made_there, make_on_branch, make_on_new_branch};
pub use names::names_git_has;
use names::no_branch_in_the_way;
pub use pull_request::{pull_request, rate_limit};
pub use removal::remove_workspace;
use worktrees::{Changing, Hold, Worktrees, held};

mod commands;

pub(crate) use commands::list_nul_separated;
pub(crate) use commands::version;
use commands::{
    GitOutput, command, commit_of, exists, git, git_error, git_ok, off_thread, ref_exists, reject_option_like, run,
    valid_branch_name,
};
pub use commands::{diff, restore_workspace, status, worktree_there};
