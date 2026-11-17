//! The address a checkout on the worker's machine fetches from, which a hub asks a worker on its own machine for, to
//! keep on the repository's record.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::RepoUrl;

/// The address the checkout at `path` fetches from: its `origin`'s first URL, as `git remote get-url origin` gives it
/// under the machine's own config, so the person's `insteadOf` rewrites it as a fetch meets it, though not the git
/// credential's rewriting of GitHub's SSH addresses, which is the worker's, nor where a push goes, which a `pushurl`
/// can say is elsewhere. Without the credential it can hold, as [`RepoUrl::without_credential`] cuts it out, so the
/// hub is never sent one. Answered with none when the checkout has no `origin`, and refused with
/// [`ErrorCode::Worker`](crate::client::ErrorCode::Worker) when `path` isn't a git repository or git fails. It changes
/// nothing, so asking again only asks again. A worker from before this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReadRepoUrl {
    pub path: String,
}

impl Request for ReadRepoUrl {
    type Reply = RepoUrlReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::ReadRepoUrl(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RepoUrlReply {
    pub url: Option<RepoUrl>,
}

impl Reply for RepoUrlReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::RepoUrl(read) = response else { return Err(response) };
        Ok(read)
    }
}
