//! What workers are to give their processes for github.com in each state of the default GitHub account, as each change
//! to the logins, and each check by the clock, works it out.

use std::path::{Path, PathBuf};

use protocol::model::{GithubAccount, GithubAppId, GithubInstallation, GithubInstallationId, Rev};
use protocol::worker::{GitCredential, GitIdentity, GithubCredential};

use super::super::blocks::{Unusable, lost};
use super::super::*;
use super::sign_ins::{approved, started};
use super::{ada, connect, logins_in, owner};

/// What a worker's link following `logins` is sent now.
pub(super) fn sent(logins: &GithubLogins) -> GithubCredential {
    logins.git_credential().borrow().for_workers()
}

/// Ada's `token`, as she's connected or signed in with no name or public email on GitHub, so she commits as her
/// username, at the address GitHub gives her.
pub(super) fn as_ada(token: &str) -> GithubCredential {
    let identity = GitIdentity { name: "ada".to_string(), email: "7+ada@users.noreply.github.com".to_string() };
    GithubCredential::Account(GitCredential {
        username: "ada".into(),
        token: GithubToken::new(token),
        identity: Some(identity),
    })
}

/// What a block says, which fails for anything but a block, since nothing but one may take a lost default's place.
pub(super) fn reason(credential: GithubCredential) -> String {
    let GithubCredential::Blocked { reason } = credential else { panic!("not a block: {credential:?}") };
    reason
}

/// The file that holds `token`, among the GitHub logins' in `dir`.
fn file_holding(dir: &Path, token: &str) -> PathBuf {
    let mut paths = std::fs::read_dir(dir.join("github-logins")).unwrap().map(|entry| entry.unwrap().path());
    paths.find(|path| std::fs::read_to_string(path).is_ok_and(|held| held == token)).unwrap()
}

/// With no default, workers on the providers Roundhouse sets up GitHub on are sent a block saying to connect one, rather
/// than nothing, which would leave them to whatever their machine has, and no account that isn't the default.
#[test]
fn workers_are_sent_a_block_saying_to_connect_an_account_while_theres_no_default() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let with_none = sent(&logins);
    logins.connect(&connect("a-token", None, "g1"), &ada()).unwrap();

    logins.set_default(None, 2000).unwrap();

    let GithubCredential::Blocked { reason } = &with_none else { panic!("{with_none:?}") };
    assert!(reason.starts_with("Roundhouse has no GitHub account") && reason.contains("Connect one"), "{reason}");
    let with_an_account = sent(&logins);
    let GithubCredential::Blocked { reason } = &with_an_account else {
        panic!("an account that isn't the default isn't sent")
    };
    assert!(reason.starts_with("Roundhouse has no default GitHub account"), "{reason}");
}

#[test]
fn workers_are_sent_the_defaults_username_and_token_as_its_connected_and_renewed_and_nothing_new_for_another() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let mut following = logins.git_credential();

    logins.connect(&connect("first-token", None, "g1"), &owner(7, "ada", Some(5000))).unwrap();
    let connected = following.borrow_and_update().for_workers();
    logins.connect(&connect("renewed-token", Some("g1"), "g2"), &ada()).unwrap();
    let renewed = following.borrow_and_update().for_workers();
    logins.connect(&connect("another-token", None, "g3"), &owner(8, "grace", None)).unwrap();
    logins.check(6000).unwrap();

    assert_eq!(connected, as_ada("first-token"), "the first account became the default");
    assert_eq!(renewed, as_ada("renewed-token"));
    assert!(!following.has_changed().unwrap(), "another account, and the first token's end, change nothing");
}

#[test]
fn a_disconnected_default_leaves_a_block_saying_to_choose_another_or_connect_one() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("a-token", None, "g1"), &ada()).unwrap();
    logins.connect(&connect("b-token", None, "g2"), &owner(8, "grace", None)).unwrap();

    logins.disconnect(&"g1".into(), 2000).unwrap();
    let choose = reason(sent(&logins));
    logins.disconnect(&"g2".into(), 2000).unwrap();
    let connect_one = reason(sent(&logins));

    assert!(choose.starts_with("Roundhouse has no default GitHub account"), "{choose}");
    assert!(choose.contains("Make one the default"), "{choose}");
    assert!(connect_one.starts_with("Roundhouse has no GitHub account"), "{connect_one}");
}

#[test]
fn a_default_whose_token_has_ended_blocks_from_its_end_saying_to_renew_it() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("a-token", None, "g1"), &owner(7, "ada", Some(5000))).unwrap();

    logins.check(4999).unwrap();
    let before_its_end = sent(&logins);
    logins.check(5000).unwrap();

    assert_eq!(before_its_end, as_ada("a-token"));
    let reason = reason(sent(&logins));
    assert!(reason.contains("ada, the default account, has ended") && reason.contains("Renew it"), "{reason}");
}

#[test]
fn a_default_whose_tokens_file_is_gone_or_cant_be_read_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("a-token", None, "g1"), &ada()).unwrap();
    let file = file_holding(dir.path(), "a-token");

    std::fs::remove_file(&file).unwrap();
    logins.check(2000).unwrap();
    let gone = reason(sent(&logins));
    // Which can't be read as a file.
    std::fs::create_dir(&file).unwrap();
    logins.check(2000).unwrap();
    let unreadable = reason(sent(&logins));

    assert!(gone.contains("can't find the token of ada, the default account"), "{gone}");
    assert!(unreadable.contains("can't read the token of ada, the default account ("), "{unreadable}");
}

/// The sign-in `id` approved as ada, with `token`, which ends at 9000, as the account `g` if the hub holds no sign-in as
/// ada.
fn signed_in(logins: &GithubLogins, store: &Store, id: &str, token: &str) {
    logins.approve_sign_in(&approved(&started(store, id), token, "g"), &ada()).unwrap();
}

/// The one file among the GitHub logins' in `dir`.
fn only_file(dir: &Path) -> PathBuf {
    let mut paths = std::fs::read_dir(dir.join("github-logins")).unwrap().map(|entry| entry.unwrap().path());
    let file = paths.next().unwrap();
    assert!(paths.next().is_none());
    file
}

#[test]
fn workers_are_sent_a_default_signed_in_with_its_access_token_as_its_approved_and_as_signing_in_again_renews_it() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let mut following = logins.git_credential();

    signed_in(&logins, &store, "first", "first-token");
    let approved = following.borrow_and_update().for_workers();
    signed_in(&logins, &store, "again", "renewed-token");
    let renewed = following.borrow_and_update().for_workers();

    assert_eq!(approved, as_ada("first-token"), "the first account became the default");
    assert_eq!(renewed, as_ada("renewed-token"), "signing in as ada again renewed it in place");
}

/// As after a Mac wakes from a sleep past its token's end, before the hub has renewed it. Choosing it as the default
/// is refused meanwhile, saying the same, and so is it once it needs signing in again.
#[test]
fn a_default_signed_in_blocks_from_its_tokens_end_saying_its_being_renewed_and_once_what_renews_it_ends_to_sign_in() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    signed_in(&logins, &store, "s", "a-token");

    logins.check(8999).unwrap();
    let before_its_end = sent(&logins);
    logins.check(9000).unwrap();
    let renewing = reason(sent(&logins));
    let chosen_while_renewing = logins.set_default(Some(&"g".into()), 9000).unwrap_err();
    // Once what renews it has ended, as the sign-in `approved` gives has at 90,000.
    logins.check(90_000).unwrap();
    let ended = reason(sent(&logins));
    let chosen_once_ended = logins.set_default(Some(&"g".into()), 90_000).unwrap_err();

    assert_eq!(before_its_end, as_ada("a-token"));
    assert!(renewing.contains("ada, the default account, has ended, and Roundhouse is renewing it"), "{renewing}");
    assert!(renewing.contains("If this goes on, sign in with GitHub again"), "{renewing}");
    assert!(ended.contains("The sign-in of ada, the default account, has ended"), "{ended}");
    assert!(ended.contains("Sign in with GitHub again") && !ended.contains("Renew"), "{ended}");
    assert_eq!((chosen_while_renewing.code, chosen_once_ended.code), (ErrorCode::Conflict, ErrorCode::Conflict));
    assert!(chosen_while_renewing.message.contains("is being renewed"), "{}", chosen_while_renewing.message);
    assert!(chosen_once_ended.message.contains("needs signing in with GitHub again"), "{}", chosen_once_ended.message);
}

#[test]
fn a_default_that_needs_signing_in_again_blocks_before_its_access_token_ends() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    signed_in(&logins, &store, "s", "a-token");

    store.github_login_needs_sign_in(&"g".into()).unwrap().unwrap();
    logins.check(2000).unwrap();

    let reason = reason(sent(&logins));
    assert!(
        reason.contains("ada, the default account, has ended") && reason.contains("Sign in with GitHub again"),
        "{reason}"
    );
}

/// Not as one whose file holds nothing, which says to connect it again, nor as the machine's own.
#[test]
fn a_default_whose_file_only_a_newer_build_reads_blocks_saying_so() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    signed_in(&logins, &store, "s", "a-token");

    std::fs::write(only_file(dir.path()), r#"{"format":"a_newer_form","token":"t"}"#).unwrap();
    logins.check(2000).unwrap();

    let reason = reason(sent(&logins));
    assert!(reason.contains("ada, the default account, is one only a newer Roundhouse can read"), "{reason}");
    assert!(reason.contains("Update Roundhouse"), "{reason}");
}

/// Every block's words, for a token's account and an installation's, which git prints in a terminal on any provider
/// Roundhouse sets up GitHub on, and Git's box shows on any device: each says once what Roundhouse can't do, and never
/// says where it's read, as "this workspace" or "there" for the hub's machine would.
#[test]
fn every_block_says_once_what_roundhouse_cant_do_and_never_where_its_read() {
    let user = |id, name: &str| GithubUser { id: GithubUserId::new(id), name: name.into() };
    let account = GithubAccount {
        id: "g".into(),
        user: user(7, "ada"),
        held_login: None,
        installation: None,
        rev: Rev::default(),
    };
    let installation =
        GithubInstallation { app: GithubAppId::new(1), id: GithubInstallationId::new(1), on: user(8, "adalabs") };
    let bot =
        GithubAccount { user: user(9, "ada-roundhouse[bot]"), installation: Some(installation), ..account.clone() };
    let every = || {
        [
            Unusable::Disconnected,
            Unusable::Ended,
            Unusable::SignInAgain,
            Unusable::Renewing,
            Unusable::Newer,
            Unusable::Unreadable(None),
            Unusable::Unreadable(Some(std::io::Error::other("e"))),
            Unusable::GhLoggedOut(NoGhLogin::LoggedOut),
            Unusable::GhLoggedOut(NoGhLogin::NotInstalled),
            Unusable::GhOtherUser("grace".into()),
            Unusable::GhUnread(None),
            Unusable::GhUnread(Some("gh took more than 15s".into())),
        ]
    };

    for of in [&account, &bot] {
        for said in every().iter().map(|unusable| lost(of, unusable)) {
            assert_eq!(said.matches("Roundhouse can't read pull requests").count(), 1, "{said}");
            assert!(!said.contains("workspace") && !said.contains(" there") && !said.contains("this Mac"), "{said}");
        }
    }
}
