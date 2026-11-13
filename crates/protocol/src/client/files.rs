//! The requests that read a workspace's worktree: what files it has, and what one holds, now or in a commit, whole
//! or a page at a time.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{CommitId, FileContents, FileOpened, FilePages, WorkspaceId, WorktreeFiles};

/// The files in the workspace's worktree, for browsing them: tracked, new and ignored, apart from what's in a
/// folder git ignores whole, or that's a repository of its own, which is listed as a folder for
/// `WorkspaceFolderRead`. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceFiles {
    pub id: WorkspaceId,
}

impl Request for WorkspaceFiles {
    type Reply = WorktreeFiles;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceFiles(self)
    }
}

/// What's directly in the folder at `path` in the workspace's worktree, its files and its folders, for opening one
/// that `WorkspaceFiles` lists as a folder. Refused unless it's a folder inside the worktree, links followed. A hub
/// older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceFolderRead {
    pub id: WorkspaceId,
    pub path: String,
}

impl Request for WorkspaceFolderRead {
    type Reply = WorktreeFiles;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceFolderRead(self)
    }
}

impl Reply for WorktreeFiles {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::WorktreeFiles(files) = response else { return Err(response) };
        Ok(files)
    }
}

/// What the file at `path` in the workspace's worktree holds, for showing it. `path` is relative to the worktree,
/// as `WorkspaceFiles` gives it, and refused unless it's a file inside the worktree, links followed. A hub older
/// than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceFileRead {
    pub id: WorkspaceId,
    pub path: String,
}

impl Request for WorkspaceFileRead {
    type Reply = FileContents;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceFileRead(self)
    }
}

/// What the file at `path` held in `commit`, in the repository of the workspace's worktree, for showing what a
/// change replaced: the diff's `base_commit`, so it's the side the diff was taken from. `path` is relative, as
/// `WorkspaceFiles` gives it, and a link there isn't followed, so it reads as the path it leads to. Refused with
/// `InvalidRequest` when `commit` isn't a full object ID, such as a short one or a branch's name, when `path`
/// isn't written as a list gives it, and when the commit has a folder or a submodule there. Refused with
/// `NotFound` when the repository has no commit by that ID, or the commit has nothing at `path`. A hub older than
/// this refuses it as unsupported, rather than reading the file as it is now, which a field added to
/// `WorkspaceFileRead` would get.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceFileReadAt {
    pub id: WorkspaceId,
    pub commit: CommitId,
    pub path: String,
}

impl Request for WorkspaceFileReadAt {
    type Reply = FileContents;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceFileReadAt(self)
    }
}

impl Reply for FileContents {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::FileContents(contents) = response else { return Err(response) };
        Ok(contents)
    }
}

/// Opens the file at `path` as `commit` held it, to be read a page of lines at a time with `WorkspaceFilePages`,
/// for showing a file that can be too big for `WorkspaceFileReadAt` to send, such as the lines a diff leaves out
/// of it. Answers with `FileOpened`, with every page when the file is small enough to send at once, and is refused
/// as `WorkspaceFileReadAt` is. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceFileOpenAt {
    pub id: WorkspaceId,
    pub commit: CommitId,
    pub path: String,
}

impl Request for WorkspaceFileOpenAt {
    type Reply = FileOpened;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceFileOpenAt(self)
    }
}

impl Reply for FileOpened {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::FileOpened(opened) = response else { return Err(response) };
        Ok(opened)
    }
}

/// Pages of the lines of the file at `path` as `commit` held it, as `WorkspaceFileOpenAt` opened it, with
/// `page_lines` lines a page, as the open said. Answers with `FilePages`: those asked for, in the order the file has
/// them, as many as fit in what's sent at once and always the first, and none past the file's end. A line too long to
/// send whole is cut short, and says so. Refused as `WorkspaceFileReadAt` is, and with `InvalidRequest` when
/// `page_lines` is 0 or more than 4096. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceFilePages {
    pub id: WorkspaceId,
    pub commit: CommitId,
    pub path: String,
    pub page_lines: u32,
    pub pages: Vec<u32>,
}

impl Request for WorkspaceFilePages {
    type Reply = FilePages;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceFilePages(self)
    }
}

impl Reply for FilePages {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::FilePages(pages) = response else { return Err(response) };
        Ok(pages)
    }
}
