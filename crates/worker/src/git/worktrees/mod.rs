//! The lock on a repository's worktrees, which every worker on the machine takes around the git commands it runs that
//! change them or read every one of them.
//!
//! git can't make a repository's worktrees at once. `git worktree add` writes the new worktree's entry in
//! `.git/worktrees/` a file at a time, and a command that reads every entry fails on one whose `commondir` is made but
//! not yet written, with `fatal: failed to read .git/worktrees/NAME/commondir`. `git worktree add` is one such command,
//! since it looks for the branch in every other worktree, and `git fetch` is another. So a command that changes the
//! worktrees holds the lock alone, and one that only reads them holds it beside others reading.
//!
//! The lock is a file in the git directory the repository's worktrees share, so the workers of every hub on the
//! machine take the same one, from the repository or any of its worktrees, and the system lets go of it when a worker
//! goes. Only a worker's own git commands take it. An agent's, and a setup, run or archive script's, don't, so one of
//! theirs that reads every worktree, such as a `git fetch` in its workspace, can still fail that way while a workspace
//! is made, and one of theirs that changes them, as Claude Code's `git worktree add` does for a subagent given a
//! worktree of its own, can still make a worker's fail.

mod locking;
#[cfg(test)]
pub(crate) mod tests;

pub(super) use locking::{Changing, Hold, Worktrees, held};
