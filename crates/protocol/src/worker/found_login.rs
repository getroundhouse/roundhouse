//! Who the worker's machine's own `gh` is logged in to github.com as, which a hub asks to record for the provider.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::{GhLogin, GithubToken};

/// How long a worker lets each run of `gh` take before it stops it, and what asked fails. So how long finding a login
/// can take follows from how many runs it makes.
pub const GH_RUNS_WITHIN: Duration = Duration::from_secs(15);

/// Who this machine's own `gh` is logged in to github.com as, for the hub to record as the provider's
/// [`FoundGithubLogin`](crate::model::FoundGithubLogin), and the token it uses, for the hub to ask GitHub with. It's
/// read as what the worker starts under the machine's own finds it, whatever git credential the hub has set: without
/// the credential's files, and without its taking out `GH_TOKEN` and the like, so a token in the worker's own
/// environment, which `gh` uses over its own login, is who it's logged in as. It's `gh`'s login alone, not git's other
/// credential helpers or an SSH key.
///
/// A login is read from `gh auth status`, which names it from `gh`'s own config even when GitHub can't be reached, and
/// its token from `gh auth token`. The worker doesn't ask GitHub whether the token works: the hub does, with the token,
/// as [`GhLoginWorks`](crate::model::GhLoginWorks) says, so a worker says it couldn't tell. Answered with
/// [`GhLogin::NotInstalled`] when there's no `gh`, and [`GhLogin::LoggedOut`] when it's logged in to nothing for
/// github.com. Refused with [`ErrorCode::Worker`](crate::client::ErrorCode::Worker) when the worker can't tell who: `gh`
/// took longer than [`GH_RUNS_WITHIN`], failed, as one older than 2.81 does, since 2.81 is the
/// first to say in JSON, printed what the worker can't read, or named nobody, as for a token in the environment whose
/// owner GitHub didn't say. Every run of `gh` for it shares [`GH_RUNS_WITHIN`], so a check that
/// has no time left can't tell. A worker from before this refuses it as unsupported. It changes nothing, so asking
/// again only asks again: `gh` is asked not to renew a token it renews by itself, where `gh` has that, as asking for
/// its status or its token would if it's due.
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
