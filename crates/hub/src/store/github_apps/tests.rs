use protocol::client::github_apps::GithubManifestCode;
use protocol::model::{GithubAppId, GithubLoginKind, GithubUser, GithubUserId};

use super::*;
use crate::login_files::LoginFile;
use crate::store::older::store_from_before;
use crate::store::schema::BEFORE_GITHUB_APPS;
use crate::store::tests::open_in;

const APP: GithubAppId = GithubAppId::new(9);

fn user(id: u64, name: &str) -> GithubUser {
    GithubUser { id: GithubUserId::new(id), name: name.into() }
}

#[test]
fn a_hub_from_before_apps_has_none_and_the_accounts_it_held_are_no_apps_installations() {
    let dir = tempfile::tempdir().unwrap();
    let held = "INSERT INTO github_accounts
                    (id, user_id, username, login_kind, login_file, login_connected_at_ms, login_ends_at_ms)
                    VALUES ('g', 7, 'ada', 'token', '6f1c2a9e-0000-4000-8000-000000000000', 1, NULL);";

    let store = store_from_before(dir.path(), BEFORE_GITHUB_APPS, held);

    let listed = store.list_records().unwrap();
    assert_eq!(listed.github_apps, []);
    let [account] = listed.github_accounts.as_slice() else { panic!("{:?}", listed.github_accounts) };
    assert_eq!((account.installation.as_ref(), account.held_login.unwrap().how), (None, GithubLoginKind::Token));
}

/// Each is a change to the record a client follows, so each has a revision of its own. Learning the same bot again
/// changes nothing, as `same_again` has it of every record.
#[test]
fn an_apps_record_has_a_revision_of_its_own_as_its_made_its_bot_is_learned_and_its_removed() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let (key, code, owner, bot) =
        (LoginFile::new(), GithubManifestCode::new("a-code"), user(1, "ada"), user(41, "ada-roundhouse[bot]"));
    let making = GithubAppMaking {
        id: APP,
        slug: &"ada-roundhouse".into(),
        name: "Ada's Roundhouse",
        owner: &owner,
        key: &key,
        made_from: &code,
    };

    let made = store.make_github_app(&making).unwrap();
    let with_bot = store.set_github_app_bot(APP, &bot).unwrap().unwrap();
    let listed = store.list_records().unwrap().github_apps;
    let removed = store.remove_github_app(APP).unwrap().unwrap();

    assert_eq!((&made.owner, made.bot.as_ref(), with_bot.bot.as_ref()), (&owner, None, Some(&bot)));
    assert!(made.rev < with_bot.rev && with_bot.rev < removed, "{} {} {removed}", made.rev, with_bot.rev);
    assert_eq!(listed, vec![with_bot]);
    assert_eq!(store.github_app_made_from(&code).unwrap(), None);
    assert_eq!(store.remove_github_app(APP).unwrap(), None);
}
