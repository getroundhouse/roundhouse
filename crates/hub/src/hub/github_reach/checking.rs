use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::HubNotice;
use protocol::model::{GithubReach, GithubRepositoryName, GithubUsername, Reach, RepoId};
use protocol::worker::GithubCredential;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio::time::{Instant, MissedTickBehavior};
use tracing::{error, warn};

use super::super::Hub;
use crate::store::{Store, StoreError, now_ms};

/// How often every repository is checked again, though nothing the hub knows of has changed, since what an account can
/// do changes on GitHub, as when an organization approves Roundhouse, or someone is given write access, and GitHub says
/// so to nobody. So a workspace says the new answer within half an hour of it, at the cost of one call to GitHub a
/// repository each time. By tokio's clock, which doesn't count a Mac's sleep, so a check due while it sleeps comes as
/// late again, which only holds back an answer. *Unconfirmed.*
pub(super) const CHECKED_AGAIN_EVERY: Duration = Duration::from_mins(30);

impl Hub {
    /// Checks whether the default GitHub account can push to each repository's repository on GitHub, and records each
    /// answer as the repository's `GithubReach`, announced as records are, until the hub stops: as it starts, for each
    /// repository the last run recorded one for, as a worker of a repository says which repository on GitHub it pushes
    /// to, as [`Hub::find_github_remote`] asks, as what workers give their processes for github.com changes, which it
    /// does as the default or its login changes, and every [`CHECKED_AGAIN_EVERY`]. Once a repository and login, not a
    /// workspace, and each repository's checks side by side, each within GitHub's time limit, never under the logins'
    /// lock.
    pub(crate) async fn watch_github_reach(self: Arc<Self>) {
        let credential = self.git_credential();
        Checks::new(self, credential).run().await;
    }
}

/// The checks of reach, as one task runs them, which alone records them, so no answer can be recorded after a newer
/// one.
pub(super) struct Checks {
    hub: Arc<Hub>,
    /// What workers give their processes for github.com, which a push meets, as the logins work it out.
    credential: watch::Receiver<GithubCredential>,
    /// Which repository on GitHub each repository was last checked for, by its record or what a worker said since.
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
    /// Whose token it asked GitHub with, if any.
    asked_as: Option<GithubUsername>,
}

impl Checks {
    pub(super) fn new(hub: Arc<Hub>, credential: watch::Receiver<GithubCredential>) -> Self {
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
        self.pushes_to = self.recorded().await;
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
                    () = self.hub.github_remotes.told() => {
                        let said = self.newly_said();
                        self.check(&credential, said);
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

    /// Which repository on GitHub each repository's record says it pushes to, so the hub checks them as it starts.
    async fn recorded(&self) -> HashMap<RepoId, Option<GithubRepositoryName>> {
        match self.hub.blocking(Store::github_reaches).await {
            Ok(recorded) => {
                let on_github = recorded.into_iter().filter_map(|record| {
                    let repository = record.reach.repository()?.clone();
                    Some((record.repo, Some(repository)))
                });
                on_github.collect()
            }
            Err(e) => {
                warn!("couldn't read which repositories to check the default can push to: {}", e.message);
                HashMap::new()
            }
        }
    }

    /// The repositories a worker has said push somewhere other than they were last checked for, which from then on
    /// they're checked for.
    fn newly_said(&mut self) -> Vec<RepoId> {
        let mut said = Vec::new();
        for (repo, repository) in self.hub.github_remotes.said() {
            if self.pushes_to.get(&repo) != Some(&repository) {
                self.pushes_to.insert(repo.clone(), repository);
                said.push(repo);
            }
        }
        said
    }

    /// Starts checking each of `repos` with `credential`, each on a task of its own, so a GitHub that doesn't answer
    /// for one holds up none of the others.
    fn check(&mut self, credential: &GithubCredential, repos: Vec<RepoId>) {
        for repo in repos {
            let Some(repository) = self.pushes_to.get(&repo).cloned() else { continue };
            self.started += 1;
            let number = self.started;
            self.newest.insert(repo.clone(), number);
            let (hub, credential) = (Arc::clone(&self.hub), credential.clone());
            let asked_as = match &credential {
                GithubCredential::Account(account) => Some(account.username.clone()),
                GithubCredential::MachineOwn | GithubCredential::Blocked { .. } | GithubCredential::Unknown => None,
            };
            self.under_way.spawn(async move {
                let reach = super::reaching::reach(&hub.github, repository, &credential).await;
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
