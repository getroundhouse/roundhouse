//! Git, for a test to make and read its repositories with.

use std::path::Path;
use std::process::Command;

use super::WholeEnvironment;

/// Where git reads no config from.
const NOTHING: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

/// `git`, to run in `dir`, in a [`WholeEnvironment`] with no home, no config but what the repository has, and a
/// made-up author, so none of the person's own git, such as their global config, their hooks or a `GIT_DIR` from a
/// hook they're in, changes what it makes.
pub fn git_in(dir: &Path) -> Command {
    let mut environment = WholeEnvironment::kept();
    environment
        .set("GIT_CONFIG_GLOBAL", NOTHING)
        .set("GIT_CONFIG_NOSYSTEM", "1")
        .set("GIT_AUTHOR_NAME", "t")
        .set("GIT_AUTHOR_EMAIL", "t@example.com")
        .set("GIT_COMMITTER_NAME", "t")
        .set("GIT_COMMITTER_EMAIL", "t@example.com");
    let mut git = environment.command("git");
    git.current_dir(dir);
    git
}

/// Runs git in `dir` with `args`, as [`git_in`] has it, and answers what it printed, trimmed.
///
/// # Panics
///
/// If it can't be run, or fails, with what it said.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = git_in(dir).args(args).output().unwrap_or_else(|e| panic!("couldn't run git {args:?}: {e}"));
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
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
}
