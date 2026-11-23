//! Reading what's in a workspace's worktree, which its worker does: its git state and its files.

use std::sync::Arc;

use protocol::client::{ApiError, files};
use protocol::model::{
    ByteRange, ChangedFileAt, ChangedFilesPage, CommitId, FileContents, FileOpened, FilePages, FileRange, GitDiff,
    GitStatus, WorkspaceId, WorktreeFiles,
};
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

    /// The page of the files the workspace changes after the one `after` names, which the worker reads, as
    /// `WorkspaceChangedFiles` says. The hub keeps none of it.
    pub(crate) async fn workspace_changed_files(
        self: &Arc<Self>,
        id: WorkspaceId,
        after: ChangedFileAt,
    ) -> Result<ChangedFilesPage, ApiError> {
        let worktree = self.ready_worktree(&id).await?;
        let (path, base_ref) = (worktree.path().to_owned(), worktree.workspace().base_ref.clone());
        worktree.link().call(worker::ListChangedFiles { path, base_ref, after }).await
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

    /// Part of a file in the workspace's worktree, such as a video's, as the worker reads it. A worker older than this
    /// refuses it as unsupported, which the client is told.
    pub(crate) async fn workspace_file_read_range(
        self: &Arc<Self>,
        id: WorkspaceId,
        file: String,
        range: ByteRange,
    ) -> Result<FileRange, ApiError> {
        let worktree = self.ready_worktree(&id).await?;
        let path = worktree.path().to_owned();
        let (start, length) = (range.start(), range.length());
        worktree.link().call(worker::FileReadRange { path, file, start, length }).await
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;
    use protocol::client::{ApiError, ErrorCode};
    use protocol::model::{ChangeStatus, ChangedFile, ChangedFileAt, ChangedFilesPage};
    use protocol::worker::{ListChangedFiles, WorkerRequest, WorkerResponse};

    use crate::hub::Hub;
    use crate::hub::tests::{hub_with_worktree, serving};
    use crate::worker_link::WorkerLink;

    fn after() -> ChangedFileAt {
        ChangedFileAt { untracked: true, path: "u1.txt".into() }
    }

    fn page() -> ChangedFilesPage {
        let file = ChangedFile {
            path: "u2.txt".into(),
            from: None,
            status: ChangeStatus::Untracked,
            lines: None,
            version: None,
        };
        ChangedFilesPage { base_commit: "c1".into(), files: vec![file], left_out: 0 }
    }

    /// A hub for the workspace `w`, whose worker answers a request for a page of its changed files with `answer`, and
    /// what that worker was asked.
    fn hub_answering(
        dir: &std::path::Path,
        answer: fn() -> Result<WorkerResponse, ApiError>,
    ) -> (Arc<Hub>, Arc<Mutex<Vec<ListChangedFiles>>>) {
        let hub = hub_with_worktree(dir);
        let asked = Arc::new(Mutex::new(Vec::new()));
        let kept = asked.clone();
        let link = WorkerLink::answering(move |request| {
            let WorkerRequest::ListChangedFiles(listing) = request else { return Some(Ok(WorkerResponse::Ok)) };
            kept.lock().push(listing);
            Some(answer())
        });
        serving(&hub, &link);
        (hub, asked)
    }

    /// The hub passes a client's read of a page of a workspace's changed files on to the workspace's worker, with its
    /// worktree and base, and the worker's page back, and refuses one of a workspace it doesn't have, asking nothing.
    #[tokio::test]
    async fn a_page_of_changed_files_is_read_from_the_workspace_s_worker() {
        let dir = tempfile::tempdir().unwrap();
        let (hub, asked) = hub_answering(dir.path(), || Ok(WorkerResponse::ChangedFilesPage(page())));

        let read = hub.workspace_changed_files("w".into(), after()).await.unwrap();
        let refused = hub.workspace_changed_files("nowhere".into(), after()).await.unwrap_err();

        assert_eq!(read, page());
        assert_eq!(refused.code, ErrorCode::NotFound);
        let base_ref = hub.workspace(&"w".into()).await.unwrap().base_ref;
        assert_eq!(*asked.lock(), [ListChangedFiles { path: "/w".into(), base_ref, after: after() }]);
    }

    /// A worker from before the request refuses it, which the hub passes on, so a client shows only how many files the
    /// summary leaves out.
    #[tokio::test]
    async fn a_worker_from_before_the_request_has_the_hub_refuse_it_as_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let (hub, _) = hub_answering(dir.path(), || Err(ApiError::new(ErrorCode::Unsupported, "unknown request")));

        let refused = hub.workspace_changed_files("w".into(), after()).await.unwrap_err();

        assert_eq!(refused.code, ErrorCode::Unsupported);
    }
}
