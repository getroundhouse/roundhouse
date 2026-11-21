use std::path::Path;
use std::time::Duration;

use protocol::model::{GithubRepositoryName, IssueNumber, IssueState, Reference, ReferenceKind, ReferenceRead};

use super::file::VERSION;
use super::kept::KEPT_FOR;
use super::{GithubCache, Item};

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

/// A hub that starts again has what was read before it stopped, what GitHub found and what it said wasn't there, but
/// not why a read failed, which says nothing of what's on GitHub.
#[test]
fn what_was_read_is_kept_across_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("github-cache.db");
    let cache = GithubCache::open(&path);
    cache.keep(vec![(item(1), Ok(Some(issue(1)))), (item(2), Ok(None)), (item(3), Err("GitHub is down".into()))]);
    drop(cache);

    let again = GithubCache::open(&path);

    assert_eq!(again.summary(&item(1)), Some(found(1)));
    assert_eq!(again.summary(&item(2)), Some(ReferenceRead::Missing));
    assert_eq!(again.summary(&item(3)), None);
}

/// A file a build with another form left, older or newer, is emptied rather than read or migrated, and a damaged one
/// is made again, so either way the hub starts, and reads GitHub again.
#[test]
fn a_file_of_another_form_or_damaged_is_started_again_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("github-cache.db");
    GithubCache::open(&path).keep(vec![(item(1), Ok(Some(issue(1))))]);
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", VERSION + 1).unwrap();
    drop(conn);

    let of_another_form = GithubCache::open(&path);

    assert_eq!(of_another_form.summary(&item(1)), None);
    assert_eq!(version_of(&path), VERSION);
    of_another_form.keep(vec![(item(1), Ok(Some(issue(1))))]);
    drop(of_another_form);
    std::fs::write(&path, "not a database").unwrap();

    let damaged = GithubCache::open(&path);

    assert_eq!(damaged.summary(&item(1)), None);
    damaged.keep(vec![(item(2), Ok(Some(issue(2))))]);
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
    cache.keep(vec![(item(1), Ok(Some(issue(1)))), (item(2), Ok(Some(issue(2))))]);
    cache.age(KEPT_FOR.checked_sub(Duration::from_secs(60)).unwrap());
    assert_eq!(cache.summary(&item(1)), Some(found(1)), "asked about, so kept");

    cache.age(Duration::from_secs(120));
    cache.keep(vec![(item(3), Ok(Some(issue(3))))]);

    assert_eq!(cache.summary(&item(1)), Some(found(1)));
    assert_eq!(cache.summary(&item(2)), None);
    drop(cache);
    let again = GithubCache::open(&path);
    assert_eq!(again.summary(&item(2)), None, "forgotten in the file too");
    assert_eq!(again.summary(&item(3)), Some(found(3)));
}
