use look::Work;
use protocol::model::{CheckCounts, QueueState, Standing};

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
    let own = protocol::model::OwnPullRequest {
        pull_request: protocol::fixtures::pull_request(7),
        local: protocol::model::Local::default(),
    };
    let none = protocol::model::PullRequestFound { pull_request: None };
    let found = protocol::model::PullRequestFound { pull_request: Some(own.clone()) };

    let lookups = [
        Lookup::of(None, Some(unseen.clone())),
        Lookup::of(Some(none), Some(unseen.clone())),
        Lookup::of(Some(found), Some(unseen.clone())),
    ];

    let unseen = Lookup::Unseen(Box::new(unseen));
    assert_eq!(lookups, [unseen.clone(), unseen, Lookup::Found(Some(Box::new(own)))]);
    assert_eq!((lookups[0].standing(), lookups[0].work()), (None, Work::NoPr), "nothing to judge, and no colour");
}
