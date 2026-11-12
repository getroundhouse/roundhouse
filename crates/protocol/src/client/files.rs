//! The requests that read a workspace's worktree: what files it has, and what one holds, now or in a commit, whole
//! or a page at a time. Each is the body of the `ClientRequest` of its name, which has the same form on the wire as
//! if its fields were the request's own.

use serde::{Deserialize, Serialize};

use crate::model::{CommitId, WorkspaceId};

/// The files in the workspace's worktree, for browsing them: tracked, new and ignored, apart from what's in a
/// folder git ignores whole, or that's a repository of its own, which is listed as a folder for
/// `WorkspaceFolderRead`. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceFiles {
    pub id: WorkspaceId,
}

/// What's directly in the folder at `path` in the workspace's worktree, its files and its folders, for opening one
/// that `WorkspaceFiles` lists as a folder. Refused unless it's a folder inside the worktree, links followed. A hub
/// older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceFolderRead {
    pub id: WorkspaceId,
    pub path: String,
}

/// What the file at `path` in the workspace's worktree holds, for showing it. `path` is relative to the worktree,
/// as `WorkspaceFiles` gives it, and refused unless it's a file inside the worktree, links followed. A hub older
/// than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceFileRead {
    pub id: WorkspaceId,
    pub path: String,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::client::ClientRequest;

    /// The same as when their fields were the requests' own, so a peer from before reads them.
    #[test]
    fn a_request_s_body_travels_beside_its_method() {
        let (id, commit, path) = (WorkspaceId::from("w1"), CommitId::from("c1"), "a.txt".to_string());
        let requests = [
            (ClientRequest::WorkspaceFiles(WorkspaceFiles { id: id.clone() }), json!({"method": "workspace_files"})),
            (
                ClientRequest::WorkspaceFolderRead(WorkspaceFolderRead { id: id.clone(), path: "src".into() }),
                json!({"method": "workspace_folder_read", "path": "src"}),
            ),
            (
                ClientRequest::WorkspaceFileRead(WorkspaceFileRead { id: id.clone(), path: path.clone() }),
                json!({"method": "workspace_file_read", "path": "a.txt"}),
            ),
            (
                ClientRequest::WorkspaceFileReadAt(WorkspaceFileReadAt {
                    id: id.clone(),
                    commit: commit.clone(),
                    path: path.clone(),
                }),
                json!({"method": "workspace_file_read_at", "commit": "c1", "path": "a.txt"}),
            ),
            (
                ClientRequest::WorkspaceFileOpenAt(WorkspaceFileOpenAt {
                    id: id.clone(),
                    commit: commit.clone(),
                    path: path.clone(),
                }),
                json!({"method": "workspace_file_open_at", "commit": "c1", "path": "a.txt"}),
            ),
            (
                ClientRequest::WorkspaceFilePages(WorkspaceFilePages {
                    id,
                    commit,
                    path,
                    page_lines: 256,
                    pages: vec![3],
                }),
                json!({"method": "workspace_file_pages", "commit": "c1", "path": "a.txt", "page_lines": 256, "pages": [3]}),
            ),
        ];

        for (request, mut form) in requests {
            form["id"] = json!("w1");
            let sent = serde_json::to_value(&request).unwrap();

            assert_eq!(sent, form);
            assert_eq!(serde_json::from_value::<ClientRequest>(sent).unwrap(), request);
        }
    }
}
