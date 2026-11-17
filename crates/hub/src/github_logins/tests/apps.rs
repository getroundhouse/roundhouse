//! Keeping an app of your own's key, connecting its installations as accounts, making each its next token, and
//! forgetting them, as GitHub accounts' logins, and what workers are sent of each.

use std::path::Path;

use protocol::client::github_apps::GithubManifestCode;
use protocol::model::{
    GithubAccount, GithubApp, GithubAppId, GithubInstallation, GithubInstallationId, GithubLoginKind, InstalledApp,
};
use protocol::worker::{GitCredential, GitIdentity, GithubCredential};

use super::super::*;
use super::credential::{reason, sent};
use super::{default_of, logins_in};
use crate::github::{GithubAppKey, Installation, MadeApp, Minted};

const APP: GithubAppId = GithubAppId::new(9);

/// When the first installation's token ends: an hour after the logins started, as GitHub has one last.
const ENDS_AT_MS: i64 = 1000 + 3_600_000;

fn user(id: u64, name: &str) -> GithubUser {
    GithubUser { id: GithubUserId::new(id), name: name.into() }
}

fn bot() -> GithubUser {
    user(41, "ada-roundhouse[bot]")
}

fn code() -> GithubManifestCode {
    GithubManifestCode::new("a-code")
}

/// The app 9, as GitHub made it from [`code`], under the organization adalabs, with a key that only needs to be kept,
/// since nothing here signs with it.
fn made() -> MadeApp {
    MadeApp {
        id: APP,
        slug: "ada-roundhouse".into(),
        name: "Ada's Roundhouse".into(),
        owner: user(2, "adalabs"),
        key: GithubAppKey::new("a-made-up-key"),
    }
}

/// The installation `id` of the app on the organization adalabs, its owner.
fn on_adalabs(id: u64) -> Installation {
    Installation { id: GithubInstallationId::new(id), on: user(2, "adalabs") }
}

fn kept(logins: &GithubLogins) -> GithubApp {
    logins.keep_app(&made(), &code()).unwrap().app
}

/// `installation`, connected at 1000 as the hub's check would, with `token`, which ends at `ends_at_ms`.
fn installed(logins: &GithubLogins, installation: &Installation, token: &str, ends_at_ms: i64) -> GithubAccount {
    let token = InstallationToken { token: GithubToken::new(token), token_ends_at_ms: ends_at_ms };
    logins.connect_installation(APP, &bot(), installation, token, false, 1000).unwrap().unwrap().account
}

/// What workers are sent while the default is an installation whose token is `token`.
fn bots(token: &str) -> GithubCredential {
    let identity = GitIdentity {
        name: "ada-roundhouse[bot]".into(),
        email: "41+ada-roundhouse[bot]@users.noreply.github.com".into(),
    };
    GithubCredential::Account(GitCredential {
        username: "x-access-token".into(),
        token: GithubToken::new(token),
        identity: Some(identity),
    })
}

fn files_in(dir: &Path) -> Vec<String> {
    let entries = std::fs::read_dir(dir.join("github-logins")).unwrap();
    entries.map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap()).collect()
}

#[test]
fn an_apps_key_is_kept_once_for_its_code_and_number_and_changes_nothing_workers_are_sent() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let following = logins.git_credential();

    let first = logins.keep_app(&made(), &code()).unwrap();
    let again = logins.keep_app(&made(), &code()).unwrap();
    let same_number = logins.keep_app(&made(), &GithubManifestCode::new("another-code")).unwrap();

    assert!(first.changed && !again.changed && !same_number.changed);
    let owner = user(2, "adalabs");
    assert_eq!((first.app.slug.as_str(), &first.app.owner, &first.app.bot), ("ada-roundhouse", &owner, &None));
    assert_eq!(logins.made_from(&code()).unwrap(), Some(first.app.clone()));
    assert_eq!(logins.app_key(APP).unwrap(), (first.app, GithubAppKey::new("a-made-up-key")));
    assert_eq!(files_in(dir.path()), [r#"{"format":"app","key":"a-made-up-key"}"#]);
    assert!(!following.has_changed().unwrap(), "{:?}", sent(&logins));
}

#[test]
fn an_installation_is_its_apps_bot_on_its_user_and_as_the_default_workers_commit_and_push_as_the_bot() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    kept(&logins);

    let account = installed(&logins, &on_adalabs(70), "ghs_first", ENDS_AT_MS);

    let installation = GithubInstallation { app: APP, id: GithubInstallationId::new(70), on: user(2, "adalabs") };
    assert_eq!((&account.user, account.installation), (&bot(), Some(installation)));
    let held = account.held_login.unwrap();
    assert_eq!((held.how, held.ends_at_ms), (GithubLoginKind::App, None), "its record changes for no new token");
    assert_eq!(default_of(&store), Some(account.id), "the first account the hub holds becomes the default");
    assert_eq!(sent(&logins), bots("ghs_first"));
    let DefaultCredential::Account { username, installation, .. } = logins.git_credential().borrow().clone() else {
        panic!("not an account's")
    };
    let installed = InstalledApp { slug: "ada-roundhouse".into(), on: "adalabs".into() };
    assert_eq!((username, installation), (bot().name, Some(installed)), "GitHub knows it as the bot");
}

/// Removing an app forgets its installations' logins before its record, so only a store changed behind the hub's back
/// has a default installation that can be used whose app the hub has no record of.
#[test]
fn a_default_installation_whose_app_has_no_record_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    kept(&logins);
    installed(&logins, &on_adalabs(70), "ghs_first", ENDS_AT_MS);

    store.remove_github_app(APP).unwrap();
    logins.check(2000).unwrap();

    let reason = reason(sent(&logins));
    let why = "no record of the app its default GitHub account, ada-roundhouse[bot], is an installation of";
    assert!(reason.contains(why), "{reason}");
}

/// GitHub lists only where the app is installed now, so one it no longer lists has been uninstalled there.
#[test]
fn an_installation_github_no_longer_lists_is_forgotten_and_a_default_that_was_it_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    kept(&logins);
    let account = installed(&logins, &on_adalabs(70), "ghs_first", ENDS_AT_MS);

    let looked = logins.installations_found(APP, &[], false, 2000).unwrap();

    assert!(looked.connect.is_empty());
    assert_eq!(
        looked.forgot.iter().map(|forgot| (&forgot.id, forgot.held_login)).collect::<Vec<_>>(),
        [(&account.id, None)]
    );
    assert_eq!(files_in(dir.path()).len(), 1, "only the app's key is left");
    let reason = reason(sent(&logins));
    assert!(
        reason.contains("ada-roundhouse[bot], installed on adalabs, was disconnected, or the app uninstalled"),
        "{reason}"
    );
}

/// Disconnecting one was the person's, so only they connect it again, and one held already isn't connected twice.
#[test]
fn the_clock_connects_an_installation_once_and_one_the_person_disconnected_only_when_they_ask() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    kept(&logins);
    let found = [on_adalabs(70)];
    let connecting = |by_person| logins.installations_found(APP, &found, by_person, 2000).unwrap().connect;

    let new = connecting(false);
    let account = installed(&logins, &found[0], "ghs_first", ENDS_AT_MS);
    let held = connecting(false);
    logins.disconnect(&account.id, 2000).unwrap();
    let by_the_clock = connecting(false);
    let by_the_person = connecting(true);
    let token = InstallationToken { token: GithubToken::new("ghs_late"), token_ends_at_ms: ENDS_AT_MS };
    let late = logins.connect_installation(APP, &bot(), &found[0], token, false, 2000).unwrap();

    assert_eq!((new, held), (found.to_vec(), Vec::new()));
    assert_eq!((by_the_clock, by_the_person), (Vec::new(), found.to_vec()));
    assert!(late.is_none(), "a token the clock made is let go once the person has disconnected it meanwhile");
}

/// As GitHub would list one once the person had made the app public there and someone else installed it, which is
/// theirs, not the person's.
#[test]
fn only_an_installation_on_the_apps_owner_is_connected() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    kept(&logins);
    let elsewhere = Installation { id: GithubInstallationId::new(80), on: user(3, "evelabs") };
    let token = InstallationToken { token: GithubToken::new("ghs_elsewhere"), token_ends_at_ms: ENDS_AT_MS };

    let looked = logins.installations_found(APP, &[elsewhere.clone(), on_adalabs(70)], true, 2000).unwrap();
    let connected = logins.connect_installation(APP, &bot(), &elsewhere, token, true, 2000).unwrap();

    assert_eq!(looked.connect, [on_adalabs(70)]);
    assert!(connected.is_none(), "{connected:?}");
    assert_eq!(sent(&logins), GithubCredential::MachineOwn, "nothing became the default");
}

/// So it's the same account as before, which may be the default, rather than another of the same bot on the same user.
#[test]
fn an_app_installed_again_where_it_was_uninstalled_connects_in_place_of_the_last() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    kept(&logins);
    let first = installed(&logins, &on_adalabs(70), "ghs_first", ENDS_AT_MS);

    let looked = logins.installations_found(APP, &[on_adalabs(71)], false, 2000).unwrap();
    let again = installed(&logins, &on_adalabs(71), "ghs_again", ENDS_AT_MS);

    assert_eq!((looked.forgot.len(), looked.connect), (1, vec![on_adalabs(71)]));
    assert_eq!(
        (&again.id, again.installation.map(|installed| installed.id)),
        (&first.id, Some(GithubInstallationId::new(71)))
    );
    assert_eq!(store.installation_accounts(APP).unwrap().len(), 1);
    assert_eq!(sent(&logins), bots("ghs_again"));
}

/// The next token is made before the last ends, and a default whose last has ended meanwhile blocks until it is.
#[test]
fn a_check_makes_an_installations_next_token_before_its_last_ends_and_workers_are_sent_it() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    kept(&logins);
    installed(&logins, &on_adalabs(70), "ghs_first", ENDS_AT_MS);
    let left = i64::try_from(MINTED_WITH_LEFT.as_millis()).unwrap();

    let early = logins.check(ENDS_AT_MS - left).unwrap();
    let due = logins.check(ENDS_AT_MS - left + 1).unwrap();
    logins.check(ENDS_AT_MS).unwrap();
    let ended = sent(&logins);
    let minted = Minted::Token { token: GithubToken::new("ghs_next"), ends_at_ms: ENDS_AT_MS + 3_600_000 };
    let changed = logins.minted(&due.minting[0], minted, ENDS_AT_MS).unwrap();

    assert!(early.minting.is_empty(), "{:?}", early.minting);
    assert_eq!(
        (due.minting.len(), due.minting[0].installation, &due.apps),
        (1, GithubInstallationId::new(70), &vec![APP])
    );
    assert_eq!(due.minting[0].key, GithubAppKey::new("a-made-up-key"));
    assert!(reason(ended).contains("has ended, and Roundhouse is making another"));
    assert!(changed.is_none(), "a new token changes nothing on the account's record");
    assert_eq!(sent(&logins), bots("ghs_next"));
    assert!(logins.check(ENDS_AT_MS + 1).unwrap().minting.is_empty(), "the next isn't due till it has 20 minutes left");
}

#[test]
fn an_installation_github_has_no_more_as_its_token_is_made_is_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    kept(&logins);
    let account = installed(&logins, &on_adalabs(70), "ghs_first", ENDS_AT_MS);
    let due = logins.check(ENDS_AT_MS).unwrap().minting.pop().unwrap();

    let forgot = logins.minted(&due, Minted::Gone, ENDS_AT_MS).unwrap().unwrap();

    assert_eq!((forgot.account.id, forgot.account.held_login), (account.id, None));
    assert!(reason(sent(&logins)).contains("the app uninstalled there"));
}

/// Its installations' logins go first, then its key, then its record, so a hub stopped between leaves no token or key
/// that was to be forgotten, and tidying removes a record whose key is gone.
#[test]
fn removing_an_app_forgets_its_installations_logins_its_key_and_its_record() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    kept(&logins);
    let account = installed(&logins, &on_adalabs(70), "ghs_first", ENDS_AT_MS);

    let removed = logins.remove_app(APP, 2000).unwrap();
    let again = logins.remove_app(APP, 2000).map(|_| ()).unwrap_err();

    assert_eq!(removed.app.id, APP);
    assert_eq!(
        removed.accounts.iter().map(|forgot| (&forgot.id, forgot.held_login)).collect::<Vec<_>>(),
        [(&account.id, None)]
    );
    assert!(files_in(dir.path()).is_empty());
    assert_eq!((again.code, logins.app_key(APP).unwrap_err().code), (ErrorCode::NotFound, ErrorCode::NotFound));
    assert_eq!(logins.made_from(&code()).unwrap(), None);
    assert!(
        store.github_account(&account.id).unwrap().is_some(),
        "its accounts' records stay, as disconnecting keeps one"
    );
    assert!(reason(sent(&logins)).contains("was disconnected, or the app uninstalled there"));
}

#[test]
fn tidying_removes_an_app_whose_key_is_gone_as_removing_it_would_have() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    kept(&logins);
    for entry in std::fs::read_dir(dir.path().join("github-logins")).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }

    let refused = logins.app_key(APP).unwrap_err();
    logins.tidy(2000).unwrap();

    assert_eq!(refused.code, ErrorCode::Conflict);
    assert_eq!(store.github_app(APP).unwrap(), None);
}
