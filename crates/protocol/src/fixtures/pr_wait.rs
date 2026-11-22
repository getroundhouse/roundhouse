//! Synthetic remote PRs shared by tests of waits, storage and their transcript rows.

use crate::model::{
    About, CheckState, Checks, GitHubRepository, GithubRepositoryName, History, MergeState, Mergeability,
    ObservedCheck, PrState, PullRequest, PullRequestNumber, PullRequestSelection, PullRequestWatch,
    PullRequestsSnapshot, Read, Review, Stack, Summary, Wait, WaitUntil,
};

/// Open pull request `number` of example/project, mergeable, with every facet, read at 1, and one required check
/// still running.
#[expect(clippy::unwrap_used, reason = "the synthetic numbers a test gives are valid")]
pub fn pull_request(number: u64) -> PullRequest {
    let read = |said| Read { said, read_at_ms: 1 };
    let head = format!("head-{number}");
    let summary = Summary {
        about: About {
            url: format!("https://github.com/example/project/pull/{number}"),
            title: format!("Change {number}"),
            author: None,
            opened_at_ms: 1,
            comments: 0,
            labels: Vec::new(),
            body: String::new(),
        },
        state: PrState::Open,
        is_draft: false,
        review: Review::NotRequired,
        queue: None,
        merge_state: MergeState::Clean,
        mergeable: Mergeability::Mergeable,
        base_branch: "main".into(),
        head: head.as_str().into(),
        base: "base".into(),
        added: 0,
        removed: 0,
        rollup: None,
    };
    let check = ObservedCheck {
        name: "checks".into(),
        source: "app:1".into(),
        url: None,
        state: CheckState::Pending,
        required: true,
    };
    PullRequest {
        repository: GithubRepositoryName::named("example/project"),
        number: PullRequestNumber::try_from(number).unwrap(),
        summary: read(summary),
        history: Some(Read { said: History::default(), read_at_ms: 1 }),
        stack: Some(Read {
            said: Stack { destination: "main".into(), dependencies: Vec::new(), linear: Some(true) },
            read_at_ms: 1,
        }),
        checks: Some(Read {
            said: Checks { revision: Some(head.as_str().into()), requirements_known: true, checks: vec![check] },
            read_at_ms: 1,
        }),
    }
}

/// `pull_request` with this many required checks passed, failed and still running, as its only checks.
pub fn checked(mut pull_request: PullRequest, passed: usize, failed: usize, pending: usize) -> PullRequest {
    let check =
        |state| ObservedCheck { name: "checks".into(), source: "app:1".into(), url: None, state, required: true };
    let states = [(passed, CheckState::Passed), (failed, CheckState::Failed), (pending, CheckState::Pending)];
    let checks = states.into_iter().flat_map(|(n, state)| std::iter::repeat_n(state, n)).map(check).collect();
    let revision = Some(pull_request.said().head.clone());
    pull_request.checks = Some(Read { said: Checks { revision, requirements_known: true, checks }, read_at_ms: 1 });
    pull_request
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
                pull_requests: vec![pull_request(7)],
            },
            baseline: Vec::new(),
            deadline_at_ms: None,
            read_error: None,
            last_attempt_at_ms: 1,
            affected: Vec::new(),
        })),
    }
}
