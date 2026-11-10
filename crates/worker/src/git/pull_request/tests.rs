use protocol::model::{Checks, Local, MergeState, PrState, QueueState, Review};

use super::super::tests::{new_repo, run};
use super::{Failing, failing, local, parse_pull_request, parse_queue, parse_rate_limit};

#[test]
fn a_pull_request_parses_from_what_gh_prints() {
    let json = br#"{"isDraft":false,"number":12,"state":"OPEN","title":"Fix login","url":"https://example.com/12",
        "baseRefName":"main","headRefOid":"abc123","mergeStateStatus":"BLOCKED","reviewDecision":"",
        "statusCheckRollup":[
            {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"SUCCESS"},
            {"__typename":"CheckRun","name":"docs","status":"COMPLETED","conclusion":"SKIPPED"},
            {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"FAILURE"},
            {"__typename":"CheckRun","name":"fmt","status":"COMPLETED","conclusion":"CANCELLED"},
            {"__typename":"CheckRun","name":"bench","status":"IN_PROGRESS","conclusion":""},
            {"__typename":"StatusContext","context":"deploy","state":"PENDING"},
            {"__typename":"StatusContext","context":"lint","state":"ERROR"}
        ]}"#;

    let (pr, head) = parse_pull_request(json).unwrap();

    assert_eq!((pr.number, pr.state, pr.is_draft, pr.base.as_deref()), (12, PrState::Open, false, Some("main")));
    assert_eq!((pr.merge_state, pr.review), (MergeState::Blocked, Review::NotRequired));
    assert_eq!(pr.checks, Checks { passed: 2, failed: 3, pending: 2 });
    assert_eq!(head.as_deref(), Some("abc123"));
}

#[test]
fn each_check_counts_once_as_its_latest_run() {
    // As GitHub reports a check cancelled for a newer push, then run again, and one queued again after failing.
    let json = br#"{"number":12,"url":"https://example.com/12","statusCheckRollup":[
        {"__typename":"CheckRun","name":"checks","workflowName":"CI","status":"COMPLETED","conclusion":"CANCELLED",
            "startedAt":"2026-11-02T10:00:00Z"},
        {"__typename":"CheckRun","name":"checks","workflowName":"CI","status":"COMPLETED","conclusion":"SUCCESS",
            "startedAt":"2026-11-02T10:05:00Z"},
        {"__typename":"CheckRun","name":"checks","workflowName":"Release","status":"COMPLETED","conclusion":"SUCCESS",
            "startedAt":"2026-11-02T09:00:00Z"},
        {"__typename":"CheckRun","name":"mac","workflowName":"CI","status":"QUEUED","conclusion":"",
            "startedAt":"0001-01-01T00:00:00Z"},
        {"__typename":"CheckRun","name":"mac","workflowName":"CI","status":"COMPLETED","conclusion":"FAILURE",
            "startedAt":"2026-11-02T10:01:00Z"},
        {"__typename":"StatusContext","context":"deploy","state":"FAILURE","startedAt":"2026-11-02T10:00:00Z"},
        {"__typename":"StatusContext","context":"deploy","state":"SUCCESS","startedAt":"2026-11-02T10:02:00Z"}
    ]}"#;

    let (pr, _) = parse_pull_request(json).unwrap();

    assert_eq!(pr.checks, Checks { passed: 3, failed: 0, pending: 1 });
}

#[test]
fn what_gh_leaves_out_is_unknown_but_a_pull_request_needs_its_number_and_url() {
    let json = br#"{"number":12,"state":"MERGED","url":"https://example.com/12"}"#;

    let (pr, head) = parse_pull_request(json).unwrap();

    assert_eq!((pr.state, pr.merge_state, pr.checks), (PrState::Merged, MergeState::Unknown, Checks::default()));
    assert_eq!(head, None);
    assert_eq!(parse_pull_request(b"not json"), None);
    assert_eq!(parse_pull_request(br#"{"state":"OPEN"}"#), None);
}

/// As `gh api rate_limit` prints it, cut to what's read.
#[test]
fn the_graphql_limit_is_read_from_githubs_rate_limit() {
    let json = br#"{"resources":{"core":{"limit":5000,"remaining":4965,"reset":1794281542,"used":35},
        "graphql":{"limit":5000,"remaining":4702,"reset":1794337867,"used":298}},
        "rate":{"limit":5000,"remaining":4965,"reset":1794281542,"used":35}}"#;

    let limit = parse_rate_limit(json).unwrap();

    assert_eq!((limit.limit, limit.remaining, limit.resets_at_ms), (5000, 4702, 1_794_337_867_000));
    assert_eq!(parse_rate_limit(br#"{"resources":{"core":{"limit":5000}}}"#), None);
}

/// In `gh`'s own words, as version 2.103 prints them. Only saying there's none is an answer, so only it can take the
/// place of a pull request already known. Needing a login, and a repository with no GitHub remote, stay that way until
/// someone changes them, and anything else might work next time.
#[test]
fn what_a_failing_gh_means() {
    assert_eq!(failing(Some(1), "no pull requests found for branch \"persimmon\"\n"), Failing::NoneFound);
    assert_eq!(failing(Some(1), "no git remotes found\n"), Failing::CantWorkHere);
    let elsewhere = "none of the git remotes configured for this repository point to a known GitHub host.";
    assert_eq!(failing(Some(1), elsewhere), Failing::CantWorkHere);
    assert_eq!(failing(Some(4), "To get started with GitHub CLI, please run:  gh auth login\n"), Failing::CantWorkHere);
    assert_eq!(failing(Some(1), "error connecting to api.github.com\n"), Failing::CouldntSay);
    assert_eq!(failing(None, ""), Failing::CouldntSay);
}

#[test]
fn the_queue_is_read_from_githubs_answer_and_none_without_an_entry() {
    let queued = br#"{"data":{"resource":{"mergeQueueEntry":{"state":"AWAITING_CHECKS"}}}}"#;
    let not_queued = br#"{"data":{"resource":{"mergeQueueEntry":null}}}"#;

    assert_eq!(parse_queue(queued), Some(QueueState::AwaitingChecks));
    assert_eq!(parse_queue(not_queued), None);
    assert_eq!(parse_queue(b"not json"), None);
}

#[tokio::test]
async fn the_worktree_is_counted_against_the_pull_requests_head() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let pushed = run(&repo, &["rev-parse", "HEAD"]);
    std::fs::write(repo.join("b.txt"), "two\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "second"]);
    std::fs::write(repo.join("c.txt"), "three\n").unwrap();
    let path = repo.to_str().unwrap();

    let ahead = local(path, Some(&pushed)).await;
    let elsewhere = local(path, Some("0123456789abcdef0123456789abcdef01234567")).await;
    let unknown = local(path, None).await;

    assert_eq!(ahead, Local { uncommitted: 1, ahead: 1, behind: 0 });
    // A head that isn't here is a commit to pull, however far the upstream says it is, and this branch has none.
    assert_eq!(elsewhere, Local { uncommitted: 1, ahead: 0, behind: 1 });
    assert_eq!(unknown, Local { uncommitted: 1, ahead: 0, behind: 0 });
}
