//! Asking stand-ins for workers who their machines' own `gh` is logged in as, as each link to them opens.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{FoundGithubLogin, GhLogin, GhLoginWorks, GithubToken, ProviderName};
use protocol::worker::{FoundToken, GithubLoginReply, WorkerRequest, WorkerResponse};
use tokio::sync::broadcast;

use super::FOUND_WITHIN;
use crate::github::{Answer as GithubAnswer, StandIn};
use crate::hub::Hub;
use crate::hub::tests::{hub_calling_github, hub_in};
use crate::worker_link::WorkerLink;

type Answer = Option<Result<WorkerResponse, ApiError>>;

/// A stand-in for a worker that answers each ask with `answer`, or never, for `None`, and counts how often it's asked.
fn worker_answering(answer: Answer) -> (Arc<WorkerLink>, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&asked);
    let link = WorkerLink::answering(move |request| {
        assert!(matches!(request, WorkerRequest::FindGithubLogin), "{request:?}");
        counted.fetch_add(1, Ordering::SeqCst);
        answer.clone()
    });
    (link, asked)
}

/// What a worker that found `login` answers, as one from before it handed the hub the login's token does.
fn found(login: GhLogin) -> WorkerResponse {
    WorkerResponse::GithubLogin(GithubLoginReply { login, token: None })
}

fn ada() -> GhLogin {
    GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Yes }
}

/// The next found login the feed tells of.
async fn told(feed: &mut broadcast::Receiver<HubNotice>) -> FoundGithubLogin {
    let found = async {
        loop {
            if let HubNotice::FoundGithubLogin(found) = feed.recv().await.unwrap() {
                return found;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), found).await.expect("the feed told of no found login")
}

/// Once the hub has done with `provider`'s ask, which it has once it's no longer asking, or doesn't need to.
async fn done_asking(hub: &Hub, provider: &ProviderName, answered: bool) {
    waiting::task::until(FOUND_WITHIN + Duration::from_secs(1), "the hub was still asking", || {
        hub.github_logins_asked.has(provider) == answered
    })
    .await;
}

#[tokio::test]
async fn the_first_link_to_each_provider_is_asked_and_what_it_found_recorded_and_announced() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut feed = hub.subscribe_feed();
    let (first, asked_first) = worker_answering(Some(Ok(found(ada()))));
    let (second, asked_second) = worker_answering(Some(Ok(found(GhLogin::NotInstalled))));

    hub.find_github_login("local".into(), &first).await;
    let local = told(&mut feed).await;
    hub.find_github_login("local".into(), &second).await;
    hub.find_github_login("incus".into(), &second).await;
    let incus = told(&mut feed).await;

    assert_eq!((local.provider.as_str(), &local.login), ("local", &ada()));
    assert_eq!((incus.provider.as_str(), &incus.login), ("incus", &GhLogin::NotInstalled));
    assert!(incus.rev > local.rev);
    assert_eq!((asked_first.load(Ordering::SeqCst), asked_second.load(Ordering::SeqCst)), (1, 1), "once a provider");
    assert_eq!(hub.records_list().await.unwrap().found_github_logins, [incus, local]);
}

/// Whatever the worker said of it, which a worker that hands the hub a token can't tell. One GitHub refused isn't kept to
/// read pull requests with.
#[tokio::test]
async fn a_found_login_works_as_github_answers_the_hub_for_the_token_its_worker_handed_it() {
    let dir = tempfile::tempdir().unwrap();
    let github = StandIn::start(vec![("ada-token", GithubAnswer::user(7, "ada", None))]).await;
    let hub = hub_calling_github(dir.path(), github.api());
    let mut feed = hub.subscribe_feed();
    let handing = |token: &str| {
        let login = GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::CouldntTell };
        let token = Some(FoundToken { token: GithubToken::new(token), renews: false });
        worker_answering(Some(Ok(WorkerResponse::GithubLogin(GithubLoginReply { login, token })))).0
    };

    hub.find_github_login("local".into(), &handing("ada-token")).await;
    let local = told(&mut feed).await;
    hub.find_github_login("incus".into(), &handing("revoked")).await;
    let incus = told(&mut feed).await;

    assert_eq!(local.login, ada());
    assert_eq!(incus.login, GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Refused });
    let (local_kept, incus_kept) = (ProviderName::from("local"), ProviderName::from("incus"));
    let local_kept = hub.found_tokens.of(&local_kept, Duration::ZERO).await;
    let incus_kept = hub.found_tokens.of(&incus_kept, Duration::ZERO).await;
    assert_eq!(local_kept.map(|found| found.token), Some(GithubToken::new("ada-token")));
    assert_eq!(incus_kept, None, "the token GitHub refused");
    assert!(github.asked().iter().all(|asked| asked.starts_with("get /user ")), "{:?}", github.asked());
}

/// One from before finding, one that can't tell, one that finds a kind of login this hub doesn't know, and one that
/// never answers, each of which the next link to the provider is asked after.
#[tokio::test(start_paused = true)]
async fn a_worker_that_doesnt_say_leaves_the_provider_to_the_next_link() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut feed = hub.subscribe_feed();
    let local = ProviderName::from("local");
    let unsupported = Some(Err(ApiError::new(ErrorCode::Unsupported, "a request this worker doesn't know")));
    let cant_tell = Some(Err(ApiError::new(ErrorCode::Worker, "couldn't tell who gh is logged in to github.com as")));

    for answer in [unsupported, cant_tell, Some(Ok(found(GhLogin::Unknown))), None] {
        let (link, asked) = worker_answering(answer.clone());
        hub.find_github_login(local.clone(), &link).await;
        done_asking(&hub, &local, false).await;
        assert_eq!(asked.load(Ordering::SeqCst), 1, "{answer:?}");
    }
    let (link, _) = worker_answering(Some(Ok(found(GhLogin::LoggedOut))));
    hub.find_github_login(local.clone(), &link).await;
    let found = told(&mut feed).await;

    assert_eq!(found.login, GhLogin::LoggedOut);
    assert_eq!(hub.records_list().await.unwrap().found_github_logins, [found]);
}

/// Two links to one provider opening at once, as when two workspaces' workers start together.
#[tokio::test(start_paused = true)]
async fn a_provider_is_asked_by_one_link_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let local = ProviderName::from("local");
    let (never, asked_first) = worker_answering(None);
    let (second, asked_second) = worker_answering(Some(Ok(found(ada()))));

    hub.find_github_login(local.clone(), &never).await;
    hub.find_github_login(local.clone(), &second).await;
    never.kill();
    done_asking(&hub, &local, false).await;

    assert_eq!((asked_first.load(Ordering::SeqCst), asked_second.load(Ordering::SeqCst)), (1, 0));
}
