//! Making another worktree for a workspace's work, in `.children/` beside the workspaces, as an [`AddChildWorktree`]
//! asks: what git does, and what's left when it fails part way. Removing one is removing any worktree, as archiving
//! does.
//!
//! [`AddChildWorktree`]: protocol::worker::AddChildWorktree

#[cfg(test)]
mod tests;

mod making;
pub(crate) use making::add_child_worktree;
#[cfg(test)]
use making::on_a_new_branch;
