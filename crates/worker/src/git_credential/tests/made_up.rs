//! What the tests give a worker's files, made up.

use std::path::Path;

use protocol::model::GithubToken;
use protocol::worker::GitCredential;

use super::super::files::Files;

/// A worker's files in the state directory `state`, whose helper is a program with a space in its path.
pub(super) fn files(state: &Path) -> Files {
    Files::new(state, vec!["/opt/Round House/roundhoused".into(), "git-credential".into(), "--".into()])
}

pub(super) fn ada() -> GitCredential {
    GitCredential { username: "ada".into(), token: GithubToken::new("made-up"), identity: None }
}
