//! Repositories for the tests of whatever runs git, made with a git that reads none of the person's config.

use super::super::prepare_workspace;
use crate::testing::given;
use protocol::client::ApiError;
use protocol::model::BranchName;
use protocol::worker::WorkspaceReady;
use std::path::{Path, PathBuf};

pub(crate) fn run(cwd: &Path, args: &[&str]) -> String {
    process::testing::git(cwd, args)
}

/// What `git status` lists in the repository at `cwd`, each untracked file on its own, leaving out what only this
/// machine's own git ignores, so a test of what a repository ignores can't pass because of it.
pub(crate) fn changes(cwd: &Path) -> String {
    run(cwd, &["-c", "core.excludesFile=/dev/null", "status", "--porcelain", "--untracked-files=all"])
}

pub(crate) fn new_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    std::fs::write(repo.join(".gitignore"), "ignored/\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "first"]);
    repo
}

/// A repository made in `dir` as [`new_repo`] makes one, and a clone of it beside it, which is returned second.
pub(crate) fn cloned(dir: &Path) -> (PathBuf, PathBuf) {
    let origin = new_repo(dir);
    let clone = dir.join("clone");
    run(dir, &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    (origin, clone)
}

/// On the branch the hub starts a workspace on.
pub(crate) async fn prepare(
    root: &Path,
    repo: &Path,
    name: &str,
    base: Option<&str>,
) -> Result<WorkspaceReady, ApiError> {
    let branch = BranchName::from(format!("{name}/start"));
    prepare_workspace(&given(), root, "repo", repo, name, &branch, base).await
}

/// Gives `repo` a `post-checkout` hook running `body` under `/bin/sh`.
pub(crate) fn post_checkout_hook(repo: &Path, body: &str) {
    let hooks = repo.with_file_name("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("post-checkout");
    process::testing::write_program(&hook, format!("#!/bin/sh\n{body}\n")).unwrap();
    run(repo, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
}
