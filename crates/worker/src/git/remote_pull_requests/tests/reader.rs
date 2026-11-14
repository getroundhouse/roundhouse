//! The whole reader, against synthetic paged GraphQL responses rather than isolated parsing helpers.

use std::collections::VecDeque;
use std::future::{Future, ready};

use super::*;

struct Replies(VecDeque<(&'static str, Result<Value, ApiError>)>);
impl Query for Replies {
    fn run(&mut self, _: &str, args: &[&str]) -> impl Future<Output = Result<Value, ApiError>> {
        let (expected, reply) = self.0.pop_front().expect("no query beyond the planned scope");
        assert!(args.last().unwrap().contains(expected), "expected {expected}: {args:?}");
        ready(reply)
    }
}

fn response(repository: &Value) -> Value {
    json!({"data":{"repository":repository,"rateLimit":{"cost":1,"limit":5000,"remaining":4900,"resetAt":"2026-01-01T01:00:00Z"}}})
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

fn pull_request(number: u64) -> Value {
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

fn page(pr: &Value) -> Value {
    response(&json!({"pullRequest":pr}))
}

fn replies() -> Replies {
    let lower = pull_request(3);
    let mut first = pull_request(7);
    first["stack"]["entries"] = connection(&[entries()[0].clone()], Some("stack-2"), 3);
    let mut second = pull_request(7);
    second["stack"]["entries"] = connection(&entries()[1..], None, 3);
    let mut checks_first = pull_request(7);
    checks_first["statusCheckRollup"]["contexts"] =
        connection(&[reported("checks", "head-7", true)], Some("checks-2"), 2);
    let mut checks_second = pull_request(7);
    checks_second["statusCheckRollup"]["contexts"] = connection(&[reported("extra", "head-7", false)], None, 2);
    let policy = response(
        &json!({"ref":{"branchProtectionRule":null,"refUpdateRule":null,"rules":connection(&[json!({"type":"REQUIRED_STATUS_CHECKS",
        "repositoryRuleset":{"enforcement":"ACTIVE"},"parameters":{"requiredStatusChecks":[{"context":"checks","integrationId":1}]}})],None,1)}}),
    );
    Replies(VecDeque::from([
        ("pullRequest(number:7)", Ok(page(&first))),
        ("after:\"stack-2\"", Ok(page(&second))),
        ("pullRequest(number:3)", Ok(page(&lower))),
        ("refs/heads/main", Ok(policy)),
        ("pullRequest(number:3)", Ok(page(&lower))),
        ("pullRequest(number:7)", Ok(page(&checks_first))),
        ("after:\"checks-2\"", Ok(page(&checks_second))),
        ("pullRequest(number:3)", Ok(page(&lower))),
        ("pullRequest(number:7)", Ok(page(&first))),
        ("after:\"stack-2\"", Ok(page(&second))),
    ]))
}

fn reader(source: Replies) -> Reader<'static, Replies> {
    Reader {
        source,
        path: "unused",
        repository: Some(GitHubRepository {
            url: "https://github.com/example/project".into(),
            name_with_owner: "example/project".into(),
        }),
        rate_limit: None,
        cost: 0,
        failed_pr: None,
        dependents: BTreeMap::new(),
    }
}
fn selection() -> PullRequestSelection {
    serde_json::from_value(json!([7])).unwrap()
}

#[tokio::test]
async fn expands_lower_dependencies_accumulates_pages_and_verifies_the_complete_read() {
    let mut reader = reader(replies());

    let result = reader.read(&selection()).await;
    assert!(reader.source.0.is_empty());
    let reply = reader.finish(result);
    let snapshot = reply.observation.unwrap();

    assert_eq!(reply.cost, 10);
    assert_eq!(snapshot.pull_requests.iter().map(|row| row.pull_request.number).collect::<Vec<_>>(), [3, 7]);
    let upper = snapshot.get(PullRequestNumber::try_from(7).unwrap()).unwrap();
    assert_eq!(upper.dependencies, [PullRequestNumber::try_from(3).unwrap()]);
    assert_eq!(upper.checks.len(), 2);
    assert_eq!(upper.linear, Some(true));
    assert!(upper.checks.iter().any(|check| check.required && check.state == CheckState::Pending));
}

#[tokio::test]
async fn metadata_changing_during_checks_discards_the_entire_mixed_revision() {
    let mut replies = replies();
    for index in [8, 9] {
        replies.0[index].1.as_mut().unwrap()["data"]["repository"]["pullRequest"]["headRefOid"] = json!("new-head");
    }
    let mut reader = reader(replies);

    let result = reader.read(&selection()).await;
    let reply = reader.finish(result);

    assert_eq!(reply.observation.unwrap_err().code, ErrorCode::Conflict);
    assert_eq!(reply.affected, [PullRequestNumber::try_from(7).unwrap()]);
}

#[tokio::test]
async fn a_lower_pr_access_failure_names_that_pr_and_its_dependents() {
    let mut replies = replies();
    replies.0[4].1 = Err(ApiError::new(ErrorCode::Unavailable, "synthetic access loss"));
    let mut reader = reader(replies);

    let result = reader.read(&selection()).await;
    let reply = reader.finish(result);

    assert_eq!(reply.observation.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(reply.affected, [PullRequestNumber::try_from(3).unwrap(), PullRequestNumber::try_from(7).unwrap()]);
    assert_eq!(reply.cost, 5, "including the request whose response was lost");
}
