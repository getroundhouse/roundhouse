use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{GhLogin, GhLoginWorks, ProviderName};
use protocol::worker::{FindGithubLogin, FoundToken, GH_RUNS_WITHIN, GithubLoginReply};
use tracing::{info, warn};

use super::super::Hub;
use super::asked::Asking;
use crate::worker_link::WorkerLink;

/// How long the hub waits for a worker to say who its machine's own `gh` is logged in as: as long as a worker lets
/// `gh` run for it, every run together, and as long again for a worker that's busy. Asking GitHub whether its token
/// works comes after, within the time the hub gives any call to GitHub. One that hasn't said by then is no longer waited on, so its
/// workspace isn't kept at work by it, and its provider is asked again by the next link to it. Not something a test
/// from outside would sit through, so not one of [`crate::Timers`].
pub(in crate::hub) const FOUND_WITHIN: Duration = GH_RUNS_WITHIN.saturating_add(GH_RUNS_WITHIN);

impl Hub {
    /// Asks `link`'s worker, of `provider`, who its machine's own `gh` is logged in to github.com as, and records what
    /// it says as the provider's `FoundGithubLogin`, announced as records are, unless a worker of `provider` has said
    /// since the hub started, or is being asked, as [`FindGithubLogin`] says. The ask is queued before this returns, so
    /// as a link opens it reaches the worker after the git credential and before anything the link is then used for,
    /// and its answer is waited for on a task of its own, so neither the link's opening nor what it's used for waits
    /// on `gh`. A worker that refuses it, as one from before it does, can't tell, finds a login of a kind this hub
    /// doesn't know, or doesn't answer within [`FOUND_WITHIN`], leaves the provider to be asked by the next link to it.
    pub(crate) async fn find_github_login(self: &Arc<Self>, provider: ProviderName, link: &Arc<WorkerLink>) {
        let Some(asking) = self.github_logins_asked.ask(provider) else { return };
        // The link has closed already, and `asking` goes, so the next link is asked.
        let Ok(answer) = link.send(FindGithubLogin).await else { return };
        tokio::spawn(Arc::clone(self).found_github_login(asking, Arc::clone(link), answer));
    }

    async fn found_github_login(
        self: Arc<Self>,
        asking: Asking,
        link: Arc<WorkerLink>,
        answer: impl Future<Output = Result<GithubLoginReply, ApiError>>,
    ) {
        let provider = asking.provider().clone();
        let login = match tokio::time::timeout(FOUND_WITHIN, answer).await {
            Ok(Ok(GithubLoginReply { login: GhLogin::Unknown, .. })) => {
                self.found_tokens.found(provider.clone(), None);
                return info!(%provider, "{link} found a GitHub login of a kind this hub doesn't know, so it isn't kept");
            }
            Ok(Ok(GithubLoginReply { login, token })) => {
                // Kept before GitHub is asked whether it works, so a read waiting on it isn't held up, and then let go
                // of if GitHub refuses it, for a read to ask for it again.
                self.found_tokens
                    .found(provider.clone(), token.clone().filter(|_| matches!(login, GhLogin::LoggedIn { .. })));
                let checked = self.checked(login, token.as_ref()).await;
                if matches!(checked, GhLogin::LoggedIn { works: GhLoginWorks::Refused, .. }) {
                    self.found_tokens.found(provider.clone(), None);
                }
                checked
            }
            Ok(Err(e)) if e.code == ErrorCode::Unsupported => {
                self.found_tokens.found(provider.clone(), None);
                return info!(%provider, "{link} can't say who its machine's own gh is logged in as: {}", e.message);
            }
            Ok(Err(e)) => return warn!(%provider, "{link} couldn't say who its machine's own gh is: {}", e.message),
            Err(_) => return warn!(%provider, "{link} didn't say who its machine's own gh is within {FOUND_WITHIN:?}"),
        };
        let recorded = self.blocking(move |store| store.record_found_github_login(&provider, &login)).await;
        match recorded {
            Ok(found) => {
                asking.answered();
                self.notify(HubNotice::FoundGithubLogin(found));
            }
            Err(e) => warn!(provider = %asking.provider(), "recording the GitHub login {link} found: {}", e.message),
        }
    }

    /// `login`, saying whether GitHub takes its token, as the hub asks GitHub with `token`, the one its worker handed
    /// it. Without one, as from a worker from before it handed one, what the worker said stands.
    async fn checked(&self, login: GhLogin, token: Option<&FoundToken>) -> GhLogin {
        match (login, token) {
            (GhLogin::LoggedIn { username, .. }, Some(token)) => {
                let works = self.github.found_login_works(token, &username).await;
                GhLogin::LoggedIn { username, works }
            }
            (login, _) => login,
        }
    }
}
