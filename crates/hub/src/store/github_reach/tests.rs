use protocol::model::{GithubRepositoryName, Reach};

use crate::store::Store;
use crate::store::older::store_from_before;
use crate::store::schema::BEFORE_GITHUB_REACH;
use crate::store::tests::open_in;

const CLI: GithubRepositoryName = GithubRepositoryName::named("cli/cli");

fn pushes() -> Reach {
    Reach::Pushes { repository: CLI, username: "ada".into() }
}

fn with_repos(store: &Store, ids: &[&str]) {
    for id in ids {
        store.add_repo(&(*id).into(), id, &format!("/{id}"), 0).unwrap();
    }
}

#[test]
fn a_hub_from_before_reach_has_none_and_records_one_once_migrated() {
    let dir = tempfile::tempdir().unwrap();

    let store = store_from_before(dir.path(), BEFORE_GITHUB_REACH, "");

    assert_eq!(store.list_records().unwrap().github_reach, []);
    let reach = store.record_github_reach(&"r".into(), &pushes(), Some(5)).unwrap();
    assert_eq!(store.list_records().unwrap().github_reach, [reach]);
    assert_eq!(store.github_reach_of(&"r".into()).unwrap().map(|r| r.answered_at_ms), Some(Some(5)));
}

/// So a client takes each in with the rest, and doesn't take one checked again for a change.
#[test]
fn each_repository_has_one_whose_revision_moves_only_when_it_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    with_repos(&store, &["a", "b"]);
    let before = store.list_records().unwrap().rev;

    let a = store.record_github_reach(&"a".into(), &pushes(), Some(5)).unwrap();
    let b = store.record_github_reach(&"b".into(), &Reach::NotOnGithub, None).unwrap();
    let same_again = store.record_github_reach(&"a".into(), &pushes(), Some(5)).unwrap();
    let answered_again = store.record_github_reach(&"a".into(), &pushes(), Some(6)).unwrap();
    let reason = "Permission denied.".into();
    let refused = Reach::Refused { repository: CLI, username: "ada".into(), reason, installation: None };
    let changed = store.record_github_reach(&"a".into(), &refused, Some(7)).unwrap();

    assert!(before < a.rev && a.rev < b.rev && b.rev < answered_again.rev && answered_again.rev < changed.rev);
    assert_eq!(same_again, a);
    assert_eq!((answered_again.answered_at_ms, b.answered_at_ms), (Some(6), None));
    assert_eq!(changed.reach, refused);
    let listed = store.list_records().unwrap();
    assert_eq!(listed.github_reach, [changed.clone(), b]);
    assert_eq!(listed.rev, changed.rev);
    assert_eq!(store.github_reaches().unwrap(), listed.github_reach);
}

/// As the hub starts, so what GitHub said in the last run reads as of then, with when it said it, until it's checked
/// again, which makes it this run's, even when GitHub's answer is the same.
#[test]
fn what_the_last_run_left_is_stale_until_its_recorded_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    with_repos(&store, &["a", "b"]);
    let a = store.record_github_reach(&"a".into(), &pushes(), Some(5)).unwrap();
    store.record_github_reach(&"b".into(), &Reach::NotOnGithub, None).unwrap();

    store.github_reach_left_by_the_last_run().unwrap();

    let left = store.github_reaches().unwrap();
    assert!(left.iter().all(|reach| reach.stale), "{left:?}");
    assert!(left[0].rev > a.rev, "a change every client is told of");
    assert_eq!((left[0].reach.clone(), left[0].answered_at_ms), (pushes(), Some(5)), "as GitHub said, and when");
    let checked = store.record_github_reach(&"a".into(), &pushes(), Some(5)).unwrap();
    assert!(!checked.stale && checked.rev > left[0].rev);
    assert!(store.github_reach_of(&"b".into()).unwrap().unwrap().stale, "until it's checked too");
}

/// As a newer build might have left it, which this one checks again rather than reading as another reach.
#[test]
fn a_reach_this_build_cant_read_reads_as_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let rows = r#"INSERT INTO github_reach (repo_id, reach) VALUES ('r', '{"state":"pushes_on_tuesdays"}');"#;

    let store = store_from_before(dir.path(), BEFORE_GITHUB_REACH + 1, rows);

    assert_eq!(store.github_reaches().unwrap()[0].reach, Reach::Unknown);
}
