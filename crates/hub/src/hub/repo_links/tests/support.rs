//! What the tests of links share: a hub reading a stand-in for GitHub, and the accounts it holds.

use std::sync::Arc;

use github::{PullRequests, Repositories, Script, StandIn};
use protocol::model::{GithubRepositoryName, Repo, RepoUrl};

use crate::hub::Hub;
use crate::hub::tests::{hub_calling_github, pushed};

pub(super) fn url(url: &str) -> RepoUrl {
    RepoUrl::try_from(url.to_owned()).unwrap()
}

pub(super) fn named(repository: &str) -> GithubRepositoryName {
    GithubRepositoryName::try_from(repository.to_owned()).unwrap()
}

/// A hub calling a stand-in for GitHub that has `repositories` and the pull requests of example/project, with `w` on
/// `branch`, pushed to origin's `branch` at example/project, and the checkout of its repository, `r`, fetching from
/// `address`, as a worker read it, with no link yet.
pub(super) async fn hub_reading(
    dir: &std::path::Path,
    address: &str,
) -> (Arc<Hub>, Repositories, PullRequests, StandIn) {
    let (repositories, pull_requests) = (Repositories::default(), PullRequests::default());
    let script = Script {
        repositories: Some(repositories.clone()),
        pull_requests: Some(pull_requests.clone()),
        ..Script::default()
    };
    let github = StandIn::with(script).await;
    let hub = hub_calling_github(dir, github.api());
    hub.store.workspace_ready(&"w".into(), "/w", &"alder".into(), "main", None).unwrap();
    hub.store.set_workspace_branches(&"w".into(), &pushed("head-7", "head-7", (0, 0))).unwrap();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&url(address))).unwrap();
    (hub, repositories, pull_requests, github)
}

pub(super) fn repo(hub: &Hub) -> Repo {
    hub.store.get_repo(&"r".into()).unwrap()
}

/// Has the hub hold the token `token` of `name`, GitHub's user `user`, as an account, as connecting it would, without
/// asking GitHub whose it is. The first is the default.
pub(super) fn holding(hub: &Hub, token: &str, user: u64, name: &str) {
    use protocol::client::github_accounts::GithubAccountConnect;
    let asked = GithubAccountConnect { token: protocol::model::GithubToken::new(token), id: None };
    let connect = crate::github_logins::Connect::new(asked, crate::store::now_ms()).unwrap();
    let user = protocol::model::GithubUser { id: protocol::model::GithubUserId::new(user), name: name.into() };
    let owner = github::TokenOwner { user, profile: github::GithubProfile::default(), ends_at_ms: None };
    hub.github_logins.connect(&connect, &owner).unwrap();
}
