//! What a hub asks a worker of GitHub, through the `gh` that runs in its worktree: the pull request of the branch
//! checked out, and how much of GitHub's limit is left. And who the machine's own `gh` is logged in as.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::{self, BranchName, GithubLogin};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PullRequest {
    pub path: String,
}

impl Request for PullRequest {
    type Reply = PullRequestReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::PullRequest(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PullRequestReply {
    pub pull_request: Option<model::PullRequest>,
    /// The branch checked out as it looked, whose pull request this is. `None` on a detached HEAD, and from a worker
    /// that doesn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<BranchName>,
}

impl Reply for PullRequestReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::PullRequest(found) = response else { return Err(response) };
        Ok(found)
    }
}

/// How much of GitHub's hourly limit is left for the `gh` that runs in this worktree. Asking costs none of it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubRateLimit {
    pub path: String,
}

impl Request for GithubRateLimit {
    type Reply = RateLimit;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::GithubRateLimit(self)
    }
}

/// GitHub's hourly limit on GraphQL, which `gh pr view` and the worker's own queries spend, as GitHub counted it when
/// asked. It counts points, not calls: a call costs one, or more when it asks for lists within lists.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimit {
    pub limit: u32,
    pub remaining: u32,
    /// When it's back to `limit`.
    pub resets_at_ms: i64,
}

impl Reply for RateLimit {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::GithubRateLimit(limit) = response else { return Err(response) };
        Ok(limit)
    }
}

/// Who this machine's own `gh` is logged in to github.com as, read without the credential the hub set, as
/// [`FoundGithubLogin`](model::FoundGithubLogin) says, so a client can say who agents here push as while the hub has
/// no default. It changes nothing, so sending it again only asks again. A worker older than this refuses it as
/// unsupported.
#[derive(Clone, Copy, Debug)]
pub struct FindGithubLogin;

impl Request for FindGithubLogin {
    type Reply = GithubLoginReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::FindGithubLogin
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubLoginReply {
    /// `None` when `gh` here is logged in to nothing, or there's no `gh`.
    #[serde(default)]
    pub login: Option<GithubLogin>,
}

impl Reply for GithubLoginReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::GithubLogin(found) = response else { return Err(response) };
        Ok(found)
    }
}
