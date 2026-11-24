//! The kinds of what a workspace's worker says of the workspace's worktrees, which branches each has, and of itself,
//! which build it is.

use protocol::model::{Branches, Observation, ObservationKey, Observed, OtherBuild, Rev, WorkspaceId};

use super::kept::Table;
use super::kinds::{Form, Kind};
use super::subjects::ChildWorktreeKey;
use super::tables::Tables;

/// A workspace's branches, as its worker last read them, sent at the workspace's level: current while its worker
/// watches them, and as of when the watch ended otherwise. The hub works out from them which pull request is the
/// workspace's.
pub(crate) struct WorkspaceBranches;

impl Kind for WorkspaceBranches {
    type Of = WorkspaceId;
    type Value = Branches;
    const FORM: Form = Form { table: "workspace_branches", version: 1 };

    fn observation(workspace_id: &WorkspaceId, branches: Observed<Branches>, rev: Rev) -> Option<Observation> {
        Some(Observation::WorkspaceBranches { workspace_id: workspace_id.clone(), branches, rev })
    }

    fn key(workspace_id: &WorkspaceId) -> Option<ObservationKey> {
        Some(ObservationKey::WorkspaceBranches { workspace_id: workspace_id.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.workspace_branches
    }
}

/// That a workspace's worker is another build than the hub's, as its hello said, sent at the hub's level: current while
/// that worker is connected, and gone once it isn't. Kept in memory alone, since only a worker the hub carries on with
/// can be another build, and the hub hears that of each as it does.
pub(crate) struct WorkspaceWorkerBuild;

impl Kind for WorkspaceWorkerBuild {
    type Of = WorkspaceId;
    type Value = OtherBuild;
    const FORM: Form = Form { table: "workspace_worker_build", version: 1 };
    const IN_MEMORY_ALONE: bool = true;

    fn observation(workspace_id: &WorkspaceId, build: Observed<OtherBuild>, rev: Rev) -> Option<Observation> {
        Some(Observation::WorkspaceWorkerBuild { workspace_id: workspace_id.clone(), build, rev })
    }

    fn key(workspace_id: &WorkspaceId) -> Option<ObservationKey> {
        Some(ObservationKey::WorkspaceWorkerBuild { workspace_id: workspace_id.clone() })
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.workspace_worker_build
    }
}

/// The branches of another worktree made for a workspace's work, as the workspace's worker last read them. Kept by the
/// hub, which reads them only to tell which branches are the workspace's as it names a worktree, and sent nowhere.
/// Kept once the worktree is removed, since a branch it had is still the workspace's, and forgotten with the
/// workspace.
pub(crate) struct ChildWorktreeBranches;

impl Kind for ChildWorktreeBranches {
    type Of = ChildWorktreeKey;
    type Value = Branches;
    const FORM: Form = Form { table: "child_worktree_branches", version: 1 };

    fn observation(_: &ChildWorktreeKey, _: Observed<Branches>, _: Rev) -> Option<Observation> {
        None
    }

    fn key(_: &ChildWorktreeKey) -> Option<ObservationKey> {
        None
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.child_worktree_branches
    }
}
