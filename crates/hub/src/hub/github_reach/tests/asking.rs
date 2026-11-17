//! Asking the first worker of each repository the hub links to which repository on GitHub it pushes to.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubRepositoryName, RepoId, Workspace};

use super::super::super::Hub;
use super::super::asking::SAID_WITHIN;
use super::support::{CLI, worker_answering, worker_saying};
use crate::hub::tests::hub_in;

/// The workspace `w2`, in `w`'s repository, `r`.
fn second_workspace(hub: &Hub) {
    let workspace = Workspace { name: "bank".into(), ..protocol::fixtures::workspace("w2") };
    hub.store.insert_workspace(&workspace, None).unwrap();
}

/// Once the hub has done with `repo`'s ask: said, or let go of for the next link.
async fn done_asking(hub: &Hub, repo: &RepoId, said: bool) {
    let deadline = tokio::time::Instant::now() + SAID_WITHIN + Duration::from_secs(1);
    while hub.github_remotes.has(repo) != said || hub.github_remotes.said().contains_key(repo) != said {
        assert!(tokio::time::Instant::now() < deadline, "the hub was still asking");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// So it's asked once a repository, however many of its workspaces' workers link, and what it said is the
/// repository's.
#[tokio::test]
async fn the_first_link_to_a_worker_of_each_repository_is_asked_where_it_pushes() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    second_workspace(&hub);
    let (first, asked_first) = worker_saying(Some(CLI));
    let (second, asked_second) = worker_saying(Some(GithubRepositoryName::named("ada/fork")));
    let repo = RepoId::from("r");

    hub.find_github_remote("w".into(), &first).await;
    done_asking(&hub, &repo, true).await;
    hub.find_github_remote("w2".into(), &second).await;
    hub.find_github_remote("w".into(), &second).await;

    assert_eq!(hub.github_remotes.said(), HashMap::from([(repo, Some(CLI))]));
    assert_eq!((asked_first.load(Ordering::SeqCst), asked_second.load(Ordering::SeqCst)), (1, 0), "once a repository");
}

/// One from before the ask, one that can't read its git, and one that never answers, each of which the next link to a
/// worker of the repository is asked after. And one whose repository is on no GitHub, which is an answer.
#[tokio::test(start_paused = true)]
async fn a_worker_that_doesnt_say_leaves_the_repository_to_the_next_link() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let repo = RepoId::from("r");
    let unsupported = Some(Err(ApiError::new(ErrorCode::Unsupported, "a request this worker doesn't know")));
    let failed = Some(Err(ApiError::new(ErrorCode::Worker, "git remote get-url --push origin failed")));

    for answer in [unsupported, failed, None] {
        let (link, asked) = worker_answering(answer.clone());
        hub.find_github_remote("w".into(), &link).await;
        done_asking(&hub, &repo, false).await;
        assert_eq!(asked.load(Ordering::SeqCst), 1, "{answer:?}");
    }
    let (link, _) = worker_saying(None);
    hub.find_github_remote("w".into(), &link).await;
    done_asking(&hub, &repo, true).await;

    assert_eq!(hub.github_remotes.said(), HashMap::from([(repo, None)]));
}
