use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use github::{Checks, FacetsRead, PullFacets, Rules};
use protocol::model::{
    About, BranchName, GithubRepositoryName, Issue, IssueNumber, IssueState, IssueSummary, PrState, PullRequest, Read,
    Reference, ReferenceRead,
};

use super::file::VERSION;
use super::kept::KEPT_FOR;
use super::{GithubCache, Item};
use crate::cache_file::CacheFile;
use crate::store::columns::OneValue;
use crate::store::now_ms;

/// Issue `n` of `example/project`.
fn issue(n: u64) -> Reference {
    let about = About {
        url: format!("https://github.com/example/project/issues/{n}"),
        title: format!("Issue {n}"),
        author: Some("ada".into()),
        opened_at_ms: 1,
        comments: 0,
        labels: Vec::new(),
        body: String::new(),
    };
    Reference::Issue(Issue {
        repository: project(),
        number: IssueNumber::try_from(n).unwrap(),
        summary: Read { said: IssueSummary { about, state: IssueState::Open }, read_at_ms: 1 },
    })
}

fn project() -> GithubRepositoryName {
    GithubRepositoryName::named("example/project")
}

/// Open pull request `n`, alone in no stack, with no checks, every facet read at `read_at_ms`.
fn pull_request(n: u64, read_at_ms: i64) -> PullRequest {
    let mut pull_request = protocol::fixtures::pull_request(n);
    pull_request.summary.read_at_ms = read_at_ms;
    for read_at in [
        pull_request.history.as_mut().map(|read| &mut read.read_at_ms),
        pull_request.stack.as_mut().map(|read| &mut read.read_at_ms),
    ]
    .into_iter()
    .flatten()
    {
        *read_at = read_at_ms;
    }
    pull_request.checks = None;
    pull_request
}

/// `pull_request` with its summary alone, as a read of summaries keeps it.
fn summary_of(pull_request: &PullRequest) -> Reference {
    Reference::PullRequest(PullRequest { history: None, stack: None, checks: None, ..pull_request.clone() })
}

/// What a read of pull requests found of `pull_request`, with no checks reported.
fn facets(pull_request: &PullRequest) -> PullFacets {
    let summary = pull_request.said().clone();
    let checks = Checks { head: summary.head.clone(), revision: Some(summary.head.clone()), reported: Vec::new() };
    PullFacets {
        summary,
        history: pull_request.history_said().unwrap().clone(),
        stack: pull_request.stack_said().unwrap().clone(),
        checks,
    }
}

/// Keeps `pull_request`, as a read of pull requests does: its summary, then its other facets.
fn keep_pull(cache: &GithubCache, pull_request: &PullRequest) {
    let read_at_ms = pull_request.summary.read_at_ms;
    let item = item(pull_request.number.get());
    cache.keep(vec![(item, Ok(Some(summary_of(pull_request))))], read_at_ms);
    let (number, facets) = (pull_request.number, facets(pull_request));
    cache.keep_pulls(vec![FacetsRead { repository: project(), number, facets, read_at_ms }]);
}

/// What the cache gives of `pull_request`, kept as [`keep_pull`] keeps it, with no checks reported, which its branch's
/// rules judge.
fn given(pull_request: &PullRequest) -> PullRequest {
    let revision = Some(pull_request.said().head.clone());
    let said = protocol::model::Checks { revision, requirements_known: true, checks: Vec::new() };
    PullRequest { checks: Some(Read { said, read_at_ms: pull_request.summary.read_at_ms }), ..pull_request.clone() }
}

fn item(n: u64) -> Item {
    Item::new(&GithubRepositoryName::named("Example/Project"), IssueNumber::try_from(n).unwrap())
}

fn found(n: u64) -> ReferenceRead {
    ReferenceRead::Found { reference: Box::new(issue(n)) }
}

/// The cache kept in the file at `path`, which has GitHub's part alone.
fn open(path: &Path) -> GithubCache {
    GithubCache::open(Arc::new(CacheFile::open(path, &[super::prepare])))
}

/// The version the file at `path` says it's of.
fn version_of(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.pragma_query_value(None, "user_version", OneValue::read).unwrap()
}

/// A hub that starts again has what was read before it stopped, what GitHub found and what it said wasn't there, and
/// branches' rules and pull requests' facets, but not why a read failed, which says nothing of what's on GitHub.
#[test]
fn what_was_read_is_kept_across_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("github-cache.db");
    let cache = open(&path);
    cache.keep(
        vec![(item(1), Ok(Some(issue(1)))), (item(2), Ok(None)), (item(3), Err("GitHub is down".into()))],
        now_ms(),
    );
    cache.keep_rules(vec![(project(), BranchName::from("main"), Rules::default())], now_ms());
    let seven = pull_request(7, now_ms());
    keep_pull(&cache, &seven);
    drop(cache);

    let again = open(&path);

    assert_eq!(again.summary(&item(1)), Some(found(1)));
    assert_eq!(again.summary(&item(2)), Some(ReferenceRead::Missing));
    assert_eq!(again.summary(&item(3)), None);
    let rules = again.rules_of(&GithubRepositoryName::named("EXAMPLE/project"), Duration::from_secs(60));
    assert_eq!(rules, [(BranchName::from("main"), Rules::default())]);
    let fresh = Duration::from_secs(60);
    assert_eq!(again.fresh_pull_request(&item(7), fresh, fresh), Some(given(&seven)));
}

/// A file a build with another form left, older or newer, is emptied rather than read or migrated, and a damaged one
/// is made again, so either way the hub starts, and reads GitHub again.
#[test]
fn a_file_of_another_form_or_damaged_is_started_again_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("github-cache.db");
    open(&path).keep(vec![(item(1), Ok(Some(issue(1))))], now_ms());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", VERSION + 1).unwrap();
    drop(conn);

    let of_another_form = open(&path);

    assert_eq!(of_another_form.summary(&item(1)), None);
    assert_eq!(version_of(&path), VERSION);
    of_another_form.keep(vec![(item(1), Ok(Some(issue(1))))], now_ms());
    drop(of_another_form);
    std::fs::write(&path, "not a database").unwrap();

    let damaged = open(&path);

    assert_eq!(damaged.summary(&item(1)), None);
    damaged.keep(vec![(item(2), Ok(Some(issue(2))))], now_ms());
    drop(damaged);
    assert_eq!(open(&path).summary(&item(2)), Some(found(2)), "kept in the file made again");
}

/// What nothing has asked about for as long as the cache keeps things is forgotten, in memory and in the file, as the
/// next read is kept, and what's asked about stays.
#[test]
fn what_nothing_asks_about_is_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("github-cache.db");
    let cache = open(&path);
    cache.keep(vec![(item(1), Ok(Some(issue(1)))), (item(2), Ok(Some(issue(2))))], now_ms());
    cache.age(KEPT_FOR.checked_sub(Duration::from_secs(60)).unwrap());
    assert!(cache.summary(&item(1)).is_some(), "asked about, so kept");

    cache.age(Duration::from_secs(120));
    cache.keep(vec![(item(3), Ok(Some(issue(3))))], now_ms());

    assert!(cache.summary(&item(1)).is_some());
    assert_eq!(cache.summary(&item(2)), None);
    drop(cache);
    let again = open(&path);
    assert_eq!(again.summary(&item(2)), None, "forgotten in the file too");
    assert_eq!(again.summary(&item(3)), Some(found(3)));
}

/// A read that began before another, and lands after it, as a slow one can, leaves what the newer one kept, of a
/// summary, a branch's rules and a pull request's facets alike, so what's shown never goes back.
#[test]
fn a_read_that_lands_late_leaves_what_a_newer_one_kept() {
    let dir = tempfile::tempdir().unwrap();
    let cache = open(&dir.path().join("github-cache.db"));
    let began = now_ms();
    let (older, newer) = (began - 1_000, began);
    let mut merged = pull_request(7, newer);
    merged.summary.said.state = PrState::Merged;
    let ruled: Rules =
        serde_json::from_value(serde_json::json!({ "required": [["test", null]], "visible_contexts": [] })).unwrap();

    let Reference::Issue(mut renamed) = issue(1) else { unreachable!() };
    renamed.summary.said.about.title = "Renamed".into();
    let renamed = Reference::Issue(renamed);
    cache.keep(vec![(item(1), Ok(Some(renamed)))], newer);
    cache.keep_rules(vec![(project(), BranchName::from("main"), ruled.clone())], newer);
    keep_pull(&cache, &merged);
    cache.keep(vec![(item(1), Ok(Some(issue(1))))], older);
    cache.keep_rules(vec![(project(), BranchName::from("main"), Rules::default())], older);
    keep_pull(&cache, &pull_request(7, older));

    let Some(ReferenceRead::Found { reference }) = cache.summary(&item(1)) else { panic!("no summary kept") };
    assert_eq!(reference.about().map(|about| about.title.as_str()), Some("Renamed"));
    assert_eq!(cache.rules_of(&project(), Duration::MAX), vec![(BranchName::from("main"), ruled)]);
    assert_eq!(cache.fresh_pull_request(&item(7), Duration::MAX, Duration::MAX), Some(given(&merged)));
}

/// A summary read since a pull request's checks were, as a chip's read can be, at a head they weren't run on, leaves
/// them out, so the commit before a push isn't taken for its head: the pull request is known without them, as checks
/// not read yet, and isn't fresh.
#[test]
fn checks_of_another_head_than_the_summarys_are_left_out() {
    let dir = tempfile::tempdir().unwrap();
    let cache = open(&dir.path().join("github-cache.db"));
    cache.keep_rules(vec![(project(), BranchName::from("main"), Rules::default())], now_ms());
    let read = pull_request(7, now_ms());
    keep_pull(&cache, &read);
    let fresh = Duration::from_secs(60);
    assert_eq!(cache.fresh_pull_request(&item(7), fresh, fresh), Some(given(&read)), "of the head they ran on");

    let mut pushed = read.clone();
    pushed.summary.said.head = "pushed".into();
    cache.keep(vec![(item(7), Ok(Some(summary_of(&pushed))))], now_ms());

    let known = cache.pull_request(&item(7)).unwrap();
    assert_eq!(known.said().head, "pushed".into());
    assert_eq!(known.checks, None, "not the head's");
    assert!(known.history.is_some() && known.stack.is_some(), "the rest still known");
    assert_eq!(cache.fresh_pull_request(&item(7), fresh, fresh), None);
}

/// An open pull request's checks can't be judged without its branch's rules, so it isn't fresh without them, but a
/// merged one's need none.
#[test]
fn an_open_pull_request_is_fresh_only_with_its_branchs_rules() {
    let dir = tempfile::tempdir().unwrap();
    let cache = open(&dir.path().join("github-cache.db"));
    let open = pull_request(7, now_ms());
    keep_pull(&cache, &open);
    let fresh = Duration::from_secs(60);
    assert_eq!(cache.fresh_pull_request(&item(7), fresh, fresh), None, "no rules");
    assert!(cache.pull_request(&item(7)).unwrap().checks_said().is_some_and(|checks| !checks.requirements_known));

    cache.keep_rules(vec![(project(), BranchName::from("main"), Rules::default())], now_ms());

    assert_eq!(cache.fresh_pull_request(&item(7), fresh, fresh), Some(given(&open)));
}
