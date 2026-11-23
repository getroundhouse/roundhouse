//! Adding a project by its link alone: one picked from GitHub's list, by its ID, or an address typed.

use protocol::client::ErrorCode;
use protocol::client::records::RepoToAdd;
use protocol::model::{AddressForm, Linking, RepoLink, RepoOrigin};

use super::super::adding::named_for;
use super::{holding, hub_reading, url};

/// One picked from GitHub's list is linked by its ID at once, found by GitHub's HTTPS address, with no checkout, and
/// a machine of its own clones it. Adding it again returns the first record.
#[tokio::test]
async fn a_repository_picked_from_githubs_list_is_added_linked_by_its_id_with_no_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, repositories, _, _github) = hub_reading(dir.path(), "https://gitlab.com/ada/elsewhere.git").await;
    repositories.set("R_project", "example/project");
    holding(&hub, "a-token", 7, "ada");
    let mut feed = hub.subscribe_feed();

    let added = hub.repo_add_linked(RepoToAdd::Github { id: "R_project".into() }).await.unwrap();
    let again = hub.repo_add_linked(RepoToAdd::Github { id: "R_project".into() }).await.unwrap();

    let link = RepoLink::Github { id: "R_project".into(), by: AddressForm::Https };
    assert_eq!((added.name.as_str(), &added.link), ("project", &Linking::Linked { link }));
    assert_eq!(added.origin, RepoOrigin::Remote { url: url("https://github.com/example/project") });
    assert_eq!(again, added, "the first record, unchanged");
    assert!(
        std::iter::from_fn(|| feed.try_recv().ok())
            .any(|notice| matches!(notice, protocol::client::HubNotice::Repo(repo) if repo.id == added.id))
    );
    assert_eq!(hub.github_name_of(&added), Some(super::named("example/project")));
    assert_eq!(hub.clone_url_of(&added, &"incus".into()).unwrap(), url("https://github.com/example/project"));
}

/// An address typed on github.com is linked as a checkout's `origin` is, by the form it's in, and one elsewhere by
/// itself. One no account can see, or with no account to ask as, isn't added.
#[tokio::test]
async fn an_address_typed_is_linked_as_a_checkouts_origin_is_and_one_no_account_sees_isnt_added() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, repositories, _, _github) = hub_reading(dir.path(), "https://gitlab.com/ada/elsewhere.git").await;
    repositories.set("R_project", "example/project");
    repositories.set("R_private", "example/private");
    repositories.hide("R_private", true);
    let typed = |address: &str| RepoToAdd::Address { url: url(address) };

    let no_account = hub.repo_add_linked(typed("git@github.com:example/project.git")).await.unwrap_err();
    holding(&hub, "a-token", 7, "ada");
    let ssh = hub.repo_add_linked(typed("git@github.com:example/project.git")).await.unwrap();
    let plain = hub.repo_add_linked(typed("https://gitlab.com/ada/tools.git")).await.unwrap();
    let unseen = hub.repo_add_linked(typed("https://github.com/example/private")).await.unwrap_err();

    assert_eq!(no_account.code, ErrorCode::Unavailable);
    assert_eq!(ssh.link, Linking::Linked { link: RepoLink::Github { id: "R_project".into(), by: AddressForm::Ssh } });
    assert_eq!(ssh.origin, RepoOrigin::Remote { url: url("git@github.com:example/project.git") });
    assert_eq!(plain.link, Linking::Linked { link: RepoLink::Plain { url: url("https://gitlab.com/ada/tools.git") } });
    assert_eq!((plain.name.as_str(), unseen.code), ("tools", ErrorCode::NotFound));
    let listed = hub.repo_list().await.unwrap();
    assert_eq!(listed.len(), 3, "the hub's own, and the two added: {listed:?}");
}

#[test]
fn a_project_added_by_an_address_is_named_as_git_clone_names_its_folder() {
    for (address, name) in [
        ("https://gitlab.com/ada/tools.git", "tools"),
        ("https://gitlab.com/ada/tools/", "tools"),
        ("git@gitlab.com:ada/tools.git", "tools"),
        ("ssh://git@example.com:2222/tools", "tools"),
    ] {
        assert_eq!(named_for(address), name, "{address}");
    }
}
