use protocol::model::{Checks, Local, MergeState, PrState, QueueState, Review};

use super::super::tests::{new_repo, run};
use super::{local, parse_pull_request, parse_queue};

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
