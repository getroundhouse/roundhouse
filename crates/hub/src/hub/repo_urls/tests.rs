//! Having stand-ins for workers read where repositories' checkouts fetch from, as links to them open and as a
//! repository is added.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{Repo, RepoOrigin, RepoUrl, Workspace};
use protocol::provider::WorkerEnvironment;
use protocol::worker::{ReadRepoUrl, RepoSource, RepoUrlReply, WorkerRequest, WorkerResponse};
use tokio::sync::broadcast;

use super::READ_WITHIN;
use crate::hub::Hub;
use crate::hub::tests::hub_in;
use crate::store::WorkspacePlacement;
use crate::worker_link::WorkerLink;

type Answer = Option<Result<WorkerResponse, ApiError>>;

fn roundhouse() -> RepoUrl {
    RepoUrl::try_from("https://github.com/ada/roundhouse.git".to_owned()).unwrap()
}

/// A stand-in for a worker that reads `url` from every checkout, as [`worker_answering`] says.
fn worker_reading(url: Option<RepoUrl>) -> (Arc<WorkerLink>, Arc<Mutex<Vec<String>>>) {
    worker_answering(Some(Ok(WorkerResponse::RepoUrl(RepoUrlReply { url, push: None }))))
}

/// A stand-in for a worker that answers each read with `answer`, or never, for `None`, and the checkouts it was asked
/// about, in turn.
fn worker_answering(answer: Answer) -> (Arc<WorkerLink>, Arc<Mutex<Vec<String>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&asked);
    let link = WorkerLink::answering(move |request| {
        let WorkerRequest::ReadRepoUrl(ReadRepoUrl { path }) = request else { panic!("{request:?}") };
        kept.lock().push(path);
        answer.clone()
    });
    (link, asked)
}

/// The next repository the feed tells of.
async fn told(feed: &mut broadcast::Receiver<HubNotice>) -> Repo {
    let repo = async {
        loop {
            if let HubNotice::Repo(repo) = feed.recv().await.unwrap() {
                return repo;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), repo).await.expect("the feed told of no repository")
}

/// Which repositories' addresses are still to be read, by ID.
fn to_read(hub: &Hub) -> Vec<String> {
    hub.store.repos_to_read().unwrap().into_iter().map(|repo| repo.id.to_string()).collect()
}

#[tokio::test]
async fn each_address_to_be_read_is_read_once_by_a_worker_on_this_machine_and_announced() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut feed = hub.subscribe_feed();
    let before = hub.store.get_repo(&"r".into()).unwrap();
    let (first, asked_first) = worker_reading(Some(roundhouse()));
    let (second, asked_second) = worker_reading(None);

    Arc::clone(&hub).read_repo_urls("w".into(), first).await;
    let linked = told(&mut feed).await;
    Arc::clone(&hub).read_repo_urls("w".into(), second).await;

    assert_eq!(linked.origin, RepoOrigin::Linked { path: "/repo".into(), url: roundhouse() });
    assert!(linked.rev > before.rev);
    assert_eq!((asked_first.lock().clone(), asked_second.lock().len()), (vec!["/repo".to_owned()], 0));
    assert_eq!(hub.records_list().await.unwrap().repos, [linked]);
    assert_eq!(to_read(&hub), [] as [String; 0]);
}

#[tokio::test]
async fn only_a_worker_whose_recorded_environment_shares_the_hubs_filesystem_is_asked() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    // Named `local`, yet placed on a separate filesystem: must never be asked to read hub checkouts.
    let separate = Workspace { provider: "local".into(), ..protocol::fixtures::workspace("sep") };
    let separate_placement = WorkspacePlacement {
        source: RepoSource::Clone { url: roundhouse().to_string() },
        environment: WorkerEnvironment::default(),
    };
    hub.store.insert_workspace(&separate, &separate_placement, None).unwrap();
    let (elsewhere, asked_elsewhere) = worker_reading(Some(roundhouse()));

    Arc::clone(&hub).read_repo_urls(separate.id.clone(), elsewhere).await;

    assert_eq!(asked_elsewhere.lock().len(), 0, "a separate-filesystem worker isn't asked even when named local");
    assert_eq!(to_read(&hub), ["r"]);

    // Named `recording` (not `local`), yet placed on the hub's filesystem: is asked.
    let on_hub = Workspace { provider: "recording".into(), ..protocol::fixtures::workspace("rec") };
    hub.store.insert_workspace(&on_hub, &WorkspacePlacement::local_checkout("/repo"), None).unwrap();
    let (local_adapter, asked_adapter) = worker_reading(Some(roundhouse()));

    Arc::clone(&hub).read_repo_urls(on_hub.id.clone(), local_adapter).await;

    assert_eq!(*asked_adapter.lock(), ["/repo"]);
    assert_eq!(to_read(&hub), [] as [String; 0]);
}

/// One from before reading addresses is asked once, whatever else is to be read, and one that fails or never answers
/// is asked about each. Each leaves the repository its checkout, for the next worker here to read.
#[tokio::test(start_paused = true)]
async fn a_worker_that_doesnt_say_leaves_the_checkout_to_be_read_by_the_next() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.store.add_repo(&"r2".into(), "second", "/second", 0).unwrap();
    let mut feed = hub.subscribe_feed();
    let unsupported = Some(Err(ApiError::new(ErrorCode::Unsupported, "a request this worker doesn't know")));
    let failed = Some(Err(ApiError::new(ErrorCode::Worker, "git couldn't read where /repo fetches from")));

    for (answer, asked_about) in [(unsupported, 1), (failed, 2), (None, 2)] {
        let (link, asked) = worker_answering(answer.clone());
        let started = tokio::time::Instant::now();
        Arc::clone(&hub).read_repo_urls("w".into(), link).await;
        assert_eq!(asked.lock().len(), asked_about, "{answer:?}");
        assert!(started.elapsed() <= READ_WITHIN * 2, "{answer:?}");
    }
    let unread = hub.records_list().await.unwrap().repos;
    let (link, _) = worker_reading(Some(roundhouse()));
    Arc::clone(&hub).read_repo_urls("w".into(), link).await;

    let checkouts: Vec<_> = unread.iter().map(|repo| &repo.origin).collect();
    let paths = [RepoOrigin::Checkout { path: "/repo".into() }, RepoOrigin::Checkout { path: "/second".into() }];
    assert_eq!(checkouts, paths.iter().collect::<Vec<_>>(), "the records were left as they were");
    let linked = [told(&mut feed).await, told(&mut feed).await];
    assert!(linked.iter().all(|repo| repo.origin.url() == Some(&roundhouse())), "{linked:?}");
    assert_eq!(to_read(&hub), [] as [String; 0]);
}

/// A directory with a `.git` in it, which is all adding a checkout looks for, since the hub runs no git.
fn checkout_in(dir: &std::path::Path) -> String {
    let checkout = dir.join("ada-checkout");
    std::fs::create_dir_all(checkout.join(".git")).unwrap();
    checkout.canonicalize().unwrap().to_string_lossy().into_owned()
}

#[tokio::test]
async fn adding_a_checkout_has_a_worker_on_this_machine_read_its_address_at_once_and_again_when_added_again() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    Arc::clone(&hub).read_repo_urls("w".into(), worker_reading(None).0).await;
    let (link, asked) = worker_reading(Some(roundhouse()));
    hub.workers.lock().insert("w".into(), link);
    let mut feed = hub.subscribe_feed();
    let checkout = checkout_in(dir.path());

    let added = hub.repo_add(checkout.clone()).await.unwrap();
    let said = (told(&mut feed).await, told(&mut feed).await);
    let again = hub.repo_add(checkout.clone()).await.unwrap();
    waiting::task::until(Duration::from_secs(5), "the worker wasn't asked again", || asked.lock().len() >= 2).await;

    assert_eq!(added.origin, RepoOrigin::Checkout { path: checkout.clone() });
    assert_eq!(said.0, added, "first as it's added");
    assert_eq!(said.1.origin, RepoOrigin::Linked { path: checkout.clone(), url: roundhouse() });
    assert_eq!(again, said.1);
    assert_eq!(*asked.lock(), [checkout.clone(), checkout]);
}

/// So with only workers elsewhere, the next worker here to link reads it.
#[tokio::test]
async fn adding_a_checkout_with_no_worker_on_this_machine_leaves_its_address_to_be_read() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let workspace = Workspace { provider: "incus".into(), ..protocol::fixtures::workspace("x") };
    let separate = WorkspacePlacement {
        source: RepoSource::Clone { url: roundhouse().to_string() },
        environment: WorkerEnvironment::default(),
    };
    hub.store.insert_workspace(&workspace, &separate, None).unwrap();
    hub.workers.lock().insert(workspace.id, worker_reading(Some(roundhouse())).0);

    let here = hub.worker_here().await;
    let added = hub.repo_add(checkout_in(dir.path())).await.unwrap();

    assert!(here.is_none(), "a worker elsewhere isn't one here");
    assert!(to_read(&hub).contains(&added.id.to_string()));
    let (local, _) = worker_answering(None);
    hub.workers.lock().insert("w".into(), Arc::clone(&local));
    assert!(hub.worker_here().await.is_some_and(|here| Arc::ptr_eq(&here, &local)), "and one here is");
}
