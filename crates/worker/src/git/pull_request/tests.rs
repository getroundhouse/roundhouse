use protocol::model::{Checks, LeftQueue, Local, MergeState, PrState, PullRequest, QueueState, Review};

use super::super::tests::{new_repo, run};
use super::local::local;
use super::{ByUrl, Failing, Viewed, failing, parse_by_url, parse_pull_request, parse_rate_limit};

#[test]
fn a_pull_request_parses_from_what_gh_prints() {
    let json = br#"{"isDraft":false,"number":12,"state":"OPEN","title":"Fix login","url":"https://example.com/12",
        "baseRefName":"main","headRefName":"aspen","headRefOid":"abc123","commits":[{"oid":"abc122"},{"oid":"abc123"}],
        "mergeStateStatus":"BLOCKED","reviewDecision":"",
        "statusCheckRollup":[
            {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"SUCCESS"},
            {"__typename":"CheckRun","name":"docs","status":"COMPLETED","conclusion":"SKIPPED"},
            {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"FAILURE"},
            {"__typename":"CheckRun","name":"fmt","status":"COMPLETED","conclusion":"CANCELLED"},
            {"__typename":"CheckRun","name":"bench","status":"IN_PROGRESS","conclusion":""},
            {"__typename":"StatusContext","context":"deploy","state":"PENDING"},
            {"__typename":"StatusContext","context":"lint","state":"ERROR"}
        ]}"#;

    let viewed = parse_pull_request(json).unwrap();

    let pr = &viewed.pull_request;
    assert_eq!((pr.number, pr.state, pr.is_draft, &pr.base), (12, PrState::Open, false, &Some("main".into())));
    assert_eq!((pr.merge_state, pr.review), (MergeState::Blocked, Review::NotRequired));
    assert_eq!(pr.checks, Checks { passed: 2, failed: 3, pending: 2 });
    assert_eq!((&viewed.from, &viewed.head), (&Some("aspen".into()), &Some("abc123".into())));
    assert_eq!(viewed.commits(), ["abc122".into(), "abc123".into()]);
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

    let pr = parse_pull_request(json).unwrap().pull_request;

    assert_eq!(pr.checks, Checks { passed: 3, failed: 0, pending: 1 });
}

#[test]
fn what_gh_leaves_out_is_unknown_but_a_pull_request_needs_its_number_and_url() {
    let json = br#"{"number":12,"state":"MERGED","url":"https://example.com/12"}"#;

    let Viewed { pull_request: pr, from, head, first } = parse_pull_request(json).unwrap();

    assert_eq!((pr.state, pr.merge_state, pr.checks), (PrState::Merged, MergeState::Unknown, Checks::default()));
    assert_eq!((from, head, first), (None, None, vec![]));
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
    let queued = br#"{"data":{"resource":{"state":"OPEN","mergeQueueEntry":{"state":"AWAITING_CHECKS"}}}}"#;
    let not_queued = br#"{"data":{"resource":{"mergeQueueEntry":null}}}"#;
    let newer = br#"{"data":{"resource":{"mergeQueueEntry":{"state":"SIDEWAYS"}}}}"#;

    let queue = |json: &[u8]| parse_by_url(json).map(|said| said.queue);
    assert_eq!(queue(queued), Some(Some(QueueState::AwaitingChecks)));
    assert_eq!(queue(not_queued), Some(None));
    assert_eq!(queue(newer), Some(Some(QueueState::Unknown)), "in the queue, somewhere this version doesn't know");
}

#[test]
fn an_answer_that_isnt_the_pull_request_isnt_taken_as_being_in_no_queue() {
    assert_eq!(parse_by_url(b"not json"), None);
    assert_eq!(parse_by_url(br#"{"data":{"resource":null}}"#), None);
    assert_eq!(parse_by_url(br#"{"data":{"resource":{}}}"#), None, "a resource that isn't a pull request");
    assert_eq!(parse_by_url(br#"{"errors":[{"message":"rate limited"}]}"#), None);
}

/// In the form GitHub answers, out of the queue, with its newest removal from it and its latest review asking for
/// changes.
fn said(removal: &str, review: &str) -> ByUrl {
    let json = format!(
        r#"{{"data":{{"resource":{{"state":"OPEN","mergeQueueEntry":null,
            "timelineItems":{{"nodes":[{removal}]}},"reviews":{{"nodes":[{review}]}}}}}}}}"#
    );
    parse_by_url(json.as_bytes()).unwrap()
}

#[test]
fn what_github_records_of_the_queue_and_of_reviews_comes_with_its_time() {
    let failed = said(r#"{"createdAt":"2026-01-02T03:04:05Z","reason":"failed_checks"}"#, "");
    let asked = said("", r#"{"submittedAt":"2026-01-02T03:04:05Z"}"#);
    let never = said("", "");

    let left = LeftQueue { at_ms: 1_767_323_045_000, reason: "failed_checks".to_string() };
    assert_eq!((failed.left_queue, failed.state), (Some(left), Some(PrState::Open)));
    assert_eq!(asked.changes_requested_at_ms, Some(1_767_323_045_000));
    assert_eq!((never.left_queue, never.changes_requested_at_ms), (None, None));
}

/// GitHub records the queue's removal as it merges one a moment before it calls it merged.
#[test]
fn a_pull_request_the_queue_has_merged_is_merged_and_hasnt_left_it() {
    let merged = said(r#"{"createdAt":"2026-01-02T03:10:00Z","reason":"merged"}"#, "");

    assert_eq!((merged.state, merged.left_queue), (Some(PrState::Merged), None));
}

#[test]
fn the_second_calls_word_on_its_state_is_the_later_one() {
    let json = br#"{"number":7,"url":"https://example.com/7","state":"OPEN"}"#;
    let mut viewed = parse_pull_request(json).unwrap().pull_request;
    let said = |state| ByUrl { state, queue: None, left_queue: None, changes_requested_at_ms: Some(5) };
    let (merged, unsaid) = (said(Some(PrState::Merged)), said(None));

    merged.fill(&mut viewed);
    let mut again = PullRequest { state: PrState::Open, ..viewed.clone() };
    unsaid.fill(&mut again);

    assert_eq!((viewed.state, viewed.changes_requested_at_ms), (PrState::Merged, Some(5)));
    assert_eq!(again.state, PrState::Open, "when it doesn't say, gh pr view's word stands");
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

/// Just after a push, GitHub can still say the pull request's head is the commit before.
#[tokio::test]
async fn a_commit_pushed_before_github_says_so_isnt_counted_as_not_pushed() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let before = run(&repo, &["rev-parse", "HEAD"]);
    std::fs::write(repo.join("b.txt"), "two\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "second"]);
    // As `git push` leaves it.
    run(&repo, &["update-ref", "refs/remotes/origin/feature", "HEAD"]);

    let lagging = local(repo.to_str().unwrap(), Some(&before)).await;

    assert_eq!(lagging, Local { uncommitted: 0, ahead: 0, behind: 0 });
}
