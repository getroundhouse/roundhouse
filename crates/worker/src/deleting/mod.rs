//! Deleting the worktrees archiving moved aside. It goes on after the worker that started it has exited, which the hub
//! has it do as soon as archiving is done, so it's a program of its own: this build's `delete` command, which runs
//! [`delete_trees`].

#[cfg(test)]
mod tests;

mod removal;
pub use removal::{Deleter, delete_trees};
