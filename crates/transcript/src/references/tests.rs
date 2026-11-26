use look::{ChipKind, GithubState, PullRequestState};
use protocol::model::{PrState, PullRequestSaid, QueueState, ReferenceSaid};

use super::card::said_of;
use super::drawing::kind_and_state;

/// A pull request in `state`, a draft or not, at `queue` in the merge queue or out of it.
fn pull_request(state: PrState, is_draft: bool, queue: Option<QueueState>) -> ReferenceSaid {
    let mut pull_request = PullRequestSaid::of(&protocol::fixtures::pull_request(1021));
    let said = &mut pull_request.summary;
    (said.state, said.is_draft, said.queue) = (state, is_draft, queue);
    ReferenceSaid::PullRequest(pull_request)
}

/// Merged and closed come before the queue, and the queue before whether it's a draft, as the bar takes them, and a
/// state this version doesn't know is drawn as the link was written.
#[test]
fn a_pull_request_in_the_merge_queue_is_drawn_queued_until_it_is_done() {
    let queued = Some(QueueState::AwaitingChecks);
    let cases = [
        ((PrState::Open, false, None), Some(PullRequestState::Open)),
        ((PrState::Open, true, None), Some(PullRequestState::Draft)),
        ((PrState::Open, false, queued), Some(PullRequestState::Queued)),
        ((PrState::Open, true, queued), Some(PullRequestState::Queued)),
        ((PrState::Merged, false, queued), Some(PullRequestState::Merged)),
        ((PrState::Closed, false, queued), Some(PullRequestState::Closed)),
        ((PrState::Unknown, false, queued), None),
    ];

    let drawn = cases.map(|((state, is_draft, queue), _)| kind_and_state(&pull_request(state, is_draft, queue)));

    let expected = cases.map(|(_, state)| state.map(|state| (ChipKind::PullRequest, GithubState::from(state))));
    assert_eq!(drawn, expected);
}

/// A card says where a pull request is in the merge queue as the bar does, and where one that's out of it stands.
#[test]
fn a_cards_words_for_a_pull_request_in_the_merge_queue_are_the_bars() {
    let queued = GithubState::from(PullRequestState::Queued);
    let cases = [
        ((queued, Some(QueueState::AwaitingChecks)), "Running checks in the queue"),
        ((queued, Some(QueueState::Unmergeable)), "Failed in the merge queue"),
        ((queued, Some(QueueState::Mergeable)), "Queued to merge"),
        ((GithubState::from(PullRequestState::Open), None), "Open"),
    ];

    let said = cases.map(|((state, queue), _)| said_of(state, queue));

    assert_eq!(said, cases.map(|(_, word)| ("Pull request", word)));
}
