//! Taking GitHub's answer apart, ask by ask, from canned answers to a query of example/project's numbers.

use protocol::client::ApiError;
use protocol::model::{MergeState, Mergeability, PrState, QueueState, REFERENCE_BODY_CHARS};

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
/// `review`, out of the merge queue, as GitHub answers for it.
fn pull_request(number: u64, state: &str, draft: bool, rollup: Option<&str>, review: Option<&str>) -> Value {
    let rollup = rollup.map(|state| json!({ "state": state }));
    let url = format!("https://github.com/example/project/pull/{number}");
    json!({"__typename": "PullRequest", "number": number, "url": url,
        "title": "Synthetic change", "state": state, "isDraft": draft, "createdAt": "2026-01-02T00:00:00Z",
        "author": null, "comments": {"totalCount": 0}, "bodyText": "", "labels": {"nodes": []},
        "additions": 120, "deletions": 4, "reviewDecision": review, "mergeQueueEntry": null,
        "mergeStateStatus": "CLEAN", "mergeable": "MERGEABLE", "baseRefName": "main", "baseRefOid": "base",
        "headRefOid": format!("head-{number}"),
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
    taken(&json!({ "data": data, "errors": errors }), &batch, 9)
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
    let about = About {
        url: "https://github.com/example/project/issues/1".to_string(),
        title: "Synthetic issue".to_string(),
        author: Some("ada".to_string()),
        opened_at_ms: 1_767_225_600_000,
        comments: 3,
        labels: vec![ReferenceLabel { name: "bug".to_string(), color: "d73a4a".to_string() }],
        body: "What it says.".to_string(),
    };
    let summary = Read { said: IssueSummary { about, state: IssueState::Open }, read_at_ms: 9 };
    assert_eq!(read[0], Reference::Issue(Issue { repository: named("Example/Project"), number: number(1), summary }));
    let states: Vec<IssueState> = read
        .iter()
        .map(|reference| match reference {
            Reference::Issue(issue) => issue.summary.said.state,
            other @ (Reference::PullRequest(_) | Reference::Unknown) => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(states, cases.map(|(_, _, state)| state));
}

/// Open, a draft, merged and closed, with checks that passed, are running, failed or aren't there, each review, and
/// in the merge queue or not.
#[test]
fn a_pull_request_says_where_it_its_checks_its_review_and_its_place_in_the_queue_stand() {
    let cases = [
        (pull_request(1, "OPEN", false, Some("SUCCESS"), Some("APPROVED")), (PrState::Open, false)),
        (pull_request(2, "OPEN", true, Some("PENDING"), None), (PrState::Open, true)),
        (pull_request(3, "MERGED", false, Some("ERROR"), Some("CHANGES_REQUESTED")), (PrState::Merged, false)),
        (pull_request(4, "CLOSED", false, None, Some("REVIEW_REQUIRED")), (PrState::Closed, false)),
        (pull_request(5, "OPEN", false, Some("EXPECTED"), None), (PrState::Open, false)),
        (pull_request(6, "OPEN", false, Some("FAILURE"), None), (PrState::Open, false)),
    ];
    let mut no_commits = pull_request(7, "OPEN", false, None, None);
    no_commits["commits"]["nodes"] = json!([]);
    let mut nodes: Vec<(u64, Value)> = (1..).zip(cases.iter().map(|(node, _)| node.clone())).collect();
    nodes.push((7, no_commits));
    nodes[4].1["mergeQueueEntry"] = json!({"state": "AWAITING_CHECKS"});

    let read = read(&project(&nodes), &[], &[&[1, 2, 3, 4, 5, 6, 7]]);

    let read: Vec<Reference> = read.into_iter().map(|answer| answer.unwrap().unwrap()).collect();
    let pulls: Vec<&Summary> = read
        .iter()
        .map(|reference| match reference {
            Reference::PullRequest(pull_request) => pull_request.said(),
            other @ (Reference::Issue(_) | Reference::Unknown) => panic!("{other:?}"),
        })
        .collect();
    assert_eq!((pulls[0].about.author.clone(), pulls[0].about.opened_at_ms), (None, 1_767_312_000_000));
    assert_eq!(
        (pulls[0].added, pulls[0].removed, pulls[0].merge_state, pulls[0].mergeable, pulls[0].head.as_str()),
        (120, 4, MergeState::Clean, Mergeability::Mergeable, "head-1")
    );
    let states: Vec<(PrState, bool)> = pulls.iter().take(6).map(|pr| (pr.state, pr.is_draft)).collect();
    assert_eq!(states, cases.map(|(_, state)| state));
    let stood: Vec<_> = pulls.iter().map(|pr| (pr.rollup, pr.review)).collect();
    let passed = Some(ChecksRollup::Passed);
    let (running, failed) = (Some(ChecksRollup::Running), Some(ChecksRollup::Failed));
    assert_eq!(
        stood,
        [
            (passed, Review::Approved),
            (running, Review::NotRequired),
            (failed, Review::ChangesRequested),
            (None, Review::Required),
            (running, Review::NotRequired),
            (failed, Review::NotRequired),
            (None, Review::NotRequired),
        ]
    );
    let queues: Vec<_> = pulls.iter().map(|pr| pr.queue).collect();
    assert_eq!(queues, [None, None, None, None, Some(QueueState::AwaitingChecks), None, None]);
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
    assert_eq!(found.unwrap().and_then(|found| found.asked()).map(|asked| asked.number), Some(number(1)));
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

    let body = read.into_iter().next().unwrap().unwrap().unwrap().about().unwrap().body.clone();
    assert_eq!(body, "é".repeat(REFERENCE_BODY_CHARS));
}
