//! What a hub asks of a workspace's other worktrees, the ones made for its work besides its own, which
//! `docs/spec/workspaces.md` describes: making one, and watching its branches. One is removed as the workspace's own
//! is, with [`RemoveWorkspace`](super::RemoveWorkspace).

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::{BranchName, Branches, CommitId};

/// Makes another worktree for a workspace's work at `path`, and copies ignored files into it as setup does, without
/// running the setup script. One there on the branch asked for, or being made there, is made already, so asking
/// again, as a new hub does what the last didn't hear back about, answers with it. Fails with `NameTaken` if
/// something else has the path, or, for a new branch, if the branch exists or git can't have it beside one that does.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AddChildWorktree {
    pub repo_path: String,
    /// The workspace's own worktree, which git runs in, so a base of `HEAD` is the commit it has checked out.
    pub own: String,
    /// Where the hub has recorded it, which is its to choose, as a workspace's own worktree's isn't: in
    /// [`CHILDREN`](crate::model::CHILDREN) beside `own`, in the workspace's folder there, as
    /// `.children/euston/try-the-parser`. A worker refuses one anywhere else.
    pub path: String,
    pub on: ChildBranch,
}

impl Request for AddChildWorktree {
    type Reply = ChildWorktreeAdded;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::AddChildWorktree(self)
    }
}

/// The branch a worktree is made on. A worker that doesn't know a kind can't read the request, and refuses it, rather
/// than make the worktree some other way.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChildBranch {
    /// Made from `base`, a branch, tag or commit, which the workspace's own worktree resolves, so `HEAD` is the
    /// commit it has checked out.
    New { branch: BranchName, base: String },
    /// One that exists, as it is.
    Existing { branch: BranchName },
}

/// The worktree an [`AddChildWorktree`] made, or found made already.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChildWorktreeAdded {
    /// The commit it has checked out.
    pub commit: CommitId,
    /// The ignored files copied into it, relative to it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copied: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Its branches, as it was made, which a watch then starts from. `None` when they couldn't be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches: Option<Branches>,
}

impl Reply for ChildWorktreeAdded {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::ChildWorktreeAdded(added) = response else { return Err(response) };
        Ok(added)
    }
}

/// Watches the branches of another of the workspace's worktrees, as [`WatchBranches`](super::WatchBranches) does its
/// own, sending a [`WorkerFrame::ChildBranches`](super::WorkerFrame::ChildBranches) for it, until it's removed through
/// this worker or the worker stops. A later watch of the same worktree replaces it, and the watch of the workspace's
/// own stays. Answered as that is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WatchChildBranches {
    pub path: String,
}

impl Request for WatchChildBranches {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::WatchChildBranches(self)
    }
}
