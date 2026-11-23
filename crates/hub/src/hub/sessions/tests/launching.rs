//! What a launch tells the agent of its workspace, on a hub whose workspaces have stand-ins for their workers.

use protocol::model::{GithubRepositoryName, RepoUrl};
use protocol::prompt::Prompt;

use crate::hub::stand_in_workers::{a_chat, a_turn, hub_with_stand_in_workers};

#[tokio::test]
async fn a_launch_names_the_workspaces_repository_on_github_as_its_link_does() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let address = RepoUrl::try_from("git@github.com:example/project.git".to_owned()).unwrap();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&address)).unwrap();
    crate::hub::tests::linked_on_github(&hub, &address);
    let (_, chat) = a_chat(&hub).await;

    let spec = a_turn(&hub, &asked, &chat.id, Prompt::new("hi"), None).await;

    assert_eq!(spec.repository, Some(GithubRepositoryName::named("example/project")));
}

#[tokio::test]
async fn a_launch_names_no_repository_for_one_that_isnt_on_github() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let address = RepoUrl::try_from("https://gitlab.com/example/project.git".to_owned()).unwrap();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&address)).unwrap();
    let (_, chat) = a_chat(&hub).await;

    let spec = a_turn(&hub, &asked, &chat.id, Prompt::new("hi"), None).await;

    assert_eq!(spec.repository, None);
}
