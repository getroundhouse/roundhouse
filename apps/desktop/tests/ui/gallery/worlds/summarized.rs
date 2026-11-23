#![cfg(test)]

use super::super::patch;
use super::scenarios::diff;
use client_core::fake_hub::World;
use desktop::patch::FileStatus;
use protocol::model::{ChangeStatus, ChangeSummary, ChangedFile, CommitId, LineCounts, Observation, Observed, Rev};

/// The diff's workspace, whose hub keeps its changed files current while it shows, as a summary that lists the diff's
/// files and leaves out 1,204 more, as an unignored folder of generated files would, with its totals over all of them.
pub(in super::super) fn summarized() -> World {
    let mut world = diff();
    let focus = world.workspaces.iter().find(|w| w.name == super::super::screens::FOCUS).unwrap().id.clone();
    let files = desktop::patch::parse(&patch::patch()).unwrap().iter().map(changed).collect();
    let summary = ChangeSummary::new(CommitId::from(patch::BASE_COMMIT), files);
    let lines = LineCounts { added: summary.lines.added + 48_160, removed: summary.lines.removed };
    let changes = Observed::Current { value: ChangeSummary { lines, left_out: 1_204, ..summary } };
    world.observations.push(Observation::WorkspaceChanges { workspace_id: focus, changes, rev: Rev::new(1) });
    world
}

/// A file of the diff, as a summary names it.
fn changed(file: &desktop::patch::FilePatch) -> ChangedFile {
    let status = match file.status() {
        FileStatus::Added => ChangeStatus::Added,
        FileStatus::Removed => ChangeStatus::Deleted,
        FileStatus::Modified => ChangeStatus::Modified,
        FileStatus::Renamed | FileStatus::Copied => ChangeStatus::Renamed,
    };
    let count = |lines: usize| u64::try_from(lines).unwrap();
    ChangedFile {
        path: file.path().to_string(),
        from: file.old_path().filter(|old| *old != file.path()).map(ToString::to_string),
        status,
        lines: Some(LineCounts { added: count(file.added()), removed: count(file.removed()) }),
        version: None,
    }
}
