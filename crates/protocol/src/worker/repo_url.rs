//! The address a checkout on the worker's machine fetches from, which a hub asks a worker on its own machine for, to
//! keep on the repository's record, and whether a push from it goes through the git credential the worker hands its
//! processes.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::RepoUrl;

/// The address the checkout at `path` fetches from: its `origin`'s first URL, as `git remote get-url origin` gives it
/// under the machine's own config, so the person's `insteadOf` rewrites it as a fetch meets it, though not the git
/// credential's rewriting of GitHub's SSH addresses, which is the worker's, nor where a push goes, which a `pushurl`
/// can say is elsewhere. Without the credential it can hold, as [`RepoUrl::without_credential`] cuts it out, so the
/// hub is never sent one. And where its pushes go, as [`PushRoute`] says. Answered with none when the checkout has no
/// `origin`, and refused with [`ErrorCode::Worker`](crate::client::ErrorCode::Worker) when `path` isn't a git
/// repository or git fails. It changes nothing, so asking again only asks again. A worker from before this refuses it
/// as unsupported.
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
    /// Where a push from the checkout goes. None for a checkout with no `origin`, and from a worker from before it said.
    #[serde(default)]
    pub push: Option<PushRoute>,
}

impl Reply for RepoUrlReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::RepoUrl(read) = response else { return Err(response) };
        Ok(read)
    }
}

/// Whether a push from a checkout goes through the git credential the worker hands every process, which is the
/// default GitHub account's while there's one, or round it, as the machine's own login. Of every address `origin`
/// pushes to, as `git remote get-url --push --all origin` gives them with the git config the worker gives every
/// process while there's a credential: the person's, with its `insteadOf` and `pushInsteadOf`, and the worker's
/// rewriting of GitHub's SSH addresses, of which git takes one rewrite of each address, the longest that fits. Never an
/// address, which can hold a token.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PushRoute {
    /// Every one is `https://github.com/`, where the worker's git config sends git to its credential helper.
    Credential,
    /// One goes round it: over SSH, as a person's `url."git@github.com:".insteadOf "https://github.com/"` has an
    /// `https://` address go, or as `ssh://github.com/…` does, which the worker's rewriting doesn't take, with the
    /// machine's key, or to another host, or with a password of its own in it, which git pushes with instead.
    Around,
    /// From a newer worker.
    #[serde(other)]
    Unknown,
}
