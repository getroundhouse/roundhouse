use protocol::model::{Local, MergeState, PrState};

use super::*;

/// An open pull request that's ready to merge, for the tests of the tools that describe one.
pub(in crate::tools) fn open_pr() -> PullRequest {
    PullRequest {
        number: 605,
        url: "https://github.com/o/r/pull/605".into(),
        title: "Wait on pull requests".into(),
        state: PrState::Open,
        is_draft: false,
        base: Some("main".into()),
        merge_state: MergeState::Clean,
        review: Review::Approved,
        checks: Checks { passed: 4, failed: 0, pending: 0 },
        queue: None,
        left_queue: None,
        changes_requested_at_ms: None,
        local: Local::default(),
    }
}

#[test]
fn a_queued_pull_request_is_said_not_to_be_merged_yet() {
    for queue in [QueueState::AwaitingChecks, QueueState::Locked, QueueState::Mergeable, QueueState::Queued] {
        let text = describe(Some(&PullRequest { queue: Some(queue), ..open_pr() }));

        assert!(text.contains("in the merge queue"), "{queue:?}: {text}");
        assert!(text.contains("isn't merged yet"), "{queue:?}: {text}");
    }
}

#[test]
fn it_says_what_it_is_where_it_stands_and_what_github_and_the_worktree_say() {
    let text = describe(Some(&PullRequest {
        checks: Checks { passed: 3, failed: 1, pending: 2 },
        local: Local { uncommitted: 0, ahead: 1, behind: 0 },
        ..open_pr()
    }));

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

#[test]
fn a_pull_request_thats_done_says_only_so() {
    let merged = describe(Some(&PullRequest { state: PrState::Merged, ..open_pr() }));
    let closed = describe(Some(&PullRequest { state: PrState::Closed, ..open_pr() }));

    assert!(merged.ends_with("\nIt's merged."), "{merged}");
    assert!(closed.ends_with("\nIt's closed, without being merged."), "{closed}");
}

#[test]
fn a_branch_without_one_says_so() {
    assert_eq!(describe(None), "This workspace's branch has no pull request on GitHub.");
}

#[test]
fn checks_still_running_say_how_many_are_done() {
    let text = describe(Some(&PullRequest { checks: Checks { passed: 2, failed: 0, pending: 3 }, ..open_pr() }));

    assert!(text.contains("Its checks are still running: 2 of 5 are done."), "{text}");
}
