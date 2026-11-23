//! GitHub showing the default no repository where a workspace's branch's pull requests are, which says nothing of
//! whether the branch has one. The hub keeps the last it heard of it for each workspace, in memory, since a read finds
//! it again as soon as a client shows the workspace, and tells clients that show the workspace as it changes.

use std::collections::HashMap;

use parking_lot::Mutex;
use protocol::client::ApiError;
use protocol::model::{GithubLoginKind, GithubRepositoryName, RepositoryUnseen, WorkspaceId};
use tokio::sync::broadcast;
use tracing::warn;

use super::recording::Found;
use crate::github_logins::DefaultCredential;
use crate::hub::Hub;

/// How many changes a client's subscription can fall behind on before it reads everything it shows again.
const TOLD_BEHIND: usize = 64;

/// Why a workspace's own pull request couldn't be read.
#[derive(Clone, Debug)]
pub(in crate::hub::pull_requests) enum OwnUnread {
    /// GitHub showed the default no repository `repository`, where the branch's pull requests are asked of, saying
    /// `said`, as [`github::HeadsUnread::Unseen`] says.
    Unseen {
        repository: GithubRepositoryName,
        said: String,
    },
    Failed(ApiError),
}

impl OwnUnread {
    /// It as an error, for whoever only reports it, such as a client or an agent that asked.
    pub(in crate::hub::pull_requests) fn error(&self) -> ApiError {
        match self {
            Self::Unseen { said, .. } => github::HeadsUnread::Unseen { said: said.clone() }.error(),
            Self::Failed(error) => error.clone(),
        }
    }
}

impl From<ApiError> for OwnUnread {
    fn from(error: ApiError) -> Self {
        Self::Failed(error)
    }
}

/// What the hub last heard of each workspace's repository being unseen, and who's told as that changes.
pub(in crate::hub) struct Unseens {
    kept: Mutex<HashMap<WorkspaceId, RepositoryUnseen>>,
    told: broadcast::Sender<WorkspaceId>,
}

impl Unseens {
    pub(in crate::hub) fn new() -> Self {
        Self { kept: Mutex::new(HashMap::new()), told: broadcast::channel(TOLD_BEHIND).0 }
    }
}

impl Hub {
    /// The repository GitHub last showed the default none of, where `id`'s branch's pull requests are, unless a read
    /// has found its pull request, or that it has none, since.
    pub(crate) fn unseen_of(&self, id: &WorkspaceId) -> Option<RepositoryUnseen> {
        self.pull_requests.unseen.kept.lock().get(id).cloned()
    }

    /// Each workspace whose [`Hub::unseen_of`] changes, as it does.
    pub(crate) fn unseen_told(&self) -> broadcast::Receiver<WorkspaceId> {
        self.pull_requests.unseen.told.subscribe()
    }

    /// Keeps what `read` of `id`'s pull request says of its repository: GitHub showing it none, or found or none
    /// found, which shows it was seen. A read that failed otherwise says nothing of it, so what was heard before stays.
    pub(in crate::hub::pull_requests) async fn keep_unseen(&self, id: &WorkspaceId, read: &Result<Found, OwnUnread>) {
        let now = match read {
            Ok(_) => None,
            Err(OwnUnread::Unseen { repository, said }) => match self.unseen_as_default(repository, said).await {
                Some(unseen) => Some(unseen),
                None => return,
            },
            Err(OwnUnread::Failed(_)) => return,
        };
        let changed = {
            let mut kept = self.pull_requests.unseen.kept.lock();
            let changed = kept.get(id) != now.as_ref();
            match now {
                Some(unseen) => kept.insert(id.clone(), unseen),
                None => kept.remove(id),
            };
            changed
        };
        if changed {
            // Nobody may be listening, which leaves nobody to tell.
            let _ = self.pull_requests.unseen.told.send(id.clone());
        }
    }

    /// `repository` unseen, GitHub saying `said`, as the default, which the hub reads GitHub as: who GitHub knows it as,
    /// how its login is held, and for a sign-in's, the page where the person grants an organization access to the app
    /// it's through. `None` while there's no default that can be used, which a read needs.
    async fn unseen_as_default(&self, repository: &GithubRepositoryName, said: &str) -> Option<RepositoryUnseen> {
        let DefaultCredential::Account { account, username, installation, .. } = self.git_credential().borrow().clone()
        else {
            return None;
        };
        let record = self.blocking(move |store| store.github_account(&account)).await;
        let login = match record {
            Ok(record) => record.and_then(|record| record.held_login).map_or(GithubLoginKind::Unknown, |held| held.how),
            Err(e) => {
                warn!("reading how the default's login is held: {}", e.message);
                GithubLoginKind::Unknown
            }
        };
        let approve_at = (login == GithubLoginKind::SignIn).then(|| self.github.app_page());
        Some(RepositoryUnseen {
            repository: repository.clone(),
            username,
            login,
            installation,
            said: said.to_string(),
            approve_at,
        })
    }
}
