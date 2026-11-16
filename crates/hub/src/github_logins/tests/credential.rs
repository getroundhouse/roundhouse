//! What workers are to give their processes for github.com in each state of the default GitHub account, as each change
//! to the logins, and each check by the clock, works it out.

use std::path::{Path, PathBuf};

use protocol::worker::{GitCredential, GithubCredential};

use super::super::*;
use super::{ada, connect, logins_in, owner};

/// What a worker's link following `logins` is sent now.
fn sent(logins: &GithubLogins) -> GithubCredential {
    logins.git_credential().borrow().clone()
}

fn account(username: &str, token: &str) -> GithubCredential {
    GithubCredential::Account(GitCredential { username: username.into(), token: GithubToken::new(token) })
}

/// What a block says, which fails for anything but a block, since nothing but one may take a lost default's place.
fn reason(credential: GithubCredential) -> String {
    let GithubCredential::Blocked { reason } = credential else { panic!("not a block: {credential:?}") };
    reason
}

/// The file that holds `token`, among the GitHub logins' in `dir`.
fn file_holding(dir: &Path, token: &str) -> PathBuf {
    let mut paths = std::fs::read_dir(dir.join("github-logins")).unwrap().map(|entry| entry.unwrap().path());
    paths.find(|path| std::fs::read_to_string(path).is_ok_and(|held| held == token)).unwrap()
}

#[test]
fn workers_are_sent_nothing_of_the_hubs_while_the_default_is_each_machines_own() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let with_none = sent(&logins);
    logins.connect(&connect("a-token", None, "g1"), &ada()).unwrap();

    logins.set_default(None, 2000).unwrap();

    assert_eq!(with_none, GithubCredential::MachineOwn);
    assert_eq!(sent(&logins), GithubCredential::MachineOwn, "an account that isn't the default isn't sent");
}

#[test]
fn workers_are_sent_the_defaults_username_and_token_as_its_connected_and_renewed_and_nothing_new_for_another() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let mut following = logins.git_credential();

    logins.connect(&connect("first-token", None, "g1"), &owner(7, "ada", Some(5000))).unwrap();
    let connected = following.borrow_and_update().clone();
    logins.connect(&connect("renewed-token", Some("g1"), "g2"), &ada()).unwrap();
    let renewed = following.borrow_and_update().clone();
    logins.connect(&connect("another-token", None, "g3"), &owner(8, "grace", None)).unwrap();
    logins.check(6000);

    assert_eq!(connected, account("ada", "first-token"), "the first account became the default");
    assert_eq!(renewed, account("ada", "renewed-token"));
    assert!(!following.has_changed().unwrap(), "another account, and the first token's end, change nothing");
}

#[test]
fn a_disconnected_default_blocks_saying_to_connect_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("a-token", None, "g1"), &ada()).unwrap();

    logins.disconnect(&"g1".into(), 2000).unwrap();

    let reason = reason(sent(&logins));
    assert!(reason.contains("default GitHub account, ada, was disconnected"), "{reason}");
    assert!(reason.contains("Connect it again in Roundhouse's Settings"), "{reason}");
}

#[test]
fn a_default_whose_token_has_ended_blocks_from_its_end_saying_to_renew_it() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("a-token", None, "g1"), &owner(7, "ada", Some(5000))).unwrap();

    logins.check(4999);
    let before_its_end = sent(&logins);
    logins.check(5000);

    assert_eq!(before_its_end, account("ada", "a-token"));
    let reason = reason(sent(&logins));
    assert!(reason.contains("ada, has ended") && reason.contains("Renew it"), "{reason}");
}

#[test]
fn a_default_whose_tokens_file_is_gone_or_cant_be_read_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("a-token", None, "g1"), &ada()).unwrap();
    let file = file_holding(dir.path(), "a-token");

    std::fs::remove_file(&file).unwrap();
    logins.check(2000);
    let gone = reason(sent(&logins));
    // Which can't be read as a file.
    std::fs::create_dir(&file).unwrap();
    logins.check(2000);
    let unreadable = reason(sent(&logins));

    assert!(gone.contains("can't find the token of its default GitHub account, ada"), "{gone}");
    assert!(unreadable.contains("can't read the token of its default GitHub account, ada ("), "{unreadable}");
}
