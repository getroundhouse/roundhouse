use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use github::{Leaving, RepositoryAsk, RepositoryFound};
use protocol::client::HubNotice;
use protocol::model::{AddressForm, GithubRepositoryId, GithubToken, Repo, RepoId, RepoLink, Unlinked};
use tokio::time::{Instant, MissedTickBehavior};
use tracing::warn;

use crate::hub::Hub;
use crate::hub::observations::GithubRepositories;
use crate::store::{RepoToLink, Store, now_ms};

/// How often the hub reads again what GitHub says of each linked repository, and looks again for the links it lacks,
/// though nothing it knows of has changed, since a repository is renamed, moved to another owner or given to an
/// account on GitHub, which says so to nobody. As often as the check of a push, at one query for every hundred
/// repositories. *Unconfirmed.*
const READ_AGAIN_EVERY: Duration = Duration::from_mins(30);

impl Hub {
    /// Looks for the links the hub's repositories lack, and reads what GitHub says of each linked one, until the hub
    /// stops: as it starts, as what a link may be found by changes, which is a worker reading an address and a GitHub
    /// account changing, as the default or its login changes, and every [`READ_AGAIN_EVERY`]. Each time, it tells the
    /// check of reach, which goes by the links.
    pub(crate) async fn watch_repo_links(self: Arc<Self>) {
        let mut credential = self.git_credential();
        let mut again = tokio::time::interval_at(Instant::now() + READ_AGAIN_EVERY, READ_AGAIN_EVERY);
        again.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            credential.borrow_and_update();
            self.link_repos().await;
            self.links_read.notify_one();
            tokio::select! {
                changed = credential.changed() => if changed.is_err() {
                    return;
                },
                () = self.links_wanted.notified() => {}
                _ = again.tick() => {}
            }
        }
    }

    /// Asks GitHub, as each connected account in turn, the default's first, which repository each repository's address
    /// on github.com names, where it isn't linked, and what's said of each linked one now, until one account has
    /// answered for each. Links each found, and keeps what GitHub says of each as its observation. One no account finds
    /// says why it isn't linked: no account to ask as, GitHub failing for one of them, or none of them seeing it. One
    /// that's linked and that no account sees now keeps its link, and what GitHub said of it last.
    pub(super) async fn link_repos(&self) {
        let read = |store: &Store| Ok((store.repos_to_link()?, store.github_links()?));
        let (mut finding, linked) = match self.blocking(read).await {
            Ok(read) => read,
            Err(e) => return warn!("finding which repositories to link: {}", e.message),
        };
        let logins = Arc::clone(&self.github_logins);
        let readers = match crate::hub::off_thread(move || logins.readers(now_ms())).await {
            Ok(readers) => readers,
            Err(e) => return warn!("finding the GitHub accounts to link repositories as: {}", e.message),
        };
        let mut reading: Vec<GithubRepositoryId> = linked.into_iter().collect();
        let mut couldnt_tell: HashMap<RepoId, String> = HashMap::new();
        for (_, token) in &readers {
            if finding.is_empty() && reading.is_empty() {
                break;
            }
            (finding, reading) = self.read_as(token, finding, reading, &mut couldnt_tell).await;
        }
        for repo in finding {
            let why = match couldnt_tell.remove(&repo.id) {
                _ if readers.is_empty() => Unlinked::NoAccount,
                Some(why) => Unlinked::CouldntTell { why },
                None => Unlinked::NotFound,
            };
            let RepoToLink { id, url, .. } = repo;
            self.announce_repo(self.blocking(move |store| store.repo_unlinked(&id, &url, &why)).await);
        }
    }

    /// Reads `finding` by their names and `reading` by their IDs as `token`, links and keeps what it finds, and returns
    /// what it didn't find, for the next account to read, keeping why GitHub failed for one in `couldnt_tell`.
    async fn read_as(
        &self,
        token: &GithubToken,
        finding: Vec<RepoToLink>,
        reading: Vec<GithubRepositoryId>,
        couldnt_tell: &mut HashMap<RepoId, String>,
    ) -> (Vec<RepoToLink>, Vec<GithubRepositoryId>) {
        let named = finding.iter().map(|repo| RepositoryAsk::Named(repo.repository.clone()));
        let asks: Vec<_> = named.chain(reading.iter().cloned().map(RepositoryAsk::Id)).collect();
        let read = self.github.read_repositories(token, &asks, Leaving::ForOthers).await;
        let read_at_ms = now_ms();
        let mut answers = read.answers.into_iter();
        let mut still_finding = Vec::new();
        for (repo, answer) in finding.into_iter().zip(answers.by_ref()) {
            match answer {
                Ok(Some(found)) => self.link(repo, found, read_at_ms).await,
                Ok(None) => still_finding.push(repo),
                Err(e) => {
                    couldnt_tell.insert(repo.id.clone(), e.message);
                    still_finding.push(repo);
                }
            }
        }
        let mut still_reading = Vec::new();
        for (id, answer) in reading.into_iter().zip(answers) {
            match answer {
                Ok(Some(found)) => self.keep_said(found, read_at_ms).await,
                Ok(None) | Err(_) => still_reading.push(id),
            }
        }
        (still_finding, still_reading)
    }

    /// Links `repo` to `found`, by the form of address it was found by, and keeps what GitHub says of it.
    async fn link(&self, repo: RepoToLink, found: RepositoryFound, read_at_ms: i64) {
        let link = RepoLink::Github { id: found.id.clone(), by: AddressForm::of(&repo.url) };
        // First, so a client that's told of the link has what it's called.
        self.keep_said(found, read_at_ms).await;
        let RepoToLink { id, url, .. } = repo;
        self.announce_repo(self.blocking(move |store| store.link_repo(&id, &url, &link)).await);
    }

    /// Keeps what GitHub says of `found`, read at `read_at_ms`, as its observation.
    pub(super) async fn keep_said(&self, found: RepositoryFound, read_at_ms: i64) {
        let RepositoryFound { id, said } = found;
        if let Err(e) = self.observation_read::<GithubRepositories>(id, said, read_at_ms).await {
            warn!("keeping what GitHub says of a repository: {}", e.message);
        }
    }

    /// Tells every client of a repository's record that `changed`, if it did.
    fn announce_repo(&self, changed: Result<Option<Repo>, protocol::client::ApiError>) {
        match changed {
            Ok(Some(repo)) => self.notify(HubNotice::Repo(repo)),
            Ok(None) => {}
            Err(e) => warn!("keeping a repository's link: {}", e.message),
        }
    }
}
