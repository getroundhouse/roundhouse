use std::sync::Arc;
use std::time::Duration;

use protocol::client::HubNotice;
use protocol::model::{GithubRepositoryName, GithubToken, Observation, Observed, Reach, RepoId, RepoUrl};
use protocol::worker::{GitCredential, RepoUrlReply, WorkerRequest, WorkerResponse};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::super::Checks;
use crate::github_logins::DefaultCredential;
use crate::hub::Hub;
use crate::worker_link::WorkerLink;

pub(super) const CLI: GithubRepositoryName = GithubRepositoryName::named("cli/cli");

/// Has a stand-in for a worker on the hub's machine read that the checkout of the hub's repository, `r`, fetches from
/// `url`, as the first worker here to link after `r` is added does, so its record has the address.
pub(super) async fn reads(hub: &Arc<Hub>, url: &str) {
    let url = RepoUrl::try_from(url.to_owned()).unwrap();
    let link = WorkerLink::answering(move |request| {
        assert!(matches!(request, WorkerRequest::ReadRepoUrl(_)), "{request:?}");
        Some(Ok(WorkerResponse::RepoUrl(RepoUrlReply { url: Some(url.clone()) })))
    });
    Arc::clone(hub).read_repo_urls("w".into(), link).await;
}

/// The credential of `username`'s token `token`.
pub(super) fn account(username: &str, token: &str) -> DefaultCredential {
    let credential = GitCredential { username: username.into(), token: GithubToken::new(token), identity: None };
    DefaultCredential::Account { account: "g".into(), credential, username: username.into(), installation: None }
}

/// The hub's checks, run as it runs them, but following `credential` rather than its logins.
pub(super) fn checking(hub: &Arc<Hub>, credential: watch::Receiver<DefaultCredential>) -> JoinHandle<()> {
    tokio::spawn(Checks::new(Arc::clone(hub), credential).run())
}

/// A repository's reach, as the feed told of it.
#[derive(Debug)]
pub(super) struct Told {
    pub(super) repo: RepoId,
    pub(super) observed: Observed<Reach>,
}

impl Told {
    /// What it says, whether current or as of a read.
    pub(super) fn reach(&self) -> Reach {
        let (Observed::Current { value } | Observed::AsOf { value, .. }) = &self.observed else {
            panic!("told of no reach: {self:?}")
        };
        value.clone()
    }
}

/// The next reach the feed tells of, within `within`.
pub(super) async fn told_within(feed: &mut crate::hub::Notices, within: Duration) -> Told {
    let reach = async {
        loop {
            let notice = std::sync::Arc::unwrap_or_clone(feed.recv().await.unwrap());
            if let HubNotice::Observation(Observation::RepositoryReach { repo, reach, .. }) = notice {
                return Told { repo, observed: reach };
            }
        }
    };
    tokio::time::timeout(within, reach).await.expect("the feed told of no reach")
}

/// The next reach the feed tells of.
pub(super) async fn told(feed: &mut crate::hub::Notices) -> Told {
    told_within(feed, Duration::from_secs(5)).await
}
