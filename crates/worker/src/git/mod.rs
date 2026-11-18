//! Worktrees and git state, by running the `git` binary. Local config, credentials and hooks then
//! behave exactly as they do for the user.

pub(crate) mod blob;
pub(crate) mod branches;
mod children;
mod clones;
pub(crate) mod files;
mod making;
mod names;
mod removal;
mod repo_url;
mod repository_variables;
#[cfg(test)]
pub(crate) mod tests;
mod worktrees;

pub(crate) use children::add_child_worktree;
pub(crate) use clones::Clones;
pub use making::prepare_workspace;
use making::{made_there, make_on_branch, make_on_new_branch};
pub use names::names_git_has;
use names::no_branch_in_the_way;
pub use removal::remove_workspace;
pub use repo_url::repo_url;
pub(crate) use repository_variables::in_no_repository;
use worktrees::{Changing, Hold, Worktrees, held};

mod commands;

pub(crate) use commands::list_nul_separated;
pub(crate) use commands::version;
use commands::{
    GitOutput, command, commit_of, exists, git, git_error, git_ok, off_thread, ref_exists, reject_option_like, run,
    valid_branch_name,
};
pub use commands::{diff, restore_workspace, status, worktree_there};
