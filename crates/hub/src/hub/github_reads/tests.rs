use std::time::Duration;

use github::{References, Script, StandIn};
use protocol::model::{
    ChecksRollup, GithubRepositoryName, IssueNumber, IssueState, Reference, ReferenceAsk, ReferenceKind, ReferenceRead,
};

use super::references::FRESH_FOR;
use crate::hub::tests::hub_reading_github;

fn number(number: u64) -> IssueNumber {
    IssueNumber::try_from(number).unwrap()
}

/// An open issue numbered `n` in `repository`.
fn issue(repository: &'static str, n: u64) -> Reference {
    Reference {
        repository: GithubRepositoryName::named(repository),
        number: number(n),
        kind: ReferenceKind::Issue { state: IssueState::Open },
        url: format!("https://github.com/{repository}/issues/{n}"),
        title: format!("Issue {n}"),
        author: Some("ada".into()),
        opened_at_ms: 1,
        comments: 0,
        labels: Vec::new(),
        body: String::new(),
    }
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

    let first = hub.references_read(&"w".into(), vec![own(1021), own(5)]).await.unwrap();
    let again = hub.references_read(&"w".into(), vec![own(5), own(1021)]).await.unwrap();

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

    let read = hub.references_read(&"w".into(), vec![named("other/tools"), own(7)]).await.unwrap();
    let again = hub.references_read(&"w".into(), vec![named("Example/Project")]).await.unwrap();

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
    assert_eq!(hub.references_read(&"w".into(), vec![own(1)]).await.unwrap(), [found(&issue("example/project", 1))]);

    let closed = Reference { kind: ReferenceKind::Issue { state: IssueState::Done }, ..issue("example/project", 1) };
    references.set(closed.clone());
    hub.github_cache.age(FRESH_FOR + Duration::from_secs(1));
    let read = hub.references_read(&"w".into(), vec![own(1)]).await.unwrap();
    assert_eq!(read, [found(&closed)]);

    references.fail(true);
    hub.github_cache.age(FRESH_FOR + Duration::from_secs(1));
    let failed = hub.references_read(&"w".into(), vec![own(1), own(2)]).await.unwrap();
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
    let read = hub.references_read(&"w".into(), vec![own(1)]).await.unwrap();
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

    assert_eq!(first.await.unwrap(), [found(&issue("example/project", 1))]);
    assert_eq!(second.await.unwrap(), [found(&issue("example/project", 1))]);
    assert_eq!(other.await.unwrap(), [found(&issue("example/project", 2))]);
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
    let ReferenceKind::PullRequest(pull_request) = reference.kind else { panic!("{reference:?}") };
    assert_eq!((reference.title.as_str(), pull_request.checks), ("Change 7", Some(ChecksRollup::Failed)));
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
    assert_eq!(again.checks.pending, 1, "the check main's rules require, as they were read");
    hub.github_cache.age(super::reading::RULES_FRESH_FOR);
    hub.workspace_pull_request("w".into()).await.unwrap();
    assert_eq!(rules_read(), 2);
}
