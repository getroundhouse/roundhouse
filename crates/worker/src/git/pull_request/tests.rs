use std::path::Path;

use crate::testing::given;
use process::Environment;
use protocol::model::{BranchName, Checks, LeftQueue, Local, MergeState, PrState, PullRequest, QueueState, Review};

use super::super::tests::{new_repo, run};
use super::link::linked;
use super::local::local;
use super::pull_request;
use super::{ByUrl, Failing, Viewed, failing, parse_by_url, parse_commits, parse_pull_request, parse_rate_limit};

#[test]
fn a_pull_request_parses_from_what_gh_prints() {
    let json = br#"{"isDraft":false,"number":12,"state":"OPEN","title":"Fix login","url":"https://example.com/12",
        "baseRefName":"main","headRefName":"aspen","headRefOid":"abc123",
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
}

/// As `gh pr view <url> --json headRefOid,commits` prints them. It lists the first hundred, so the newest of a pull
/// request with more comes after them.
#[test]
fn a_pull_requests_commits_are_its_first_hundred_and_its_newest() {
    let few = br#"{"headRefOid":"abc123","commits":[{"oid":"abc122"},{"oid":"abc123"}]}"#;
    let many = br#"{"headRefOid":"abc999","commits":[{"oid":"abc122"},{"oid":"abc123"}]}"#;
    let unlisted = br#"{"headRefOid":"abc123"}"#;

    assert_eq!(parse_commits(few), Some(vec!["abc122".into(), "abc123".into()]));
    assert_eq!(parse_commits(many), Some(vec!["abc122".into(), "abc123".into(), "abc999".into()]));
    assert_eq!(parse_commits(unlisted), Some(vec!["abc123".into()]));
    assert_eq!(parse_commits(b"not json"), None);
    assert_eq!(parse_commits(b"[]"), None, "not a pull request");
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

    let Viewed { pull_request: pr, from, head } = parse_pull_request(json).unwrap();

    assert_eq!((pr.state, pr.merge_state, pr.checks), (PrState::Merged, MergeState::Unknown, Checks::default()));
    assert_eq!((from, head), (None, None));
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

    let ahead = local(&given(), path, Some(&pushed)).await;
    let elsewhere = local(&given(), path, Some("0123456789abcdef0123456789abcdef01234567")).await;
    let unknown = local(&given(), path, None).await;

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

    let lagging = local(&given(), repo.to_str().unwrap(), Some(&before)).await;

    assert_eq!(lagging, Local { uncommitted: 0, ahead: 0, behind: 0 });
}

/// A stand-in for `gh`, first on the `PATH` of what it's given, which writes down what it's asked in `dir/asked`, and
/// answers `gh pr view` from files in `dir`: by the branch's name from `by-name`, and by a URL ending in a number from
/// `pr-` and that number, as `gh` prints them, commits and all. With no file, it says there's none, as `gh` does. It
/// says no pull request is in a merge queue, and fails anything else.
fn gh(dir: &Path) -> Environment {
    use std::os::unix::fs::PermissionsExt as _;
    let (bin, at) = (dir.join("bin"), dir.display());
    std::fs::create_dir_all(&bin).unwrap();
    let script = format!(
        r#"#!/bin/sh
printf '%s\n' "$*" >> '{at}/asked'
case "$1 $2" in
'api graphql') echo '{{"data":{{"resource":{{"mergeQueueEntry":null}}}}}}'; exit 0 ;;
'pr view') ;;
*) exit 9 ;;
esac
case "$3" in
http*) said='{at}/pr-'"${{3##*/}}" ;;
*) said='{at}/by-name' ;;
esac
[ -f "$said" ] && exec cat "$said"
echo 'no pull requests found for branch "aspen"' >&2
exit 1
"#
    );
    std::fs::write(bin.join("gh"), script).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    given().with("PATH", std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&path))).unwrap())
}

/// An open pull request numbered `number`, from a branch named `aspen`, whose one commit is `commit`.
fn open_at(number: u64, commit: &str) -> String {
    format!(
        r#"{{"number":{number},"url":"https://example.com/pull/{number}","title":"T","state":"OPEN",
        "headRefName":"aspen","headRefOid":"{commit}","commits":[{{"oid":"{commit}"}}]}}"#
    )
}

/// What each run of the stand-in in `dir` asked, since the last look, in short: `by name`, or the number of one asked
/// for by its URL, each with `commits of` before it when it asked for those, and `queue` for its place in the queue.
fn asked_since(dir: &Path) -> Vec<String> {
    let asked = std::fs::read_to_string(dir.join("asked")).unwrap_or_default();
    std::fs::remove_file(dir.join("asked")).unwrap_or_default();
    let short = |line: &str| {
        let args: Vec<&str> = line.split(' ').collect();
        let (which, fields) = match args.as_slice() {
            ["api", "graphql", ..] => return "queue".to_string(),
            ["pr", "view", "--json", fields] => ("by name".to_string(), *fields),
            ["pr", "view", url, "--json", fields] => (url.rsplit('/').next().unwrap().to_string(), *fields),
            _ => return format!("something else: {line}"),
        };
        if fields.split(',').any(|field| field == "commits") { format!("commits of {which}") } else { which }
    };
    asked.lines().map(short).collect()
}

/// Which pull request is a branch's is `of_branch`'s to say, as the table of `choose` in `link/tests.rs` has it. Here,
/// through a stand-in for `gh`, is what each read of it asks GitHub, and what it links: the one found by the branch's
/// name is asked for its commits only until the branch is linked to it, and once another has the name, the linked one
/// is read by its URL. A branch with none has none, rather than a failure.
#[tokio::test]
async fn a_found_pull_request_s_commits_are_asked_for_only_until_the_branch_is_linked_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    run(&repo, &["switch", "-q", "-c", "aspen"]);
    run(&repo, &["commit", "-q", "--allow-empty", "-m", "work"]);
    let own = open_at(7, &run(&repo, &["rev-parse", "HEAD"]));
    let someone_elses = open_at(9, "fedcba9876543210fedcba9876543210fedcba98");
    std::fs::write(dir.path().join("pr-7"), &own).unwrap();
    std::fs::write(dir.path().join("pr-9"), &someone_elses).unwrap();
    let (given, branch) = (gh(dir.path()), BranchName::from("aspen"));
    let read = async |by_name: Option<&str>| {
        match by_name {
            Some(found) => std::fs::write(dir.path().join("by-name"), found).unwrap(),
            None => std::fs::remove_file(dir.path().join("by-name")).unwrap_or_default(),
        }
        let read = pull_request(&given, repo.to_str().unwrap(), Some(&branch)).await.unwrap();
        let linked = linked(&given, &repo, &branch).await.unwrap();
        (read.map(|read| read.number), asked_since(dir.path()), linked)
    };
    let url = Some("https://example.com/pull/7".to_string());

    let none = read(None).await;
    let only_the_name = read(Some(&someone_elses)).await;
    let found = read(Some(&own)).await;
    let linked = read(Some(&own)).await;
    let by_its_link = read(Some(&someone_elses)).await;

    assert_eq!(none, (None, vec!["by name".into()], None), "none");
    assert_eq!(only_the_name, (None, vec!["by name".into(), "commits of 9".into()], None), "another's, by name");
    assert_eq!(found, (Some(7), vec!["by name".into(), "commits of 7".into(), "queue".into()], url.clone()), "found");
    assert_eq!(linked, (Some(7), vec!["by name".into(), "queue".into()], url.clone()), "linked");
    let read_by_link = vec!["by name".into(), "commits of 9".into(), "7".into(), "queue".into()];
    assert_eq!(by_its_link, (Some(7), read_by_link, url), "the branch's own by its link, once another has the name");
}
