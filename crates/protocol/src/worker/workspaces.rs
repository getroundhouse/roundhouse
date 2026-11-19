//! What a hub asks of a workspace's worktree as a whole: which names git already has, before one is made, making it,
//! putting it back from its branch and removing it, and running its setup and archive scripts.

use serde::{Deserialize, Serialize};

use super::archived::ArchivedGit;
use super::repo_source::{RepoSource, beside_repo_path};
use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::{BranchName, Branches};

/// The names git already has in the repository, which a new workspace can't take, since its branches go in a folder of
/// its name, which is its own: the first part of each local branch's name, and of each of `origin`'s as last fetched,
/// each remote's name, `HEAD` where there's a remote, and each directory where the repository's workspaces go. As git
/// and the file system spell them, for the hub to compare whatever their case, since git keeps a branch as a file, and
/// a file system that doesn't tell case apart takes `Euston` for `euston`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NamesGitHas {
    pub repo_name: String,
    /// The repository, which a clone of is made first if the worker has none.
    #[serde(flatten, with = "beside_repo_path")]
    pub source: RepoSource,
}

impl Request for NamesGitHas {
    type Reply = NamesGitHasReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::NamesGitHas(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NamesGitHasReply {
    pub names: Vec<String>,
}

impl Reply for NamesGitHasReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::NamesGitHas(names) = response else { return Err(response) };
        Ok(names)
    }
}

/// Creates the worktree and its branch, from the repository's checkout, or its clone, made first if the worker has none.
/// Fails with `NameTaken` if the worktree's directory exists, or the branch, or a branch git can't have beside it,
/// whatever its case.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PrepareWorkspace {
    pub repo_name: String,
    #[serde(flatten, with = "beside_repo_path")]
    pub source: RepoSource,
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

/// Puts the worktree back from its kept branch, which is in the repository's checkout, or in its clone. When
/// `archived` is given, the worker ensures the clone exists and imports the saved bundle and branch refs into it
/// before recreating the worktree.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RestoreWorkspace {
    #[serde(flatten, with = "beside_repo_path")]
    pub source: RepoSource,
    pub path: String,
    pub branch: BranchName,
    /// Local branches and unpushed commits saved by the hub when an ephemeral workspace was archived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<ArchivedGit>,
}

impl Request for RestoreWorkspace {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::RestoreWorkspace(self)
    }
}

/// Removes the worktree at `path`, a workspace's own or another made for it, and keeps its branch. Fails with
/// `NeedsForce` if that would lose work, unless `force`. Stops watching the branches of another made for it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RemoveWorkspace {
    #[serde(flatten, with = "beside_repo_path")]
    pub source: RepoSource,
    pub path: String,
    pub force: bool,
    /// The workspace's own worktree, when `path` is another made for it, which is then moved aside to be deleted
    /// where the workspace's own would be, since every archive in the repository clears what waits there. Left out
    /// for the workspace's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own: Option<String>,
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

/// Copies local files and runs the setup script. Resolves when it's finished, however long that is. A worktree made from
/// a clone has no local files to copy, nor local settings.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupWorkspace {
    #[serde(flatten, with = "beside_repo_path")]
    pub source: RepoSource,
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
    /// The workspace's worktree, which a worker doesn't read, since it keeps one setup log, in its state directory.
    /// Kept for workers from before, which can't read the request without it, until the worker protocol next breaks.
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
    #[serde(flatten, with = "beside_repo_path")]
    pub source: RepoSource,
    pub path: String,
    pub env: Vec<(String, String)>,
}

impl Request for ArchiveScript {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::ArchiveScript(self)
    }
}
