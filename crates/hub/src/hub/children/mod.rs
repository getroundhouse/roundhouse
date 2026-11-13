//! A workspace's other worktrees, made for its work, which its agents ask for through the worker's tools. The hub
//! records each before asking the workspace's worker to make it, so a hub that stops meanwhile leaves a record of
//! what may be there, and archiving the workspace removes them with it. `naming/mod.rs` chooses each one's branch and
//! path, `removing.rs` removes them, and `settling.rs` puts right the records an earlier hub left part way.

mod naming;
mod removing;
mod settling;
#[cfg(test)]
mod tests;

pub(super) use removing::would_lose;

mod creation;
use creation::{Making, add, git_status, watch_child};
