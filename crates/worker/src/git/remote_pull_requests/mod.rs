//! Reads explicit remote PRs and their native GitHub stack dependencies, without changing the worktree.

mod parse;
mod query;
mod reading;
pub(crate) use reading::read;
#[cfg(test)]
mod tests;
