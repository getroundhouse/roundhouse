//! Who the worker's machine's own `gh` is logged in to github.com as, which a hub from before Settings' Providers asked
//! for, to record for the provider. Kept so what such a hub sends, and what a worker from then answers, can still be
//! read.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::{GhLogin, GithubToken};

/// Who this machine's own `gh` is logged in to github.com as, for the hub to record as the provider's
/// [`FoundGithubLogin`](crate::model::FoundGithubLogin), and the token it uses, which a hub from before Settings'
/// Providers read pull requests with while it had no GitHub account. A hub from since never asks, since it reads GitHub
/// only as its default account, as `docs/decisions/github-on-provisioned-machines.md` says, and a worker from since
/// refuses it as unsupported, as a worker from before it was asked did, so an older hub reads nothing.
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
    pub login: GhLogin,
    /// The token `gh` uses for `login`, for the hub alone, which never stores it or hands it to a client. `None` for a
    /// login that's logged out or not installed, when `gh` didn't print one, and from a worker from before the hub
    /// asked GitHub itself, which said whether it works in `login`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<FoundToken>,
}

/// The token a machine's own `gh` uses for github.com, as `gh auth token` prints it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FoundToken {
    pub token: GithubToken,
    /// Whether it's one `gh` renews by itself, which `gh` 2.103 is the first to have. One whose time is up is renewed
    /// only when `gh` next runs, so until then GitHub refusing it doesn't mean `gh` needs logging in again.
    #[serde(default)]
    pub renews: bool,
}

impl Reply for GithubLoginReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::GithubLogin(found) = response else { return Err(response) };
        Ok(found)
    }
}
