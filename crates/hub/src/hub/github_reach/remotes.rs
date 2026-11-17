use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::model::{GithubRepositoryName, RepoId};
use tokio::sync::Notify;

/// Which repository on GitHub each repository pushes to, as a worker of it has said since the hub started, and which
/// are being asked, so each is asked once a start, and by one worker at a time, and the check hears of each as it's
/// said.
#[derive(Default)]
pub(crate) struct Remotes {
    said: Mutex<HashMap<RepoId, Said>>,
    /// Told as each is said, for the check to look again at what has been.
    told: Notify,
}

enum Said {
    Asking,
    PushesTo(Option<GithubRepositoryName>),
}

impl Remotes {
    /// `repo`'s ask, unless a worker of it has said since the hub started, or one is being asked.
    pub(super) fn ask(self: &Arc<Self>, repo: RepoId) -> Option<Asking> {
        let mut said = self.said.lock();
        if said.contains_key(&repo) {
            return None;
        }
        said.insert(repo.clone(), Said::Asking);
        Some(Asking { remotes: Arc::clone(self), repo, answered: false })
    }

    /// Which repository on GitHub each repository pushes to, of those a worker has said of since the hub started.
    pub(super) fn said(&self) -> HashMap<RepoId, Option<GithubRepositoryName>> {
        let said = self.said.lock();
        let pushes_to = said.iter().filter_map(|(repo, said)| match said {
            Said::PushesTo(repository) => Some((repo.clone(), repository.clone())),
            Said::Asking => None,
        });
        pushes_to.collect()
    }

    /// Once one has been said since the last time this said so, or since the hub started.
    pub(super) async fn told(&self) {
        self.told.notified().await;
    }

    /// Whether `repo` has been said of, or is being asked.
    #[cfg(test)]
    pub(super) fn has(&self, repo: &RepoId) -> bool {
        self.said.lock().contains_key(repo)
    }
}

/// One repository's ask. Let go of before it's [`Asking::answered`], however that happens, it leaves the repository to
/// be asked again, by the next link to a worker of it.
pub(super) struct Asking {
    remotes: Arc<Remotes>,
    repo: RepoId,
    answered: bool,
}

impl Asking {
    pub(super) fn repo(&self) -> &RepoId {
        &self.repo
    }

    /// A worker said the repository pushes to `repository`, so it isn't asked again until the hub starts again, and the
    /// check is told.
    pub(super) fn answered(mut self, repository: Option<GithubRepositoryName>) {
        self.remotes.said.lock().insert(self.repo.clone(), Said::PushesTo(repository));
        self.answered = true;
        self.remotes.told.notify_one();
    }
}

impl Drop for Asking {
    fn drop(&mut self) {
        if !self.answered {
            self.remotes.said.lock().remove(&self.repo);
        }
    }
}
