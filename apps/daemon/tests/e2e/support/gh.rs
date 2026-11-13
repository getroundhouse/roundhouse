//! A stand-in for `gh`, since GitHub can't be reached from a test.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{Stack, install_executable};

/// What `gh pr view` prints for an open pull request.
pub(crate) const OPEN: &str =
    r#"{"isDraft":false,"number":7,"state":"OPEN","title":"T","url":"https://example.com/7"}"#;
/// What it prints once that one has merged.
pub(crate) const MERGED: &str =
    r#"{"isDraft":false,"number":7,"state":"MERGED","title":"T","url":"https://example.com/7"}"#;
/// Its URL.
pub(crate) const OPEN_URL: &str = "https://example.com/7";
/// What it prints for that one once someone else has pushed to it, so its newest commit isn't in the worktree, though
/// the one before is.
pub(crate) const MOVED_ON: &str = r#"{"isDraft":false,"number":7,"state":"OPEN","title":"T",
    "url":"https://example.com/7","headRefOid":"0123456789abcdef0123456789abcdef01234567"}"#;
/// What it prints for one from another machine's branch of the same name, none of whose commits were ever here.
pub(crate) const SOMEONE_ELSES: &str = r#"{"isDraft":false,"number":9,"state":"OPEN","title":"U",
    "url":"https://example.com/9","headRefOid":"fedcba9876543210fedcba9876543210fedcba98",
    "commits":[{"oid":"fedcba9876543210fedcba9876543210fedcba98"}]}"#;
/// What it prints for another of the branch's own, opened from it after the first.
pub(crate) const NEWER: &str =
    r#"{"isDraft":false,"number":8,"state":"OPEN","title":"V","url":"https://example.com/8"}"#;
/// What it says for a branch with none, in its own words, as against failing.
pub(crate) const NONE: &str = "none";
/// What it says when it can't reach GitHub.
pub(crate) const OFFLINE: &str = "offline";
/// What it says before `gh auth login`.
pub(crate) const LOGGED_OUT: &str = "logged-out";

/// A `gh` at the front of the hub's `PATH`, which says what a test tells it to for `gh pr view` and `gh api rate_limit`,
/// that the pull request isn't in a merge queue, and fails anything else. By URL, `gh pr view` says what it was told
/// of the pull request at that URL, but for the exception [`StandInGh::script`] names. It prints the commits only when
/// they're asked for, as `gh` does, and fails when they're asked for by the branch's name, which GitHub charges 31
/// points of its hourly limit for, where by URL they cost 1. A pull request that doesn't say its newest commit, or its
/// commits, gets the one checked out for them, as one pushed from the worktree has, so it's the branch's own. It
/// writes down each time it's run.
pub(crate) struct StandInGh {
    bin: PathBuf,
    says: PathBuf,
    /// What it says for `gh pr view` of the URL in [`StandInGh::said_of`], which is how a branch's linked pull
    /// request is read, once a test has said.
    says_of: PathBuf,
    said_of: PathBuf,
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
            says_of: stack.path("gh-says-of"),
            said_of: stack.path("gh-said-of"),
            rate_limit: stack.path("gh-rate-limit"),
            ran: stack.path("gh-ran"),
            held: stack.path("gh-held"),
        };
        let source = stack.path("gh.source");
        std::fs::write(&source, gh.script()).unwrap();
        install_executable(&source, &gh.bin.join("gh"));
        gh.say(NONE);
        gh.limit_left(5000);
        stack.add_env("PATH", format!("{}:{}", gh.bin.display(), std::env::var("PATH").unwrap()));
        // No login shell, so workers get this PATH, with the stand-in at its front.
        stack.add_env("SHELL", "/usr/bin/false".to_string());
        stack.start_hub();
        gh
    }

    /// The stand-in itself, as a shell script. By URL, as `gh` does, it fails for one with no pull request rather than
    /// saying there's none, except while it's told to say [`NONE`] for the branch, when it says none by any URL too,
    /// which `gh` never does.
    fn script(&self) -> String {
        format!(
            r#"#!/bin/sh
echo "$1 $2" >> '{ran}'
[ "$1 $2" = 'api rate_limit' ] && exec cat '{rate_limit}'
[ "$1 $2" = 'api graphql' ] && exec echo '{{"data":{{"resource":{{"mergeQueueEntry":null}}}}}}'
[ "$1 $2" = 'pr view' ] || exit 1
while [ -f '{held}' ]; do sleep 0.02; done
says='{says}' url=
if [ "${{3#http}}" != "$3" ]; then
  url=$3 fields=$5
  [ -f '{said_of}' ] && [ "$url" = "$(cat '{said_of}')" ] && says='{says_of}'
else
  fields=$4
  case ",$fields," in
    *,commits,*) echo 'commits asked for by the branch name, which costs 31 points' >&2; exit 1 ;;
  esac
fi
case "$(cat "$says")" in
  {NONE}) echo 'no pull requests found for branch "x"' >&2; exit 1 ;;
  {OFFLINE}) echo 'error connecting to api.github.com' >&2; exit 1 ;;
  {LOGGED_OUT}) echo 'To get started with GitHub CLI, please run:  gh auth login' >&2; exit 4 ;;
esac
answer=$(tr -d '\n' < "$says") head=$(git rev-parse HEAD)
if [ -n "$url" ]; then
  case "$answer" in
    *"\"url\":\"$url\""*) ;;
    *) echo "GraphQL: Could not resolve to a PullRequest with the number of ${{url##*/}}." >&2; exit 1 ;;
  esac
fi
case "$answer" in *headRefOid*) ;; *) answer="{{\"headRefOid\":\"$head\",${{answer#\{{}}" ;; esac
case ",$fields," in
  *,commits,*)
    case "$answer" in *'"commits"'*) ;; *) answer="{{\"commits\":[{{\"oid\":\"$head\"}}],${{answer#\{{}}" ;; esac ;;
  *) answer=$(printf '%s' "$answer" | sed 's/,[[:space:]]*"commits":\[[^]]*\]//') ;;
esac
printf '%s\n' "$answer"
"#,
            ran = self.ran.display(),
            rate_limit = self.rate_limit.display(),
            held = self.held.display(),
            says = self.says.display(),
            says_of = self.says_of.display(),
            said_of = self.said_of.display()
        )
    }

    pub(crate) fn say(&self, what: &str) {
        write_whole(&self.says, what);
    }

    /// What it says of the pull request at `url`, read by it as a branch's linked one is. By any other URL, it says
    /// what it says of the branch's, where that one is at that URL, and fails anywhere else, but for the exception
    /// [`StandInGh::script`] names.
    pub(crate) fn say_of(&self, url: &str, what: &str) {
        write_whole(&self.says_of, what);
        write_whole(&self.said_of, url);
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
