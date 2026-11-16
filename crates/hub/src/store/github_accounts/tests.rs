use protocol::model::GithubSettings;

use super::*;
use crate::store::older::store_from_before;
use crate::store::schema::BEFORE_GITHUB_ACCOUNTS;
use crate::store::tests::open_in;

fn ada() -> GithubUser {
    GithubUser { id: GithubUserId::new(7), name: "ada".into() }
}

/// A token of `user`'s for `id`, kept in `file`, connected at 100.
fn connecting<'a>(id: &'a GithubAccountId, user: &'a GithubUser, file: &'a LoginFile) -> GithubConnecting<'a> {
    GithubConnecting { account: id, user, how: GithubLoginKind::Token, file, connected_at_ms: 100, ends_at_ms: None }
}

#[test]
fn a_hub_from_before_github_accounts_holds_none_and_each_machines_own_login_is_the_default() {
    let dir = tempfile::tempdir().unwrap();

    let store = store_from_before(dir.path(), BEFORE_GITHUB_ACCOUNTS, "");

    let settings = store.hub_settings().unwrap();
    assert_eq!(settings.github, Some(GithubSettings { account: None }));
    assert_eq!(store.list_records().unwrap().github_accounts, []);
}

#[test]
fn connecting_disconnecting_and_choosing_each_move_their_records_revision_and_the_same_again_doesnt() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let (id, user, file) = (GithubAccountId::from("g"), ada(), LoginFile::new());
    let at_first = store.hub_settings().unwrap().rev;

    let connected = store.connect_github_login(&connecting(&id, &user, &file)).unwrap();
    let cleared = store.clear_github_login(&id).unwrap().unwrap();
    let cleared_again = store.clear_github_login(&id).unwrap();
    let machines_own = store.set_github_default(None).unwrap();
    let same_again = store.set_github_default(None).unwrap();

    let made_default = connected.settings.unwrap();
    assert!(made_default.rev > at_first, "{} isn't past {at_first}", made_default.rev);
    assert!(cleared.rev > connected.account.rev, "{} isn't past {}", cleared.rev, connected.account.rev);
    assert_eq!(cleared_again, None);
    assert!(machines_own.rev > made_default.rev, "{} isn't past {}", machines_own.rev, made_default.rev);
    assert_eq!(same_again, machines_own);
    assert_eq!(store.list_records().unwrap().rev, machines_own.rev, "the list is as new as the last change");
}

#[test]
fn a_row_naming_a_file_the_hub_doesnt_make_holds_no_login_and_is_listed_for_tidying() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let (id, user, file) = (GithubAccountId::from("g"), ada(), LoginFile::new());
    store.connect_github_login(&connecting(&id, &user, &file)).unwrap();
    store.writer.lock().execute("UPDATE github_accounts SET login_file = '../hub.db'", []).unwrap();

    let account = store.github_account(&id).unwrap().unwrap();

    assert_eq!(account.held_login, None);
    assert_eq!(store.github_login_file(&id).unwrap(), None);
    assert_eq!(store.held_github_logins().unwrap(), []);
    assert_eq!(store.github_login_files().unwrap(), [(id, None)]);
}

/// Beside a token's account, which isn't listed to renew.
#[test]
fn renewing_a_sign_in_and_its_needing_signing_in_again_each_move_its_records_revision_and_once_only() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let (user, token_file, sign_in_file) = (ada(), LoginFile::new(), LoginFile::new());
    let (by_token, signed_in) = (GithubAccountId::from("t"), GithubAccountId::from("s"));
    store.connect_github_login(&connecting(&by_token, &user, &token_file)).unwrap();
    let signing_in = connecting(&signed_in, &user, &sign_in_file);
    let signing_in = GithubConnecting { how: GithubLoginKind::SignIn, ends_at_ms: Some(9000), ..signing_in };
    let connected = store.connect_github_login(&signing_in).unwrap().account;

    let listed = store.held_github_sign_ins().unwrap();
    let renewed = store.renew_github_sign_in(&signed_in, Some(20_000)).unwrap().unwrap();
    let needing = store.github_login_needs_sign_in(&signed_in).unwrap().unwrap();
    let needing_again = store.github_login_needs_sign_in(&signed_in).unwrap();

    assert_eq!(listed, [(connected.clone(), sign_in_file)]);
    let held = renewed.held_login.unwrap();
    assert_eq!((held.connected_at_ms, held.ends_at_ms, held.needs_sign_in), (100, Some(20_000), false));
    assert!(renewed.rev > connected.rev, "{} isn't past {}", renewed.rev, connected.rev);
    assert!(needing.held_login.unwrap().needs_sign_in);
    assert!(needing.rev > renewed.rev, "{} isn't past {}", needing.rev, renewed.rev);
    assert_eq!(needing_again, None);
}
