//! Reading what's in a workspace's worktree, which its worker does: its git state and its files.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::model::{
    CommitId, FileContents, GitDiff, GitStatus, Workspace, WorkspaceId, WorkspaceStatus, WorktreeFiles,
};
use protocol::worker::{WorkerRequest, WorkerResponse};

use super::{Hub, not_ready, unexpected};
use crate::error::internal;
use crate::worker_link::WorkerLink;

impl Hub {
    /// A workspace that's ready, its worktree's path, and its worker, started if it wasn't running.
    pub(super) async fn ready_worktree(
        self: &Arc<Self>,
        id: &WorkspaceId,
    ) -> Result<(Workspace, String, Arc<WorkerLink>), ApiError> {
        let workspace = self.workspace(id).await?;
        if workspace.status != WorkspaceStatus::Ready {
            return Err(not_ready(&workspace));
        }
        let path = workspace.path.clone().ok_or_else(|| internal("a ready workspace has no path"))?;
        let link = self.ensure_worker(id).await?;
        Ok((workspace, path, link))
    }

    pub(crate) async fn workspace_status(self: &Arc<Self>, id: WorkspaceId) -> Result<GitStatus, ApiError> {
        let (_, path, link) = self.ready_worktree(&id).await?;
        let reply = link.call(WorkerRequest::GitStatus { path }).await?;
        let WorkerResponse::GitStatus(status) = reply else {
            return Err(unexpected(&reply));
        };
        Ok(status)
    }

    pub(crate) async fn workspace_diff(self: &Arc<Self>, id: WorkspaceId) -> Result<GitDiff, ApiError> {
        let (workspace, path, link) = self.ready_worktree(&id).await?;
        let reply = link.call(WorkerRequest::GitDiff { path, base_ref: workspace.base_ref }).await?;
        let WorkerResponse::GitDiff(diff) = reply else {
            return Err(unexpected(&reply));
        };
        Ok(diff)
    }

    /// The files in the workspace's worktree, as the worker lists them.
    pub(crate) async fn workspace_files(self: &Arc<Self>, id: WorkspaceId) -> Result<WorktreeFiles, ApiError> {
        let (_, path, link) = self.ready_worktree(&id).await?;
        let reply = link.call(WorkerRequest::ListFiles { path }).await?;
        let WorkerResponse::Files(files) = reply else {
            return Err(unexpected(&reply));
        };
        Ok(files)
    }

    /// What's directly in a folder of the workspace's worktree, as the worker finds it.
    pub(crate) async fn workspace_folder_read(
        self: &Arc<Self>,
        id: WorkspaceId,
        folder: String,
    ) -> Result<WorktreeFiles, ApiError> {
        let (_, path, link) = self.ready_worktree(&id).await?;
        let reply = link.call(WorkerRequest::FolderRead { path, folder }).await?;
        let WorkerResponse::Files(files) = reply else {
            return Err(unexpected(&reply));
        };
        Ok(files)
    }

    /// What a file in the workspace's worktree holds, as the worker finds it.
    pub(crate) async fn workspace_file_read(
        self: &Arc<Self>,
        id: WorkspaceId,
        file: String,
    ) -> Result<FileContents, ApiError> {
        let (_, path, link) = self.ready_worktree(&id).await?;
        let reply = link.call(WorkerRequest::FileRead { path, file }).await?;
        let WorkerResponse::FileContents(contents) = reply else {
            return Err(unexpected(&reply));
        };
        Ok(contents)
    }

    /// What a file held in a commit of the workspace's repository, as the worker finds it.
    pub(crate) async fn workspace_file_read_at(
        self: &Arc<Self>,
        id: WorkspaceId,
        commit: CommitId,
        file: String,
    ) -> Result<FileContents, ApiError> {
        let (_, path, link) = self.ready_worktree(&id).await?;
        let reply = link.call(WorkerRequest::FileReadAt { path, commit, file }).await?;
        let WorkerResponse::FileContents(contents) = reply else {
            return Err(unexpected(&reply));
        };
        Ok(contents)
    }
}
