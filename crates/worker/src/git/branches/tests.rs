use std::path::Path;

use protocol::model::{BranchName, CommitId, Head, RemoteName, Upstream};

use super::*;
use crate::git::tests::{new_repo, run};

/// A repository, and a worktree of it on a branch of its own, `aspen`, as a workspace's is.
fn worktree(dir: &Path) -> (PathBuf, PathBuf) {
    let repo = new_repo(dir);
    let worktree = dir.join("aspen");
    run(&repo, &["worktree", "add", "-q", "--no-track", "-b", "aspen", worktree.to_str().unwrap(), "main"]);
    (repo, worktree)
}

async fn read_branches(worktree: &Path) -> Branches {
    let places = places(worktree).await.unwrap();
    read(worktree, &places).await.unwrap().branches
}

fn names(branches: &Branches) -> Vec<&str> {
    branches.had.iter().map(|branch| branch.name.as_str()).collect()
}

fn tip(worktree: &Path, name: &str) -> CommitId {
    CommitId::from(run(worktree, &["rev-parse", name]).as_str())
}

#[tokio::test]
async fn a_new_worktree_has_only_its_own_branch() {
    let dir = tempfile::tempdir().unwrap();
    let (_, worktree) = worktree(dir.path());

    let branches = read_branches(&worktree).await;

    assert_eq!(branches.head, Head::Branch { name: BranchName::from("aspen") });
    assert_eq!(names(&branches), ["aspen"], "not main, which only the repository's own worktree checked out");
    assert_eq!(branches.checked_out().map(|branch| &branch.tip), Some(&tip(&worktree, "aspen")));
}

#[tokio::test]
async fn every_branch_the_worktree_has_checked_out_counts_until_its_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let (repo, worktree) = worktree(dir.path());
    run(&repo, &["branch", "elsewhere"]);
    for args in [
        &["commit", "-q", "--allow-empty", "-m", "on aspen"][..],
        &["checkout", "-q", "-b", "header"],
        &["commit", "-q", "--allow-empty", "-m", "on header"],
        &["branch", "-m", "header", "fix-the-header"],
        &["switch", "-q", "-c", "doomed"],
        &["switch", "-q", "aspen"],
        &["branch", "-q", "-D", "doomed"],
        &["checkout", "-q", "--detach", "main"],
    ] {
        run(&worktree, args);
    }

    let branches = read_branches(&worktree).await;

    assert_eq!(branches.head, Head::Detached { commit: tip(&repo, "main") });
    assert_eq!(names(&branches), ["aspen", "fix-the-header"], "not the one made elsewhere, nor the one deleted");
    assert_eq!(branches.had[1].tip, tip(&worktree, "fix-the-header"));
    assert_eq!(branches.checked_out(), None);
}

#[tokio::test]
async fn a_branch_pushed_under_another_name_has_that_name_upstream_and_its_tip_as_of_the_push() {
    let dir = tempfile::tempdir().unwrap();
    let (repo, worktree) = worktree(dir.path());
    let origin = dir.path().join("origin.git");
    run(dir.path(), &["init", "-q", "--bare", origin.to_str().unwrap()]);
    run(&repo, &["remote", "add", "origin", origin.to_str().unwrap()]);
    run(&worktree, &["commit", "-q", "--allow-empty", "-m", "pushed"]);
    run(&worktree, &["push", "-q", "-u", "origin", "aspen:aspen/fix-the-header"]);
    run(&worktree, &["commit", "-q", "--allow-empty", "-m", "not pushed"]);
    run(&worktree, &["switch", "-q", "-c", "unpushed"]);

    let branches = read_branches(&worktree).await;

    let upstream = Upstream {
        remote: RemoteName::from("origin"),
        name: BranchName::from("aspen/fix-the-header"),
        tip: Some(tip(&worktree, "aspen~1")),
    };
    assert_eq!(branches.had[0].upstream, Some(upstream));
    assert_eq!(branches.had[1].upstream, None);
}

#[tokio::test]
async fn a_branch_being_rebased_still_counts_while_head_is_detached() {
    let dir = tempfile::tempdir().unwrap();
    let (repo, worktree) = worktree(dir.path());
    std::fs::write(worktree.join("a.txt"), "aspen's\n").unwrap();
    run(&worktree, &["commit", "-qam", "on aspen"]);
    std::fs::write(repo.join("a.txt"), "main's\n").unwrap();
    run(&repo, &["commit", "-qam", "on main"]);
    let rebasing = std::process::Command::new("git").args(["rebase", "main"]).current_dir(&worktree).output().unwrap();
    assert!(!rebasing.status.success(), "it stops at the conflict");

    let branches = read_branches(&worktree).await;

    assert!(matches!(branches.head, Head::Detached { .. }));
    assert_eq!(names(&branches), ["aspen"]);
}

/// In a repository that keeps its refs in a reftable rather than as files, which git's own reflog command reads.
#[tokio::test]
async fn a_reftable_worktree_counts_every_branch_it_left() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    if !reftable(&repo) {
        eprintln!("skipped: this git can't make a reftable");
        return;
    }
    let worktree = dir.path().join("aspen");
    run(&repo, &["worktree", "add", "-q", "--no-track", "-b", "aspen", worktree.to_str().unwrap(), "main"]);
    run(&worktree, &["switch", "-q", "-c", "header"]);
    run(&worktree, &["switch", "-q", "-c", "footer"]);

    let branches = read_branches(&worktree).await;

    assert_eq!(branches.head, Head::Branch { name: BranchName::from("footer") });
    assert_eq!(names(&branches), ["aspen", "footer", "header"]);
}

/// A repository made with its refs in a reftable, with one commit, or false where git is too old to make one.
pub(crate) fn reftable(repo: &Path) -> bool {
    std::fs::create_dir_all(repo).unwrap();
    let made = std::process::Command::new("git")
        .args(["init", "-q", "--ref-format=reftable", "-b", "main"])
        .current_dir(repo)
        .output()
        .unwrap();
    if made.status.success() {
        run(repo, &["commit", "-q", "--allow-empty", "-m", "first"]);
    }
    made.status.success()
}

#[tokio::test]
async fn a_repository_that_keeps_no_reflogs_knows_only_the_branch_checked_out() {
    let dir = tempfile::tempdir().unwrap();
    let (repo, worktree) = worktree(dir.path());
    run(&worktree, &["switch", "-q", "-c", "header"]);
    run(&repo, &["config", "core.logAllRefUpdates", "false"]);
    std::fs::remove_dir_all(places(&worktree).await.unwrap().git_dir.join("logs")).unwrap();

    let branches = read_branches(&worktree).await;

    assert_eq!(names(&branches), ["header"]);
}

/// HEAD names a branch with no commits yet, so git can't read HEAD's reflog.
#[tokio::test]
async fn a_branch_with_no_commits_yet_is_the_one_checked_out() {
    let dir = tempfile::tempdir().unwrap();
    let (_, worktree) = worktree(dir.path());
    run(&worktree, &["switch", "-q", "--orphan", "fresh"]);

    let branches = read_branches(&worktree).await;

    assert_eq!(branches.head, Head::Branch { name: BranchName::from("fresh") });
}

#[test]
fn a_branch_counts_from_the_reflog_once_its_been_left() {
    let subjects = [
        "checkout: moving from 3 to fix-the-header",
        "checkout: moving from header to main",
        "checkout: moving from aspen to header",
        "commit: on aspen",
        "reset: moving to HEAD",
    ]
    .join("\n");

    let names = left(&subjects);

    let expected = ["3", "aspen", "header"].map(BranchName::from);
    assert_eq!(names.into_iter().collect::<Vec<_>>(), expected, "not main, which it may have checked out detached");
}

#[tokio::test]
async fn only_what_git_writes_the_branches_in_matters() {
    let dir = tempfile::tempdir().unwrap();
    let (_, worktree) = worktree(dir.path());
    let places = places(&worktree).await.unwrap();
    let read = read(&worktree, &places).await.unwrap();
    let (own, shared) = (places.git_dir.clone(), places.common_dir.clone());
    let matters = |file: PathBuf| matters(&places, Some(&read), &file);

    let mattering =
        [own.join("HEAD"), own.join("logs").join("HEAD"), shared.join("config"), shared.join("packed-refs")];
    let ref_read = shared.join("refs").join("heads").join("aspen");
    let not = [own.join("index"), shared.join("index"), shared.join("refs").join("heads").join("main")];
    let elsewhere = shared.join("worktrees").join("other").join("HEAD");

    assert!(mattering.into_iter().all(matters));
    assert!(matters(ref_read), "the ref of a branch it has");
    assert!(matters(shared.join("refs").join("heads")), "a directory one is in, which git makes again after packing");
    assert!(!matters(shared.join("refs")), "but not every ref's");
    assert!(matters(shared.join("reftable").join("tables.list")) && matters(own.join("reftable").join("tables.list")));
    assert!(!not.into_iter().any(matters), "not the index, nor a branch it hasn't had");
    assert!(!matters(elsewhere), "nor another worktree's HEAD");
}
