//! The whole read, against a stand-in for GitHub answering each round's query in turn, as GitHub would for synthetic
//! pull requests: #3 and #7 of a stack of three, where #8 is above #7, which a read of #7 doesn't need.

use protocol::model::{GithubRepositoryName, GithubToken, PullRequestSelection};

use super::*;
use crate::{Answer, Github, GithubConfig, PullRequestsAsk, Script, StandIn};

/// What GitHub answers a query whose repositories answer as `repositories` say, costing a point, with `remaining` of
/// its limit of 5,000 left, and with `errors`.
pub(super) fn answer_with(repositories: Value, remaining: u32, errors: &[Value]) -> Answer {
    let mut data = repositories;
    data["rateLimit"] = json!({"cost":1,"limit":5000,"remaining":remaining,"resetAt":"2026-01-01T01:00:00Z"});
    let mut body = json!({ "data": data });
    if !errors.is_empty() {
        body["errors"] = json!(errors);
    }
    Answer::With { status: 200, headers: Vec::new(), body: body.to_string() }
}

/// The repository `name` of example's, answering with `parts`, under its alias `r{at}`.
pub(super) fn repository(at: usize, name: &str, parts: &[(&str, Value)]) -> (String, Value) {
    let mut repository =
        json!({"nameWithOwner": format!("example/{name}"), "url": format!("https://github.com/example/{name}")});
    for (alias, part) in parts {
        repository[*alias] = part.clone();
    }
    (format!("r{at}"), repository)
}

pub(super) fn answer(repositories: Vec<(String, Value)>) -> Answer {
    answer_with(Value::Object(repositories.into_iter().collect()), 4900, &[])
}

fn connection(nodes: &[Value], after: Option<&str>, count: u64) -> Value {
    json!({"nodes":nodes,"totalCount":count,"pageInfo":{"hasNextPage":after.is_some(),"endCursor":after}})
}

fn entries() -> Vec<Value> {
    [(3, 1), (7, 2), (8, 3)]
        .into_iter()
        .map(|(number, position)| json!({"position":position,"pullRequest":{"number":number,"state":"OPEN"}}))
        .collect()
}

fn reported(name: &str, head: &str, pending: bool) -> Value {
    json!({"__typename":"CheckRun","name":name,"url":"https://example.test/check","startedAt":"2026-01-01T00:00:00Z",
        "isRequired":name=="checks","status":if pending {"IN_PROGRESS"} else {"COMPLETED"},"conclusion":"SUCCESS",
        "checkSuite":{"createdAt":"2026-01-01T00:00:00Z","commit":{"oid":head},"app":{"databaseId":1},"workflowRun":{"workflow":{"id":"workflow"}}}})
}

/// Pull request `number` as every part asks for it, which GitHub would answer only in part.
pub(super) fn pull_request(number: u64) -> Value {
    let head = format!("head-{number}");
    let base = if number == 7 { "head-3" } else { "main-head" };
    json!({"number":number,"url":format!("https://example.test/pull/{number}"),"title":"Synthetic change","state":"OPEN","isDraft":false,
        "headRefOid":head,"baseRefOid":base,"baseRefName":if number==7 {"lower"} else {"main"},"headRefName":"branch",
        "mergeStateStatus":"CLEAN","mergeable":"MERGEABLE","reviewDecision":null,"potentialMergeCommit":null,"mergeQueueEntry":null,
        "reviews":{"nodes":[]},"timelineItems":{"nodes":[]},"stackEntry":{"position":if number==7 {2} else {1}},
        "stack":{"baseRefName":"main","entries":connection(&entries(),None,3)},
        "baseRef":{"compare":{"behindBy":0,"baseTarget":{"oid":base},"headTarget":{"oid":head}}},
        "statusCheckRollup":{"commit":{"oid":head},"contexts":connection(&[reported("checks",&head,number==7)],None,1)}})
}

/// `main`'s rules, which require `checks` from the app numbered 1.
pub(super) fn rules() -> Value {
    json!({"branchProtectionRule":null,"refUpdateRule":null,"rules":connection(&[json!({"type":"REQUIRED_STATUS_CHECKS",
        "repositoryRuleset":{"enforcement":"ACTIVE"},"parameters":{"requiredStatusChecks":[{"context":"checks","integrationId":1}]}})],None,1)})
}

/// #7 as it's looked at first: the first page of its stack and of its checks.
fn looked_at() -> Value {
    let mut first = pull_request(7);
    first["stack"]["entries"] = connection(&[entries()[0].clone()], Some("stack-2"), 3);
    first["statusCheckRollup"]["contexts"] = connection(&[reported("checks", "head-7", true)], Some("checks-2"), 2);
    first
}

/// The four rounds of a read of #7: it, then the rest of its stack and its checks, then #3, which the stack says is
/// under it, then `main`'s rules with both confirmed, as `confirmed` has #7.
fn rounds(confirmed: Value) -> Vec<Answer> {
    let mut stack = json!({"headRefOid":"head-7"});
    stack["stack"] = json!({"baseRefName":"main","entries":connection(&entries()[1..], None, 3)});
    let mut checks = json!({"headRefOid":"head-7","potentialMergeCommit":null});
    checks["statusCheckRollup"] =
        json!({"commit":{"oid":"head-7"},"contexts":connection(&[reported("extra", "head-7", false)], None, 2)});
    vec![
        answer(vec![repository(0, "project", &[("p7", looked_at())])]),
        answer(vec![repository(0, "project", &[("s7", stack), ("c7", checks)])]),
        answer(vec![repository(0, "project", &[("p3", pull_request(3))])]),
        answer(vec![repository(0, "project", &[("b0", rules()), ("v3", pull_request(3)), ("v7", confirmed)])]),
    ]
}

/// #7 confirmed as it was looked at, and as it is after its branch was pushed.
fn unmoved() -> Value {
    looked_at()
}
fn moved() -> Value {
    let mut moved = looked_at();
    moved["headRefOid"] = json!("new-head");
    moved
}

pub(super) fn ask(name: &'static str, numbers: &[u64]) -> PullRequestsAsk {
    PullRequestsAsk {
        repository: GithubRepositoryName::new("example", name).unwrap(),
        selection: PullRequestSelection::try_from(
            numbers.iter().map(|n| PullRequestNumber::try_from(*n).unwrap()).collect::<Vec<_>>(),
        )
        .unwrap(),
    }
}

/// What a read of `asks` finds as a stand-in answers its queries with `answers`, in turn, and each query it asked.
pub(super) async fn read(
    answers: Vec<Answer>,
    asks: Vec<PullRequestsAsk>,
) -> (super::super::asked::PullRequestsRead, Vec<String>) {
    let github = StandIn::with(Script { graphql: answers, ..Script::default() }).await;
    let calls = Github::new(GithubConfig::at(github.api()));

    let read = calls.read_pull_requests(&GithubToken::new("a-token"), asks, crate::Leaving::ForOthers).await;

    let queries = github.asked().into_iter().filter(|asked| asked.starts_with("post /graphql ")).collect();
    (read, queries)
}

fn number(n: u64) -> PullRequestNumber {
    PullRequestNumber::try_from(n).unwrap()
}

#[tokio::test]
async fn a_read_takes_in_what_is_under_it_reads_every_page_and_confirms_nothing_moved() {
    let (read, queries) = read(rounds(unmoved()), vec![ask("project", &[7])]).await;

    let snapshot = read.answers.into_iter().next().unwrap().unwrap();
    assert_eq!(snapshot.repository.name_with_owner, "example/project");
    assert_eq!(snapshot.pull_requests.iter().map(|row| row.pull_request.number).collect::<Vec<_>>(), [3, 7]);
    let upper = snapshot.get(number(7)).unwrap();
    assert_eq!(upper.dependencies, [number(3)]);
    assert_eq!(upper.checks.len(), 2);
    assert_eq!(upper.linear, Some(true));
    assert!(upper.checks.iter().any(|check| check.required && check.state == CheckState::Pending));
    let asked = ["p7: pullRequest", "s7: pullRequest", "p3: pullRequest", "v7: pullRequest"];
    for (query, asked) in queries.iter().zip(asked) {
        assert!(query.contains(asked), "{asked} in {query}");
    }
    assert!(queries[1].contains("c7: pullRequest") && queries[3].contains("b0: ref("), "{queries:?}");
    assert!(queries[3].contains("compare(headRef:") && queries.len() == 4, "{queries:?}");
}

/// Its branch pushed between the first look and the last, so the read starts again, and once that has moved too, and
/// again, it fails, about #7.
#[tokio::test]
async fn a_read_whose_pull_requests_moved_under_it_reads_again_and_fails_once_they_keep_moving() {
    let once = [rounds(moved()), rounds(unmoved())].concat();
    let always = [rounds(moved()), rounds(moved()), rounds(moved())].concat();

    let (read_once, asked_once) = read(once, vec![ask("project", &[7])]).await;
    let (read_always, asked_always) = read(always, vec![ask("project", &[7])]).await;

    let snapshot = read_once.answers.into_iter().next().unwrap().unwrap();
    assert_eq!(snapshot.get(number(7)).unwrap().head.as_str(), "head-7");
    assert_eq!(asked_once.len(), 8);
    let unread = read_always.answers.into_iter().next().unwrap().unwrap_err();
    assert_eq!((unread.error.code, unread.affected), (ErrorCode::Conflict, vec![number(7)]));
    assert_eq!(asked_always.len(), 12);
}

#[tokio::test]
async fn a_pull_request_under_it_that_cant_be_seen_fails_the_read_about_it_and_those_above_it() {
    let mut answers = rounds(unmoved());
    let not_found = json!({"type":"NOT_FOUND","path":["r0","p3"],"message":"Could not resolve to a PullRequest."});
    answers[2] = answer_with(json!({"r0":{"nameWithOwner":"example/project","url":"u","p3":null}}), 4900, &[not_found]);

    let (read, queries) = read(answers, vec![ask("project", &[7])]).await;

    let unread = read.answers.into_iter().next().unwrap().unwrap_err();
    assert_eq!((unread.error.code, unread.affected), (ErrorCode::Unavailable, vec![number(3), number(7)]));
    assert_eq!(queries.len(), 3, "nothing more once it failed");
}
