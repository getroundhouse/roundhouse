use std::path::Path;
use std::time::Duration;

use github::{Checks, FacetsRead, PullFacets, Reviews, Rules, Stack, Status};
use protocol::model::{
    BranchName, CommitId, GithubRepositoryName, IssueNumber, IssueState, MergeState, Mergeability, PrState, Reference,
    ReferenceKind, ReferenceRead, Review,
};

use super::file::VERSION;
use super::kept::KEPT_FOR;
use super::{GithubCache, Item};
use crate::store::now_ms;

/// Issue `n` of `example/project`.
fn issue(n: u64) -> Reference {
    Reference {
        repository: GithubRepositoryName::named("example/project"),
        number: IssueNumber::try_from(n).unwrap(),
        kind: ReferenceKind::Issue { state: IssueState::Open },
        url: format!("https://github.com/example/project/issues/{n}"),
        title: format!("Issue {n}"),
        author: Some("ada".into()),
        opened_at_ms: 1,
        comments: 0,
        labels: Vec::new(),
        body: String::new(),
    }
}

fn project() -> GithubRepositoryName {
    GithubRepositoryName::named("example/project")
}

/// Open pull request `n`, alone in no stack, with no checks.
fn facets(n: u64) -> PullFacets {
    let status = Status {
        number: n,
        url: format!("https://github.com/example/project/pull/{n}"),
        title: format!("Change {n}"),
        state: PrState::Open,
        is_draft: false,
        base_name: BranchName::from("main"),
        head: CommitId::from("head"),
        base: CommitId::from("base"),
        merge_state: MergeState::Clean,
        mergeable: Mergeability::Mergeable,
        review: Review::NotRequired,
        queue: None,
        left_queue: None,
    };
    let stack = Stack { destination: BranchName::from("main"), dependencies: Vec::new(), linear: Some(true) };
    let checks = Checks { revision: Some(CommitId::from("head")), reported: Vec::new() };
    PullFacets { status, reviews: Reviews { changes_requested_at_ms: None }, stack, checks }
}

fn item(n: u64) -> Item {
    Item::new(&GithubRepositoryName::named("Example/Project"), IssueNumber::try_from(n).unwrap())
}

fn found(n: u64) -> ReferenceRead {
    ReferenceRead::Found { reference: Box::new(issue(n)) }
}

/// The version the file at `path` says it's of.
fn version_of(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap()
}

/// A hub that starts again has what was read before it stopped, what GitHub found and what it said wasn't there, and
/// branches' rules and pull requests' facets, but not why a read failed, which says nothing of what's on GitHub.
#[test]
fn what_was_read_is_kept_across_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("github-cache.db");
    let cache = GithubCache::open(&path);
    cache.keep(
        vec![(item(1), Ok(Some(issue(1)))), (item(2), Ok(None)), (item(3), Err("GitHub is down".into()))],
        now_ms(),
    );
    cache.keep_rules(vec![(project(), BranchName::from("main"), Rules::default())], now_ms());
    cache.keep_pulls(vec![FacetsRead { repository: project(), facets: facets(7), read_at_ms: now_ms() }]);
    drop(cache);

    let again = GithubCache::open(&path);

    assert_eq!(again.summary(&item(1)), Some(found(1)));
    assert_eq!(again.summary(&item(2)), Some(ReferenceRead::Missing));
    assert_eq!(again.summary(&item(3)), None);
    let rules = again.rules_of(&GithubRepositoryName::named("EXAMPLE/project"), Duration::from_secs(60));
    assert_eq!(rules, [(BranchName::from("main"), Rules::default())]);
    assert_eq!(again.pull(&item(7), Duration::from_secs(60)), Some(facets(7)));
}

/// A file a build with another form left, older or newer, is emptied rather than read or migrated, and a damaged one
/// is made again, so either way the hub starts, and reads GitHub again.
#[test]
fn a_file_of_another_form_or_damaged_is_started_again_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("github-cache.db");
    GithubCache::open(&path).keep(vec![(item(1), Ok(Some(issue(1))))], now_ms());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", VERSION + 1).unwrap();
    drop(conn);

    let of_another_form = GithubCache::open(&path);

    assert_eq!(of_another_form.summary(&item(1)), None);
    assert_eq!(version_of(&path), VERSION);
    of_another_form.keep(vec![(item(1), Ok(Some(issue(1))))], now_ms());
    drop(of_another_form);
    std::fs::write(&path, "not a database").unwrap();

    let damaged = GithubCache::open(&path);

    assert_eq!(damaged.summary(&item(1)), None);
    damaged.keep(vec![(item(2), Ok(Some(issue(2))))], now_ms());
    drop(damaged);
    assert_eq!(GithubCache::open(&path).summary(&item(2)), Some(found(2)), "kept in the file made again");
}

/// What nothing has asked about for as long as the cache keeps things is forgotten, in memory and in the file, as the
/// next read is kept, and what's asked about stays.
#[test]
fn what_nothing_asks_about_is_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("github-cache.db");
    let cache = GithubCache::open(&path);
    cache.keep(vec![(item(1), Ok(Some(issue(1)))), (item(2), Ok(Some(issue(2))))], now_ms());
    cache.age(KEPT_FOR.checked_sub(Duration::from_secs(60)).unwrap());
    assert_eq!(cache.summary(&item(1)), Some(found(1)), "asked about, so kept");

    cache.age(Duration::from_secs(120));
    cache.keep(vec![(item(3), Ok(Some(issue(3))))], now_ms());

    assert_eq!(cache.summary(&item(1)), Some(found(1)));
    assert_eq!(cache.summary(&item(2)), None);
    drop(cache);
    let again = GithubCache::open(&path);
    assert_eq!(again.summary(&item(2)), None, "forgotten in the file too");
    assert_eq!(again.summary(&item(3)), Some(found(3)));
}

/// A read that began before another, and lands after it, as a slow one can, leaves what the newer one kept, of a
/// summary, a branch's rules and a pull request's facets alike, so what's shown never goes back.
#[test]
fn a_read_that_lands_late_leaves_what_a_newer_one_kept() {
    let dir = tempfile::tempdir().unwrap();
    let cache = GithubCache::open(&dir.path().join("github-cache.db"));
    let began = now_ms();
    let (older, newer) = (began - 1_000, began);
    let mut merged = facets(7);
    merged.status.state = PrState::Merged;
    let ruled: Rules =
        serde_json::from_value(serde_json::json!({ "required": [["test", null]], "visible_contexts": [] })).unwrap();

    let mut renamed = issue(1);
    renamed.title = "Renamed".into();
    cache.keep(vec![(item(1), Ok(Some(renamed)))], newer);
    cache.keep_rules(vec![(project(), BranchName::from("main"), ruled.clone())], newer);
    cache.keep_pulls(vec![FacetsRead { repository: project(), facets: merged, read_at_ms: newer }]);
    cache.keep(vec![(item(1), Ok(Some(issue(1))))], older);
    cache.keep_rules(vec![(project(), BranchName::from("main"), Rules::default())], older);
    cache.keep_pulls(vec![FacetsRead { repository: project(), facets: facets(7), read_at_ms: older }]);

    let Some(ReferenceRead::Found { reference }) = cache.summary(&item(1)) else { panic!("no summary kept") };
    assert_eq!(reference.title, "Renamed");
    assert_eq!(cache.rules_of(&project(), Duration::MAX), vec![(BranchName::from("main"), ruled)]);
    assert_eq!(cache.pull(&item(7), Duration::MAX).map(|facets| facets.status.state), Some(PrState::Merged));
}
