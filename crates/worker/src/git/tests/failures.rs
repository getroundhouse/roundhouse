//! What the worker says when git fails, which reaches a person as the reason a request failed.

use super::super::{git_failed, git_ok};
use crate::testing::given;

/// It says how git ended, beside what git printed, so a failure says whether git exited or was ended.
#[tokio::test]
async fn a_git_that_fails_is_said_with_how_it_ended_and_what_it_printed() {
    let dir = tempfile::tempdir().unwrap();
    let ended = process::testing::git_in(dir.path()).args(["rev-parse", "HEAD"]).output().unwrap().status;

    let failed = git_ok(&given(), dir.path(), &["rev-parse", "HEAD"]).await.unwrap_err();

    let said = &failed.message;
    assert!(said.starts_with(&format!("git rev-parse HEAD failed ({ended}): ")), "{said}");
    assert!(said.contains("not a git repository"), "{said}");
}

/// One ended by a signal can print nothing, and then says so.
#[cfg(unix)]
#[test]
fn a_git_ended_by_a_signal_having_printed_nothing_says_so() {
    use std::os::unix::process::ExitStatusExt as _;

    let killed = git_failed("git commit -qm first", std::process::ExitStatus::from_raw(9), "\n");

    assert_eq!(killed.message, "git commit -qm first failed (signal: 9 (SIGKILL)), and printed nothing");
}
