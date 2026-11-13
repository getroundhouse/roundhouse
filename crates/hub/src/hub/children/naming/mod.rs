//! What a workspace's other worktrees are called and where they go: the branch each is on, which of a repository's
//! branches are a workspace's, and each one's directory in `.children/` beside the workspaces. All of it is the hub's
//! to choose, and the worker makes what it's given.

#[cfg(test)]
mod tests;

mod branches;

#[cfg(test)]
pub(super) use branches::child_path;
pub(super) use branches::{Try, new_branches, places_of_its};
