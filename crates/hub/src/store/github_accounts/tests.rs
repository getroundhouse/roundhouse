use github::GithubProfile;
use protocol::model::GithubSettings;

use super::*;
use crate::store::older::store_from_before;
use crate::store::schema::{BEFORE_GITHUB_ACCOUNTS, BEFORE_GITHUB_PROFILES};
use crate::store::tests::open_in;

fn ada() -> GithubUser {
    GithubUser { id: GithubUserId::new(7), name: "ada".into() }
}

/// A token of `user`'s for `id`, kept in `file`, connected at 100.
fn connecting<'a>(id: &'a GithubAccountId, user: &'a GithubUser, file: &'a LoginFile) -> GithubConnecting<'a> {
    let how = GithubLoginKind::Token;
    let (connected_at_ms, ends_at_ms, installation, profile) = (100, None, None, None);
    GithubConnecting { account: id, user, how, file, connected_at_ms, ends_at_ms, installation, profile }
}

#[test]
fn a_hub_from_before_github_accounts_holds_none_and_has_no_default() {
    let dir = tempfile::tempdir().unwrap();

    let store = store_from_before(dir.path(), BEFORE_GITHUB_ACCOUNTS, "");

    let settings = store.hub_settings().unwrap();
    assert_eq!(
        settings.github,
        Some(GithubSettings { account: None, sent: None, left_to_machine: Vec::new(), set_up: Vec::new() })
    );
    assert_eq!(store.list_records().unwrap().github_accounts, []);
}

/// The same again moves neither, as `same_again` has it of every record.
#[test]
fn connecting_disconnecting_and_choosing_each_move_their_records_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let (id, user, file) = (GithubAccountId::from("g"), ada(), LoginFile::new());
    let at_first = store.hub_settings().unwrap().rev;

    let connected = store.connect_github_login(&connecting(&id, &user, &file)).unwrap();
    let cleared = store.clear_github_login(&id).unwrap().unwrap();
    let machines_own = store.set_github_default(None).unwrap();

    let made_default = connected.settings.unwrap();
    assert!(made_default.rev > at_first, "{} isn't past {at_first}", made_default.rev);
    assert!(cleared.rev > connected.account.rev, "{} isn't past {}", cleared.rev, connected.account.rev);
    assert!(machines_own.rev > made_default.rev, "{} isn't past {}", machines_own.rev, made_default.rev);
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

/// Beside a token's account, which isn't listed to renew. Needing it once more moves nothing, as `same_again` has it of
/// every record.
#[test]
fn renewing_a_sign_in_and_its_needing_signing_in_again_each_move_its_records_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let (user, token_file, sign_in_file) = (ada(), LoginFile::new(), LoginFile::new());
    let (by_token, signed_in) = (GithubAccountId::from("t"), GithubAccountId::from("s"));
    store.connect_github_login(&connecting(&by_token, &user, &token_file)).unwrap();
    let signing_in = connecting(&signed_in, &user, &sign_in_file);
    let signing_in = GithubConnecting { how: GithubLoginKind::SignIn, ends_at_ms: Some(9000), ..signing_in };
    let connected = store.connect_github_login(&signing_in).unwrap().account;

    let listed = store.held_github_logins_of(GithubLoginKind::SignIn).unwrap();
    let renewed = store.renew_github_sign_in(&signed_in, Some(20_000)).unwrap().unwrap();
    let needing = store.github_login_needs_sign_in(&signed_in).unwrap().unwrap();

    assert_eq!(listed, [(connected.clone(), sign_in_file)]);
    let held = renewed.held_login.unwrap();
    assert_eq!((held.connected_at_ms, held.ends_at_ms, held.needs_sign_in), (100, Some(20_000), false));
    assert!(renewed.rev > connected.rev, "{} isn't past {}", renewed.rev, connected.rev);
    assert!(needing.held_login.unwrap().needs_sign_in);
    assert!(needing.rev > renewed.rev, "{} isn't past {}", needing.rev, renewed.rev);
}

fn profile(name: &str, email: &str) -> GithubProfile {
    GithubProfile { name: Some(name.to_string()), email: Some(email.to_string()) }
}

/// Its token's, but not one disconnected, nor an app's installation, whose bot has no profile.
#[test]
fn a_hub_from_before_profiles_has_asked_for_none_so_each_held_token_and_sign_in_is_due() {
    let dir = tempfile::tempdir().unwrap();
    let held = "INSERT INTO github_accounts
                    (id, user_id, username, login_kind, login_file, login_connected_at_ms, login_ends_at_ms)
                    VALUES ('g', 7, 'ada', 'token', '6f1c2a9e-0000-4000-8000-000000000000', 1, NULL),
                           ('gone', 8, 'grace', NULL, NULL, NULL, NULL),
                           ('bot', 41, 'ada-roundhouse[bot]', 'app', '6f1c2a9e-0000-4000-8000-000000000001', 1, NULL);";

    let store = store_from_before(dir.path(), BEFORE_GITHUB_PROFILES, held);

    assert_eq!(store.github_profile(&"g".into()).unwrap(), None);
    let due: Vec<GithubAccountId> =
        store.github_profiles_due(i64::MAX).unwrap().into_iter().map(|(account, _)| account.id).collect();
    assert_eq!(due, ["g".into()] as [GithubAccountId; 1]);
}

/// As a check's answer would come once the account was connected again.
#[test]
fn a_profile_is_kept_as_its_connected_and_read_again_only_when_asked_after_the_one_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let (id, user, file) = (GithubAccountId::from("g"), ada(), LoginFile::new());
    let first = profile("Ada", "ada@example.com");
    store.connect_github_login(&GithubConnecting { profile: Some(&first), ..connecting(&id, &user, &file) }).unwrap();
    let as_connected = store.github_profile(&id).unwrap();

    store.read_github_profile(&id, &user, &profile("Ada, before", "before@example.com"), 99).unwrap();
    let after_an_older_answer = store.github_profile(&id).unwrap();
    store.read_github_profile(&id, &user, &profile("Ada Lovelace", "ada@example.org"), 300).unwrap();

    assert_eq!((as_connected.as_ref(), after_an_older_answer.as_ref()), (Some(&first), Some(&first)));
    assert_eq!(store.github_profile(&id).unwrap(), Some(profile("Ada Lovelace", "ada@example.org")));
    let due = |by| store.github_profiles_due(by).unwrap().len();
    assert_eq!((due(299), due(300)), (0, 1), "it's due once it was last asked by then");
}

/// Its record, which every client follows, has a username, and nothing else of the profile, so only another username
/// moves its revision. GitHub refusing the token keeps what it said before, and is an ask, so it isn't due again yet.
#[test]
fn reading_a_profile_moves_the_records_revision_only_for_a_new_username_and_a_refusal_keeps_what_was_said() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());
    let (id, user, file) = (GithubAccountId::from("g"), ada(), LoginFile::new());
    let connected = store.connect_github_login(&connecting(&id, &user, &file)).unwrap().account;
    let said = profile("Ada Lovelace", "ada@example.com");

    let same_name = store.read_github_profile(&id, &user, &said, 200).unwrap();
    let after_the_same_name = store.github_account(&id).unwrap().unwrap();
    let renamed_user = GithubUser { name: "ada-lovelace".into(), ..user.clone() };
    let renamed = store.read_github_profile(&id, &renamed_user, &said, 300).unwrap().unwrap();
    store.github_profile_refused(&id, 400).unwrap();

    assert_eq!((same_name, after_the_same_name), (None, connected.clone()), "no revision until it's renamed");
    assert_eq!(renamed.user, renamed_user);
    assert!(renamed.rev > connected.rev, "{} isn't past {}", renamed.rev, connected.rev);
    assert_eq!(store.github_profile(&id).unwrap(), Some(said));
    assert_eq!(store.github_profiles_due(399).unwrap(), []);
}
