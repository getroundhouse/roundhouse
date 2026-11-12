//! What a hub asks of a workspace's worktree as a whole: making it, putting it back from its branch and removing it,
//! and running its setup and archive scripts.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::{BranchName, Branches};

/// Creates the worktree and its branch. Fails with `NameTaken` if either already exists.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PrepareWorkspace {
    pub repo_name: String,
    pub repo_path: String,
    pub workspace_name: String,
    pub branch: BranchName,
    pub base_ref: Option<String>,
}

impl Request for PrepareWorkspace {
    type Reply = WorkspaceReady;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::PrepareWorkspace(self)
    }
}

/// The worktree a [`PrepareWorkspace`] made.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceReady {
    pub path: String,
    pub branch: BranchName,
    pub base_ref: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// The worktree's branches, as it was made, which a watch then starts from. `None` when they couldn't be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches: Option<Branches>,
}

impl Reply for WorkspaceReady {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::WorkspaceReady(ready) = response else { return Err(response) };
        Ok(ready)
    }
}

/// Puts the worktree back from its kept branch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RestoreWorkspace {
    pub repo_path: String,
    pub path: String,
    pub branch: BranchName,
}

impl Request for RestoreWorkspace {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::RestoreWorkspace(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RemoveWorkspace {
    pub repo_path: String,
    pub path: String,
    pub force: bool,
}

impl Request for RemoveWorkspace {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::RemoveWorkspace(self)
    }
}

/// Whether a worktree is at `path`, for a hub that finds a workspace an earlier one was archiving or restoring.
/// Answered once any [`RemoveWorkspace`] or [`RestoreWorkspace`] this worker is still carrying out has ended, so it
/// says where that left the worktree.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorktreeThere {
    pub path: String,
}

impl Request for WorktreeThere {
    type Reply = WorktreeThereReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::WorktreeThere(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorktreeThereReply {
    pub there: bool,
}

impl Reply for WorktreeThereReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::WorktreeThere(there) = response else { return Err(response) };
        Ok(there)
    }
}

/// Copies local files and runs the setup script. Resolves when it's finished, however long that is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupWorkspace {
    pub repo_path: String,
    pub path: String,
    pub env: Vec<(String, String)>,
}

impl Request for SetupWorkspace {
    type Reply = SetupDone;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::SetupWorkspace(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupDone {
    /// `None` if it was stopped.
    pub code: Option<i32>,
}

impl Reply for SetupDone {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::SetupDone(done) = response else { return Err(response) };
        Ok(done)
    }
}

/// Stops the setup script, including one asked for just before this that hasn't started yet. Harmless if there isn't
/// one.
#[derive(Clone, Copy, Debug)]
pub struct CancelSetup;

impl Request for CancelSetup {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::CancelSetup
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupLog {
    pub path: String,
}

impl Request for SetupLog {
    type Reply = SetupLogReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::SetupLog(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupLogReply {
    pub text: String,
}

impl Reply for SetupLogReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::SetupLog(log) = response else { return Err(response) };
        Ok(log)
    }
}

/// Runs the archive script, if there is one. Fails if the script does.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ArchiveScript {
    pub repo_path: String,
    pub path: String,
    pub env: Vec<(String, String)>,
}

impl Request for ArchiveScript {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::ArchiveScript(self)
    }
}
