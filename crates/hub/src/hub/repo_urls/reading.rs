use std::sync::Arc;
use std::time::Duration;

use protocol::client::HubNotice;
use protocol::model::{RepoUrl, WorkspaceId};
use protocol::provider::WorkerFilesystem;
use protocol::worker::{ReadRepoUrl, RepoUrlReply};
use tracing::{info, warn};

use super::super::Hub;
use crate::store::{RepoToRead, Store};
use crate::worker_link::WorkerLink;

/// How long the hub waits for a worker to read a checkout's address, which takes git a moment, so only a worker that's
/// stuck takes this long. One that hasn't said by then leaves the address to be read by the next worker to link. Not
/// something a test from outside would sit through, so not one of [`crate::Timers`].
pub(super) const READ_WITHIN: Duration = Duration::from_secs(10);

impl Hub {
    /// Whether `workspace`'s worker runs on the hub's own filesystem, read from its recorded placement without
    /// contacting its provider's adapter. A workspace whose placement can't be read (e.g. removed meanwhile) is treated
    /// as not here so background URL reads never fail a link open.
    async fn workspace_shares_hub_filesystem(&self, workspace: &WorkspaceId) -> bool {
        self.workspace_placement(workspace)
            .await
            .is_ok_and(|placement| placement.environment.filesystem == WorkerFilesystem::Hub)
    }

    /// As a link to `link`'s worker, of `workspace`, opens: if the worker runs on the hub's machine, has it read the
    /// address of each repository whose address is to be read, and keeps what it says, as [`Hub::read_repo_urls_by`]
    /// says.
    pub(crate) async fn read_repo_urls(self: Arc<Self>, workspace: WorkspaceId, link: Arc<WorkerLink>) {
        if self.workspace_shares_hub_filesystem(&workspace).await {
            self.read_repo_urls_by(&link).await;
        }
    }

    /// As a repository is added: has a worker on the hub's machine read its address, and any other to be read, if one
    /// is running, on a task of its own, so the reply waits for none. None is started for it, nor kept running for it,
    /// so with none running, the next worker here to link reads them.
    pub(in super::super) async fn read_repo_urls_here(self: &Arc<Self>) {
        if let Some(link) = self.worker_here().await {
            let hub = Arc::clone(self);
            tokio::spawn(async move { hub.read_repo_urls_by(&link).await });
        }
    }

    /// An open link to a worker on the hub's machine, of any workspace, if one is running.
    pub(super) async fn worker_here(&self) -> Option<Arc<WorkerLink>> {
        let open: Vec<_> = self
            .workers
            .lock()
            .iter()
            .filter(|(_, link)| link.is_open())
            .map(|(id, link)| (id.clone(), Arc::clone(link)))
            .collect();
        for (id, link) in open {
            if self.workspace_shares_hub_filesystem(&id).await {
                return Some(link);
            }
        }
        None
    }

    /// Has `link`'s worker, which runs on the hub's machine, read the address of each repository whose address is to
    /// be read, one at a time, and keeps each it says, announcing each record that changes. One the worker can't say,
    /// or doesn't within [`READ_WITHIN`], is left to be read, so the record stays its checkout until a worker here can
    /// say.
    async fn read_repo_urls_by(&self, link: &WorkerLink) {
        let to_read = match self.blocking(Store::repos_to_read).await {
            Ok(to_read) => to_read,
            Err(e) => {
                return warn!(
                    name: "repo.urls_unfound",
                    error = e.message,
                    "Couldn't find which repositories' addresses to read"
                );
            }
        };
        for repo in to_read {
            self.read_repo_url(link, repo).await;
        }
    }

    async fn read_repo_url(&self, link: &WorkerLink, repo: RepoToRead) {
        let RepoToRead { id, name, checkout } = repo;
        let url = match tokio::time::timeout(READ_WITHIN, link.call(ReadRepoUrl { path: checkout.clone() })).await {
            Ok(Ok(RepoUrlReply { url })) => url,
            Ok(Err(e)) => {
                warn!(
                    name: "repo.url_unread",
                    repo = name,
                    link = %link,
                    checkout = %checkout,
                    error = e.message,
                    "A worker couldn't read where a checkout fetches from"
                );
                return;
            }
            Err(_) => {
                warn!(
                    name: "repo.url_unsaid",
                    repo = name,
                    link = %link,
                    checkout = %checkout,
                    within = ?READ_WITHIN,
                    "A worker didn't say in time where a checkout fetches from"
                );
                return;
            }
        };
        let said = url.as_ref().map_or_else(|| "no address".to_owned(), RepoUrl::to_string);
        match self.blocking(move |store| store.read_repo_url(&id, &checkout, url.as_ref())).await {
            Ok(changed) => {
                if let Some(repo) = changed {
                    info!(
                        name: "repo.url_read",
                        repo = name,
                        link = %link,
                        said,
                        "A worker read where a checkout fetches from"
                    );
                    self.notify(HubNotice::Repo(repo));
                }
                self.addresses_read.notify_one();
            }
            Err(e) => {
                warn!(
                    name: "repo.url_unkept",
                    repo = name,
                    link = %link,
                    error = e.message,
                    "Couldn't keep the address a worker read"
                );
            }
        }
    }
}
