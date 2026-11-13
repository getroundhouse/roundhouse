//! What a hub asks of what's in a worktree: what its git says, and its files, read as a person browses them, or
//! written into it for a prompt that carries them.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::attachment::AttachmentBytes;
use crate::model::{self, CommitId, FileContents, FileOpened, WorktreeFiles};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GitStatus {
    pub path: String,
}

impl Request for GitStatus {
    type Reply = model::GitStatus;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::GitStatus(self)
    }
}

impl Reply for model::GitStatus {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::GitStatus(status) = response else { return Err(response) };
        Ok(status)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GitDiff {
    pub path: String,
    pub base_ref: Option<String>,
}

impl Request for GitDiff {
    type Reply = model::GitDiff;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::GitDiff(self)
    }
}

impl Reply for model::GitDiff {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::GitDiff(diff) = response else { return Err(response) };
        Ok(diff)
    }
}

/// Watches the branches of the worktree at `path`, and sends a [`super::WorkerFrame::Branches`] with what they are at
/// once, and again each time they change, until the worker stops. A later watch replaces it. Replies once it's
/// watching and has sent that first frame, or found it can't read them, so a hub that waits for the reply has the
/// frame first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WatchBranches {
    pub path: String,
}

impl Request for WatchBranches {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::WatchBranches(self)
    }
}

/// Writes a file a prompt carries into the worktree at `path`, in `.context/attachments/`, which git ignores, and
/// replies with where it went. The same bytes under the same name find the copy already there, so sending it again
/// makes no second one, and a different file with the same name gets a name of its own.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AttachmentWrite {
    pub path: String,
    pub name: String,
    pub data: AttachmentBytes,
}

impl Request for AttachmentWrite {
    type Reply = AttachmentWritten;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::AttachmentWrite(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AttachmentWritten {
    /// Where it was written, in full.
    pub path: String,
}

impl Reply for AttachmentWritten {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::AttachmentWritten(written) = response else { return Err(response) };
        Ok(written)
    }
}

/// The files in the worktree at `path`, as [`crate::client::files::WorkspaceFiles`] says.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ListFiles {
    pub path: String,
}

impl Request for ListFiles {
    type Reply = WorktreeFiles;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::ListFiles(self)
    }
}

/// What's directly in the folder at `folder`, relative to the worktree at `path`. Refused unless it's a folder inside
/// the worktree once every link is followed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FolderRead {
    pub path: String,
    pub folder: String,
}

impl Request for FolderRead {
    type Reply = WorktreeFiles;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::FolderRead(self)
    }
}

impl Reply for WorktreeFiles {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::Files(files) = response else { return Err(response) };
        Ok(files)
    }
}

/// What the file at `file` holds, relative to the worktree at `path`. Refused unless it's a file inside the worktree
/// once every link is followed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FileRead {
    pub path: String,
    pub file: String,
}

impl Request for FileRead {
    type Reply = FileContents;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::FileRead(self)
    }
}

/// What the file at `file` held in `commit`, relative to the worktree at `path`, from the worktree's repository.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FileReadAt {
    pub path: String,
    pub commit: CommitId,
    pub file: String,
}

impl Request for FileReadAt {
    type Reply = FileContents;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::FileReadAt(self)
    }
}

impl Reply for FileContents {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::FileContents(contents) = response else { return Err(response) };
        Ok(contents)
    }
}

/// Opens the file at `file` as `commit` held it, relative to the worktree at `path`, from the worktree's repository,
/// to be read a page at a time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FileOpenAt {
    pub path: String,
    pub commit: CommitId,
    pub file: String,
}

impl Request for FileOpenAt {
    type Reply = FileOpened;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::FileOpenAt(self)
    }
}

impl Reply for FileOpened {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::FileOpened(opened) = response else { return Err(response) };
        Ok(opened)
    }
}

/// Pages of a file opened with [`FileOpenAt`], with `page_lines` lines a page.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FilePages {
    pub path: String,
    pub commit: CommitId,
    pub file: String,
    pub page_lines: u32,
    pub pages: Vec<u32>,
}

impl Request for FilePages {
    type Reply = model::FilePages;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::FilePages(self)
    }
}

impl Reply for model::FilePages {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::FilePages(pages) = response else { return Err(response) };
        Ok(pages)
    }
}
