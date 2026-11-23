//! What a worktree changes against its base, file by file, with how many lines each adds and removes but none of the
//! lines themselves, which a worker works out from git without taking a patch.

use serde::{Deserialize, Serialize};

use super::CommitId;

/// Every file a worktree changes against its base, committed, uncommitted or untracked, and the lines they add and
/// remove in all. A file git ignores isn't in it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChangeSummary {
    /// The commit the summary is taken against: the merge base with the workspace's base ref, or HEAD's commit when
    /// there's no base ref or git finds no merge base with it, as a [`super::GitDiff`] is. A file's lines change when
    /// this does, though its content and its version don't.
    pub base_commit: CommitId,
    /// The sums of the lines of `files`. A file with no lines counted adds none.
    pub lines: LineCounts,
    /// In order of their paths.
    pub files: Vec<ChangedFile>,
}

impl ChangeSummary {
    /// The summary of `files` against `base_commit`, with its totals the sums of theirs.
    pub fn new(base_commit: CommitId, files: Vec<ChangedFile>) -> Self {
        let lines = files.iter().filter_map(|file| file.lines).fold(LineCounts::default(), LineCounts::plus);
        Self { base_commit, lines, files }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_summary_s_lines_are_the_sums_of_its_files_and_a_file_with_none_counted_adds_none() {
        let file = |path: &str, lines| ChangedFile {
            path: path.into(),
            from: None,
            status: ChangeStatus::Modified,
            lines,
            version: None,
        };
        let files = vec![
            file("a.rs", Some(LineCounts { added: 3, removed: 1 })),
            file("b.png", None),
            file("c.rs", Some(LineCounts { added: 0, removed: 4 })),
        ];

        let summary = ChangeSummary::new("c1".into(), files);

        assert_eq!(summary.lines, LineCounts { added: 3, removed: 5 });
    }
}
