use super::*;
use crate::store::older::store_from_before;
use crate::store::schema::BEFORE_FOUND_GITHUB_LOGINS;
use crate::store::tests::open_in;

fn ada() -> GhLogin {
    GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Yes }
}

#[test]
fn a_hub_from_before_found_logins_has_none_and_records_one_once_migrated() {
    let dir = tempfile::tempdir().unwrap();

    let store = store_from_before(dir.path(), BEFORE_FOUND_GITHUB_LOGINS, "");

    assert_eq!(store.list_records().unwrap().found_github_logins, []);
    let found = store.record_found_github_login(&"local".into(), &ada()).unwrap();
    assert_eq!(store.list_records().unwrap().found_github_logins, [found]);
}

/// So a client takes each in with the rest, and doesn't take one found again for a change.
#[test]
fn each_provider_has_one_whose_revision_moves_only_when_another_login_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let before = store.list_records().unwrap().rev;

    let local = store.record_found_github_login(&"local".into(), &GhLogin::LoggedOut).unwrap();
    let incus = store.record_found_github_login(&"incus".into(), &GhLogin::NotInstalled).unwrap();
    let same_again = store.record_found_github_login(&"local".into(), &GhLogin::LoggedOut).unwrap();
    let logged_in = store.record_found_github_login(&"local".into(), &ada()).unwrap();

    assert!(before < local.rev && local.rev < incus.rev && incus.rev < logged_in.rev);
    assert_eq!(same_again, local);
    assert_eq!((logged_in.provider.as_str(), &logged_in.login), ("local", &ada()));
    let listed = store.list_records().unwrap();
    assert_eq!(listed.found_github_logins, [incus, logged_in.clone()]);
    assert_eq!(listed.rev, logged_in.rev);
}

#[test]
fn forgetting_them_leaves_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    store.record_found_github_login(&"local".into(), &ada()).unwrap();

    store.forget_found_github_logins().unwrap();

    assert_eq!(store.list_records().unwrap().found_github_logins, []);
}
