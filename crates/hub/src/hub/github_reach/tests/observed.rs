//! Whether the default can push to a repository, which the hub keeps as an observation, and makes the record
//! `GithubReach` from for a client of a version from before it: across a restart, from a database from before, as the
//! checks start with nothing known, and forgotten with the repository.

use protocol::model::{GithubReach, Observation, Observed, Reach, RecordKey, RepoUrl};
use protocol::worker::{RepoUrlReply, WorkerRequest, WorkerResponse};
use rusqlite::Connection;
use tokio::sync::watch;

use super::support::{CLI, account, checking, told};
use crate::hub::Hub;
use crate::hub::RepositoryReach;
use crate::hub::tests::{hub_calling_github, hub_calling_github_again, serving};
use crate::store::BEFORE_REACH_OBSERVED;
use crate::store::columns::OneValue;
use crate::store::older::store_from_before;
use crate::worker_link::WorkerLink;
use github::{Answer, Script, StandIn};

/// GitHub, refusing every push to `cli/cli`.
async fn refusing() -> StandIn {
    let refused = Answer::git_says(403, "Permission to cli/cli.git denied.");
    StandIn::with(Script { pushes: vec![("cli/cli", vec![refused])], ..Script::default() }).await
}

/// What GitHub says of a push to `cli/cli` as ada.
fn refused() -> Reach {
    let reason = "Permission to cli/cli.git denied.".into();
    Reach::Refused { repository: CLI, username: "ada".into(), reason, installation: None }
}

/// What `hub` lists of `repo`'s reach, as an observation, and as the record made from it.
async fn listed(hub: &Hub, repo: &str) -> (Option<Observed<Reach>>, Vec<GithubReach>) {
    let records = hub.records_list().await.unwrap();
    let observed = records.observations.into_iter().find_map(|observation| {
        let Observation::RepositoryReach { repo: of, reach, .. } = observation else { return None };
        (of.as_str() == repo).then_some(reach)
    });
    (observed, records.github_reach)
}

/// Has the hub's repository, `r`, fetch from `cli/cli`, as a worker read it.
fn on_cli(hub: &Hub) {
    let address = RepoUrl::try_from("https://github.com/cli/cli.git".to_string()).unwrap();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&address)).unwrap();
}

/// A hub that starts again has each repository's reach as of GitHub's answer, which the record made from it calls
/// stale, with when GitHub answered, until it's checked again.
#[tokio::test]
async fn a_reach_is_kept_across_a_restart_as_of_githubs_answer() {
    let github = refusing().await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    on_cli(&hub);
    let mut feed = hub.subscribe_feed();
    let (_credential, following) = watch::channel(account("ada", "a-token"));
    let checks = checking(&hub, following);
    let answered = told(&mut feed).await;
    checks.abort();
    hub.observations_written().await;
    drop(hub);

    let again = hub_calling_github_again(dir.path(), github.api());

    let (observed, records) = listed(&again, "r").await;
    let Some(Observed::AsOf { value, as_of_ms }) = observed else { panic!("{observed:?}") };
    assert_eq!((value, Some(as_of_ms)), (refused(), answered.answered_at_ms));
    let [record] = records.as_slice() else { panic!("{records:?}") };
    assert_eq!((&record.reach, record.answered_at_ms, record.stale), (&refused(), answered.answered_at_ms, true));
    assert!(!answered.stale, "it was this run's as it was answered");
}

/// A database from before reach left it opens with its table gone, the repository as it was, and its reach not known,
/// so none is listed, and the first check as the hub starts reads it.
#[tokio::test]
async fn a_repository_from_before_opens_with_its_reach_not_known_until_the_first_check() {
    let dir = tempfile::tempdir().unwrap();
    let rows = r#"UPDATE repos SET url = 'https://github.com/cli/cli.git', url_to_read = 0 WHERE id = 'r';
        INSERT INTO github_reach (repo_id, reach, answered_at_ms)
            VALUES ('r', '{"state":"not_found","repository":"cli/cli","username":"ada"}', 5);"#;
    drop(store_from_before(dir.path(), BEFORE_REACH_OBSERVED, rows));
    let github = refusing().await;

    let hub = hub_calling_github_again(dir.path(), github.api());
    let before_a_check = listed(&hub, "r").await;
    let mut feed = hub.subscribe_feed();
    let (_credential, following) = watch::channel(account("ada", "a-token"));
    let _checking = checking(&hub, following);
    let checked = told(&mut feed).await;

    let db = Connection::open(dir.path().join("hub.db")).unwrap();
    let tables: Vec<String> = db
        .prepare("SELECT name FROM sqlite_schema WHERE name = 'github_reach'")
        .unwrap()
        .query_map([], OneValue::read)
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(tables, [] as [String; 0]);
    assert_eq!(
        hub.store.get_repo(&"r".into()).unwrap().origin.url().map(ToString::to_string).as_deref(),
        Some("https://github.com/cli/cli.git")
    );
    assert_eq!(before_a_check, (None, Vec::new()), "not known");
    assert_eq!(checked.reach, refused());
}

/// A repository whose address is still to be read, as one an older hub added, with no reach known, as on the first
/// start after reach left the database, can't be told which repository on GitHub it is, so the checks have a worker
/// on the hub's machine read its address as they start, and check it then, rather than once a worker next links.
#[tokio::test]
async fn as_the_checks_start_a_repository_whose_reach_and_address_arent_known_has_its_address_read_and_is_checked() {
    let github = refusing().await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    let address = RepoUrl::try_from("https://github.com/cli/cli.git".to_string()).unwrap();
    let here = WorkerLink::answering(move |request| {
        assert!(matches!(request, WorkerRequest::ReadRepoUrl(_)), "{request:?}");
        Some(Ok(WorkerResponse::RepoUrl(RepoUrlReply { url: Some(address.clone()), push: None })))
    });
    serving(&hub, &here);
    let mut feed = hub.subscribe_feed();
    let (_credential, following) = watch::channel(account("ada", "a-token"));

    let _checking = checking(&hub, following);
    let checked = told(&mut feed).await;

    assert_eq!(checked.reach, refused());
}

/// A repository's reach goes with it: a hub that starts on a database without the repository keeps none of it.
#[tokio::test]
async fn a_reach_is_forgotten_with_its_repository() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), crate::hub::tests::NO_GITHUB.to_string());
    let pushes = Reach::Pushes { repository: CLI, username: "ada".into() };
    hub.observation_read::<RepositoryReach>("gone".into(), pushes.clone(), 1_000).await.unwrap();
    hub.observation_read::<RepositoryReach>("r".into(), pushes, 1_000).await.unwrap();
    hub.observations_written().await;
    drop(hub);

    let again = hub_calling_github_again(dir.path(), crate::hub::tests::NO_GITHUB.to_string());

    let repos: Vec<RecordKey> =
        listed(&again, "r").await.1.iter().map(|reach| RecordKey::GithubReach { repo: reach.repo.clone() }).collect();
    assert_eq!(repos, [RecordKey::GithubReach { repo: "r".into() }]);
    assert!(!again.observations.keeps::<RepositoryReach>(&"gone".into()));
}
