use look::Work;
use protocol::model::{Checks, Local, MergeState, PrState, PullRequest, QueueState, Review, Standing};

use super::words::action;
use super::{Action, Ask, Lookup};

#[test]
fn what_waits_on_someone_else_offers_nothing_to_do() {
    let waiting = [
        Standing::Queued(QueueState::AwaitingChecks),
        Standing::ChecksRunning(Checks::default()),
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

#[test]
fn only_the_newest_answer_is_taken_and_a_failure_keeps_what_was_known() {
    let merged = PullRequest {
        number: 7,
        url: "https://example.com/pull/7".to_string(),
        title: "T".to_string(),
        state: PrState::Merged,
        is_draft: false,
        base: None,
        merge_state: MergeState::Unknown,
        review: Review::NotRequired,
        checks: Checks::default(),
        queue: None,
        left_queue: None,
        changes_requested_at_ms: None,
        local: Local::default(),
    };
    let mut lookup = Lookup::Looking;

    let failed = lookup.answer(true, Err(()));
    let first = lookup.clone();
    let answered = lookup.answer(true, Ok::<_, ()>(Some(merged)));
    let older = lookup.answer(false, Ok::<_, ()>(None));

    assert_eq!((failed, first), (false, Lookup::Looking), "a failure doesn't say there's none");
    assert!(answered && !older, "an older answer arriving after the newest isn't taken");
    assert_eq!(lookup.standing(), Some(Standing::Merged));
}
