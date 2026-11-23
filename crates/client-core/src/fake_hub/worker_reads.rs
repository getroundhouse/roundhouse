//! What a fake hub's world holds of what a workspace's worker reads of its changes on request, past what the hub keeps
//! of them, and how the fake hub answers from it by the worker's rules.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ChangedFile, ChangedFileAt, ChangedFilesPage, CommitId, FileDiff, MOST_FILES_LISTED};

/// What a workspace's worker reads on request. What it has none of, it reads as a worker from before the request.
#[derive(Clone, Debug, Default)]
pub struct WorkerReads {
    /// Every file the workspace changes, against its base, which the hub pages through past its summary.
    pub changed_files: Option<(CommitId, Vec<ChangedFile>)>,
    /// Each page of one file's diff, by the file's path and the patch's line the page starts at, as the worker reads it
    /// each time it's asked for: the first the first time, and so on, and the last from then on, so a test has the file
    /// change between reads.
    pub file_diffs: HashMap<(String, u32), Vec<FileDiff>>,
    /// The path of each page of one file's diff asked for, and the line it starts at, in order, refused or not, which
    /// every clone shares, for a test to read.
    pub diffs_read: Arc<Mutex<Vec<(String, u32)>>>,
}

impl WorkerReads {
    /// The page of the changed files after the one `after` names, as a worker pages through them.
    pub(super) fn page_after(&self, after: &ChangedFileAt) -> Result<ChangedFilesPage, ApiError> {
        let (base, files) = self.changed_files.as_ref().ok_or_else(|| older("page through its files"))?;
        Ok(ChangedFilesPage::after(base.clone(), files.clone(), after, MOST_FILES_LISTED))
    }

    /// The page of `path`'s diff from line `first_line`, as it reads this time.
    pub(super) fn file_diff(&self, path: &str, first_line: u32) -> Result<FileDiff, ApiError> {
        let asked = (path.to_string(), first_line);
        let mut read = self.diffs_read.lock();
        let before = read.iter().filter(|read| **read == asked).count();
        read.push(asked.clone());
        if self.file_diffs.is_empty() {
            return Err(older("read one file's diff"));
        }
        let reads = self.file_diffs.get(&asked);
        let page = reads.and_then(|reads| reads.get(before).or(reads.last()));
        page.cloned().ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no page of {path} at {first_line}")))
    }
}

/// The refusal of a worker too old to do `what`.
fn older(what: &str) -> ApiError {
    ApiError::new(ErrorCode::Unsupported, format!("the workspace's worker can't {what}"))
}
