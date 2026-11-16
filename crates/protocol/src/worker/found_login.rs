//! Who the worker's machine's own `gh` is logged in to github.com as, which a hub asks to record for the provider.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::GhLogin;

/// Who this machine's own `gh` is logged in to github.com as, for the hub to record as the provider's
/// [`FoundGithubLogin`](crate::model::FoundGithubLogin). It's read as what the worker starts under the machine's own
/// finds it, whatever git credential the hub has set: without the credential's files, and without its taking out
/// `GH_TOKEN` and the like, so a token in the worker's own environment, which `gh` uses over its own login, is who
/// it's logged in as. It's `gh`'s login alone, not git's other credential helpers or an SSH key.
///
/// A login is read from `gh auth status`, which names it from `gh`'s own config even when GitHub can't be reached, and
/// then whether it works from GitHub's answer to `gh api --include user`, by its HTTP status, as
/// [`GhLoginWorks`](crate::model::GhLoginWorks) says. Answered with [`GhLogin::NotInstalled`] when there's no `gh`,
/// and [`GhLogin::LoggedOut`] when it's logged in to nothing for github.com. Refused with
/// [`ErrorCode::Worker`](crate::client::ErrorCode::Worker) when the worker can't tell who: `gh` took longer than
/// [`GH_RUNS_WITHIN`](super::GH_RUNS_WITHIN), failed, as one older than 2.81 does, since 2.81 is the first to say in
/// JSON, printed what the worker can't read, or named nobody, as for a token in the environment whose owner GitHub
/// didn't say. Every run of `gh` for it shares [`GH_RUNS_WITHIN`](super::GH_RUNS_WITHIN), so a check that has no time
/// left can't tell. A worker from before this refuses it as unsupported. It changes nothing, so asking again only asks
/// again: `gh auth status` is asked not to renew a token `gh` renews by itself, where `gh` has that, and such a token
/// isn't checked, since `gh api` would renew it if it's due.
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
}

impl Reply for GithubLoginReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::GithubLogin(found) = response else { return Err(response) };
        Ok(found)
    }
}
