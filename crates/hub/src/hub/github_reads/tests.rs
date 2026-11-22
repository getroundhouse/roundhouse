use std::time::Duration;

use github::{References, Script, StandIn};
use protocol::model::{
    About, ChecksRollup, GithubRepositoryName, Issue, IssueNumber, IssueState, IssueSummary, Read, Reference,
    ReferenceAsk, ReferenceRead,
};

use super::ShownBy;
use super::references::FRESH_FOR;
use crate::hub::Hub;
use crate::hub::tests::hub_reading_github;
use protocol::client::github::GithubShown;

fn number(number: u64) -> IssueNumber {
    IssueNumber::try_from(number).unwrap()
}

/// Issue `n` in `repository`, titled `title`, where `state` says, read at 1.
fn titled(repository: &'static str, n: u64, title: &str, state: IssueState) -> Reference {
    let about = About {
        url: format!("https://github.com/{repository}/issues/{n}"),
        title: title.to_string(),
        author: Some("ada".into()),
        opened_at_ms: 1,
        comments: 0,
        labels: Vec::new(),
        body: String::new(),
    };
    Reference::Issue(Issue {
        repository: GithubRepositoryName::named(repository),
        number: number(n),
        summary: Read { said: IssueSummary { about, state }, read_at_ms: 1 },
    })
}

/// An open issue numbered `n` in `repository`, read at 1.
fn issue(repository: &'static str, n: u64) -> Reference {
    titled(repository, n, &format!("Issue {n}"), IssueState::Open)
}

/// `read` as if what it found was read at 1, as [`issue`]'s are, since a read takes the time it began.
fn at_one(read: ReferenceRead) -> ReferenceRead {
    let ReferenceRead::Found { mut reference } = read else { return read };
    match &mut *reference {
        Reference::Issue(issue) => issue.summary.read_at_ms = 1,
        Reference::PullRequest(pull_request) => pull_request.summary.read_at_ms = 1,
        Reference::Unknown => {}
    }
    ReferenceRead::Found { reference }
}

/// Each of `reads`, as [`at_one`] has it.
fn all_at_one(reads: Vec<ReferenceRead>) -> Vec<ReferenceRead> {
    reads.into_iter().map(at_one).collect()
}

/// The issue or pull request numbered `n` in `example/project`, the repository the hub's test workspace is on.
fn own(n: u64) -> ReferenceAsk {
    ReferenceAsk { repository: GithubRepositoryName::named("example/project"), number: number(n) }
}

fn found(reference: &Reference) -> ReferenceRead {
    ReferenceRead::Found { reference: Box::new(reference.clone()) }
}

async fn on_github(references: &References) -> StandIn {
    StandIn::with(Script { references: Some(references.clone()), ..Script::default() }).await
}

/// A second client showing the same chat, or the same client asking again, is answered from what the hub kept, so
/// GitHub is asked once, and what's missing is kept as missing.
#[tokio::test]
async fn what_a_chat_names_is_read_from_github_once_while_what_was_found_is_fresh() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue("example/project", 1021));
    let github = on_github(&references).await;
    let hub = hub_reading_github(dir.path(), github.api());

    let first = all_at_one(hub.references_read(&"w".into(), vec![own(1021), own(5)]).await.unwrap());
    let again = all_at_one(hub.references_read(&"w".into(), vec![own(5), own(1021)]).await.unwrap());

    assert_eq!(first, [found(&issue("example/project", 1021)), ReferenceRead::Missing]);
    assert_eq!(again, [ReferenceRead::Missing, found(&issue("example/project", 1021))]);
    assert_eq!(references.queries(), 1);
}

/// A reference is read in the repository it names, which needn't be the workspace's, and the same one named in
/// another case is the same issue to the hub, since GitHub's names ignore case.
#[tokio::test]
async fn a_reference_is_read_in_the_repository_it_names_whatever_its_case() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue("example/project", 7));
    references.set(issue("other/tools", 7));
    let github = on_github(&references).await;
    let hub = hub_reading_github(dir.path(), github.api());
    let named = |repository: &'static str| ReferenceAsk {
        repository: GithubRepositoryName::named(repository),
        number: number(7),
    };

    let read = all_at_one(hub.references_read(&"w".into(), vec![named("other/tools"), own(7)]).await.unwrap());
    let again = all_at_one(hub.references_read(&"w".into(), vec![named("Example/Project")]).await.unwrap());

    assert_eq!(read, [found(&issue("other/tools", 7)), found(&issue("example/project", 7))]);
    assert_eq!(again, [found(&issue("example/project", 7))]);
    assert_eq!(references.queries(), 1);
}

/// One read longer ago than it stays fresh is read again, so a pull request that merges while its chip is in view
/// says so, and one that couldn't be read says why while nothing was known of it, and what was known after.
#[tokio::test]
async fn what_was_found_is_read_again_once_its_stale_and_kept_when_github_fails() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue("example/project", 1));
    let github = on_github(&references).await;
    let hub = hub_reading_github(dir.path(), github.api());
    let first = all_at_one(hub.references_read(&"w".into(), vec![own(1)]).await.unwrap());
    assert_eq!(first, [found(&issue("example/project", 1))]);

    let closed = titled("example/project", 1, "Issue 1", IssueState::Done);
    references.set(closed.clone());
    hub.github_cache.age(FRESH_FOR + Duration::from_secs(1));
    let read = all_at_one(hub.references_read(&"w".into(), vec![own(1)]).await.unwrap());
    assert_eq!(read, [found(&closed)]);

    references.fail(true);
    hub.github_cache.age(FRESH_FOR + Duration::from_secs(1));
    let failed = all_at_one(hub.references_read(&"w".into(), vec![own(1), own(2)]).await.unwrap());
    assert_eq!(failed[0], found(&closed), "what was known before");
    assert!(matches!(failed[1], ReferenceRead::Unread { .. }), "{:?}", failed[1]);
    assert_eq!(references.queries(), 3);
}

/// A read that failed isn't tried again until it would have been read again had it worked, so a GitHub that's down
/// is asked no more often than one that answers, and every client is told why meanwhile.
#[tokio::test]
async fn a_read_that_failed_is_tried_again_only_once_it_would_be_read_again() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue("example/project", 1));
    references.fail(true);
    let github = on_github(&references).await;
    let hub = hub_reading_github(dir.path(), github.api());

    let failed = hub.references_read(&"w".into(), vec![own(1)]).await.unwrap();
    let again = hub.references_read(&"w".into(), vec![own(1)]).await.unwrap();
    assert!(matches!(&failed[..], [ReferenceRead::Unread { .. }]), "{failed:?}");
    assert_eq!(again, failed, "told why, without asking GitHub again");
    assert_eq!(references.queries(), 1);

    references.fail(false);
    hub.github_cache.age(FRESH_FOR + Duration::from_secs(1));
    let read = all_at_one(hub.references_read(&"w".into(), vec![own(1)]).await.unwrap());
    assert_eq!(read, [found(&issue("example/project", 1))]);
    assert_eq!(references.queries(), 2);
}

/// An ask for what another is reading waits for that read rather than asking GitHub again, and one for something
/// else isn't held up by it.
#[tokio::test]
async fn an_ask_waits_only_for_the_reads_of_what_it_asks_for() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue("example/project", 1));
    references.set(issue("example/project", 2));
    let github = on_github(&references).await;
    let hub = std::sync::Arc::new(hub_reading_github(dir.path(), github.api()));
    references.hold();
    let first = tokio::spawn({
        let hub = hub.clone();
        async move { hub.references_read(&"w".into(), vec![own(1)]).await.unwrap() }
    });
    while references.queries() == 0 {
        tokio::task::yield_now().await;
    }
    let second = tokio::spawn({
        let hub = hub.clone();
        async move { hub.references_read(&"w".into(), vec![own(1)]).await.unwrap() }
    });
    tokio::task::yield_now().await;
    assert!(!second.is_finished(), "waiting for the first read");

    let other = tokio::spawn({
        let hub = hub.clone();
        async move { hub.references_read(&"w".into(), vec![own(2)]).await.unwrap() }
    });
    let read_meanwhile = async {
        while references.queries() < 2 {
            tokio::task::yield_now().await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), read_meanwhile).await.expect("#2 read while #1 is held");
    references.release();

    assert_eq!(all_at_one(first.await.unwrap()), [found(&issue("example/project", 1))]);
    assert_eq!(all_at_one(second.await.unwrap()), [found(&issue("example/project", 1))]);
    assert_eq!(all_at_one(other.await.unwrap()), [found(&issue("example/project", 2))]);
    assert_eq!(references.queries(), 2, "one for #1, and one for #2 while #1 was held");
}

/// A chip of the workspace's own pull request says what the bar's read of it found, without GitHub being asked again,
/// so the two agree.
#[tokio::test]
async fn a_chip_of_a_pull_request_read_for_the_bar_is_what_that_read_found() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = crate::hub::tests::hub_on_github(dir.path()).await;
    pull_requests.set(7, github::PullRequestState::Failed);
    hub.workspace_pull_request("w".into()).await.unwrap();
    let asked = github.asked().len();

    let read = hub.references_read(&"w".into(), vec![own(7)]).await.unwrap();

    let [ReferenceRead::Found { reference }] = &read[..] else { panic!("{read:?}") };
    let Reference::PullRequest(pull_request) = &**reference else { panic!("{reference:?}") };
    let said = pull_request.said();
    assert_eq!((said.about.title.as_str(), said.rollup), ("Change 7", Some(ChecksRollup::Failed)));
    assert_eq!(github.asked().len(), asked, "GitHub wasn't asked again");
}

/// A branch's rules for required checks are read with the first read of a pull request into it, and taken as they were
/// for the reads after, until they're as old as they're kept for.
#[tokio::test]
async fn a_branchs_rules_are_read_again_only_once_theyre_stale() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = crate::hub::tests::hub_on_github(dir.path()).await;
    pull_requests.set(7, github::PullRequestState::Open);
    let rules_read = || github.asked().iter().filter(|asked| asked.contains("refUpdateRule")).count();

    hub.workspace_pull_request("w".into()).await.unwrap();
    let again = hub.workspace_pull_request("w".into()).await.unwrap().unwrap();

    assert_eq!(rules_read(), 1);
    let checks = again.pull_request.checks_said().unwrap().counts();
    assert_eq!(checks.pending, 1, "the check main's rules require, as they were read");
    hub.github_cache.age(super::reading::RULES_FRESH_FOR);
    hub.workspace_pull_request("w".into()).await.unwrap();
    assert_eq!(rules_read(), 2);
}

/// What a client shows as a chip is read again once it's as old as a client's ask takes, without the client asking,
/// for as long as it's shown.
#[tokio::test]
async fn what_a_client_shows_is_read_again_once_its_stale() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue("example/project", 12));
    let github = on_github(&references).await;
    let hub = hub_reading_github(dir.path(), github.api());
    let shown = GithubShown { references: vec![own(12)], ..GithubShown::default() };
    let showing = hub.show_github(ShownBy { connection: 1, subscription: 1 }, shown);
    let watching = tokio::spawn(hub.clone().watch_shown_references());
    let title = |hub: &Hub| match hub.reference_kept(&own(12)) {
        Some(ReferenceRead::Found { reference }) => reference.about().map(|about| about.title.clone()),
        _ => None,
    };
    waiting::task::until(Duration::from_secs(30), "the chip to be read", || title(&hub).is_some()).await;

    references.set(titled("example/project", 12, "Renamed", IssueState::Open));
    hub.github_cache.age(FRESH_FOR);
    waiting::task::until(Duration::from_secs(30), "the chip to be read again", || {
        title(&hub).as_deref() == Some("Renamed")
    })
    .await;

    watching.abort();
    drop(showing);
    assert_eq!(hub.github_shown.references(), [], "no longer shown, so not read");
}

/// A pull request the cache forgot is tried again only once the wait since it was last tried has passed, however many
/// ask, while another is tried at once, and the wait is twice as long after each try, up to the longest.
#[test]
fn a_forgotten_pull_request_is_tried_again_only_once_its_due_and_less_often_each_time() {
    let link = |n: u64| protocol::model::PullRequestLink {
        repository: GithubRepositoryName::named("o/r"),
        number: number(n),
        url: format!("https://github.com/o/r/pull/{n}"),
        uncommitted: 0,
    };
    let forgotten = super::Forgotten::default();
    let (again, start) = (Duration::from_secs(60), std::time::Instant::now());
    let due = |after: u64, numbers: &[u64]| {
        let links = numbers.iter().map(|n| link(*n)).collect();
        let due = forgotten.due(links, again, start + Duration::from_secs(after));
        due.iter().map(|link| link.number.get()).collect::<Vec<_>>()
    };

    assert_eq!(due(0, &[7, 7]), [7], "once, however many ask");
    assert_eq!(due(59, &[7, 8]), [8], "not 7 again yet");
    assert_eq!(due(60, &[7, 8]), [7], "7 is due, and 8 isn't yet");
    assert_eq!(due(60 + 119, &[7]), Vec::<u64>::new(), "twice as long after the second try");
    assert_eq!(due(60 + 120, &[7]), [7]);
    let mut at = 180;
    for wait in [240, 480, 960, 1800, 1800] {
        assert_eq!(due(at + wait - 1, &[7]), Vec::<u64>::new(), "not before {wait}s");
        assert_eq!(due(at + wait, &[7]), [7], "after {wait}s, half an hour at the longest");
        at += wait;
    }
}
