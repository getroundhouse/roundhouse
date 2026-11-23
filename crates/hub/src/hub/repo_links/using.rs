use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    AddressForm, GithubRepositoryName, GithubRepositorySaid, Linking, ProviderName, Repo, RepoLink, RepoUrl, Unlinked,
};

use crate::hub::Hub;
use crate::hub::observations::GithubRepositories;

impl Hub {
    /// What GitHub last said of the repository on GitHub `repo` is linked to, if it's linked to one and GitHub has said.
    fn github_said_of(&self, repo: &Repo) -> Option<GithubRepositorySaid> {
        self.observations.said::<GithubRepositories>(repo.link.github()?)
    }

    /// Which repository on GitHub `repo` is now, as GitHub last said of the one it's linked to, so it's what GitHub calls
    /// it after a rename. `None` for one linked to none, which has nothing of GitHub's, or that GitHub hasn't said of.
    pub(crate) fn github_name_of(&self, repo: &Repo) -> Option<GithubRepositoryName> {
        Some(self.github_said_of(repo)?.repository)
    }

    /// What a machine of `provider`'s with no checkout of `repo` clones, by its link: GitHub's address for it now, over
    /// HTTPS where Roundhouse sets up GitHub, which sends GitHub's SSH addresses over HTTPS anyway, and otherwise in the
    /// form the link was found by, since a machine left to its own git login may have a login for only one. A plain
    /// link's is its address. Refused with `Unavailable` for one with no link, or whose repository GitHub hasn't said
    /// where it is, saying why.
    pub(crate) fn clone_url_of(&self, repo: &Repo, provider: &ProviderName) -> Result<RepoUrl, ApiError> {
        let name = &repo.name;
        let link = match &repo.link {
            Linking::Linked { link } => link,
            Linking::NotLinked { repository, why } => {
                let message = format!("{name} isn't linked to {repository} on GitHub, since {}", unlinked(why));
                return Err(ApiError::new(ErrorCode::Unavailable, message));
            }
            Linking::NoAddress => {
                let message = format!("{name} has no address to clone, since its checkout's origin has none");
                return Err(ApiError::new(ErrorCode::Unavailable, message));
            }
            Linking::Unknown => {
                let message = format!("{name} is linked in a way this hub doesn't know");
                return Err(ApiError::new(ErrorCode::Unavailable, message));
            }
        };
        let by = match link {
            RepoLink::Plain { url } => return Ok(url.clone()),
            RepoLink::Github { by, .. } => *by,
            RepoLink::Unknown => {
                let message = format!("{name} is linked to a host this hub doesn't know");
                return Err(ApiError::new(ErrorCode::Unavailable, message));
            }
        };
        let said = self.github_said_of(repo).ok_or_else(|| {
            let message = format!("GitHub hasn't said where {name}'s repository is yet; try again in a minute");
            ApiError::new(ErrorCode::Unavailable, message)
        })?;
        Ok(github_clone(said, by, self.leaves_github_to_machine(provider)))
    }
}

/// Which of GitHub's addresses for a repository, as `said`, a machine clones by, whose link was found by an address
/// of the form `by`, as [`Hub::clone_url_of`] says, where Roundhouse leaves GitHub to the machine's own login, as
/// `left_to_machine` says.
pub(super) fn github_clone(said: GithubRepositorySaid, by: AddressForm, left_to_machine: bool) -> RepoUrl {
    if by == AddressForm::Ssh && left_to_machine { said.ssh } else { said.https }
}

/// Why a project isn't linked, as a clause after "since".
fn unlinked(why: &Unlinked) -> String {
    match why {
        Unlinked::NotAskedYet => "the hub hasn't asked GitHub yet".to_owned(),
        Unlinked::NoAccount => "no GitHub account is connected to ask GitHub as".to_owned(),
        Unlinked::NotFound => "GitHub has no such repository that any connected account can see".to_owned(),
        Unlinked::CouldntTell { why } => format!("GitHub couldn't say which repository it is ({why})"),
        Unlinked::Unknown => "of something this hub doesn't know".to_owned(),
    }
}
