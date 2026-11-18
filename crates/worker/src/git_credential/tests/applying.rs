use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use protocol::client::ErrorCode;
use protocol::worker::GithubCredential;

use super::super::git_version::reads_the_config;
use super::super::{CredentialHelper, GitCredentials};
use super::ada;
use crate::testing::given;

fn credentials(state: &Path) -> GitCredentials {
    GitCredentials::new(state, &CredentialHelper::new("/opt/roundhoused", ["git-credential", "--"]), given())
}

/// Where a process started now is told git's config is, if it's told.
fn config_given(credentials: &GitCredentials) -> Option<String> {
    let mut command = Command::new("git");
    credentials.given().apply(&mut command);
    let value = command.get_envs().find(|(name, _)| *name == OsStr::new("GIT_CONFIG_VALUE_0")).and_then(|(_, v)| v);
    value.map(|value| value.to_string_lossy().into_owned())
}

/// With this machine's git, which reads it.
#[tokio::test]
async fn a_process_is_given_the_files_only_while_the_credential_isnt_the_machines_own() {
    let state = tempfile::tempdir().unwrap();
    let credentials = credentials(state.path());
    let config = state.path().join("git-credential").join("config");
    assert_eq!(config_given(&credentials), None, "a worker starts with the machine's own");

    credentials.set(GithubCredential::Account(ada())).await.unwrap();
    let under_an_account = config_given(&credentials);
    credentials.set(GithubCredential::MachineOwn).await.unwrap();

    assert_eq!(under_an_account, Some(config.display().to_string()));
    assert_eq!(config_given(&credentials), None);
    assert!(!state.path().join("git-credential").exists(), "nothing is left to change what git or gh use");
}

#[tokio::test]
async fn a_git_older_than_2_31_is_refused_and_leaves_the_machines_own() {
    let state = tempfile::tempdir().unwrap();
    let credentials = credentials(state.path());
    credentials.apply(GithubCredential::Account(ada()), &Ok(())).await.unwrap();

    let old = reads_the_config("git version 2.30.9");
    let refused = credentials.apply(GithubCredential::Account(ada()), &old).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::Unavailable);
    assert_eq!(config_given(&credentials), None);
    assert!(!state.path().join("git-credential").exists());
}

/// From a newer hub, which may mean an account this worker can't hand on, so nothing goes out as the wrong one.
#[tokio::test]
async fn a_credential_of_a_kind_this_worker_doesnt_know_blocks_and_is_refused() {
    let state = tempfile::tempdir().unwrap();
    let credentials = credentials(state.path());

    let refused = credentials.apply(GithubCredential::Unknown, &Ok(())).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::Unsupported);
    assert!(config_given(&credentials).is_some(), "what it starts is given the block");
    let answer = std::fs::read(state.path().join("git-credential").join("credential.json")).unwrap();
    let answer: GithubCredential = serde_json::from_slice(&answer).unwrap();
    assert!(matches!(&answer, GithubCredential::Blocked { reason } if reason.contains("older than")), "{answer:?}");
}

#[test]
fn only_a_git_from_2_31_on_reads_the_config() {
    for read in ["git version 2.31.0", "git version 2.54.0 (Apple Git-157)", "git version 2.45.1.windows.1"] {
        assert_eq!(reads_the_config(read), Ok(()), "{read}");
    }
    for unread in ["git version 2.30.9", "git version 1.99.0", "git version", "not git"] {
        assert!(reads_the_config(unread).is_err(), "{unread}");
    }
}
