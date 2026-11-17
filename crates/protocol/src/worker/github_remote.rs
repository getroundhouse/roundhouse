//! Which repository on github.com a repository pushes to, which a hub asks to check whether its default GitHub account
//! can push there.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::GithubRepositoryName;

/// Which repository on github.com the repository at `repo_path` pushes to: where its `origin` remote pushes, as `git
/// remote get-url --push origin` gives it, with what the worker gives every process, so the git credential's own
/// rewriting of GitHub's SSH addresses, and the person's `insteadOf` and `pushInsteadOf`, are as a push meets them. An
/// address over HTTPS, SSH or git's own protocol to github.com, with or without `.git`, names one, and only its owner
/// and name are sent, never the address, which can hold a token. Answered with none when there's no `origin`, or it
/// pushes anywhere else, such as another host or a path. A worker from before this refuses it as unsupported. It
/// changes nothing. Only until a repository's record has its address, which the hub then reads instead, as
/// `Hub::find_github_remote` says.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GithubRemote {
    pub repo_path: String,
}

impl Request for GithubRemote {
    type Reply = GithubRemoteReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::GithubRemote(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubRemoteReply {
    /// `None` when it isn't on github.com.
    pub repository: Option<GithubRepositoryName>,
}

impl Reply for GithubRemoteReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::GithubRemote(found) = response else { return Err(response) };
        Ok(found)
    }
}
