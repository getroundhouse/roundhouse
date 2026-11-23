//! What a worktree changes against its base, file by file, with how many lines each adds and removes but none of the
//! lines themselves, which a worker works out from git without taking a patch.

use serde::{Deserialize, Serialize};

use super::CommitId;

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
    /// order. 0 from a worker from before the bound, which leaves none out.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub left_out: u64,
}

/// The most files a [`ChangeSummary`] lists. At about 150 bytes a file in JSON, the most a summary sends is about
/// 750 KB, which every client that follows its workspace is sent whole on each change.
pub const MOST_FILES_LISTED: usize = 5_000;

impl ChangeSummary {
    /// The summary of `files` against `base_commit`. Its totals are the sums of all of `files`, and it lists at most
    /// [`MOST_FILES_LISTED`] of them, tracked files first, as [`ChangeSummary::files`] says.
    pub fn new(base_commit: CommitId, mut files: Vec<ChangedFile>) -> Self {
        let lines = files.iter().filter_map(|file| file.lines).fold(LineCounts::default(), LineCounts::plus);
        files.sort_by(|a, b| {
            let untracked = |file: &ChangedFile| file.status == ChangeStatus::Untracked;
            untracked(a).cmp(&untracked(b)).then_with(|| a.path.cmp(&b.path))
        });
        let left_out = files.split_off(files.len().min(MOST_FILES_LISTED)).len();
        Self { base_commit, lines, files, left_out: u64::try_from(left_out).unwrap_or(u64::MAX) }
    }
}

/// Whether a count is 0, which [`ChangeSummary::left_out`] leaves out of its serde form.
#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde's `skip_serializing_if` passes a reference")]
fn is_zero(count: &u64) -> bool {
    *count == 0
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

    fn file(path: &str, status: ChangeStatus, lines: Option<LineCounts>) -> ChangedFile {
        ChangedFile { path: path.into(), from: None, status, lines, version: None }
    }

    #[test]
    fn a_summary_s_lines_are_the_sums_of_its_files_and_a_file_with_none_counted_adds_none() {
        let files = vec![
            file("a.rs", ChangeStatus::Modified, Some(LineCounts { added: 3, removed: 1 })),
            file("b.png", ChangeStatus::Modified, None),
            file("c.rs", ChangeStatus::Modified, Some(LineCounts { added: 0, removed: 4 })),
        ];

        let summary = ChangeSummary::new("c1".into(), files);

        assert_eq!(summary.lines, LineCounts { added: 3, removed: 5 });
        assert_eq!(summary.left_out, 0);
    }

    /// Past the bound, a summary lists tracked files before untracked ones, each in order of their paths, and leaves
    /// out the rest, whose lines its totals still count.
    #[test]
    fn a_summary_lists_tracked_files_first_up_to_its_bound_and_sums_those_it_leaves_out() {
        let one = Some(LineCounts { added: 1, removed: 0 });
        let mut files =
            Vec::from_fn(MOST_FILES_LISTED, |n| file(&format!("node_modules/{n:05}.js"), ChangeStatus::Untracked, one));
        files.extend([
            file("src/b.rs", ChangeStatus::Modified, Some(LineCounts { added: 2, removed: 3 })),
            file("src/a.rs", ChangeStatus::Deleted, None),
            file("a.txt", ChangeStatus::Untracked, one),
        ]);

        let summary = ChangeSummary::new("c1".into(), files);

        let paths = summary.files.iter().map(|file| file.path.as_str());
        assert_eq!(summary.files.len(), MOST_FILES_LISTED);
        assert_eq!(
            paths.clone().take(4).collect::<Vec<_>>(),
            ["src/a.rs", "src/b.rs", "a.txt", "node_modules/00000.js"]
        );
        assert_eq!(summary.files.last().map(|file| file.path.as_str()), Some("node_modules/04996.js"));
        assert_eq!(summary.left_out, 3);
        let untracked = u64::try_from(MOST_FILES_LISTED).unwrap() + 1;
        assert_eq!(summary.lines, LineCounts { added: untracked + 2, removed: 3 });
    }
}
