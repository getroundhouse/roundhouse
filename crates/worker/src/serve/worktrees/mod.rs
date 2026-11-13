//! The workspace's worktrees, its own and each other one made for its work: which names git already has, before its
//! own is made, making its own, putting it back and saying whether it's there, making another, removing any of them,
//! and watching the branches of each.

#[cfg(test)]
mod tests;

mod operations;
pub(super) use operations::{Watches, add_child, names_git_has, prepare, remove, restore, there, watch_branches};
