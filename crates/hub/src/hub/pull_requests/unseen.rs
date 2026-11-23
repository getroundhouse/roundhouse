//! GitHub showing the default no repository where a workspace's branch's pull requests are, which says nothing of
//! whether the branch has one. The store owns the last the hub heard of it for each workspace, as the observation
//! [`WorkspaceRepositoryUnseen`], which the task that works out GitHub's observations keeps from what each read heard,
//! so a hub that starts again has it as of the read that heard it, as `docs/principles/observations.md` says.

use protocol::client::ApiError;
use protocol::model::{GithubLoginKind, GithubRepositoryName, RepositoryUnseen, WorkspaceId};
use tracing::warn;

use super::recording::Found;
use crate::github_logins::DefaultCredential;
use crate::hub::Hub;
use crate::hub::github_observed::Heard;
use crate::hub::observations::WorkspaceRepositoryUnseen;

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

impl Hub {
    /// The repository GitHub last showed the default none of, where `id`'s branch's pull requests are, unless a read
    /// has found its pull request, or that it has none, since, as the store keeps it, across restarts too.
    pub(crate) fn unseen_of(&self, id: &WorkspaceId) -> Option<RepositoryUnseen> {
        self.observations.value_of::<WorkspaceRepositoryUnseen>(id).flatten()
    }

    /// What `read` of a workspace's pull request heard of its repository: GitHub showing it none, or found or none
    /// found, which shows it was seen. A read that failed otherwise heard nothing of it.
    pub(in crate::hub::pull_requests) async fn heard_of_repository(&self, read: &Result<Found, OwnUnread>) -> Heard {
        match read {
            Ok(_) => Heard::Seen,
            Err(OwnUnread::Unseen { repository, said }) => {
                self.unseen_as_default(repository, said).await.map_or(Heard::Nothing, Heard::Unseen)
            }
            Err(OwnUnread::Failed(_)) => Heard::Nothing,
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
