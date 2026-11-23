use protocol::model::{AddressForm, GithubRepositoryName, Linking, Repo, RepoLink, RepoOrigin, RepoUrl, Unlinked};

use crate::store::Store;
use crate::store::older::store_from_before;
use crate::store::schema::BEFORE_REPO_LINKS;
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

/// One on github.com is linked once GitHub says which repository it is, only while its address is the one it was found
/// by, and an address read again that's another takes the link with it. Each change moves the revision once.
#[test]
fn a_link_is_kept_for_the_address_it_was_found_by_until_the_checkout_is_added_again_with_another() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let repo = added(&store);
    let linked = store.read_repo_url(&repo.id, "/repo", Some(&roundhouse())).unwrap().unwrap();
    let link = RepoLink::Github { id: "R_roundhouse".into(), by: AddressForm::Https };
    let elsewhere = RepoUrl::try_from("https://gitlab.com/ada/roundhouse.git".to_owned()).unwrap();

    let unseen = store.repo_unlinked(&repo.id, &roundhouse(), &Unlinked::NotFound).unwrap().unwrap();
    let same_again = store.repo_unlinked(&repo.id, &roundhouse(), &Unlinked::NotFound).unwrap();
    let of_another = store.link_repo(&repo.id, &elsewhere, &link).unwrap();
    let found = store.link_repo(&repo.id, &roundhouse(), &link).unwrap().unwrap();
    let to_link_once_linked = store.repos_to_link().unwrap();
    let github_links: Vec<_> = store.github_links().unwrap().into_iter().collect();
    added(&store);
    let moved = store.read_repo_url(&repo.id, "/repo", Some(&elsewhere)).unwrap().unwrap();

    let named = GithubRepositoryName::named("ada/roundhouse");
    assert_eq!(linked.link, Linking::NotLinked { repository: named.clone(), why: Unlinked::NotAskedYet });
    assert_eq!(unseen.link, Linking::NotLinked { repository: named, why: Unlinked::NotFound });
    assert!(unseen.rev > linked.rev);
    assert_eq!((same_again, of_another), (None, None));
    assert_eq!(found.link, Linking::Linked { link });
    assert_eq!(github_links, ["R_roundhouse".into()]);
    assert_eq!(to_link_once_linked, []);
    assert_eq!(moved.link, Linking::Linked { link: RepoLink::Plain { url: elsewhere } });
}

/// A repository whose address a worker read before links is linked by it where it isn't on github.com, moving its
/// revision, and left to the hub to link where it is.
#[test]
fn a_repository_from_before_links_is_linked_by_an_address_elsewhere_and_left_to_link_on_github() {
    let dir = tempfile::tempdir().unwrap();
    let rows = "UPDATE repos SET url = 'https://gitlab.com/ada/project.git', url_to_read = 0, rev = 1 WHERE id = 'r';
        INSERT INTO repos (id, name, path, created_at_ms, url, url_to_read)
            VALUES ('g', 'gh', '/gh', 0, 'git@github.com:ada/gh.git', 0), ('n', 'none', '/none', 0, NULL, 0);";

    let store = store_from_before(dir.path(), BEFORE_REPO_LINKS, rows);

    let plain = store.get_repo(&"r".into()).unwrap();
    let gitlab = RepoUrl::try_from("https://gitlab.com/ada/project.git".to_owned()).unwrap();
    assert_eq!(plain.link, Linking::Linked { link: RepoLink::Plain { url: gitlab } });
    assert!(plain.rev.get() > 1, "a record that changed moves its revision");
    let on_github =
        Linking::NotLinked { repository: GithubRepositoryName::named("ada/gh"), why: Unlinked::NotAskedYet };
    assert_eq!(store.get_repo(&"g".into()).unwrap().link, on_github);
    assert_eq!(store.get_repo(&"n".into()).unwrap().link, Linking::NoAddress);
    let to_link: Vec<_> = store.repos_to_link().unwrap().into_iter().map(|repo| repo.id.to_string()).collect();
    assert_eq!(to_link, ["g"]);
}
