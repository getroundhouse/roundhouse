//! A stand-in for `gh`, since GitHub can't be reached from a test.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{Stack, install_executable};

/// What `gh pr view` prints for an open pull request.
pub(crate) const OPEN: &str =
    r#"{"isDraft":false,"number":7,"state":"OPEN","title":"T","url":"https://example.com/7"}"#;
/// What it says for a branch with none, in its own words, as against failing.
pub(crate) const NONE: &str = "none";
/// What it says when it can't reach GitHub.
pub(crate) const OFFLINE: &str = "offline";
/// What it says before `gh auth login`.
pub(crate) const LOGGED_OUT: &str = "logged-out";

/// A `gh` at the front of the hub's `PATH`, which says what a test tells it to for `gh pr view` and `gh api rate_limit`,
/// that the pull request isn't in a merge queue, and fails anything else. It writes down each time it's run.
pub(crate) struct StandInGh {
    bin: PathBuf,
    says: PathBuf,
    rate_limit: PathBuf,
    ran: PathBuf,
    /// While this file is there, `gh pr view` waits.
    held: PathBuf,
}

impl StandInGh {
    /// Replaces the stack's hub with one that uses it. Until told otherwise, it says there's no pull request, and that
    /// nothing of GitHub's limit has been spent.
    pub(crate) fn start(stack: &mut Stack) -> Self {
        stack.kill_hub();
        let bin = stack.path("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let gh = Self {
            bin,
            says: stack.path("gh-says"),
            rate_limit: stack.path("gh-rate-limit"),
            ran: stack.path("gh-ran"),
            held: stack.path("gh-held"),
        };
        let script = format!(
            r#"#!/bin/sh
echo "$1 $2" >> '{ran}'
[ "$1 $2" = 'api rate_limit' ] && exec cat '{rate_limit}'
[ "$1 $2" = 'api graphql' ] && exec echo '{{"data":{{"resource":{{"mergeQueueEntry":null}}}}}}'
[ "$1 $2" = 'pr view' ] || exit 1
while [ -f '{held}' ]; do sleep 0.02; done
case "$(cat '{says}')" in
  {NONE}) echo 'no pull requests found for branch "x"' >&2; exit 1 ;;
  {OFFLINE}) echo 'error connecting to api.github.com' >&2; exit 1 ;;
  {LOGGED_OUT}) echo 'To get started with GitHub CLI, please run:  gh auth login' >&2; exit 4 ;;
  *) exec cat '{says}' ;;
esac
"#,
            ran = gh.ran.display(),
            rate_limit = gh.rate_limit.display(),
            held = gh.held.display(),
            says = gh.says.display()
        );
        let source = stack.path("gh.source");
        std::fs::write(&source, script).unwrap();
        install_executable(&source, &gh.bin.join("gh"));
        gh.say(NONE);
        gh.limit_left(5000);
        stack.add_env("PATH", format!("{}:{}", gh.bin.display(), std::env::var("PATH").unwrap()));
        // No login shell, so workers get this PATH, with the stand-in at its front.
        stack.add_env("SHELL", "/usr/bin/false".to_string());
        stack.start_hub();
        gh
    }

    pub(crate) fn say(&self, what: &str) {
        write_whole(&self.says, what);
    }

    /// What GitHub's limit has left of 5,000, as `gh api rate_limit` prints it, resetting in an hour.
    pub(crate) fn limit_left(&self, remaining: u32) {
        let in_an_hour = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 3600;
        let graphql = format!(r#"{{"limit":5000,"remaining":{remaining},"reset":{in_an_hour},"used":0}}"#);
        write_whole(&self.rate_limit, &format!(r#"{{"resources":{{"graphql":{graphql}}}}}"#));
    }

    /// How many times `gh pr view` has run, which is a call to GitHub each.
    pub(crate) fn views(&self) -> usize {
        self.ran("pr view")
    }

    /// How many times `gh` has run with these first two arguments.
    pub(crate) fn ran(&self, what: &str) -> usize {
        std::fs::read_to_string(&self.ran).unwrap_or_default().lines().filter(|line| *line == what).count()
    }

    pub(crate) fn hold(&self) {
        std::fs::write(&self.held, "").unwrap();
    }

    pub(crate) fn release(&self) {
        std::fs::remove_file(&self.held).unwrap();
    }
}

/// Whole or not at all, since a worker may run the stand-in at any moment.
fn write_whole(path: &Path, text: &str) {
    let next = path.with_extension("next");
    std::fs::write(&next, text).unwrap();
    std::fs::rename(&next, path).unwrap();
}
