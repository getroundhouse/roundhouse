//! Taking GitHub's answer apart, ask by ask, from canned answers to a query of example/project's numbers.

use protocol::client::ApiError;
use protocol::model::{PullRequestState, REFERENCE_BODY_CHARS};

use super::super::read::taken;
use super::*;

pub(super) fn named(name: &str) -> GithubRepositoryName {
    GithubRepositoryName::try_from(name.to_string()).unwrap()
}

pub(super) fn number(number: u64) -> IssueNumber {
    IssueNumber::try_from(number).unwrap()
}

/// Issue `number`, in `state`, closed for `reason`, as GitHub answers for it.
fn issue(number: u64, state: &str, reason: Option<&str>) -> Value {
    let url = format!("https://github.com/example/project/issues/{number}");
    json!({"__typename": "Issue", "number": number, "url": url,
        "title": "Synthetic issue", "state": state, "stateReason": reason, "createdAt": "2026-01-01T00:00:00Z",
        "author": {"login": "ada"}, "comments": {"totalCount": 3}, "bodyText": "What it says.",
        "labels": {"nodes": [{"name": "bug", "color": "d73a4a"}]}})
}

/// Pull request `number`, in `state`, a draft or not, whose newest commit's checks are `rollup`, and whose review is
/// `review`, as GitHub answers for it.
fn pull_request(number: u64, state: &str, draft: bool, rollup: Option<&str>, review: Option<&str>) -> Value {
    let rollup = rollup.map(|state| json!({ "state": state }));
    let url = format!("https://github.com/example/project/pull/{number}");
    json!({"__typename": "PullRequest", "number": number, "url": url,
        "title": "Synthetic change", "state": state, "isDraft": draft, "createdAt": "2026-01-02T00:00:00Z",
        "author": null, "comments": {"totalCount": 0}, "bodyText": "", "labels": {"nodes": []},
        "additions": 120, "deletions": 4, "reviewDecision": review,
        "commits": {"nodes": [{"commit": {"statusCheckRollup": rollup}}]}})
}

/// What `taken` makes of an answer of `data`, with `errors`, to a query of `asked`, each of example/project's but for
/// those of `r1`, which are example/gone's.
fn read(data: &Value, errors: &[Value], asked: &[&[u64]]) -> Vec<Result<Option<Reference>, ApiError>> {
    let repositories = [named("example/project"), named("example/gone")];
    let batch: Vec<_> = asked
        .iter()
        .zip(&repositories)
        .map(|(numbers, repository)| (repository, numbers.iter().map(|n| number(*n)).collect()))
        .collect();
    taken(&json!({ "data": data, "errors": errors }), &batch)
}

/// Whatever is asked of `nodes`, as example/project answers for them, as GitHub writes its name.
fn project(nodes: &[(u64, Value)]) -> Value {
    let mut repository = json!({"nameWithOwner": "Example/Project"});
    for (number, node) in nodes {
        repository[format!("n{number}")] = node.clone();
    }
    json!({ "r0": repository })
}

/// Open, reopened, and closed as done, as not planned, as a duplicate, and from before GitHub said why, as done.
#[test]
fn an_issue_is_read_whole_and_says_how_it_was_closed() {
    let cases = [
        ("OPEN", None, IssueState::Open),
        ("OPEN", Some("REOPENED"), IssueState::Open),
        ("CLOSED", Some("COMPLETED"), IssueState::Done),
        ("CLOSED", Some("NOT_PLANNED"), IssueState::NotPlanned),
        ("CLOSED", Some("DUPLICATE"), IssueState::NotPlanned),
        ("CLOSED", None, IssueState::Done),
    ];
    let nodes: Vec<(u64, Value)> =
        (1..).zip(&cases).map(|(n, (state, reason, _))| (n, issue(n, state, *reason))).collect();
    let numbers: Vec<u64> = nodes.iter().map(|(n, _)| *n).collect();

    let read = read(&project(&nodes), &[], &[&numbers]);

    let read: Vec<Reference> = read.into_iter().map(|answer| answer.unwrap().unwrap()).collect();
    assert_eq!(
        read[0],
        Reference {
            repository: named("Example/Project"),
            number: number(1),
            kind: ReferenceKind::Issue { state: IssueState::Open },
            url: "https://github.com/example/project/issues/1".to_string(),
            title: "Synthetic issue".to_string(),
            author: Some("ada".to_string()),
            opened_at_ms: 1_767_225_600_000,
            comments: 3,
            labels: vec![ReferenceLabel { name: "bug".to_string(), color: "d73a4a".to_string() }],
            body: "What it says.".to_string(),
        }
    );
    let kinds: Vec<ReferenceKind> = read.iter().map(|reference| reference.kind).collect();
    assert_eq!(kinds, cases.map(|(_, _, state)| ReferenceKind::Issue { state }));
}

/// Open, a draft, merged and closed, with checks that passed, are running, failed or aren't there, and each review.
#[test]
fn a_pull_request_says_where_it_its_checks_and_its_review_stand() {
    let cases = [
        (pull_request(1, "OPEN", false, Some("SUCCESS"), Some("APPROVED")), PullRequestState::Open),
        (pull_request(2, "OPEN", true, Some("PENDING"), None), PullRequestState::Draft),
        (pull_request(3, "MERGED", false, Some("ERROR"), Some("CHANGES_REQUESTED")), PullRequestState::Merged),
        (pull_request(4, "CLOSED", false, None, Some("REVIEW_REQUIRED")), PullRequestState::Closed),
        (pull_request(5, "OPEN", false, Some("EXPECTED"), None), PullRequestState::Open),
        (pull_request(6, "OPEN", false, Some("FAILURE"), None), PullRequestState::Open),
    ];
    let mut no_commits = pull_request(7, "OPEN", false, None, None);
    no_commits["commits"]["nodes"] = json!([]);
    let mut nodes: Vec<(u64, Value)> = (1..).zip(cases.iter().map(|(node, _)| node.clone())).collect();
    nodes.push((7, no_commits));

    let read = read(&project(&nodes), &[], &[&[1, 2, 3, 4, 5, 6, 7]]);

    let read: Vec<Reference> = read.into_iter().map(|answer| answer.unwrap().unwrap()).collect();
    let pulls: Vec<ReferencedPullRequest> = read
        .iter()
        .map(|reference| match reference.kind {
            ReferenceKind::PullRequest(pull_request) => pull_request,
            other @ (ReferenceKind::Issue { .. } | ReferenceKind::Unknown) => panic!("{other:?}"),
        })
        .collect();
    assert_eq!((read[0].author.clone(), read[0].opened_at_ms), (None, 1_767_312_000_000));
    assert_eq!(
        pulls[0],
        ReferencedPullRequest {
            state: PullRequestState::Open,
            added: 120,
            removed: 4,
            checks: Some(ChecksRollup::Passed),
            review: Review::Approved
        }
    );
    let states: Vec<PullRequestState> = pulls.iter().take(6).map(|pull_request| pull_request.state).collect();
    assert_eq!(states, cases.map(|(_, state)| state));
    let stood: Vec<_> = pulls.iter().map(|pr| Some((pr.checks, pr.review))).collect();
    let passed = Some(ChecksRollup::Passed);
    let (running, failed) = (Some(ChecksRollup::Running), Some(ChecksRollup::Failed));
    assert_eq!(
        stood,
        [
            Some((passed, Review::Approved)),
            Some((running, Review::NotRequired)),
            Some((failed, Review::ChangesRequested)),
            Some((None, Review::Required)),
            Some((running, Review::NotRequired)),
            Some((failed, Review::NotRequired)),
            Some((None, Review::NotRequired)),
        ]
    );
}

/// #1 is there, #2 isn't, #3 the token is forbidden, #4 GitHub left out without saying why, and example/gone isn't
/// there, so neither is its #5.
#[test]
fn one_that_isnt_there_is_missing_and_any_other_error_fails_only_its_own() {
    let not_found = |path: Value| json!({"type": "NOT_FOUND", "path": path, "message": "Could not resolve."});
    let forbidden = json!({"type": "FORBIDDEN", "path": ["r0", "n3"], "message": "Resource not accessible."});
    let mut data = project(&[(1, issue(1, "OPEN", None)), (2, Value::Null), (3, Value::Null), (4, Value::Null)]);
    data["r1"] = Value::Null;
    let errors = [not_found(json!(["r0", "n2"])), forbidden, not_found(json!(["r1"]))];

    let read = read(&data, &errors, &[&[1, 2, 3, 4], &[5]]);

    let [found, missing, forbidden, left_out, gone] = <[_; 5]>::try_from(read).unwrap();
    assert_eq!(found.unwrap().map(|found| found.number), Some(number(1)));
    assert_eq!((missing.unwrap(), gone.unwrap()), (None, None));
    assert_eq!(forbidden.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(left_out.unwrap_err().code, ErrorCode::Worker);
}

/// An error that names none of what was asked, as GitHub gives when the query is too big, fails every ask in it.
#[test]
fn an_error_about_the_whole_query_fails_every_ask_in_it() {
    let refused = json!({"type": "MAX_NODE_LIMIT_EXCEEDED", "message": "too many nodes"});

    let read = read(&Value::Null, &[refused], &[&[1, 2], &[3]]);

    let codes: Vec<ErrorCode> = read.into_iter().map(|answer| answer.unwrap_err().code).collect();
    assert_eq!(codes, [ErrorCode::Worker; 3]);
}

/// What it says, past what's kept, in letters of two bytes each, so a cut by bytes would land inside one.
#[test]
fn what_it_says_is_cut_short_between_characters() {
    let mut long = issue(1, "OPEN", None);
    long["bodyText"] = json!("é".repeat(REFERENCE_BODY_CHARS + 100));

    let read = read(&project(&[(1, long)]), &[], &[&[1]]);

    let body = read.into_iter().next().unwrap().unwrap().unwrap().body;
    assert_eq!(body, "é".repeat(REFERENCE_BODY_CHARS));
}
