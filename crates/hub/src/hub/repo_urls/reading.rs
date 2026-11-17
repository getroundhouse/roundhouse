use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ErrorCode, HubNotice};
use protocol::model::{ProviderName, RepoUrl};
use protocol::worker::{ReadRepoUrl, RepoUrlReply};
use tracing::{info, warn};

use super::super::Hub;
use crate::store::{RepoToRead, Store};
use crate::worker_link::WorkerLink;

/// How long the hub waits for a worker to read a checkout's address, which takes git a moment, so only a worker that's
/// stuck takes this long. One that hasn't said by then leaves the address to be read by the next worker to link. Not
/// something a test from outside would sit through, so not one of [`crate::Timers`].
pub(super) const READ_WITHIN: Duration = Duration::from_secs(10);

/// Whether `provider`'s workers run on the hub's machine, where the checkouts of its repositories are. Only the local
/// provider's do, as [`ProviderName::LOCAL`] says. *Unconfirmed:* going by the provider's name rather than by something
/// it declares, since nothing else says yet.
fn on_this_machine(provider: &ProviderName) -> bool {
    provider.is_local()
}

/// The worker can't read any repository's address, as one from before [`ReadRepoUrl`] can't.
struct Unsupported;

impl Hub {
    /// As a link to `link`'s worker, of `provider`, opens: if the worker runs on the hub's machine, has it read the
    /// address of each repository whose address is to be read, and keeps what it says, as [`Hub::read_repo_urls_by`]
    /// says.
    pub(crate) async fn read_repo_urls(self: Arc<Self>, provider: ProviderName, link: Arc<WorkerLink>) {
        if on_this_machine(&provider) {
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
        if open.is_empty() {
            return None;
        }
        let workspaces = match self.blocking(|store| store.list_workspaces(false)).await {
            Ok(workspaces) => workspaces,
            Err(e) => {
                warn!("finding which workers run on this machine, to read addresses: {}", e.message);
                return None;
            }
        };
        let here =
            |id: &_| workspaces.iter().any(|workspace| &workspace.id == id && on_this_machine(&workspace.provider));
        open.into_iter().find(|(id, _)| here(id)).map(|(_, link)| link)
    }

    /// Has `link`'s worker, which runs on the hub's machine, read the address of each repository whose address is to
    /// be read, one at a time, and keeps each it says, with where its pushes go, which the check of whether the
    /// default can push reads, announcing each record that changes. One the worker can't
    /// say, as one from before [`ReadRepoUrl`] can't, or doesn't within [`READ_WITHIN`], is left to be read, so the
    /// record stays its checkout until a worker here can say.
    async fn read_repo_urls_by(&self, link: &WorkerLink) {
        let to_read = match self.blocking(Store::repos_to_read).await {
            Ok(to_read) => to_read,
            Err(e) => return warn!("finding which repositories' addresses to read: {}", e.message),
        };
        for repo in to_read {
            if self.read_repo_url(link, repo).await.is_err() {
                return;
            }
        }
    }

    async fn read_repo_url(&self, link: &WorkerLink, repo: RepoToRead) -> Result<(), Unsupported> {
        let RepoToRead { id, name, checkout } = repo;
        let (url, push) =
            match tokio::time::timeout(READ_WITHIN, link.call(ReadRepoUrl { path: checkout.clone() })).await {
                Ok(Ok(RepoUrlReply { url, push })) => (url, push),
                Ok(Err(e)) if e.code == ErrorCode::Unsupported => {
                    info!(repo = name, "{link} can't read where a checkout fetches from: {}", e.message);
                    return Err(Unsupported);
                }
                Ok(Err(e)) => {
                    warn!(repo = name, "{link} couldn't read where {checkout} fetches from: {}", e.message);
                    return Ok(());
                }
                Err(_) => {
                    warn!(repo = name, "{link} didn't say where {checkout} fetches from within {READ_WITHIN:?}");
                    return Ok(());
                }
            };
        let said = url.as_ref().map_or_else(|| "no address".to_owned(), RepoUrl::to_string);
        match self.blocking(move |store| store.read_repo_url(&id, &checkout, url.as_ref(), push)).await {
            Ok(changed) => {
                if let Some(repo) = changed {
                    info!(repo = name, "{link} read that its checkout fetches from {said}");
                    self.notify(HubNotice::Repo(repo));
                }
                self.addresses_read.notify_one();
            }
            Err(e) => warn!(repo = name, "keeping the address {link} read: {}", e.message),
        }
        Ok(())
    }
}
