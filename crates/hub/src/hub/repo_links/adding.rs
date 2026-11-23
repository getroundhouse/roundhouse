//! Adding a project by its link alone, with no checkout on the hub's machine: one GitHub's list gave, by its ID, or an
//! address a person typed, which is linked as a checkout's `origin` is.

use github::{Leaving, RepositoryAsk, RepositoryFound};
use protocol::client::records::RepoToAdd;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{AddressForm, Repo, RepoId, RepoLink, RepoUrl};

use crate::hub::{Hub, new_id, off_thread};
use crate::store::now_ms;

impl Hub {
    /// Adds the project `asked` names, linked at once, as `RepoAddLinked` says, and tells every client of it. One on
    /// github.com is found as each connected account in turn, the default's first. Adding the same again returns the
    /// first record.
    pub(crate) async fn repo_add_linked(&self, asked: RepoToAdd) -> Result<Repo, ApiError> {
        let (ask, typed) = match asked {
            RepoToAdd::Github { id } => (RepositoryAsk::Id(id), None),
            RepoToAdd::Address { url } => {
                let Some(repository) = url.on_github() else {
                    let name = name_in(&url);
                    return self.add_linked(name, url.clone(), RepoLink::Plain { url }).await;
                };
                (RepositoryAsk::Named(repository), Some(url))
            }
        };
        let found = self.found_as_any(&ask).await?;
        // One picked from GitHub's list was found by no address, so it's taken as found by HTTPS, which every machine
        // can reach a public repository by. *Unconfirmed.*
        let (url, by) = match typed {
            Some(url) => {
                let by = AddressForm::of(&url);
                (url, by)
            }
            None => (found.said.https.clone(), AddressForm::Https),
        };
        let name = found.said.repository.as_str().rsplit('/').next().unwrap_or_default().to_owned();
        let link = RepoLink::Github { id: found.id.clone(), by };
        self.keep_said(found, now_ms()).await;
        self.add_linked(name, url, link).await
    }

    /// Adds the project `name`, found by `url`, linked by `link`, tells every client, and has the check of reach look
    /// at it.
    async fn add_linked(&self, name: String, url: RepoUrl, link: RepoLink) -> Result<Repo, ApiError> {
        let id = RepoId::from(new_id());
        let repo = self.blocking(move |store| store.add_linked_repo(&id, &name, &url, &link, now_ms())).await?;
        self.notify(HubNotice::Repo(repo.clone()));
        self.links_read.notify_one();
        Ok(repo)
    }

    /// The repository `ask` names, as the first connected account that sees it says. Refused with `NotFound` when GitHub
    /// says none of them can see it, and with `Unavailable` when there's no account to ask as, or GitHub failed for one
    /// and none found it.
    async fn found_as_any(&self, ask: &RepositoryAsk) -> Result<RepositoryFound, ApiError> {
        let logins = std::sync::Arc::clone(&self.github_logins);
        let readers = off_thread(move || logins.readers(now_ms())).await?;
        if readers.is_empty() {
            let message = "no GitHub account is connected to find the repository as; connect one in Settings first";
            return Err(ApiError::new(ErrorCode::Unavailable, message));
        }
        let mut failed = None;
        for (_, token) in &readers {
            let read = self.github.read_repositories(token, std::slice::from_ref(ask), Leaving::Nothing).await;
            match read.answers.into_iter().next() {
                Some(Ok(Some(found))) => return Ok(found),
                Some(Ok(None)) => {}
                Some(Err(e)) => failed = Some(e),
                None => failed = Some(ApiError::new(ErrorCode::Unavailable, "GitHub said nothing of the repository")),
            }
        }
        Err(match failed {
            Some(e) => ApiError::new(ErrorCode::Unavailable, e.message),
            None => {
                ApiError::new(ErrorCode::NotFound, "GitHub has no such repository that any connected account can see")
            }
        })
    }
}

/// What a project added by `url` is called: the last part of its path, without `.git`, as `git clone` names its
/// folder, such as `project` for `https://gitlab.com/ada/project.git`.
fn name_in(url: &RepoUrl) -> String {
    let path = url.as_str().trim_end_matches('/');
    let last = path.rsplit(['/', ':']).next().unwrap_or(path);
    last.strip_suffix(".git").unwrap_or(last).to_owned()
}

/// What [`name_in`] names a project added by `url`, for a test.
#[cfg(test)]
pub(super) fn named_for(url: &str) -> String {
    name_in(&RepoUrl::try_from(url.to_owned()).unwrap_or_else(|_| panic!("{url}")))
}
