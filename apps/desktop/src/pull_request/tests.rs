use look::Work;
use protocol::model::{Checks, Local, MergeState, PrState, PullRequest, QueueState, Review, Standing};

use super::words::{action, title, work};
use super::{Action, Ask, Lookup};

#[test]
fn the_bar_says_how_many_and_of_what() {
    let failing = Checks { passed: 6, failed: 2, pending: 1 };

    assert_eq!(title(Standing::ChecksFailed(failing), Some("main")), "2 checks failed");
    assert_eq!(
        title(Standing::ChecksRunning(Checks { pending: 1, ..Checks::default() }), Some("main")),
        "1 check running"
    );
    assert_eq!(title(Standing::Unpushed(3), Some("main")), "3 commits to push");
    assert_eq!(title(Standing::Uncommitted(1), Some("main")), "1 uncommitted file");
    assert_eq!(title(Standing::Conflicts, Some("trunk")), "Conflicts with trunk");
    assert_eq!(title(Standing::Conflicts, None), "Conflicts with its base");
}

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
fn only_a_pull_request_that_can_merge_now_is_shown_as_ready() {
    assert_eq!(work(Standing::Ready), Work::Ready);
    assert_eq!(work(Standing::ChecksRunning(Checks::default())), Work::Open);
    assert_eq!(work(Standing::Queued(QueueState::Mergeable)), Work::Queued);
    assert_eq!(work(Standing::Draft), Work::Draft);
    assert_eq!(work(Standing::Unknown), Work::Open);
    // Until it's known, nothing is offered, and the mark shows no pull request.
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

/// A wait goes by what GitHub says, so its line says only that: not the worktree's uncommitted files, nor the bar's
/// word to see it on GitHub, and nothing once it's done, which ends a wait. A wait until the checks goes on through a
/// draft, so for one it says how they stand, while that says anything.
#[test]
fn a_waits_line_says_where_the_pull_request_stands_on_github_alone() {
    let open = PullRequest {
        number: 7,
        url: "https://example.com/pull/7".to_string(),
        title: "T".to_string(),
        state: PrState::Open,
        is_draft: false,
        base: Some("main".into()),
        merge_state: MergeState::Blocked,
        review: Review::Required,
        checks: Checks { passed: 3, failed: 0, pending: 2 },
        queue: None,
        left_queue: None,
        changes_requested_at_ms: None,
        local: Local { uncommitted: 4, ahead: 0, behind: 0 },
    };
    let said = |pr: PullRequest| Lookup::Found(Some(pr)).for_a_wait().map(|standing| (standing.number, standing.words));

    assert_eq!(said(open.clone()), Some((7, "2 checks running".to_string())), "the files aren't GitHub's");
    assert_eq!(
        said(PullRequest { queue: Some(QueueState::AwaitingChecks), ..open.clone() }).unwrap().1,
        "Running checks in the queue"
    );
    let draft = PullRequest { is_draft: true, merge_state: MergeState::Draft, ..open.clone() };
    assert_eq!(said(draft.clone()).unwrap().1, "2 checks running", "a draft's checks");
    let failing = Checks { failed: 1, ..open.checks };
    assert_eq!(said(PullRequest { checks: failing, ..draft.clone() }).unwrap().1, "1 check failed");
    assert_eq!(said(PullRequest { checks: Checks::default(), ..draft }).unwrap().1, "Draft", "none running");
    for nothing in [
        PullRequest { state: PrState::Merged, ..open.clone() },
        PullRequest { checks: Checks::default(), review: Review::Unknown, ..open.clone() },
    ] {
        assert_eq!(said(nothing.clone()), None, "{nothing:?}");
    }
    assert_eq!((Lookup::Looking.for_a_wait(), Lookup::Found(None).for_a_wait()), (None, None));
}
