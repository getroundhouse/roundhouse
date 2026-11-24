//! What a fake hub's world says of a workspace's changes as git's patch of all of them, `World::diffs`, read as a
//! worker reads the worktree: the summary of its changed files that the workspace's level sends, and one file's diff,
//! for a world that says nothing else of them.

use protocol::model::{
    ChangeStatus, ChangeSummary, ChangedFile, CommitId, FileDiff, FileDiffBody, GitDiff, LineCounts, Observation,
    Observed, WorkspaceId,
};

use super::revisions::MADE;
use super::world::World;

/// One file of a patch of the whole worktree.
struct Patched<'a> {
    path: &'a str,
    from: Option<&'a str>,
    status: ChangeStatus,
    /// `None` for a file git takes as binary.
    lines: Option<LineCounts>,
    patch: String,
}

impl World {
    /// The summary of `workspace`'s changes the world's patch of them makes, as the workspace's level sends it,
    /// current, where the world has one.
    pub(super) fn summary_of_diff(&self, workspace: &WorkspaceId) -> Option<Observation> {
        let diff = self.diffs.get(workspace)?;
        let files = files_of(&diff.patch).into_iter().map(|file| ChangedFile {
            path: file.path.to_string(),
            from: file.from.map(str::to_string),
            status: file.status,
            lines: file.lines,
            version: None,
            name_not_utf8: false,
        });
        let summary = ChangeSummary::new(base_of(diff), files.collect());
        let changes = Observed::Current { value: summary };
        Some(Observation::WorkspaceChanges { workspace_id: workspace.clone(), changes, rev: MADE })
    }

    /// The diff of the file at `path` alone, as the world's patch of `workspace`'s changes has it, in one page, or the
    /// file unchanged where it lacks it. `None` where the world has no patch.
    pub(super) fn file_diff_of_diff(&self, workspace: &WorkspaceId, path: &str) -> Option<FileDiff> {
        let diff = self.diffs.get(workspace)?;
        let file = files_of(&diff.patch).into_iter().find(|file| file.path == path);
        let body = match file {
            Some(Patched { lines: None, .. }) => FileDiffBody::Binary,
            Some(file) => FileDiffBody::Patch { patch: file.patch, next_line: None },
            None => FileDiffBody::Unchanged,
        };
        Some(FileDiff { base_commit: base_of(diff), version: None, body })
    }
}

fn base_of(diff: &GitDiff) -> CommitId {
    diff.base_commit.clone().unwrap_or_else(|| CommitId::from("0000000"))
}

/// Each file `patch` changes, in the order it has them.
fn files_of(patch: &str) -> Vec<Patched<'_>> {
    let mut files: Vec<Patched<'_>> = Vec::new();
    for line in patch.split_inclusive('\n') {
        if let Some(names) = line.strip_prefix("diff --git a/") {
            let (from, to) = names.trim_end().split_once(" b/").unwrap_or((names.trim_end(), names.trim_end()));
            let from = (from != to).then_some(from);
            let status = if from.is_some() { ChangeStatus::Renamed } else { ChangeStatus::Modified };
            let lines = Some(LineCounts::default());
            files.push(Patched { path: to, from, status, lines, patch: String::new() });
        }
        let Some(file) = files.last_mut() else { continue };
        file.patch.push_str(line);
        if line.starts_with("new file mode") {
            file.status = ChangeStatus::Added;
        } else if line.starts_with("deleted file mode") {
            file.status = ChangeStatus::Deleted;
        } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            file.lines = None;
        } else if let Some(lines) = file.lines.as_mut() {
            if line.starts_with('+') && !line.starts_with("+++") {
                lines.added += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                lines.removed += 1;
            }
        }
    }
    files
}
