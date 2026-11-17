use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use protocol::client::{ApiError, HubNotice};
use protocol::model::{GithubReach, GithubRepositoryName, GithubToken};
use protocol::worker::{GitCredential, GithubCredential, GithubRemoteReply, WorkerRequest, WorkerResponse};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

use super::super::Checks;
use crate::hub::Hub;
use crate::worker_link::WorkerLink;

pub(super) const CLI: GithubRepositoryName = GithubRepositoryName::named("cli/cli");

/// What a worker answers an ask with: `None` for never.
pub(super) type Answer = Option<Result<WorkerResponse, ApiError>>;

/// A stand-in for a worker that answers every ask of where its repository pushes with `answer`, and counts how often
/// it's asked.
pub(super) fn worker_answering(answer: Answer) -> (Arc<WorkerLink>, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&asked);
    let link = WorkerLink::answering(move |request| {
        assert!(matches!(request, WorkerRequest::GithubRemote(_)), "{request:?}");
        counted.fetch_add(1, Ordering::SeqCst);
        answer.clone()
    });
    (link, asked)
}

/// A stand-in for a worker whose repository pushes to `repository`, as [`worker_answering`] says.
pub(super) fn worker_saying(repository: Option<GithubRepositoryName>) -> (Arc<WorkerLink>, Arc<AtomicUsize>) {
    worker_answering(Some(Ok(WorkerResponse::GithubRemote(GithubRemoteReply { repository }))))
}

/// The credential of `username`'s token `token`.
pub(super) fn account(username: &str, token: &str) -> GithubCredential {
    let (username, token) = (username.into(), GithubToken::new(token));
    GithubCredential::Account(GitCredential { username, token, identity: None })
}

/// The hub's checks, run as it runs them, but following `credential` rather than its logins.
pub(super) fn checking(hub: &Arc<Hub>, credential: watch::Receiver<GithubCredential>) -> JoinHandle<()> {
    tokio::spawn(Checks::new(Arc::clone(hub), credential).run())
}

/// The next reach the feed tells of, within `within`.
pub(super) async fn told_within(feed: &mut broadcast::Receiver<HubNotice>, within: Duration) -> GithubReach {
    let reach = async {
        loop {
            if let HubNotice::GithubReach(reach) = feed.recv().await.unwrap() {
                return reach;
            }
        }
    };
    tokio::time::timeout(within, reach).await.expect("the feed told of no reach")
}

/// The next reach the feed tells of.
pub(super) async fn told(feed: &mut broadcast::Receiver<HubNotice>) -> GithubReach {
    told_within(feed, Duration::from_secs(5)).await
}
