use protocol::client::github_apps::GithubManifestCode;
use protocol::model::{GithubAppId, GithubUser, GithubUserId};

use super::*;
use crate::login_files::LoginFile;
use crate::store::tests::open_in;

const APP: GithubAppId = GithubAppId::new(9);

fn user(id: u64, name: &str) -> GithubUser {
    GithubUser { id: GithubUserId::new(id), name: name.into() }
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
