//! Where a prompt's files go in its worktree, for the agent to open: `.context/attachments/`, which git is told to
//! ignore, as `crate::context_dir` says, so the files never show as changes.

#[cfg(test)]
mod tests;

mod writing;

#[cfg(test)]
use writing::numbered;
pub use writing::write;
