//! Summarizing a worktree's changes, for whoever asks: its hub, or a watch on the worktree as its files change, which
//! isn't built.

use super::listing::{Listed, listed};
use super::seen::Seen;
use parking_lot::Mutex;
use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ChangeStatus, ChangeSummary, ChangedFile, LineCounts};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// What the worker last read of each worktree it summarized, which each clone shares, so whatever summarizes a worktree
/// reads only the files that have changed since any summary of it did.
#[derive(Clone, Default)]
pub(crate) struct ChangeSummaries {
    seen: Arc<Mutex<HashMap<PathBuf, Seen>>>,
}

impl ChangeSummaries {
    /// What the worktree at `worktree` changes against `base_ref`, as [`ChangeSummary`] says, with at most
    /// [`protocol::model::MOST_FILES_LISTED`] files listed. What it keeps of the worktree's files is only of those the
    /// worktree changes, including those the summary leaves out, whose lines its totals count.
    pub(crate) async fn summarize(
        &self,
        given: &Environment,
        worktree: &Path,
        base_ref: Option<&str>,
    ) -> Result<ChangeSummary, ApiError> {
        let (base, files) = listed(given, worktree, base_ref).await?;
        // Taken out while the files are read, so the lock isn't held over reading them. Another summary of the
        // worktree meanwhile reads each of its files again, and whichever finishes last is kept.
        let before = self.seen.lock().remove(worktree).unwrap_or_default();
        let at = worktree.to_path_buf();
        let (files, seen) = tokio::task::spawn_blocking(move || versioned(&at, files, &before)).await.map_err(|e| {
            ApiError::new(ErrorCode::Internal, format!("reading a worktree's changed files stopped: {e}"))
        })?;
        self.seen.lock().insert(worktree.to_path_buf(), seen);
        Ok(ChangeSummary::new(base, files))
    }
}

/// Each file of `files` with its version, and an untracked one with its lines, from what the worktree holds of it, and
/// what was read of them.
fn versioned(worktree: &Path, files: Vec<Listed>, before: &Seen) -> (Vec<ChangedFile>, Seen) {
    let at = SystemTime::now();
    let mut seen = Seen::default();
    let changed = files
        .into_iter()
        .map(|file| {
            // One deleted from the index can be in the worktree still, untracked, as a file of its own here.
            let deleted = file.status == ChangeStatus::Deleted;
            let read = (!deleted).then(|| before.look(&mut seen, worktree, &file.path, at)).flatten();
            let lines = if file.status == ChangeStatus::Untracked {
                read.as_ref().and_then(|read| read.lines).map(new_lines)
            } else {
                file.lines
            };
            ChangedFile {
                path: lossy(file.path),
                from: file.from.map(lossy),
                status: file.status,
                lines,
                version: read.map(|read| read.version),
            }
        })
        .collect();
    (changed, seen)
}

fn new_lines(added: u64) -> LineCounts {
    LineCounts { added, removed: 0 }
}

/// As a [`protocol::model::GitDiff`]'s patch has a path that isn't UTF-8.
fn lossy(path: Vec<u8>) -> String {
    String::from_utf8_lossy_owned(path)
}
