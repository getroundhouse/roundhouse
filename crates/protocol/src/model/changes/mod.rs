//! What a worktree changes against its base, file by file, with how many lines each adds and removes but none of the
//! lines themselves, which a worker works out from git without taking a patch, and one file's diff, read apart.

mod diff;
mod files;
mod summary;
#[cfg(test)]
mod tests;

pub use diff::{FileDiff, FileDiffBody, MOST_PATCH_PAGE_BYTES};
pub use files::{ChangeStatus, ChangedFile, ChangedFileAt, FileVersion, LineCounts};
pub use summary::{ChangeSummary, ChangedFilesPage, MOST_FILES_LISTED};
