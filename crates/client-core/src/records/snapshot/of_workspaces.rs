//! What a view reads of a workspace, and of the hub, that git, a worker or the update feed last said, rather than what
//! the hub decides: a workspace's branches, which build its worker is, and the release the hub would update to. Each is
//! read here alone, from the record that holds it now, so that where the hub sends one apart from the record, as an
//! observation, only these change.

use protocol::model::{Branches, Workspace};

use super::Snapshot;

impl Snapshot {
    /// What's checked out in `workspace`'s worktree, and each branch it has had that still exists, as its worker last
    /// said. `None` until one has.
    pub fn workspace_branches<'a>(&'a self, workspace: &'a Workspace) -> Option<&'a Branches> {
        workspace.branches.as_ref()
    }

    /// Which build `workspace`'s worker is. A hub of version 4 says only whether it's another build than its own, and
    /// which, so one that isn't reads as the hub's own, connected or not.
    pub fn worker_build<'a>(&'a self, workspace: &'a Workspace) -> WorkerBuild<'a> {
        match workspace.worker_build.as_deref() {
            Some(build) => WorkerBuild::Other(Some(build)),
            None => WorkerBuild::Hubs,
        }
    }

    /// The release the update feed offers a hub installed as a service, as the hub last read it. `None` where no
    /// release is newer, or the hub doesn't read the feed, or hasn't yet.
    pub fn release_offered(&self) -> Option<&str> {
        self.hub_update()?.version.as_deref()
    }
}

/// Which build a workspace's worker is, as a view reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerBuild<'a> {
    /// Not known, as while no worker is connected.
    NotKnown,
    /// The hub's own build.
    Hubs,
    /// Another build than the hub's, which the hub moves the workspace to once nothing in it is at work: the one it
    /// named, or `None` for a worker too old to say.
    Other(Option<&'a str>),
}
