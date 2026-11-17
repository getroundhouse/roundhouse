use protocol::model::{GithubRepositoryName, Reach};
use protocol::worker::GithubCredential;

use crate::github::{Github, PushAnswer};

/// What a push to `repository` meets as `credential`: GitHub's answer, for an account's, as [`Github::push_reach`]
/// says, and otherwise what the credential says without asking, since the hub holds no token for each machine's own
/// login, and a block is what every push meets. Not on GitHub, for `None`, whatever the credential.
pub(super) async fn reach(
    github: &Github,
    repository: Option<GithubRepositoryName>,
    credential: &GithubCredential,
) -> Reach {
    let Some(repository) = repository else { return Reach::NotOnGithub };
    let credential = match credential {
        GithubCredential::Account(credential) => credential,
        GithubCredential::MachineOwn => return Reach::MachineOwn { repository },
        GithubCredential::Blocked { reason } => return Reach::Blocked { repository, reason: reason.clone() },
        // The logins never work one out, since it only stands for a newer hub's, which a worker reads.
        GithubCredential::Unknown => {
            return Reach::CouldntTell { repository, why: "the hub sends workers what it can't check".to_string() };
        }
    };
    let username = credential.username.clone();
    match github.push_reach(&repository, credential).await {
        Ok(PushAnswer::Takes) => Reach::Pushes { repository, username },
        Ok(PushAnswer::Refused(reason)) => Reach::Refused { repository, username, reason },
        Ok(PushAnswer::NotFound) => Reach::NotFound { repository, username },
        Ok(PushAnswer::TokenRefused) => Reach::TokenRefused { repository, username },
        Err(e) => Reach::CouldntTell { repository, why: e.message },
    }
}
