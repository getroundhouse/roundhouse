//! One file's diff against a worktree's base, which a worker reads a page at a time apart from the summary.

use serde::{Deserialize, Serialize};

use super::FileVersion;
use crate::model::CommitId;

/// One page of the diff of one file a worktree changes, as a worker reads it for
/// [`WorkspaceFileDiff`](crate::client::workspaces::WorkspaceFileDiff).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    /// The commit it's taken against, as [`ChangeSummary::base_commit`](super::ChangeSummary::base_commit) is.
    pub base_commit: CommitId,
    /// The version of what the worktree held at the file's path while it was diffed, as [`ChangedFile::version`](super::ChangedFile::version) names
    /// versions. `None` where the worktree holds nothing to read, as for a deleted file, and where the file changed
    /// while it was diffed, so the patch may be of neither version.
    pub version: Option<FileVersion>,
    pub body: FileDiffBody,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "diff", rename_all = "snake_case")]
pub enum FileDiffBody {
    /// git's unified diff of this file alone, with three lines of context: its lines from the one the request names up
    /// to `next_line`, at most [`MOST_PATCH_PAGE_BYTES`] of them. `next_line` is `None` on the last page. A single line
    /// longer than a page is cut to one, and still ends with a newline.
    Patch { patch: String, next_line: Option<u32> },
    /// git takes it as binary, or it's larger than git reads to compare, so it has no lines. A client reads either side
    /// whole, with `WorkspaceFileReadAt` and `WorkspaceFileRead`, as it reads an image.
    Binary,
    /// The worktree changes nothing at the path against `base_commit` now, as when the file was put back, or an
    /// untracked one was deleted.
    Unchanged,
    /// One a newer peer sends, which this build shows as a diff it can't draw.
    #[serde(other)]
    Unknown,
}

/// The most of a patch one page of a [`FileDiff`] holds, in bytes. It's the 4 MB past which the file tab reads a file a
/// page of lines at a time, as `docs/spec/diff.md` says, and a diff past it comes in pages, as
/// `docs/principles/observations.md` asks of detail past a size.
pub const MOST_PATCH_PAGE_BYTES: usize = 4 * 1024 * 1024;
