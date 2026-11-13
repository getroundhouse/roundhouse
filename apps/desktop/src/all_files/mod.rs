//! All files: every file in the workspace's worktree, tracked, new or ignored, in folders that start closed. A folder
//! git ignores whole, such as `target`, is read only when it's opened, since it can hold hundreds of thousands of
//! files. Clicking a file shows it in the workspace's file tab as it is in the worktree, rather than its diff.

mod listed;

mod browser;
pub use browser::{AllFiles, AllFilesEvent};
