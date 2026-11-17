//! Approving and ending sign-ins with GitHub, under the logins' lock, and tidying what they kept.

use std::path::Path;

use protocol::model::{GithubLoginKind, GithubSignInId};

use super::*;
use crate::login_files::{LoginFile, Stored};

/// The sign-in `id`, started on `store`, waiting.
pub(super) fn started(store: &Store, id: &str) -> GithubSignInId {
    let id = GithubSignInId::from(id);
    store.start_github_sign_in(&id, &protocol::fixtures::github_sign_in("s").status).unwrap();
    id
}

/// What GitHub gave the sign-in `id`, the token `token`, signed in at 1000 and ending at 9000, as the account `new_id`
/// if it's new.
pub(super) fn approved(id: &GithubSignInId, token: &str, new_id: &str) -> Approved {
    let login = SignedIn {
        token: GithubToken::new(token),
        token_ends_at_ms: Some(9000),
        refresh: Some(crate::github::GithubRefreshToken::new(format!("{token}-refresh"))),
        refresh_ends_at_ms: Some(90_000),
    };
    Approved { sign_in: id.clone(), login, new_id: new_id.into(), signed_in_at_ms: 1000 }
}

/// Has `logins` keep `login` as what a sign-in approved at 1000 as `owner` gave, as the account `new_id` if the hub
/// holds no sign-in as that user, and answers with the account whose login it is.
pub(super) fn sign_in_kept(
    logins: &GithubLogins,
    store: &Store,
    owner: &TokenOwner,
    new_id: &str,
    login: SignedIn,
) -> GithubAccountId {
    let sign_in = started(store, &uuid::Uuid::new_v4().to_string());
    let approved = Approved { sign_in, login, new_id: new_id.into(), signed_in_at_ms: 1000 };
    logins.approve_sign_in(&approved, owner).unwrap().account.unwrap().account.id
}

/// The same, as ada, as the account `g` if the hub holds no sign-in as ada, which is the default as the first account.
pub(crate) fn ada_signed_in(logins: &GithubLogins, store: &Store, login: SignedIn) -> GithubAccountId {
    sign_in_kept(logins, store, &ada(), "g", login)
}

/// What a sign-in gives: `token`, ending at `ends_at_ms`, and `{token}-refresh`, which renews it, ending a day after.
pub(crate) fn login_ending(token: &str, ends_at_ms: i64) -> SignedIn {
    SignedIn {
        token: GithubToken::new(token),
        token_ends_at_ms: Some(ends_at_ms),
        refresh: Some(crate::github::GithubRefreshToken::new(format!("{token}-refresh"))),
        refresh_ends_at_ms: Some(ends_at_ms + 24 * 60 * 60 * 1000),
    }
}

/// The sign-in the login file of the account `id` holds, as the logins read it.
pub(crate) fn sign_in_held(logins: &GithubLogins, store: &Store, id: &GithubAccountId) -> SignedIn {
    let file = store.github_login_file(id).unwrap().unwrap();
    let Stored::Secret(GithubLogin::SignIn(held)) = logins.files.lock().read(&file).unwrap() else {
        panic!("the file of {id} holds no sign-in")
    };
    held
}

/// What's in every file in the GitHub logins' directory, as the logins read it.
fn logins_kept(logins: &GithubLogins, dir: &Path) -> Vec<Stored<GithubLogin>> {
    let Ok(entries) = std::fs::read_dir(dir.join("github-logins")) else { return Vec::new() };
    let files = logins.files.lock();
    entries
        .map(|entry| LoginFile::parse(entry.unwrap().file_name().to_str().unwrap()).unwrap())
        .map(|file| files.read(&file).unwrap())
        .collect()
}

#[test]
fn approving_keeps_what_github_gave_in_a_file_of_its_own_and_makes_an_account_the_default_as_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let id = started(&store, "s");
    let given = approved(&id, "a-token", "g");

    let change = logins.approve_sign_in(&given, &ada()).unwrap();

    assert!(change.changed);
    assert_eq!(change.sign_in.status, GithubSignInStatus::Approved { account: "g".into() });
    let account = change.account.unwrap();
    assert!(account.changed);
    assert_eq!(account.account.user, ada().user);
    let held = account.account.held_login.unwrap();
    assert_eq!((held.how, held.connected_at_ms, held.ends_at_ms), (GithubLoginKind::SignIn, 1000, Some(9000)));
    assert_eq!(account.settings.and_then(|settings| settings.github).unwrap().account, Some("g".into()));
    assert_eq!(logins_kept(&logins, dir.path()), [Stored::Secret(GithubLogin::SignIn(given.login))]);
}

#[test]
fn a_second_sign_in_as_the_same_user_renews_its_login_in_place_keeping_the_default_and_a_token_is_another_account() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    logins.connect(&connect("a-pasted-token", None, "by-token"), &ada()).unwrap();
    let [first, second] = ["first", "second"].map(|id| started(&store, id));
    logins.approve_sign_in(&approved(&first, "an-old-token", "signed-in"), &ada()).unwrap();
    logins.set_default(Some(&"signed-in".into()), 2000).unwrap();
    let renamed = owner(7, "ada-renamed", None);

    let change = logins.approve_sign_in(&approved(&second, "a-new-token", "unused"), &renamed).unwrap();

    let account = change.account.unwrap();
    assert_eq!((account.account.id.as_str(), account.account.user.name.as_str()), ("signed-in", "ada-renamed"));
    assert_eq!(change.sign_in.status, GithubSignInStatus::Approved { account: "signed-in".into() });
    assert_eq!(default_of(&store), Some("signed-in".into()));
    let accounts = store.list_records().unwrap().github_accounts;
    assert_eq!(accounts.iter().map(|account| account.id.as_str()).collect::<Vec<_>>(), ["by-token", "signed-in"]);
    let mut tokens: Vec<String> = logins_kept(&logins, dir.path())
        .into_iter()
        .map(|kept| match kept {
            Stored::Secret(GithubLogin::Token(token)) => token.as_str().to_string(),
            Stored::Secret(GithubLogin::SignIn(signed_in)) => signed_in.token.as_str().to_string(),
            other @ (Stored::Secret(GithubLogin::Installation(_) | GithubLogin::App(_))
            | Stored::Newer
            | Stored::Nothing) => {
                panic!("{other:?}")
            }
        })
        .collect();
    tokens.sort();
    assert_eq!(tokens, ["a-new-token", "a-pasted-token"], "the old sign-in's file is gone");
}

#[test]
fn a_sign_in_that_ended_first_keeps_nothing_and_ending_one_that_has_ended_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let id = started(&store, "s");

    let cancelled = logins.end_sign_in(&id, SignInEnd::Cancelled).unwrap();
    let approved_after = logins.approve_sign_in(&approved(&id, "a-token", "g"), &ada()).unwrap();
    let refused_after = logins.end_sign_in(&id, SignInEnd::Refused).unwrap();
    let nobodys = logins.end_sign_in(&"nobody's".into(), SignInEnd::Ended);

    assert!(cancelled.changed);
    assert_eq!(cancelled.sign_in.status, GithubSignInStatus::Cancelled);
    let unchanged = SignInChange { sign_in: cancelled.sign_in, changed: false, account: None };
    assert_eq!(approved_after, unchanged);
    assert_eq!(refused_after, unchanged);
    assert_eq!(nobodys.unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(store.list_records().unwrap().github_accounts, []);
    assert_eq!(logins_kept(&logins, dir.path()), [], "nothing was kept");
}
