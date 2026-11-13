use super::*;

fn run(created: &str, conclusion: &str) -> Value {
    json!({"__typename":"CheckRun", "name":"checks", "url":"https://example.test/check", "status":"COMPLETED",
        "conclusion":conclusion, "createdAt":created, "startedAt":created, "isRequired":true,
        "checkSuite":{"createdAt":"0","commit":{"oid":"head"},"app":{"databaseId":1},"workflowRun":{"workflow":{"id":"w"}}}})
}
fn pr() -> Value {
    json!({"headRefOid":"head", "potentialMergeCommit":{"oid":"merge"}})
}
fn checks(nodes: &[Value], revision: &str) -> Value {
    json!({"commit":{"oid":revision},"contexts":{"nodes":nodes}})
}

#[test]
fn an_old_revision_cannot_satisfy_the_new_heads_checks() {
    let mut out = Vec::new();
    let stale = parse::checks(&checks(&[run("1", "SUCCESS")], "old-head"), &pr(), &mut out);
    assert_eq!(stale.unwrap_err().code, ErrorCode::Conflict);
    let mismatched = parse::checks(&checks(&[run("1", "SUCCESS")], "merge"), &pr(), &mut out);
    assert_eq!(mismatched.unwrap_err().code, ErrorCode::Conflict);
}

#[test]
fn a_queued_rerun_replaces_a_previous_success_without_needing_a_start_time() {
    let mut pending = run("2", "SUCCESS");
    pending["status"] = json!("QUEUED");
    pending["conclusion"] = Value::Null;
    pending["startedAt"] = Value::Null;
    let mut out = Vec::new();
    parse::checks(&checks(&[pending, run("1", "SUCCESS")], "head"), &pr(), &mut out).unwrap();

    let observed = Policy::default().checks(out);

    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].state, CheckState::Pending);
}

#[test]
fn effective_requirements_union_rules_and_protections_and_missing_checks_wait() {
    let mut policy = Policy::default();
    policy
        .include(&json!({"refUpdateRule":null,"branchProtectionRule":{"requiresStatusChecks":true,
        "requiredStatusChecks":[{"context":"classic", "app":null}]},
        "rules":{"nodes":[{"type":"REQUIRED_STATUS_CHECKS", "repositoryRuleset":{"enforcement":"ACTIVE"},
        "parameters":{"requiredStatusChecks":[{"context":"checks","integrationId":1}]}}]}}))
        .unwrap();
    let mut reported = Vec::new();
    parse::checks(&checks(&[run("1", "SUCCESS")], "head"), &pr(), &mut reported).unwrap();

    let observed = policy.checks(reported);

    assert_eq!(observed.len(), 2);
    assert!(
        observed.iter().any(|check| check.name == "classic" && check.required && check.state == CheckState::Pending)
    );
}

#[test]
fn native_stacks_include_only_open_dependencies_below_the_target() {
    let mut metadata = Metadata::read(
        &json!({"number":7,"state":"OPEN","headRefOid":"h", "baseRefOid":"b", "stackEntry":{"position":3},
        "stack":{"baseRefName":"main"}}),
    )
    .unwrap();
    metadata
        .include(vec![
            json!({"position":1,"pullRequest":{"number":2,"state":"MERGED"}}),
            json!({"position":2,"pullRequest":{"number":3,"state":"OPEN"}}),
            json!({"position":3,"pullRequest":{"number":7,"state":"OPEN"}}),
            json!({"position":4,"pullRequest":{"number":8,"state":"OPEN"}}),
        ])
        .unwrap();

    assert_eq!(metadata.dependencies, [PullRequestNumber::try_from(3).unwrap()]);
    assert_eq!(metadata.destination, "main");
    assert!(metadata.include(vec![json!({"position":2,"pullRequest":null})]).is_err());
}

#[test]
fn incomplete_pagination_is_an_error_not_an_empty_connection() {
    assert!(next(&json!({"pageInfo":{}})).is_err());
    assert!(next(&json!({"pageInfo":{"hasNextPage":true,"endCursor":null}})).is_err());
    assert_eq!(next(&json!({"pageInfo":{"hasNextPage":false}})).unwrap(), None);
}

#[test]
fn partial_graphql_errors_still_charge_the_reported_cost() {
    let mut reader = Reader {
        source: Gh,
        path: "",
        repository: None,
        rate_limit: None,
        cost: 1,
        failed_pr: None,
        dependents: BTreeMap::new(),
    };
    let reply = json!({"data":{"repository":null,"rateLimit":{"cost":5,"limit":5000,"remaining":4995,"resetAt":"2026-01-01T01:00:00Z"}},
        "errors":[{"type":"FORBIDDEN","message":"access lost"}]});

    let error = reader.received(&reply).unwrap_err();

    assert_eq!(error.code, ErrorCode::Unavailable);
    assert_eq!(reader.cost, 5);
    assert_eq!(reader.rate_limit.unwrap().remaining, 4995);
}

#[test]
fn non_admin_protection_keeps_missing_required_contexts_pending() {
    let mut policy = Policy::default();
    policy
        .include(&json!({"branchProtectionRule":null,
        "refUpdateRule":{"requiredStatusCheckContexts":["checks","build"]},"rules":{"nodes":[]}}))
        .unwrap();
    let mut reported = Vec::new();
    parse::checks(&checks(&[run("1", "SUCCESS")], "head"), &pr(), &mut reported).unwrap();

    let observed = policy.checks(reported);

    assert!(observed.iter().any(|check| check.name == "build" && check.required && check.state == CheckState::Pending));
    assert!(observed.iter().any(|check| check.name == "checks" && check.required && check.state == CheckState::Passed));
}

#[test]
fn fallback_names_do_not_weaken_provider_specific_requirements() {
    let mut policy = Policy::default();
    policy.include(&json!({"branchProtectionRule":{"requiresStatusChecks":true,"requiredStatusChecks":[{"context":"checks","app":{"databaseId":2}}]},
        "refUpdateRule":{"requiredStatusCheckContexts":["checks"]},"rules":{"nodes":[]}})).unwrap();
    let mut wrong_provider = run("1", "SUCCESS");
    wrong_provider["isRequired"] = json!(false);
    let mut reported = Vec::new();
    parse::checks(&checks(&[wrong_provider], "head"), &pr(), &mut reported).unwrap();

    let observed = policy.checks(reported);

    assert!(observed.iter().any(|check| check.source == "app:2" && check.state == CheckState::Pending));
    assert!(!observed.iter().any(|check| check.required && check.state == CheckState::Passed));
}

#[test]
fn visible_rules_do_not_satisfy_a_hidden_stricter_provider_requirement() {
    let mut policy = Policy::default();
    policy
        .include(&json!({"branchProtectionRule":null,
        "refUpdateRule":{"requiredStatusCheckContexts":["checks"]},
        "rules":{"nodes":[{"type":"REQUIRED_STATUS_CHECKS", "repositoryRuleset":{"enforcement":"ACTIVE"},
        "parameters":{"requiredStatusChecks":[{"context":"checks","integrationId":null}]}}]}}))
        .unwrap();
    let mut wrong_provider = run("1", "SUCCESS");
    wrong_provider["isRequired"] = json!(false);
    let mut reported = Vec::new();
    parse::checks(&checks(&[wrong_provider.clone()], "head"), &pr(), &mut reported).unwrap();

    let observed = policy.checks(reported);

    assert!(observed.iter().any(|check| check.required && check.state == CheckState::Pending));
    let mut right_provider = run("1", "SUCCESS");
    right_provider["checkSuite"]["app"]["databaseId"] = json!(2);
    let mut reported = Vec::new();
    parse::checks(&checks(&[wrong_provider, right_provider], "head"), &pr(), &mut reported).unwrap();

    let observed = policy.checks(reported);

    assert_eq!(observed.len(), 2);
    assert!(observed.iter().all(|check| check.required && check.state == CheckState::Passed));
}
