use protocol::model::{Local, PrState, Review, Summary};

use super::*;

/// An open pull request that's ready to merge, with four checks passed, for the tests of the tools that describe one.
pub(in crate::tools) fn open_pr() -> OwnPullRequest {
    let mut pull_request = protocol::fixtures::pull_request(605);
    let said = &mut pull_request.summary.said;
    said.about.url = "https://github.com/o/r/pull/605".into();
    said.about.title = "Wait on pull requests".into();
    said.review = Review::Approved;
    let own = OwnPullRequest { pull_request, local: Local::default() };
    checked(own, 4, 0, 0)
}

/// `own` with this many checks passed, failed and still running.
pub(in crate::tools) fn checked(own: OwnPullRequest, passed: usize, failed: usize, pending: usize) -> OwnPullRequest {
    OwnPullRequest { pull_request: protocol::fixtures::checked(own.pull_request, passed, failed, pending), ..own }
}

/// `own` with its summary changed by `change`.
pub(in crate::tools) fn with(mut own: OwnPullRequest, change: impl FnOnce(&mut Summary)) -> OwnPullRequest {
    change(&mut own.pull_request.summary.said);
    own
}

#[test]
fn a_queued_pull_request_is_said_not_to_be_merged_yet() {
    for queue in [QueueState::AwaitingChecks, QueueState::Locked, QueueState::Mergeable, QueueState::Queued] {
        let text = describe(Some(&with(open_pr(), |said| said.queue = Some(queue))));

        assert!(text.contains("in the merge queue"), "{queue:?}: {text}");
        assert!(text.contains("isn't merged yet"), "{queue:?}: {text}");
    }
}

#[test]
fn it_says_what_it_is_where_it_stands_and_what_github_and_the_worktree_say() {
    let pr = OwnPullRequest { local: Local { uncommitted: 0, ahead: 1, behind: 0 }, ..checked(open_pr(), 3, 1, 2) };

    let text = describe(Some(&pr));

    assert_eq!(
        text,
        "Pull request #605, \"Wait on pull requests\", into main: https://github.com/o/r/pull/605\n\
         1 commit here isn't on the pull request yet, so what GitHub says is about an older commit.\n\
         Checks on its newest commit: 3 passed, 1 failed, 2 running.\n\
         Review: approved.\n\
         GitHub's merge state: CLEAN.\n\
         This worktree: 0 files changed and not committed, 1 commit not pushed, and 0 commits on the pull request \
         that aren't here."
    );
}

/// Its checks not read yet, as just after a chip's read found it, it says so rather than none.
#[test]
fn checks_not_read_yet_are_said_to_be() {
    let mut pr = open_pr();
    pr.pull_request.checks = None;

    let text = describe(Some(&pr));

    assert!(text.contains("Roundhouse hasn't read its checks yet"), "{text}");
    assert!(text.contains("Checks on its newest commit: not read yet."), "{text}");
}

#[test]
fn a_pull_request_thats_done_says_only_so() {
    let merged = describe(Some(&with(open_pr(), |said| said.state = PrState::Merged)));
    let closed = describe(Some(&with(open_pr(), |said| said.state = PrState::Closed)));

    assert!(merged.ends_with("\nIt's merged."), "{merged}");
    assert!(closed.ends_with("\nIt's closed, without being merged."), "{closed}");
}

#[test]
fn a_branch_without_one_says_so() {
    assert_eq!(describe(None), "This workspace's branch has no pull request on GitHub.");
}

#[test]
fn checks_still_running_say_how_many_are_done() {
    let text = describe(Some(&checked(open_pr(), 2, 0, 3)));

    assert!(text.contains("Its checks are still running: 2 of 5 are done."), "{text}");
}
