//! What a view reads of a workspace, and of the hub, that git, a worker or the update feed last said, rather than what
//! the hub decides: a workspace's branches, which build its worker is, and the release the hub would update to. Each is
//! read from its observation: the branches at the workspace's own level, so only while something follows it, and the
//! rest at the hub's.

use protocol::model::{Branches, Observation, ObservationKey, Workspace};

use super::Snapshot;
use super::observed::said;

impl Snapshot {
    /// What's checked out in `workspace`'s worktree, and each branch it has had that still exists, as its worker last
    /// said. `None` until one has, and while nothing follows the workspace.
    pub fn workspace_branches<'a>(&'a self, workspace: &'a Workspace) -> Option<&'a Branches> {
        let key = ObservationKey::WorkspaceBranches { workspace_id: workspace.id.clone() };
        let Some(Observation::WorkspaceBranches { branches, .. }) = self.observation(&key) else { return None };
        said(branches)
    }

    /// Which build `workspace`'s worker is, which the hub says only of a connected worker that's another build than
    /// its own. One whose hello named no build, which no worker of protocol 5 or 6 sends, isn't known.
    pub fn worker_build<'a>(&'a self, workspace: &'a Workspace) -> WorkerBuild<'a> {
        let key = ObservationKey::WorkspaceWorkerBuild { workspace_id: workspace.id.clone() };
        let Some(Observation::WorkspaceWorkerBuild { build, .. }) = self.observation(&key) else {
            return WorkerBuild::NotKnown;
        };
        said(build).and_then(|other| other.said.as_deref()).map_or(WorkerBuild::NotKnown, WorkerBuild::Other)
    }

    /// The release the update feed offers a hub installed as a service, as the hub last read it. `None` where no
    /// release is newer, or the hub doesn't read the feed, or hasn't yet.
    pub fn release_offered(&self) -> Option<&str> {
        let Some(Observation::ReleaseOffered { offered, .. }) = self.observation(&ObservationKey::ReleaseOffered)
        else {
            return None;
        };
        said(offered)?.version.as_deref()
    }
}

/// Which build a workspace's worker is, as a view reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerBuild<'a> {
    /// Not known, as while no worker is connected, or one of the hub's own build is, which the hub doesn't say.
    NotKnown,
    /// Another build than the hub's, the one its worker named, which the hub moves the workspace off once nothing in it
    /// is at work.
    Other(&'a str),
}
