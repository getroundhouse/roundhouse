//! A summary of every file a worktree changes, and a page of the files past those it lists.

use serde::{Deserialize, Serialize};

use super::{ChangedFile, ChangedFileAt, LineCounts};
use crate::model::CommitId;

/// Every file a worktree changes against its base, committed, uncommitted or untracked, and the lines they add and
/// remove in all. A file git ignores isn't in it. A worker sends it to its hub, and the hub sends it to clients as
/// [`Observation::WorkspaceChanges`](super::Observation::WorkspaceChanges), so the two protocols share one form.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChangeSummary {
    /// The commit the summary is taken against: the merge base with the workspace's base ref, or HEAD's commit when
    /// there's no base ref or git finds no merge base with it, as a [`super::GitDiff`] is. A file's lines change when
    /// this does, though its content and its version don't.
    pub base_commit: CommitId,
    /// The sums of the lines of every file the worktree changes, including those `files` leaves out. A file with no
    /// lines counted adds none.
    pub lines: LineCounts,
    /// The files git tracks, in order of their paths, and then the untracked ones, in order of theirs: at most
    /// [`MOST_FILES_LISTED`], the first of that order, so an unignored folder of 300,000 files doesn't crowd out the
    /// agent's ten edits.
    pub files: Vec<ChangedFile>,
    /// How many changed files `files` leaves out past [`MOST_FILES_LISTED`], which come after its last in the same
    /// order, and which a client reads a page at a time with
    /// [`WorkspaceChangedFiles`](crate::client::workspaces::WorkspaceChangedFiles). 0 from a worker from before the
    /// bound, which leaves none out.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub left_out: u64,
}

/// The most files a [`ChangeSummary`] lists. At about 150 bytes a file in JSON, the most a summary sends is about
/// 750 KB, which every client that follows its workspace is sent whole on each change.
pub const MOST_FILES_LISTED: usize = 5_000;

impl ChangeSummary {
    /// The summary of `files` against `base_commit`. Its totals are the sums of all of `files`, and it lists at most
    /// [`MOST_FILES_LISTED`] of them, tracked files first, as [`ChangeSummary::files`] says.
    pub fn new(base_commit: CommitId, files: Vec<ChangedFile>) -> Self {
        Self::listing_at_most(base_commit, files, MOST_FILES_LISTED)
    }

    /// The same, listing at most `most` files, as a worker's tests list fewer than [`MOST_FILES_LISTED`].
    pub fn listing_at_most(base_commit: CommitId, files: Vec<ChangedFile>, most: usize) -> Self {
        let lines = files.iter().filter_map(|file| file.lines).fold(LineCounts::default(), LineCounts::plus);
        let (files, left_out) = first_in_order(files, most);
        Self { base_commit, lines, files, left_out }
    }
}

/// Whether a count is 0, which [`ChangeSummary::left_out`] leaves out of its serde form.
#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde's `skip_serializing_if` passes a reference")]
fn is_zero(count: &u64) -> bool {
    *count == 0
}

/// The files a worktree changes after one, in the order a [`ChangeSummary`] lists them, read as a client asks for the
/// files the summary leaves out, a page at a time. A page is read from the worktree as it is when it's asked for, so
/// it can be newer than the summary it follows.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChangedFilesPage {
    /// The commit its files are taken against, as [`ChangeSummary::base_commit`] is. A page against another base than
    /// the summary it follows is of the worktree after the base moved, so doesn't follow it.
    pub base_commit: CommitId,
    /// At most [`MOST_FILES_LISTED`], the first of that order after the file the page was asked to follow.
    pub files: Vec<ChangedFile>,
    /// How many changed files come after its last, in the same order. 0 on the last page.
    pub left_out: u64,
}

impl ChangedFilesPage {
    /// The page of `files`, against `base_commit`, that follows the file `after` names, of at most `most` files. One
    /// that's no longer changed, or never was, needn't be in `files`: the page starts at the first file after where it
    /// would be.
    pub fn after(base_commit: CommitId, mut files: Vec<ChangedFile>, after: &ChangedFileAt, most: usize) -> Self {
        files.retain(|file| file.order() > (after.untracked, after.path.as_str()));
        let (files, left_out) = first_in_order(files, most);
        Self { base_commit, files, left_out }
    }
}

/// The first `most` of `files`, in the order a summary lists them, and how many are left out after them.
fn first_in_order(mut files: Vec<ChangedFile>, most: usize) -> (Vec<ChangedFile>, u64) {
    files.sort_by(|a, b| a.order().cmp(&b.order()));
    let left_out = files.split_off(files.len().min(most)).len();
    (files, u64::try_from(left_out).unwrap_or(u64::MAX))
}
