//! The whole read, against a stand-in for GitHub answering from the issues and pull requests a test sets, or from
//! answers scripted in turn.

use protocol::model::{GithubToken, PullRequestNumber, PullRequestSelection};

use super::parsing::{named, number};
use super::*;
use crate::{
    Answer, Github, GithubConfig, Leaving, PullRequestState, PullRequests, PullRequestsAsk, References, Script, StandIn,
};

/// Issue #1021 of example/project, closed as done, and pull request #3 of ada/tools, a draft whose checks are running.
fn set() -> (Reference, Reference) {
    let issue = Reference {
        repository: named("example/project"),
        number: number(1021),
        kind: ReferenceKind::Issue { state: IssueState::Done },
        url: "https://github.com/example/project/issues/1021".to_string(),
        title: "Draw references as chips".to_string(),
        author: Some("ada".to_string()),
        opened_at_ms: 1_767_225_600_000,
        comments: 2,
        labels: vec![ReferenceLabel { name: "enhancement".to_string(), color: "a2eeef".to_string() }],
        body: "A reply names issues by number.".to_string(),
    };
    let pull_request = Reference {
        repository: named("ada/tools"),
        number: number(3),
        kind: ReferenceKind::PullRequest(ReferencedPullRequest {
            state: protocol::model::PullRequestState::Draft,
            added: 40,
            removed: 2,
            checks: Some(ChecksRollup::Running),
            review: Review::Required,
        }),
        url: "https://github.com/ada/tools/pull/3".to_string(),
        title: "Read references".to_string(),
        author: None,
        labels: Vec::new(),
        body: String::new(),
        ..issue.clone()
    };
    (issue, pull_request)
}

/// Both read back as they were set, the issue twice for being asked twice, in one query, beside #5, which was set and
/// then removed, and a repository with nothing set, which isn't there. Pull requests set beside them are still read
/// as they were, and once GitHub fails, every ask does.
#[tokio::test]
async fn a_read_finds_what_github_has_in_one_query_and_says_what_it_hasnt() {
    let (issue, pull_request) = set();
    let references = References::default();
    references.set(issue.clone());
    references.set(pull_request.clone());
    references.set(Reference { number: number(5), ..issue.clone() });
    references.remove(&named("example/project"), number(5));
    let pull_requests = PullRequests::default();
    pull_requests.set(7, PullRequestState::Passed);
    let script =
        Script { references: Some(references.clone()), pull_requests: Some(pull_requests), ..Script::default() };
    let github = StandIn::with(script).await;
    let calls = Github::new(GithubConfig::at(github.api()));
    let token = GithubToken::new("a-token");
    let asks = [
        (named("example/project"), number(1021)),
        (named("ada/tools"), number(3)),
        (named("example/project"), number(5)),
        (named("example/gone"), number(1)),
        (named("Example/Project"), number(1021)),
    ];

    let read = calls.read_references(&token, &asks, Leaving::ForOthers).await;

    let answers: Vec<Option<Reference>> = read.answers.into_iter().map(Result::unwrap).collect();
    assert_eq!(answers, [Some(issue.clone()), Some(pull_request), None, None, Some(issue)]);
    assert_eq!((references.queries(), read.rate_limit.unwrap().remaining, read.refused), (1, 4900, false));
    let selection = PullRequestSelection::try_from(vec![PullRequestNumber::try_from(7).unwrap()]).unwrap();
    let ask = PullRequestsAsk { repository: named("example/project"), selection };
    let pull_requests = calls.read_pull_requests(&token, vec![ask], Leaving::ForOthers).await;
    assert!(pull_requests.answers[0].is_ok(), "{:?}", pull_requests.answers);
    assert_eq!(references.queries(), 1, "a read of pull requests is answered as one");
    references.fail(true);
    let failed = calls.read_references(&token, &asks[..1], Leaving::ForOthers).await;
    assert_eq!(failed.answers[0].as_ref().unwrap_err().code, ErrorCode::Worker);
}

/// More than a query carries, once GitHub says its limit is down to the part kept for others, so the rest wait for it
/// to reset, and a token GitHub refuses, which the read says.
#[tokio::test]
async fn a_read_stops_once_githubs_limit_is_nearly_spent_and_says_when_the_token_is_refused() {
    let gone = json!({"type": "NOT_FOUND", "path": ["r0"], "message": "Could not resolve to a Repository."});
    let rate_limit = json!({"limit": 5000, "remaining": 900, "resetAt": "2026-01-01T01:00:00Z"});
    let body = json!({"data": {"r0": null, "rateLimit": rate_limit}, "errors": [gone]}).to_string();
    let answers = vec![Answer::With { status: 200, headers: Vec::new(), body }];
    let github = StandIn::with(Script { graphql: answers, ..Script::default() }).await;
    let refusing = StandIn::with(Script { graphql: vec![Answer::status(401, Vec::new())], ..Script::default() }).await;
    let asks: Vec<_> = (1..=60).map(|n| (named("example/gone"), number(n))).collect();
    let token = GithubToken::new("a-token");

    let read = Github::new(GithubConfig::at(github.api())).read_references(&token, &asks, Leaving::ForOthers).await;
    let refused =
        Github::new(GithubConfig::at(refusing.api())).read_references(&token, &asks[..1], Leaving::Nothing).await;

    assert!(read.answers[..50].iter().all(|answer| answer.as_ref().is_ok_and(Option::is_none)));
    assert!(read.answers[50..].iter().all(|answer| answer.as_ref().is_err_and(|e| e.code == ErrorCode::Worker)));
    assert_eq!(github.asked().len(), 1, "the second waits for GitHub's limit to reset");
    assert!(refused.refused && refused.answers[0].is_err());
}
