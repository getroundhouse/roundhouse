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
