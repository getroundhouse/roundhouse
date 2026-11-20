use protocol::model::{GithubRepositoryName, Reach};

use crate::github_logins::DefaultCredential;
use github::{Github, PushAnswer};

/// What a push to `repository` meets as `default`: GitHub's answer, for an account's, as [`Github::push_reach`] says,
/// asked with the username and token git gives GitHub, and naming the account as GitHub knows it, and otherwise what
/// the default says without asking, since the hub holds no token for each machine's own login, and a block is what
/// every push meets. Not on GitHub, for none, whatever the default. It's what a push meets on a provider where
/// Roundhouse sets up GitHub, whose workers send even a push to GitHub's SSH addresses through the default, so where a
/// checkout's own pushes go doesn't change it.
pub(super) async fn reach(
    github: &Github,
    repository: Option<GithubRepositoryName>,
    default: &DefaultCredential,
) -> Reach {
    let Some(repository) = repository else { return Reach::NotOnGithub };
    let (credential, username, installation) = match default {
        DefaultCredential::Account { credential, username, installation, .. } => {
            (credential, username.clone(), installation.clone())
        }
        DefaultCredential::NoAccount { reason } | DefaultCredential::Blocked { reason, .. } => {
            return Reach::Blocked { repository, reason: reason.clone() };
        }
    };
    match github.push_reach(&repository, credential).await {
        Ok(PushAnswer::Takes) => Reach::Pushes { repository, username },
        Ok(PushAnswer::Refused(reason)) => Reach::Refused { repository, username, reason, installation },
        Ok(PushAnswer::NotFound) => Reach::NotFound { repository, username, installation },
        Ok(PushAnswer::TokenRefused) => Reach::TokenRefused { repository, username, installation },
        Err(e) => Reach::CouldntTell { repository, why: e.message },
    }
}
