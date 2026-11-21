//! What one read of a workspace asks of GitHub: its own pull request and those its waits watch, one ask for each
//! group of waits, which takes in the workspace's own when it's among them. So the bar and every wait on a pull request
//! go by the same reading of it, and a wait's stack is read from one moment, since a reading of part of it can't tell a
//! lower pull request merging from one leaving the stack.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    GitHubRepository, GithubRepositoryName, PullRequestNumber, PullRequestSelection, PullRequestsSnapshot,
};

use crate::hub::pull_requests::remote::Waited;
use github::{PullRequestsAsk, Unread};

/// Pull requests of one repository that one ask reads.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::hub::pull_requests) struct Scope {
    pub(in crate::hub::pull_requests) ask: PullRequestsAsk,
    /// The workspace's own pull request, when it's among them.
    pub(in crate::hub::pull_requests) own: Option<PullRequestNumber>,
    /// The repository the waits among them are pinned to, as GitHub named it, when any watches them.
    pub(in crate::hub::pull_requests) waited: Option<GitHubRepository>,
}

/// What a read found of the pull requests a workspace's waits watch in a repository, for those waits to judge.
#[derive(Debug)]
pub(in crate::hub::pull_requests) struct WaitsRead {
    pub(in crate::hub::pull_requests) repository: GitHubRepository,
    pub(in crate::hub::pull_requests) read: Result<PullRequestsSnapshot, Unread>,
}

impl WaitsRead {
    /// Unread, as `error` says, which goes for every pull request of it.
    pub(in crate::hub::pull_requests) fn failed(repository: GitHubRepository, error: ApiError) -> Self {
        Self { repository, read: Err(Unread { error, affected: Vec::new() }) }
    }
}

/// The asks of a workspace's read: one for each group of its waits, `waited`, on github.com, with `own`, the
/// workspace's own pull request, in the group that has it, or else in an ask of its own. Those of a repository that
/// isn't on github.com can't be read, and come back as why.
pub(in crate::hub::pull_requests) fn scopes(
    own: Option<(GithubRepositoryName, PullRequestNumber)>,
    waited: Vec<Waited>,
) -> (Vec<Scope>, Vec<WaitsRead>) {
    let (mut scopes, mut unreadable) = (Vec::new(), Vec::new());
    for Waited { repository, selection } in waited {
        match on_github(&repository) {
            Ok(name) => {
                let ask = PullRequestsAsk { repository: name, selection, known_rules: Vec::new() };
                scopes.push(Scope { ask, own: None, waited: Some(repository) });
            }
            Err(error) => unreadable.push(WaitsRead::failed(repository, error)),
        }
    }
    if let Some((repository, number)) = own {
        let same = |scope: &&mut Scope| {
            scope.ask.repository.as_str().eq_ignore_ascii_case(repository.as_str())
                && scope.ask.selection.numbers().contains(&number)
        };
        if let Some(scope) = scopes.iter_mut().find(same) {
            scope.own = Some(number);
        } else {
            let ask =
                PullRequestsAsk { repository, selection: PullRequestSelection::from(number), known_rules: Vec::new() };
            scopes.push(Scope { ask, own: Some(number), waited: None });
        }
    }
    (scopes, unreadable)
}

/// Which repository on github.com a wait's pinned repository is, or unsupported for one elsewhere, as a wait started
/// through a worker on another GitHub could be, which the hub doesn't read.
fn on_github(repository: &GitHubRepository) -> Result<GithubRepositoryName, ApiError> {
    let on_github = repository.url.starts_with("https://github.com/");
    GithubRepositoryName::try_from(repository.name_with_owner.clone())
        .ok()
        .filter(|_| on_github)
        .ok_or_else(|| ApiError::new(ErrorCode::Unsupported, "the hub reads pull requests on github.com alone"))
}
