//! Synthetic remote PRs shared by tests of waits, storage and their transcript rows.

use crate::model::{
    CheckState, Checks, GitHubRepository, Local, MergeState, Mergeability, ObservedCheck, ObservedPullRequest, PrState,
    PullRequest, PullRequestNumber, PullRequestSelection, PullRequestWatch, PullRequestsSnapshot, Review, Wait,
    WaitUntil,
};

/// An open, mergeable PR with one required check still running.
pub fn observed_pull_request(number: u64) -> ObservedPullRequest {
    ObservedPullRequest {
        pull_request: PullRequest {
            number,
            url: format!("https://github.com/example/project/pull/{number}"),
            title: format!("Change {number}"),
            state: PrState::Open,
            is_draft: false,
            base: Some("main".into()),
            merge_state: MergeState::Clean,
            review: Review::NotRequired,
            checks: Checks { passed: 0, failed: 0, pending: 1 },
            queue: None,
            left_queue: None,
            changes_requested_at_ms: None,
            local: Local::default(),
        },
        head: format!("head-{number}").as_str().into(),
        base: "base".into(),
        destination: "main".into(),
        dependencies: Vec::new(),
        mergeability: Mergeability::Mergeable,
        linear: Some(true),
        checks: vec![ObservedCheck {
            name: "checks".into(),
            source: "app:1".into(),
            url: None,
            state: CheckState::Pending,
            required: true,
        }],
        checks_revision: Some(format!("head-{number}").as_str().into()),
        requirements_known: true,
        checks_known: true,
    }
}

/// A wait by chat `s` in workspace `w`, watching #7's required checks.
#[expect(clippy::unwrap_used, reason = "the synthetic wait uses the valid, nonempty selection [7]")]
pub fn pull_request_wait(id: &str) -> Wait {
    let requested = PullRequestSelection::try_from(vec![PullRequestNumber::try_from(7).unwrap()]).unwrap();
    Wait {
        id: id.into(),
        session_id: "s".into(),
        workspace_id: "w".into(),
        number: 7,
        call: Some(id.into()),
        until: WaitUntil::RequiredChecks,
        started_at_ms: 1,
        ended: None,
        watch: Some(Box::new(PullRequestWatch {
            requested,
            snapshot: PullRequestsSnapshot {
                repository: GitHubRepository {
                    url: "https://github.com/example/project".into(),
                    name_with_owner: "example/project".into(),
                },
                observed_at_ms: 1,
                pull_requests: vec![observed_pull_request(7)],
            },
            baseline: Vec::new(),
            deadline_at_ms: None,
            read_error: None,
            last_attempt_at_ms: 1,
            affected: Vec::new(),
        })),
    }
}
