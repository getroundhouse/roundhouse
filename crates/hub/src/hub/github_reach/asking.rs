use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::WorkspaceId;
use protocol::worker::{GithubRemote, GithubRemoteReply};
use tracing::{info, warn};

use super::super::Hub;
use super::remotes::Asking;
use crate::worker_link::WorkerLink;

/// How long the hub waits for a worker to say which repository on GitHub its repository pushes to. It's a read of
/// git's config, which takes a moment, so this is for a worker busy with other calls. One that hasn't said by then is
/// no longer waited on, and its repository is asked again by the next link to a worker of it. Not something a test
/// from outside would sit through, so not one of [`crate::Timers`].
pub(super) const SAID_WITHIN: Duration = Duration::from_secs(30);

impl Hub {
    /// Asks `link`'s worker, of the workspace `workspace_id`, which repository on GitHub the workspace's repository
    /// pushes to, unless a worker of the repository has said since the hub started, or is being asked, as
    /// [`GithubRemote`] says, and tells the check of reach what it says, as [`Hub::watch_github_reach`] says. The ask
    /// is queued before this returns, so as a link opens it reaches the worker after the ask of who its machine's own
    /// `gh` is logged in as, and before anything the link is then used for, and its answer is waited for on a task of
    /// its own, so neither the link's opening nor what it's used for waits on it. A worker that refuses it, as one from
    /// before it does, fails, or doesn't answer within [`SAID_WITHIN`], leaves the repository to be asked by the next
    /// link to a worker of it.
    ///
    /// A workaround, until a repository's record has its address: the hub runs no git, and the record has only the
    /// repository's checkout, so which repository on GitHub that pushes to is what git says of a checkout, which is a
    /// worker's to say. Once the record has the address, the hub reads it there, and this and [`GithubRemote`] go, as
    /// #1615's last item says.
    pub(crate) async fn find_github_remote(self: &Arc<Self>, workspace_id: WorkspaceId, link: &Arc<WorkerLink>) {
        let repo = self.blocking(move |store| store.get_repo(&store.get_workspace(&workspace_id)?.repo_id)).await;
        let repo = match repo {
            Ok(repo) => repo,
            Err(e) => return warn!("couldn't find the repository {link} is of, to ask where it pushes: {}", e.message),
        };
        let Some(asking) = self.github_remotes.ask(repo.id.clone()) else { return };
        // The link has closed already, and `asking` goes, so the next link is asked.
        let Ok(answer) = link.send(GithubRemote { repo_path: repo.path }).await else { return };
        tokio::spawn(github_remote_said(asking, Arc::clone(link), answer));
    }
}

async fn github_remote_said(
    asking: Asking,
    link: Arc<WorkerLink>,
    answer: impl Future<Output = Result<GithubRemoteReply, ApiError>>,
) {
    let said = match tokio::time::timeout(SAID_WITHIN, answer).await {
        Ok(Ok(said)) => said,
        Ok(Err(e)) if e.code == ErrorCode::Unsupported => {
            return info!(repo = %asking.repo(), "{link} can't say where its repository pushes: {}", e.message);
        }
        Ok(Err(e)) => return warn!(repo = %asking.repo(), "{link} couldn't say where it pushes: {}", e.message),
        Err(_) => return warn!(repo = %asking.repo(), "{link} didn't say where it pushes within {SAID_WITHIN:?}"),
    };
    asking.answered(said.repository);
}
