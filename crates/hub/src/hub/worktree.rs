//! Reading what's in a workspace's worktree, which its worker does: its git state and its files.

use std::sync::Arc;

use protocol::client::{ApiError, files};
use protocol::model::{CommitId, FileContents, FileOpened, FilePages, GitDiff, GitStatus, WorkspaceId, WorktreeFiles};
use protocol::worker;

use super::Hub;

impl Hub {
    pub(crate) async fn workspace_status(self: &Arc<Self>, id: WorkspaceId) -> Result<GitStatus, ApiError> {
        let worktree = self.ready_worktree(&id).await?;
        worktree.link().call(worker::GitStatus { path: worktree.path().to_owned() }).await
    }

    pub(crate) async fn workspace_diff(self: &Arc<Self>, id: WorkspaceId) -> Result<GitDiff, ApiError> {
        let worktree = self.ready_worktree(&id).await?;
        let (path, base_ref) = (worktree.path().to_owned(), worktree.workspace().base_ref.clone());
        worktree.link().call(worker::GitDiff { path, base_ref }).await
    }

    /// The files in the workspace's worktree, as the worker lists them.
    pub(crate) async fn workspace_files(self: &Arc<Self>, id: WorkspaceId) -> Result<WorktreeFiles, ApiError> {
        let worktree = self.ready_worktree(&id).await?;
        let path = worktree.path().to_owned();
        worktree.link().call(worker::ListFiles { path }).await
    }

    /// What's directly in a folder of the workspace's worktree, as the worker finds it.
    pub(crate) async fn workspace_folder_read(
        self: &Arc<Self>,
        id: WorkspaceId,
        folder: String,
    ) -> Result<WorktreeFiles, ApiError> {
        let worktree = self.ready_worktree(&id).await?;
        let path = worktree.path().to_owned();
        worktree.link().call(worker::FolderRead { path, folder }).await
    }

    /// What a file in the workspace's worktree holds, as the worker finds it.
    pub(crate) async fn workspace_file_read(
        self: &Arc<Self>,
        id: WorkspaceId,
        file: String,
    ) -> Result<FileContents, ApiError> {
        let worktree = self.ready_worktree(&id).await?;
        let path = worktree.path().to_owned();
        worktree.link().call(worker::FileRead { path, file }).await
    }

    /// What a file held in a commit of the workspace's repository, as the worker finds it.
    pub(crate) async fn workspace_file_read_at(
        self: &Arc<Self>,
        id: WorkspaceId,
        commit: CommitId,
        file: String,
    ) -> Result<FileContents, ApiError> {
        let worktree = self.ready_worktree(&id).await?;
        let path = worktree.path().to_owned();
        worktree.link().call(worker::FileReadAt { path, commit, file }).await
    }

    /// Opens a file as a commit of the workspace's repository held it, to be read a page at a time, as the worker
    /// finds it.
    pub(crate) async fn workspace_file_open_at(
        self: &Arc<Self>,
        id: WorkspaceId,
        commit: CommitId,
        file: String,
    ) -> Result<FileOpened, ApiError> {
        let worktree = self.ready_worktree(&id).await?;
        let path = worktree.path().to_owned();
        worktree.link().call(worker::FileOpenAt { path, commit, file }).await
    }

    /// Pages of a file opened with [`Self::workspace_file_open_at`], as the worker reads them.
    pub(crate) async fn workspace_file_pages(
        self: &Arc<Self>,
        asked: files::WorkspaceFilePages,
    ) -> Result<FilePages, ApiError> {
        let files::WorkspaceFilePages { id, commit, path: file, page_lines, pages } = asked;
        let worktree = self.ready_worktree(&id).await?;
        let path = worktree.path().to_owned();
        worktree.link().call(worker::FilePages { path, commit, file, page_lines, pages }).await
    }
}
