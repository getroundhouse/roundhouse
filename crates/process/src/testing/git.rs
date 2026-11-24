//! Git, for a test to make and read its repositories with.

use std::path::Path;
use std::process::Command;

use super::WholeEnvironment;

/// Where git reads no config from.
const NOTHING: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

/// The whole environment git runs in for a test: no home, no config but what the repository has, a made-up author, and
/// no prompt for a password, so none of the person's own git, such as their global config, their hooks, their
/// credential helper or a `GIT_DIR` from a hook they're in, changes what it does. A test's own git runs in it, through
/// [`git_in`], and so does the git that the code under test starts, given it with [`WholeEnvironment::given`].
pub fn git_environment() -> WholeEnvironment {
    let mut environment = WholeEnvironment::kept();
    environment
        .set("GIT_CONFIG_GLOBAL", NOTHING)
        .set("GIT_CONFIG_NOSYSTEM", "1")
        .set("GIT_AUTHOR_NAME", "t")
        .set("GIT_AUTHOR_EMAIL", "t@example.com")
        .set("GIT_COMMITTER_NAME", "t")
        .set("GIT_COMMITTER_EMAIL", "t@example.com")
        .set("GIT_TERMINAL_PROMPT", "0");
    environment
}

/// `git`, to run in `dir`, in [`git_environment`], so what it makes is the same whoever runs the test.
pub fn git_in(dir: &Path) -> Command {
    let mut git = git_environment().command("git");
    git.current_dir(dir);
    git
}

/// Runs git in `dir` with `args`, as [`git_in`] has it, and answers what it printed, trimmed.
///
/// # Panics
///
/// If it can't be run, or fails, with how it ended, with a code or by a signal, and what it said.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = git_in(dir).args(args).output().unwrap_or_else(|e| panic!("couldn't run git {args:?}: {e}"));
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "git {args:?} failed ({}): {said}", output.status);
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever the person's own config and environment say, such as their name, or a `GIT_DIR` or `GIT_CONFIG_COUNT`.
    #[test]
    fn a_test_s_git_reads_only_the_repository_s_config() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);

        let scopes = git(dir.path(), &["config", "--list", "--show-scope"]);

        assert!(scopes.lines().all(|line| line.starts_with("local\t")), "{scopes}");
    }

    /// Beside what git said, since git ended by a signal can have said nothing.
    #[test]
    fn a_failure_says_how_git_ended() {
        let dir = tempfile::tempdir().unwrap();
        let ended = git_in(dir.path()).args(["rev-parse", "HEAD"]).output().unwrap().status;

        let failed = std::panic::catch_unwind(|| git(dir.path(), &["rev-parse", "HEAD"])).unwrap_err();

        let said = failed.downcast_ref::<String>().expect("a message");
        assert!(said.starts_with(&format!("git [\"rev-parse\", \"HEAD\"] failed ({ended}): ")), "{said}");
        assert!(said.contains("not a git repository"), "{said}");
    }
}
