use client_core::records::WorkspacePullRequest;
use look::Work;
use protocol::model::{
    CheckCounts, Local, Observed, OwnSummary, PullRequestDetail, PullRequestSaid, QueueState, Standing,
};

use super::words::action;
use super::{Action, Ask, Lookup};

#[test]
fn what_waits_on_someone_else_offers_nothing_to_do() {
    let waiting = [
        Standing::Queued(QueueState::AwaitingChecks),
        Standing::ChecksRunning(CheckCounts::default()),
        Standing::Reading,
        Standing::ReviewRequired,
        Standing::Blocked,
        Standing::Checking,
        Standing::Unknown,
    ];

    for standing in waiting {
        assert_eq!(action(standing), None, "{standing:?}");
    }
    assert_eq!(action(Standing::Ready), Some(Action::Ask(Ask::Merge)));
    assert_eq!(action(Standing::Merged), Some(Action::Archive));
    assert_eq!(action(Standing::NoPullRequest), Some(Action::Ask(Ask::CreatePr)));
}

#[test]
fn until_the_pull_request_is_known_nothing_is_offered_and_the_mark_shows_none() {
    assert_eq!((Lookup::Looking.standing(), Lookup::Looking.work()), (None, Work::NoPr));
    assert_eq!(Lookup::Found(None).standing(), Some(Standing::NoPullRequest));
}

/// GitHub not showing the repository stands in for none found, and for not having looked, since neither is known then,
/// but not for a pull request found before, which is still the workspace's.
#[test]
fn an_unseen_repository_hides_only_what_isn_t_known() {
    let unseen = protocol::model::RepositoryUnseen {
        repository: protocol::model::GithubRepositoryName::named("o/r"),
        username: "ada".into(),
        login: protocol::model::GithubLoginKind::Token,
        installation: None,
        said: "Could not resolve to a Repository with the name 'o/r'.".into(),
        approve_at: None,
    };
    let pull_request = protocol::fixtures::pull_request(7);
    let own = OwnSummary { pull_request: PullRequestSaid::of(&pull_request), local: Local::default() };
    let found = WorkspacePullRequest { own: &own, detail: Observed::NotKnown };
    let (unseen, seen) = (Observed::AsOf { value: Some(unseen), as_of_ms: 1 }, Observed::Current { value: None });

    let lookups = [
        Lookup::of(Observed::NotKnown, &unseen),
        Lookup::of(Observed::Current { value: None }, &unseen),
        Lookup::of(Observed::AsOf { value: Some(found.clone()), as_of_ms: 1 }, &unseen),
        Lookup::of(Observed::NotKnown, &seen),
        Lookup::of(Observed::Current { value: None }, &Observed::NotKnown),
    ];

    let Observed::AsOf { value: Some(unseen), .. } = &unseen else { unreachable!() };
    let expected = [
        Lookup::Unseen(unseen),
        Lookup::Unseen(unseen),
        Lookup::Found(Some(found)),
        Lookup::Looking,
        Lookup::Found(None),
    ];
    assert_eq!(lookups, expected);
    assert_eq!((lookups[0].standing(), lookups[0].work()), (None, Work::NoPr), "nothing to judge, and no colour");
}

/// What's in the way of merging it is read from its checks, which until they're known are being read.
#[test]
fn the_standing_takes_the_checks_from_the_detail_where_they_re_known() {
    let pull_request = protocol::fixtures::pull_request(7);
    let own = OwnSummary { pull_request: PullRequestSaid::of(&pull_request), local: Local::default() };
    let detail = PullRequestDetail::of(&pull_request);
    let standing = |detail| Lookup::Found(Some(WorkspacePullRequest { own: &own, detail })).standing();
    let running = CheckCounts { passed: 0, failed: 0, pending: 1 };

    assert_eq!(standing(Observed::NotKnown), Some(Standing::Reading));
    assert_eq!(standing(Observed::AsOf { value: &detail, as_of_ms: 1 }), Some(Standing::ChecksRunning(running)));
}
