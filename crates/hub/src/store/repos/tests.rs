use protocol::model::{Repo, RepoOrigin, RepoUrl};

use crate::store::Store;
use crate::store::older::store_from_before;
use crate::store::schema::{BEFORE_PUSH_ROUTES, BEFORE_REPO_URLS};
use crate::store::tests::open_in;

fn roundhouse() -> RepoUrl {
    RepoUrl::try_from("https://github.com/ada/roundhouse.git".to_owned()).unwrap()
}

fn added(store: &Store) -> Repo {
    store.add_repo(&"r".into(), "repo", "/repo", 0).unwrap()
}

/// Which repositories a worker on the hub's machine is to be asked about, by ID.
fn to_read(store: &Store) -> Vec<String> {
    store.repos_to_read().unwrap().into_iter().map(|repo| repo.id.to_string()).collect()
}

#[test]
fn a_repository_from_before_addresses_is_its_checkout_whose_address_is_to_be_read() {
    let dir = tempfile::tempdir().unwrap();

    let store = store_from_before(dir.path(), BEFORE_REPO_URLS, "");

    let repo = store.get_repo(&"r".into()).unwrap();
    assert_eq!(repo.origin, RepoOrigin::Checkout { path: "/repo".into() });
    assert_eq!(to_read(&store), ["r"]);
}

/// So a client takes it in with the rest, and doesn't take an address read again for a change.
#[test]
fn an_address_read_is_kept_once_and_moves_the_revision_only_when_it_changes_the_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let repo = added(&store);
    assert_eq!(to_read(&store), ["r"], "a new one is to be read");

    let linked = store.read_repo_url(&repo.id, "/repo", Some(&roundhouse())).unwrap();
    let answered_again = store.read_repo_url(&repo.id, "/repo", None).unwrap();
    let same_again = {
        added(&store);
        store.read_repo_url(&repo.id, "/repo", Some(&roundhouse())).unwrap()
    };

    let linked = linked.expect("the address changed the record");
    assert_eq!(linked.origin, RepoOrigin::Linked { path: "/repo".into(), url: roundhouse() });
    assert!(linked.rev > repo.rev);
    assert_eq!((answered_again, same_again), (None, None), "nothing changed");
    assert_eq!(store.get_repo(&repo.id).unwrap(), linked);
    assert_eq!(store.list_records().unwrap().rev, linked.rev, "nothing since moved the hub's revision");
    assert_eq!(to_read(&store), [] as [String; 0]);
}

#[test]
fn adding_a_checkout_again_returns_its_record_and_has_its_address_read_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let first = added(&store);
    let linked = store.read_repo_url(&first.id, "/repo", Some(&roundhouse())).unwrap().unwrap();

    let again = store.add_repo(&"other".into(), "repo", "/repo", 1).unwrap();
    let to_read_again = to_read(&store);
    let gone = store.read_repo_url(&first.id, "/repo", None).unwrap();

    assert_eq!(again, linked, "the record is as it was, at the same revision");
    assert_eq!(to_read_again, ["r"]);
    let gone = gone.expect("its origin having gone changed the record");
    assert_eq!(gone.origin, RepoOrigin::Checkout { path: "/repo".into() });
    assert!(gone.rev > linked.rev);
}

#[test]
fn an_address_read_from_another_checkout_than_the_repositorys_is_not_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let repo = added(&store);

    let kept = store.read_repo_url(&repo.id, "/elsewhere", Some(&roundhouse())).unwrap();

    assert_eq!(kept, None);
    assert_eq!(store.get_repo(&repo.id).unwrap(), repo);
    assert_eq!(to_read(&store), ["r"]);
}

/// One whose address a worker read before workers said where pushes go is to be read again, so a worker says, with
/// its record as it was.
#[test]
fn a_repository_read_before_pushes_were_said_of_is_read_again_with_its_record_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let read = "UPDATE repos SET url = 'https://github.com/ada/roundhouse.git', url_to_read = 0;";

    let store = store_from_before(dir.path(), BEFORE_PUSH_ROUTES, read);

    let repo = store.get_repo(&"r".into()).unwrap();
    assert_eq!(repo.origin, RepoOrigin::Linked { path: "/repo".into(), url: roundhouse() });
    assert_eq!(to_read(&store), ["r"]);
}
