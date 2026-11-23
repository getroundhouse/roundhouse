//! Linking a project by its checkout's `origin`: which repository on GitHub it is, its pull requests, and what a
//! machine of its own clones.

use github::PullRequestState;
use protocol::client::ErrorCode;
use protocol::model::{AddressForm, GithubRepositorySaid, Linking, ProviderName, Repo, RepoLink, Unlinked};

use super::super::using::github_clone;
use super::{hub_reading, named, repo, url};
use crate::hub::Hub;
use crate::hub::tests::reading_as_ada;

/// The link is GitHub's ID for the repository, so once it's renamed on GitHub, the project's pull requests are read by
/// its new name, and a workspace on a machine of its own clones it there. The link itself doesn't change, and nor does
/// the record.
#[tokio::test]
async fn a_project_linked_by_githubs_id_goes_on_working_once_its_repository_is_renamed() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, repositories, pull_requests, _github) =
        hub_reading(dir.path(), "https://github.com/example/project.git").await;
    repositories.set("R_project", "example/project");
    pull_requests.set(7, PullRequestState::Open);
    reading_as_ada(&hub);
    let elsewhere = ProviderName::from("incus");

    hub.link_repos().await;
    let linked = repo(&hub);
    repositories.set("R_project", "example/renamed");
    pull_requests.rename("example/renamed");
    hub.link_repos().await;
    let found = hub.workspace_pull_request("w".into()).await.unwrap();

    let link = RepoLink::Github { id: "R_project".into(), by: AddressForm::Https };
    assert_eq!(linked.link, Linking::Linked { link });
    assert_eq!(repo(&hub), linked, "a rename changes nothing of the record");
    assert_eq!(hub.github_name_of(&linked), Some(named("example/renamed")));
    assert_eq!(found.map(|found| found.pull_request.number.get()), Some(7), "read by its new name");
    assert_eq!(hub.clone_url_of(&linked, &elsewhere).unwrap(), url("https://github.com/example/renamed"));
}

/// A machine left to its own git login clones by the form of address the link was found by, and one where Roundhouse
/// sets up GitHub clones over HTTPS, which it sends GitHub's SSH addresses over anyway.
#[test]
fn a_link_found_by_an_ssh_address_clones_over_ssh_only_on_a_machine_left_to_its_own_login() {
    let said = GithubRepositorySaid {
        repository: named("example/project"),
        https: url("https://github.com/example/project"),
        ssh: url("git@github.com:example/project.git"),
    };
    let cloned = |by, left| github_clone(said.clone(), by, left);

    assert_eq!(cloned(AddressForm::Ssh, true), said.ssh);
    assert_eq!(cloned(AddressForm::Ssh, false), said.https);
    assert_eq!(cloned(AddressForm::Https, true), said.https);
    assert_eq!(cloned(AddressForm::Unknown, true), said.https);
}

/// One on github.com isn't linked until GitHub says which repository it is, and says why, so nothing of GitHub's works
/// for it, cloning included: with no account to ask as, while GitHub fails, and when no account can see it. A link
/// found once the reason has gone is kept.
#[tokio::test]
async fn a_project_on_github_that_no_account_can_see_says_it_isnt_linked_and_why() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, repositories, _, _github) = hub_reading(dir.path(), "https://github.com/example/private").await;
    repositories.set("R_private", "example/private");
    repositories.hide("R_private", true);
    let why = |hub: &Hub| {
        let link = repo(hub).link;
        let Linking::NotLinked { repository, why } = link else { panic!("{link:?}") };
        (repository, why)
    };

    let before = why(&hub);
    hub.link_repos().await;
    let no_account = why(&hub);
    reading_as_ada(&hub);
    hub.link_repos().await;
    let unseen = why(&hub);
    repositories.fail(true);
    hub.link_repos().await;
    let failing = why(&hub);
    repositories.fail(false);
    repositories.hide("R_private", false);
    hub.link_repos().await;

    let private = named("example/private");
    assert_eq!(before, (private.clone(), Unlinked::NotAskedYet));
    assert_eq!(no_account, (private.clone(), Unlinked::NoAccount));
    assert_eq!(unseen, (private.clone(), Unlinked::NotFound));
    assert!(matches!(failing, (_, Unlinked::CouldntTell { .. })), "{failing:?}");
    let not_linked = hub.clone_url_of(
        &Repo { link: Linking::NotLinked { repository: private, why: Unlinked::NotFound }, ..repo(&hub) },
        &"incus".into(),
    );
    assert_eq!(not_linked.unwrap_err().code, ErrorCode::Unavailable);
    assert!(matches!(repo(&hub).link, Linking::Linked { .. }), "{:?}", repo(&hub).link);
}

/// One whose remote is on another host is linked by its address as a worker reads it, and a machine of its own is
/// handed that address to clone, asking GitHub nothing.
#[tokio::test]
async fn a_project_on_another_host_is_linked_by_its_address_and_cloned_from_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, repositories, _, _github) = hub_reading(dir.path(), "https://gitlab.com/ada/project.git").await;
    reading_as_ada(&hub);

    hub.link_repos().await;

    let linked = repo(&hub);
    let link = RepoLink::Plain { url: url("https://gitlab.com/ada/project.git") };
    assert_eq!(linked.link, Linking::Linked { link });
    assert_eq!(hub.clone_url_of(&linked, &"incus".into()).unwrap(), url("https://gitlab.com/ada/project.git"));
    assert_eq!(hub.github_name_of(&linked), None);
    assert_eq!(repositories.queries(), 0);
}
