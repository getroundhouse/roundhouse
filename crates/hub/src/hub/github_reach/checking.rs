use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use protocol::client::HubNotice;
#[cfg(doc)]
use protocol::model::RepoUrl;
use protocol::model::{GithubReach, GithubRepositoryName, GithubUsername, Reach, RepoId};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio::time::{Instant, MissedTickBehavior};
use tracing::{error, warn};

use super::super::Hub;
use crate::github_logins::DefaultCredential;
use crate::store::{Store, StoreError, now_ms};

/// How often every repository is checked again, though nothing the hub knows of has changed, since what an account can
/// do changes on GitHub, as when an organization approves Roundhouse, or someone is given write access, and GitHub says
/// so to nobody. So a workspace says the new answer within half an hour of it, at the cost of one call to GitHub a
/// repository each time. By tokio's clock, which doesn't count a Mac's sleep, so a check due while it sleeps comes as
/// late again, which only holds back an answer. *Unconfirmed.*
pub(super) const CHECKED_AGAIN_EVERY: Duration = Duration::from_mins(30);

impl Hub {
    /// Checks whether the default GitHub account can push to each repository's repository on GitHub, and records each
    /// answer as the repository's `GithubReach`, announced as records are, until the hub stops: as it starts, as a
    /// worker reads a repository's address, as `Hub::read_repo_urls` says, for one whose address then names another
    /// repository on GitHub, as what workers give their processes for github.com changes, which it does as the
    /// default, its login or its commit identity changes, and every [`CHECKED_AGAIN_EVERY`]. Once a repository and login, not a workspace, and each repository's checks side by
    /// side, each within GitHub's time limit, never under the logins' lock.
    ///
    /// Which repository on GitHub a repository is, is the one its record's address names, as [`RepoUrl::on_github`]
    /// says, so one with no address, or an address elsewhere, isn't on GitHub. That's where its checkout fetches from,
    /// and a `pushurl` of the checkout's can send its pushes elsewhere, which this doesn't see. *Unconfirmed.* What it
    /// finds is what a push meets on a provider where Roundhouse sets up GitHub. A workspace on one where it leaves
    /// GitHub to the machine says nothing of it, as `GithubSettings::left_to_machine` says.
    pub(crate) async fn watch_github_reach(self: Arc<Self>) {
        let credential = self.git_credential();
        Checks::new(self, credential).run().await;
    }
}

/// The checks of reach, as one task runs them, which alone records them, so no answer can be recorded after a newer
/// one.
pub(super) struct Checks {
    hub: Arc<Hub>,
    /// What workers give their processes for github.com, which a push meets, and who GitHub knows that as, as the
    /// logins work it out.
    credential: watch::Receiver<DefaultCredential>,
    /// Which repository on GitHub each repository was when it was last checked, by its record's address.
    pushes_to: HashMap<RepoId, Option<GithubRepositoryName>>,
    under_way: JoinSet<Checked>,
    /// The number of each repository's newest check, whose answer alone is recorded.
    newest: HashMap<RepoId, u64>,
    started: u64,
}

/// What a check found.
struct Checked {
    repo: RepoId,
    number: u64,
    reach: Reach,
    /// When GitHub answered, if `reach` is its answer.
    answered_at_ms: Option<i64>,
    /// Who GitHub knows the account it asked with as, if any.
    asked_as: Option<GithubUsername>,
}

impl Checks {
    pub(super) fn new(hub: Arc<Hub>, credential: watch::Receiver<DefaultCredential>) -> Self {
        Self {
            hub,
            credential,
            pushes_to: HashMap::new(),
            under_way: JoinSet::new(),
            newest: HashMap::new(),
            started: 0,
        }
    }

    /// Until what workers are given can't change, as once the logins are gone, with the hub.
    pub(super) async fn run(mut self) {
        self.pushes_to = self.recorded().await.unwrap_or_default();
        let mut again = tokio::time::interval_at(Instant::now() + CHECKED_AGAIN_EVERY, CHECKED_AGAIN_EVERY);
        again.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            let credential = self.credential.borrow_and_update().clone();
            // Of another login, so none of their answers is kept.
            self.under_way.abort_all();
            self.check(&credential, self.pushes_to.keys().cloned().collect());
            loop {
                tokio::select! {
                    changed = self.credential.changed() => match changed {
                        Ok(()) => break,
                        Err(_) => return,
                    },
                    () = self.hub.addresses_read.notified() => {
                        let read = self.newly_read().await;
                        self.check(&credential, read);
                    }
                    _ = again.tick() => self.check(&credential, self.pushes_to.keys().cloned().collect()),
                    Some(checked) = self.under_way.join_next() => match checked {
                        Ok(checked) => self.keep(checked).await,
                        Err(e) if e.is_cancelled() => {}
                        Err(e) => error!("checking whether the default can push to a repository failed: {e}"),
                    },
                }
            }
        }
    }

    /// Which repository on GitHub each repository is, as its record's address names it, so the hub checks them as it
    /// starts and as a worker reads an address. One with no address whose address a worker is still to read, as each
    /// is once the hub that added it is upgraded to keep addresses, is checked for the one its last check was of, if
    /// any, so it isn't left unchecked until a worker on the hub's machine links. *Unconfirmed.* `None` if the store
    /// can't say.
    async fn recorded(&self) -> Option<HashMap<RepoId, Option<GithubRepositoryName>>> {
        let read = |store: &Store| {
            let (repos, to_read) = (store.list_repos()?, store.repos_to_read()?);
            Ok((repos, to_read, store.github_reaches()?))
        };
        let (repos, to_read, reaches) = match self.hub.blocking(read).await {
            Ok(recorded) => recorded,
            Err(e) => {
                warn!("couldn't read which repositories to check the default can push to: {}", e.message);
                return None;
            }
        };
        let to_read: HashSet<_> = to_read.into_iter().map(|repo| repo.id).collect();
        let mut checked: HashMap<_, _> =
            reaches.into_iter().filter_map(|record| Some((record.repo, record.reach.repository()?.clone()))).collect();
        let named = repos.into_iter().filter_map(|repo| {
            let repository = match repo.origin.url() {
                Some(url) => url.on_github(),
                None if to_read.contains(&repo.id) => Some(checked.remove(&repo.id)?),
                None => None,
            };
            Some((repo.id, repository))
        });
        Some(named.collect())
    }

    /// The repositories whose records say their pushes go somewhere other than they were last checked for, as once a
    /// worker reads an address, which from then on they're checked for. One removed since isn't checked again.
    async fn newly_read(&mut self) -> Vec<RepoId> {
        let Some(recorded) = self.recorded().await else { return Vec::new() };
        let read = recorded.iter().filter(|&(repo, pushes_to)| self.pushes_to.get(repo) != Some(pushes_to));
        let read = read.map(|(repo, _)| repo.clone()).collect();
        self.pushes_to = recorded;
        read
    }

    /// Starts checking each of `repos` with `credential`, each on a task of its own, so a GitHub that doesn't answer
    /// for one holds up none of the others.
    fn check(&mut self, credential: &DefaultCredential, repos: Vec<RepoId>) {
        for repo in repos {
            let Some(pushes_to) = self.pushes_to.get(&repo).cloned() else { continue };
            self.started += 1;
            let number = self.started;
            self.newest.insert(repo.clone(), number);
            let (hub, credential) = (Arc::clone(&self.hub), credential.clone());
            let asked_as = match &credential {
                DefaultCredential::Account { username, .. } => Some(username.clone()),
                DefaultCredential::MachineOwn | DefaultCredential::Blocked { .. } => None,
            };
            self.under_way.spawn(async move {
                let reach = super::reaching::reach(&hub.github, pushes_to, &credential).await;
                let answered_at_ms = reach.answered_for().is_some().then(now_ms);
                Checked { repo, number, reach, answered_at_ms, asked_as }
            });
        }
    }

    /// Records what `checked` found, and tells every client, unless a newer check of its repository has started since,
    /// or what workers are given has changed, as when the default's login changes, since then it's of another login.
    async fn keep(&self, checked: Checked) {
        let changed = self.credential.has_changed().unwrap_or(true);
        if changed || self.newest.get(&checked.repo) != Some(&checked.number) {
            return;
        }
        match self.hub.blocking(move |store| recorded(store, &checked)).await {
            Ok(recorded) => self.hub.notify(HubNotice::GithubReach(recorded)),
            Err(e) => warn!("couldn't record whether the default can push to a repository: {}", e.message),
        }
    }
}

/// Records what `checked` found as its repository's reach, as of this run, and returns the record, whose revision
/// moves only when it changes, so every client drops a copy it has. Not when GitHub didn't answer and the record is
/// GitHub's answer for the same repository and account, which stays as it was, with when GitHub gave it, and stale
/// while it's the last run's, since what GitHub says is a cache of GitHub's, and an ask that failed says nothing of it,
/// as when GitHub can't be reached. *Unconfirmed.*
fn recorded(store: &Store, checked: &Checked) -> Result<GithubReach, StoreError> {
    let Checked { repo, reach, answered_at_ms, asked_as, .. } = checked;
    if let Reach::CouldntTell { repository, .. } = reach
        && let Some(asked_as) = asked_as
        && let Some(kept) = store.github_reach_of(repo)?
        && kept.reach.answered_for() == Some((repository, asked_as))
    {
        return Ok(kept);
    }
    store.record_github_reach(repo, reach, *answered_at_ms)
}
