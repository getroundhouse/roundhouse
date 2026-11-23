//! What a fake hub's world holds of what a workspace's worker reads of its changes on request, past what the hub keeps
//! of them, and how the fake hub answers from it by the worker's rules.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ChangedFileAt, FileDiff};

/// What a workspace's worker reads on request. What it has none of, it reads as a worker from before the request.
#[derive(Clone, Debug, Default)]
pub struct WorkerReads {
    /// Each page of one file's diff, by the file's entry, as a summary names it, and the patch's line the page starts
    /// at, as the worker reads it each time it's asked for: the first the first time, and so on, and the last from then
    /// on, so a test has the file change between reads.
    pub file_diffs: HashMap<(ChangedFileAt, u32), Vec<FileDiff>>,
    /// The entry of each page of one file's diff asked for, as it was asked for, and the line it starts at, in order,
    /// refused or not, which every clone shares, for a test to read.
    pub diffs_read: Arc<Mutex<Vec<(ChangedFileAt, u32)>>>,
}

impl WorkerReads {
    /// The page of the diff of the entry `asked` names from line `first_line`, as it reads this time. One that isn't
    /// said to be untracked is the tracked entry, or the untracked one where there's none, as the worker has it.
    pub(super) fn file_diff(&self, asked: ChangedFileAt, first_line: u32) -> Result<FileDiff, ApiError> {
        let mut read = self.diffs_read.lock();
        let asked = (asked, first_line);
        let before = read.iter().filter(|read| **read == asked).count();
        read.push(asked.clone());
        if self.file_diffs.is_empty() {
            return Err(older("read one file's diff"));
        }
        let (at, line) = asked;
        let untracked = ChangedFileAt { untracked: true, ..at.clone() };
        let tracked_none = !self.file_diffs.keys().any(|(listed, _)| *listed == at);
        let at = if !at.untracked && tracked_none { untracked } else { at };
        let reads = self.file_diffs.get(&(at.clone(), line));
        let page = reads.and_then(|reads| reads.get(before).or(reads.last()));
        page.cloned().ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no page of {} at {line}", at.path)))
    }
}

/// The refusal of a worker too old to do `what`.
fn older(what: &str) -> ApiError {
    ApiError::new(ErrorCode::Unsupported, format!("the workspace's worker can't {what}"))
}
