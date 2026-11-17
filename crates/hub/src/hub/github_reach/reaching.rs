use protocol::model::{GithubRepositoryName, Reach};

use crate::github::{Github, PushAnswer};
use crate::github_logins::DefaultCredential;

/// What a push to `repository` meets as `default`: GitHub's answer, for an account's, as [`Github::push_reach`] says,
/// asked with the username and token git gives GitHub, and naming the account as GitHub knows it, and otherwise what
/// the default says without asking, since the hub holds no token for each machine's own login, and a block is what
/// every push meets. Not on GitHub, for `None`, whatever the default.
pub(super) async fn reach(
    github: &Github,
    repository: Option<GithubRepositoryName>,
    default: &DefaultCredential,
) -> Reach {
    let Some(repository) = repository else { return Reach::NotOnGithub };
    let (credential, username, installation) = match default {
        DefaultCredential::Account { credential, username, installation } => {
            (credential, username.clone(), installation.clone())
        }
        DefaultCredential::MachineOwn => return Reach::MachineOwn { repository },
        DefaultCredential::Blocked { reason } => return Reach::Blocked { repository, reason: reason.clone() },
    };
    match github.push_reach(&repository, credential).await {
        Ok(PushAnswer::Takes) => Reach::Pushes { repository, username },
        Ok(PushAnswer::Refused(reason)) => Reach::Refused { repository, username, reason, installation },
        Ok(PushAnswer::NotFound) => Reach::NotFound { repository, username, installation },
        Ok(PushAnswer::TokenRefused) => Reach::TokenRefused { repository, username, installation },
        Err(e) => Reach::CouldntTell { repository, why: e.message },
    }
}
