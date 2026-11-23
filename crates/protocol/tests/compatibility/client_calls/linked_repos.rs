//! Adding a project by its link alone, and listing the repositories the hub's GitHub accounts can see to pick one
//! from, as a client writes the requests and reads the replies.

use protocol::client::github_repositories::{GithubRepositories, GithubRepositoryList, GithubRepositorySeen};
use protocol::client::records::{RepoAddLinked, RepoToAdd};
use protocol::client::{ClientRequest, ClientResponse};
use protocol::frame;
use protocol::model::{GithubRepositoryName, RepoUrl};

use super::{answered, asked};

#[test]
fn adding_a_project_by_githubs_id_or_by_an_address_keeps_its_form() {
    let address = RepoUrl::try_from("https://gitlab.com/ada/project.git".to_owned()).unwrap();

    asked(
        ClientRequest::RepoAddLinked(RepoAddLinked { repository: RepoToAdd::Github { id: "R_kgDOJbBqvw".into() } }),
        r#"{"method":"repo_add_linked","repository":{"by":"github","id":"R_kgDOJbBqvw"}}"#,
    );
    asked(
        ClientRequest::RepoAddLinked(RepoAddLinked { repository: RepoToAdd::Address { url: address } }),
        r#"{"method":"repo_add_linked","repository":{"by":"address","url":"https://gitlab.com/ada/project.git"}}"#,
    );
}

/// Its search and where its page starts, each left out when there's none, and its reply, with each repository once,
/// with the accounts that can see it.
#[test]
fn listing_the_repositories_the_hubs_accounts_can_see_keeps_its_form() {
    let next = GithubRepositoryList { search: "round".into(), after: Some("50".into()) };
    let seen = GithubRepositorySeen {
        id: "R_kgDOJbBqvw".into(),
        repository: GithubRepositoryName::named("getroundhouse/roundhouse"),
        private: false,
        seen_by: vec!["ada".into(), "grace".into()],
    };

    asked(
        ClientRequest::GithubRepositoryList(next),
        r#"{"method":"github_repository_list","search":"round","after":"50"}"#,
    );
    let bare = frame::decode::<ClientRequest>(br#"{"method":"github_repository_list"}"#).unwrap();
    assert_eq!(bare, ClientRequest::GithubRepositoryList(GithubRepositoryList::default()));
    answered(
        ClientResponse::GithubRepositories(GithubRepositories { repositories: vec![seen], after: None }),
        r#"{"result":"github_repositories","repositories":[{"id":"R_kgDOJbBqvw","repository":"getroundhouse/roundhouse","private":false,"seen_by":["ada","grace"]}]}"#,
    );
}
