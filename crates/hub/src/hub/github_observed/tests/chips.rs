//! What GitHub says of a chip's issue or pull request, worked out as an observation from what the cache keeps, with a
//! stand-in for GitHub.

use std::sync::Arc;
use std::time::Duration;

use github::References;
use protocol::client::{HubNotice, ObservationNotice};
use protocol::model::{
    About, GithubRepositoryName, Issue, IssueNumber, IssueSaid, IssueState, IssueSummary, Observation, ObservationKey,
    Observed, Read, Reference, ReferenceAsk, ReferenceSaid,
};
use tokio::sync::broadcast;

use crate::hub::tests::hub_on_github_with;
use crate::hub::{Followed, FollowerId, Hub};

/// Long enough for anything here on a busy machine.
const WAIT: Duration = Duration::from_secs(10);

/// Older than any read of a chip is kept fresh for, so the next ask reads it again.
const STALE: Duration = Duration::from_mins(5);

/// Longer than the cache keeps what nothing asks for.
const FORGOTTEN: Duration = Duration::from_hours(24 * 8);

fn number(n: u64) -> IssueNumber {
    IssueNumber::try_from(n).unwrap()
}

/// The issue or pull request numbered `n` in `example/project`, the repository the hub's test workspace is on.
fn own(n: u64) -> ReferenceAsk {
    ReferenceAsk { repository: GithubRepositoryName::named("example/project"), number: number(n) }
}

/// What GitHub says of an open issue numbered `n` in `example/project`, titled `title`.
fn summary(title: &str, n: u64) -> IssueSummary {
    let about = About {
        url: format!("https://github.com/example/project/issues/{n}"),
        title: title.to_string(),
        author: Some("ada".into()),
        opened_at_ms: 1,
        comments: 0,
        labels: Vec::new(),
        body: String::new(),
    };
    IssueSummary { about, state: IssueState::Open }
}

/// That issue, as the stand-in answers for it.
fn issue(n: u64, title: &str) -> Reference {
    Reference::Issue(Issue {
        repository: GithubRepositoryName::named("example/project"),
        number: number(n),
        summary: Read { said: summary(title, n), read_at_ms: 1 },
    })
}

/// That issue, as its observation says it.
fn said(n: u64, title: &str) -> ReferenceSaid {
    ReferenceSaid::Issue(IssueSaid {
        repository: GithubRepositoryName::named("example/project"),
        number: number(n),
        summary: summary(title, n),
    })
}

/// A hub reading the stand-in's `references`, working out what's sent of GitHub as it serves.
async fn observing(dir: &std::path::Path, references: &References) -> Arc<Hub> {
    let (hub, _, _) = hub_on_github_with(dir, references).await;
    tokio::spawn(hub.clone().observe_github());
    hub
}

/// Has the hub read `asks` again now, as a reply that links to them does, however fresh what it has of them is.
async fn read_again(hub: &Hub, asks: &[ReferenceAsk]) {
    hub.github_cache.age(STALE);
    hub.references_read(&"w".into(), asks.to_vec()).await.unwrap();
}

/// The next notice the feed tells of an observation.
async fn next(feed: &mut broadcast::Receiver<std::sync::Arc<HubNotice>>) -> ObservationNotice {
    let next = async {
        loop {
            if let Ok(notice) = ObservationNotice::try_from(std::sync::Arc::unwrap_or_clone(feed.recv().await.unwrap()))
            {
                return notice;
            }
        }
    };
    tokio::time::timeout(WAIT, next).await.expect("the feed told of an observation")
}

/// The next copy the feed tells of `ask`'s observation, as it says it, with its revision.
async fn next_of(
    feed: &mut broadcast::Receiver<std::sync::Arc<HubNotice>>,
    ask: &ReferenceAsk,
) -> (Observed<ReferenceSaid>, u64) {
    let ObservationNotice::Copy(Observation::Reference { reference, said, rev }) = next(feed).await else {
        panic!("not a copy of a chip's")
    };
    assert_eq!(reference, *ask);
    (said, rev.get())
}

fn current(value: ReferenceSaid) -> Observed<ReferenceSaid> {
    Observed::Current { value }
}

/// A chip that something follows is read, and is current from that read, under a new revision each time GitHub says
/// something else of it, and none when it says the same. A read that fails makes it as of the read before, until the
/// next that doesn't, and once nothing follows it, it's as of its last read.
#[tokio::test]
async fn a_followed_chip_is_current_from_its_read_and_moves_only_as_github_says_something_else() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue(12, "Decide"));
    let hub = observing(dir.path(), &references).await;
    let mut feed = hub.subscribe_feed();

    let following = hub.follow(FollowerId { connection: 1, subscription: 1 }, [Followed::Reference(own(12))]);

    let (first, first_rev) = next_of(&mut feed, &own(12)).await;
    assert_eq!(first, current(said(12, "Decide")));

    read_again(&hub, &[own(12)]).await;
    references.set(issue(12, "Decided"));
    read_again(&hub, &[own(12)]).await;

    let (changed, changed_rev) = next_of(&mut feed, &own(12)).await;
    assert_eq!(changed, current(said(12, "Decided")), "the read that found the same told nothing");
    assert!(changed_rev > first_rev);

    references.fail(true);
    read_again(&hub, &[own(12)]).await;

    let (failed, _) = next_of(&mut feed, &own(12)).await;
    let Observed::AsOf { value, .. } = failed else { panic!("{failed:?}") };
    assert_eq!(value, said(12, "Decided"));

    references.fail(false);
    read_again(&hub, &[own(12)]).await;

    assert_eq!(next_of(&mut feed, &own(12)).await.0, current(said(12, "Decided")));

    drop(following);

    let (let_go, _) = next_of(&mut feed, &own(12)).await;
    assert!(matches!(let_go, Observed::AsOf { ref value, .. } if *value == said(12, "Decided")), "{let_go:?}");
}

/// What the cache has of a chip from before anything followed it is as of when it was read, until the read that
/// following it starts. One that GitHub has none of says so.
#[tokio::test]
async fn a_chip_read_before_it_was_followed_is_as_of_that_read_and_a_missing_one_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue(12, "Decide"));
    let hub = observing(dir.path(), &references).await;
    hub.references_read(&"w".into(), vec![own(12), own(13)]).await.unwrap();
    let mut feed = hub.subscribe_feed();

    let _twelve = hub.follow(FollowerId { connection: 1, subscription: 1 }, [Followed::Reference(own(12))]);

    let (before, _) = next_of(&mut feed, &own(12)).await;

    assert!(matches!(before, Observed::AsOf { ref value, .. } if *value == said(12, "Decide")), "{before:?}");
    read_again(&hub, &[own(12)]).await;
    let (now, _) = next_of(&mut feed, &own(12)).await;
    assert_eq!(now, current(said(12, "Decide")));

    let _thirteen = hub.follow(FollowerId { connection: 1, subscription: 2 }, [Followed::Reference(own(13))]);

    let (missing, _) = next_of(&mut feed, &own(13)).await;
    assert!(
        matches!(
            missing,
            Observed::AsOf { value: ReferenceSaid::Missing, .. } | Observed::Current { value: ReferenceSaid::Missing }
        ),
        "{missing:?}"
    );
}

/// Once the cache forgets a chip, as it does what nothing has asked for in a week, its observation is gone.
#[tokio::test]
async fn a_chip_the_cache_forgets_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue(12, "Decide"));
    references.set(issue(14, "Another"));
    let hub = observing(dir.path(), &references).await;
    let mut feed = hub.subscribe_feed();
    let following = hub.follow(FollowerId { connection: 1, subscription: 1 }, [Followed::Reference(own(12))]);
    next_of(&mut feed, &own(12)).await;
    drop(following);
    next_of(&mut feed, &own(12)).await;

    hub.github_cache.age(FORGOTTEN);
    hub.references_read(&"w".into(), vec![own(14)]).await.unwrap();

    let ObservationNotice::Removed { observation, .. } = next(&mut feed).await else { panic!("not a removal") };
    assert_eq!(observation, ObservationKey::Reference { reference: own(12) });
}

/// A hub that starts again on a cache that has nothing of a chip it kept an observation of, as once the cache forgot
/// it while no hub was there to hear, removes that observation.
#[tokio::test]
async fn a_hub_that_starts_again_removes_what_its_cache_no_longer_has() {
    use crate::hub::observations::Referenced;
    use crate::hub::tests::{hub_in, hub_in_again};

    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_worked_out::<Referenced>(own(12), ReferenceSaid::Missing, 1_000).await.unwrap();
    drop(hub);
    let again = hub_in_again(dir.path());
    let mut feed = again.subscribe_feed();

    tokio::spawn(again.clone().observe_github());

    let ObservationNotice::Removed { observation, .. } = next(&mut feed).await else { panic!("not a removal") };
    assert_eq!(observation, ObservationKey::Reference { reference: own(12) });
}
