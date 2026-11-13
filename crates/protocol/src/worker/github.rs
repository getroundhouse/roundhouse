//! What a hub asks a worker of GitHub, through the `gh` that runs in its worktree: the pull request of the branch
//! checked out, and how much of GitHub's limit is left.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::{self, BranchName};

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

/// Reads explicit targets and their dependencies using this worker's credentials. A pinned repository avoids a
/// changed git remote redirecting an existing wait. `None` resolves the worktree's repository for a new request.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReadRemotePullRequests {
    pub path: String,
    pub repository: Option<model::GitHubRepository>,
    pub pull_requests: model::PullRequestSelection,
}

impl Request for ReadRemotePullRequests {
    type Reply = RemotePullRequestsReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::ReadRemotePullRequests(self)
    }
}

/// Accounting accompanies failed reads too: pages already fetched still spend GitHub's budget.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RemotePullRequestsReply {
    pub observation: Result<model::PullRequestsSnapshot, crate::client::ApiError>,
    pub rate_limit: Option<RateLimit>,
    pub cost: u32,
    /// Failed PR and its dependents; empty for a repository-wide failure.
    pub affected: Vec<model::PullRequestNumber>,
}

impl Reply for RemotePullRequestsReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::RemotePullRequests(snapshot) = response else { return Err(response) };
        Ok(snapshot)
    }
}
