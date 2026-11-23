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
    /// Each page of one file's diff, by the file's path and the patch's line the page starts at.
    pub file_diffs: HashMap<(String, u32), FileDiff>,
    /// The path of each page of one file's diff read, in order, which every clone shares, for a test to read.
    pub diffs_read: Arc<Mutex<Vec<String>>>,
}

impl WorkerReads {
    /// The page of the changed files after the one `after` names, as a worker pages through them.
    pub(super) fn page_after(&self, after: &ChangedFileAt) -> Result<ChangedFilesPage, ApiError> {
        let (base, files) = self.changed_files.as_ref().ok_or_else(|| older("page through its files"))?;
        Ok(ChangedFilesPage::after(base.clone(), files.clone(), after, MOST_FILES_LISTED))
    }

    /// The page of `path`'s diff from line `first_line`.
    pub(super) fn file_diff(&self, path: &str, first_line: u32) -> Result<FileDiff, ApiError> {
        if self.file_diffs.is_empty() {
            return Err(older("read one file's diff"));
        }
        self.diffs_read.lock().push(path.to_string());
        let page = self.file_diffs.get(&(path.to_string(), first_line));
        page.cloned().ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no page of {path} at {first_line}")))
    }
}

/// The refusal of a worker too old to do `what`.
fn older(what: &str) -> ApiError {
    ApiError::new(ErrorCode::Unsupported, format!("the workspace's worker can't {what}"))
}
