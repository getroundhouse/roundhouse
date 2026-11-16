use std::path::Path;

use protocol::model::GithubToken;
use protocol::worker::GitCredential;

use super::super::answer_credential_request;
use super::super::files::Handed;
use super::{ada, files};

/// What git asks for a remote on github.com whose URL names another user.
const ASKED: &str = "protocol=https\nhost=github.com\nusername=someone-else\ncapability[]=authtype\n\n";

/// What the helper answers for the worker whose files are in `state`, and what it says, when git asks `action` with
/// `asked`.
fn answered(state: &Path, action: &str, asked: &str) -> (String, String) {
    let (mut answer, mut said) = (Vec::new(), Vec::new());
    let dir = state.join("git-credential");
    answer_credential_request(&dir, action, asked.as_bytes(), &mut answer, &mut said).unwrap();
    (String::from_utf8(answer).unwrap(), String::from_utf8(said).unwrap())
}

#[test]
fn an_account_is_answered_for_github_whatever_user_the_remote_names() {
    let state = tempfile::tempdir().unwrap();
    files(state.path()).write(&Handed::Account(ada())).unwrap();

    let answer = answered(state.path(), "get", ASKED);

    assert_eq!(answer, ("username=ada\npassword=made-up\n".to_string(), String::new()));
}

#[test]
fn a_block_says_why_and_tells_git_to_stop_rather_than_ask_another() {
    let state = tempfile::tempdir().unwrap();
    files(state.path()).write(&Handed::Blocked("Renew ada in Settings".into())).unwrap();

    let answer = answered(state.path(), "get", ASKED);

    assert_eq!(answer, ("quit=1\n".to_string(), "Renew ada in Settings\n".to_string()));
}

/// Git asks every helper to keep or forget what worked or didn't, and only the worker's hub changes what's given.
#[test]
fn nothing_is_answered_but_a_get_for_github_with_something_given() {
    let (given, none) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    files(given.path()).write(&Handed::Account(ada())).unwrap();
    let nothing = (String::new(), String::new());

    for action in ["store", "erase", "newer"] {
        assert_eq!(answered(given.path(), action, ASKED), nothing, "{action}");
    }
    assert_eq!(answered(given.path(), "get", "protocol=https\nhost=gitlab.com\n\n"), nothing, "another host");
    assert_eq!(answered(given.path(), "get", "protocol=http\nhost=github.com\n\n"), nothing, "without TLS");
    assert_eq!(answered(none.path(), "get", ASKED), nothing, "once the worker's files have gone");
}

/// Git reads what it's answered a line at a time, so a line in a value would answer what the hub never sent.
#[test]
fn a_value_with_a_line_in_it_is_never_answered() {
    let state = tempfile::tempdir().unwrap();
    let credential = GitCredential { username: "ada".into(), token: GithubToken::new("made-up\nusername=eve") };
    files(state.path()).write(&Handed::Account(credential)).unwrap();

    let (answer, said) = answered(state.path(), "get", ASKED);

    assert_eq!(answer, "quit=1\n");
    assert!(said.contains("push as nobody"), "{said}");
}
