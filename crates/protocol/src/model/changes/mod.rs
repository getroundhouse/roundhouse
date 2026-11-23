//! What a worktree changes against its base, file by file, with how many lines each adds and removes but none of the
//! lines themselves, which a worker works out from git without taking a patch.

mod files;
mod summary;
#[cfg(test)]
mod tests;

pub use files::{ChangeStatus, ChangedFile, ChangedFileAt, FileVersion, LineCounts};
pub use summary::{ChangeSummary, ChangedFilesPage, MOST_FILES_LISTED};
