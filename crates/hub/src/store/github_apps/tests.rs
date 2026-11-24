use protocol::client::github_apps::GithubManifestCode;
use protocol::model::GithubAppId;

use super::*;
use crate::login_files::LoginFile;
use crate::store::tests::open_in;

const APP: GithubAppId = GithubAppId::new(9);

/// Each is a change to the record a client follows, so each has a revision of its own.
#[test]
fn an_apps_record_has_a_revision_of_its_own_as_its_made_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let (key, code) = (LoginFile::new(), GithubManifestCode::new("a-code"));
    let making = GithubAppMaking { id: APP, key: &key, made_from: &code };

    let made = store.make_github_app(&making).unwrap();
    let moved = store.github_app_moved(APP).unwrap().unwrap();
    let listed = store.list_records().unwrap().github_apps;
    let removed = store.remove_github_app(APP).unwrap().unwrap();

    assert_eq!(made.id, APP);
    assert!(made.rev < moved.rev && moved.rev < removed, "{} {} {removed}", made.rev, moved.rev);
    assert_eq!(listed, vec![moved]);
    assert_eq!(store.github_app_made_from(&code).unwrap(), None);
    assert_eq!(store.remove_github_app(APP).unwrap(), None);
}
