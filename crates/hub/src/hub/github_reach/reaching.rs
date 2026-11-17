use protocol::model::{GithubRepositoryName, Reach};

use crate::github::{Github, PushAnswer};
use crate::github_logins::DefaultCredential;

/// Where a repository's pushes go, as its record and the worker that read its address say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PushesTo {
    /// The repository on GitHub its address names, if it names one.
    pub(super) repository: Option<GithubRepositoryName>,
    /// Whether a push from its checkout goes round the git credential the hub hands workers, as
    /// [`PushRoute::Around`](protocol::worker::PushRoute::Around) says.
    pub(super) around: bool,
}

/// What a push to `pushes_to` meets as `default`: GitHub's answer, for an account's, as [`Github::push_reach`] says,
/// asked with the username and token git gives GitHub, and naming the account as GitHub knows it, and otherwise what
/// the default says without asking, since the hub holds no token for each machine's own login, and a block is what
/// every push meets. Not on GitHub, for none, whatever the default. One that goes round the git credential goes as the
/// machine's own login whatever the default is, so GitHub isn't asked, and nor is a block met, but under the machine's
/// own, which is what that says already.
pub(super) async fn reach(github: &Github, pushes_to: PushesTo, default: &DefaultCredential) -> Reach {
    let PushesTo { repository, around } = pushes_to;
    let Some(repository) = repository else { return Reach::NotOnGithub };
    let (credential, username, installation) = match default {
        DefaultCredential::MachineOwn => return Reach::MachineOwn { repository },
        _ if around => return Reach::PushesAsMachineOwn { repository },
        DefaultCredential::Account { credential, username, installation, .. } => {
            (credential, username.clone(), installation.clone())
        }
        DefaultCredential::Blocked { reason, .. } => return Reach::Blocked { repository, reason: reason.clone() },
    };
    match github.push_reach(&repository, credential).await {
        Ok(PushAnswer::Takes) => Reach::Pushes { repository, username },
        Ok(PushAnswer::Refused(reason)) => Reach::Refused { repository, username, reason, installation },
        Ok(PushAnswer::NotFound) => Reach::NotFound { repository, username, installation },
        Ok(PushAnswer::TokenRefused) => Reach::TokenRefused { repository, username, installation },
        Err(e) => Reach::CouldntTell { repository, why: e.message },
    }
}
