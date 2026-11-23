//! One file a worktree changes, as a summary or a page of one names it.

use serde::{Deserialize, Serialize};

/// Lines added and removed, as git counts them: one changed file's, or a whole worktree's, as a summary totals them and
/// the hub observes them for each workspace.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LineCounts {
    pub added: u64,
    pub removed: u64,
}

impl LineCounts {
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self { added: self.added + other.added, removed: self.removed + other.removed }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    /// From the top of the worktree, with `/` between folders, as git names it. Bytes that aren't UTF-8 are replaced,
    /// as a [`super::GitDiff`]'s patch replaces them.
    pub path: String,
    /// Where a renamed file was at the base commit. `None` for any other change.
    pub from: Option<String>,
    pub status: ChangeStatus,
    /// `None` where git counts no lines: for a file it takes as binary, by a NUL byte near its start, and for one
    /// larger than it reads to compare. The worker counts an untracked file's lines the same way, and none of a folder
    /// or of a file it couldn't read.
    pub lines: Option<LineCounts>,
    /// Changes when what the worktree holds at `path` does, and stays the same while that does, but for a file larger
    /// than git reads to compare, whose version changes with its modified time. `None` where the worktree holds nothing
    /// to read: for a deleted file, a folder, as a submodule is, and a file the worker couldn't read.
    pub version: Option<FileVersion>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    /// Not at the base commit, and tracked now.
    Added,
    Modified,
    Deleted,
    /// Moved from [`ChangedFile::from`], whose content git found close enough to call it the same file.
    Renamed,
    /// Changed between a file, a symbolic link and a submodule.
    TypeChanged,
    /// Not tracked, and not ignored, so committing everything would add it.
    Untracked,
    /// One a newer peer sends, which this build shows as a change of no kind it knows.
    #[serde(other)]
    Unknown,
}

/// What a file held, as a hash of it, compared only with another version of the same file. Its form is the worker's,
/// so a worker of another build can give the same content another version, which only costs a client that shows the
/// file one more read of it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct FileVersion(String);

impl FileVersion {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for FileVersion {
    fn from(version: String) -> Self {
        Self(version)
    }
}

impl ChangedFile {
    /// Where it is in the one order a summary and its pages list files in, as [`ChangedFileAt`] says.
    pub fn at(&self) -> ChangedFileAt {
        ChangedFileAt { untracked: self.status == ChangeStatus::Untracked, path: self.path.clone() }
    }

    /// Where it is in that order, to compare with another's.
    pub(super) fn order(&self) -> (bool, &str) {
        (self.status == ChangeStatus::Untracked, self.path.as_str())
    }
}

/// Where a file is in the one order a summary and its pages list files in: those git tracks, in order of their paths,
/// and then the untracked ones, in order of theirs. The same path can be in both, as a file deleted from the index and
/// left in the worktree is, so where a file is names both.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChangedFileAt {
    pub untracked: bool,
    pub path: String,
}
