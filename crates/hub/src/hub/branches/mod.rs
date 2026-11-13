//! Each workspace's branches, which its worker watches and the hub keeps on the workspace's record, so every client
//! knows which branch each worktree is on, and what's been committed and pushed, as it happens rather than when a turn
//! ends. A pull request is its branch's, so the hub reads it again when the worktree checks out a branch other than
//! the one it was found on, a merged one included, or pushes the one it's on, spaced out as an agent's own reads are.

#[cfg(test)]
use protocol::model::{Branches, Head, Upstream, WorkspaceId};
#[cfg(test)]
use protocol::worker::WorkerFrame;

#[cfg(test)]
mod tests;

mod branch_records;

#[cfg(test)]
use branch_records::{Moved, moved};
