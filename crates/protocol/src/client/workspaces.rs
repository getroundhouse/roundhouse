//! The requests about a workspace: making, listing, archiving and restoring it, its title, its setup, and what git and
//! GitHub say of its worktree.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{ClientRequestId, GitDiff, GitStatus, PullRequest, RepoId, Workspace, WorkspaceId};

/// Generates a name if none is given. Uses the repository's default branch if no base is given. Sending the same
/// `client_request_id` again returns the workspace it made, in whatever state it has reached, rather than making
/// another.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceCreate {
    pub repo_id: RepoId,
    pub name: Option<String>,
    pub base_ref: Option<String>,
    #[serde(default)]
    pub client_request_id: Option<ClientRequestId>,
}

impl Request for WorkspaceCreate {
    type Reply = Workspace;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceCreate(self)
    }
}

/// A workspace's record as it is then, which most requests that make a workspace or set something on it, such as
/// whether it's archived, its title, its setup or its comment draft, are answered with. Looking for its pull request is
/// answered with what was found.
impl Reply for Workspace {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Workspace(workspace) = response else { return Err(response) };
        Ok(workspace)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceList {
    pub include_archived: bool,
}

impl Request for WorkspaceList {
    type Reply = Workspaces;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceList(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Workspaces {
    pub workspaces: Vec<Workspace>,
}

impl Reply for Workspaces {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Workspaces(workspaces) = response else { return Err(response) };
        Ok(workspaces)
    }
}

/// Fails if work would be lost or the archive script fails, unless `force` is set. The branch is kept. Archiving it
/// again is refused, changing nothing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceArchive {
    pub id: WorkspaceId,
    pub force: bool,
}

impl Request for WorkspaceArchive {
    type Reply = Workspace;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceArchive(self)
    }
}

/// Restoring one that isn't archived is refused, changing nothing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceRestore {
    pub id: WorkspaceId,
}

impl Request for WorkspaceRestore {
    type Reply = Workspace;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceRestore(self)
    }
}

/// Sets the title someone typed, which nothing generated replaces. It's tidied as
/// [`Title::typed`](crate::model::Title::typed) says, and refused if that leaves nothing or too much. Sent again, it
/// sets the same again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceRename {
    pub id: WorkspaceId,
    pub title: String,
}

impl Request for WorkspaceRename {
    type Reply = Workspace;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceRename(self)
    }
}

/// Titles the workspace again from every prompt its chats have had, by a call of its own to a small model, and replies
/// once that has answered. The new title replaces the one that was there when this was asked, one typed by hand
/// included, since someone asked, and a title set meanwhile stays. Refused as a conflict unless the workspace is
/// ready, and until a chat has had a prompt with something typed in it, since there's nothing to title it from. Sent
/// again, it asks again, since it carries no ID.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceRegenerateTitle {
    pub id: WorkspaceId,
}

impl Request for WorkspaceRegenerateTitle {
    type Reply = Workspace;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceRegenerateTitle(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceStatus {
    pub id: WorkspaceId,
}

impl Request for WorkspaceStatus {
    type Reply = GitStatus;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceStatus(self)
    }
}

impl Reply for GitStatus {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::GitStatus(status) = response else { return Err(response) };
        Ok(status)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceDiff {
    pub id: WorkspaceId,
}

impl Request for WorkspaceDiff {
    type Reply = GitDiff;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceDiff(self)
    }
}

impl Reply for GitDiff {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::GitDiff(diff) = response else { return Err(response) };
        Ok(diff)
    }
}

/// The end of the setup log, which the worker keeps.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceSetupLog {
    pub id: WorkspaceId,
}

impl Request for WorkspaceSetupLog {
    type Reply = SetupLog;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceSetupLog(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupLog {
    pub text: String,
}

impl Reply for SetupLog {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::SetupLog(log) = response else { return Err(response) };
        Ok(log)
    }
}

/// Copies local files and runs the setup script again. Refused while setup is running. Sent again once it has
/// finished, it runs setup again, since it carries no ID.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspaceRerunSetup {
    pub id: WorkspaceId,
}

impl Request for WorkspaceRerunSetup {
    type Reply = Workspace;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspaceRerunSetup(self)
    }
}

/// Reads the pull request for this workspace's branch now, and records it on the workspace, which every client then
/// hears of. Safe to send again, since one asked for while another read waits shares the answer of the next. The reply
/// is `None` if there isn't one, or if it can't be read here, such as without GitHub's CLI, a login to it or a GitHub
/// remote, and none was known. When it can't be read just now, such as while GitHub can't be reached or its hourly
/// limit is used up, the reply is what was known, and an error if nothing was.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkspacePullRequest {
    pub id: WorkspaceId,
}

impl Request for WorkspacePullRequest {
    type Reply = PullRequestReply;

    fn into_request(self) -> ClientRequest {
        ClientRequest::WorkspacePullRequest(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PullRequestReply {
    pub pull_request: Option<PullRequest>,
}

impl Reply for PullRequestReply {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::PullRequest(found) = response else { return Err(response) };
        Ok(found)
    }
}
