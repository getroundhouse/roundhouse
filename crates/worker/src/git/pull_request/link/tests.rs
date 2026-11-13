use std::path::{Path, PathBuf};

use protocol::model::{BranchName, CommitId};

use super::{Choice, choose, has_been_at, link, linked, made_from, remote_branch};
use crate::git::tests::{new_repo, run};

const URL: &str = "https://example.com/pull/7";

/// A repository with `origin`, and a worktree of it on a branch of its own, `aspen`, as a workspace's is.
fn worktree(dir: &Path) -> PathBuf {
    let (repo, origin, worktree) = (new_repo(dir), dir.join("origin.git"), dir.join("aspen"));
    run(dir, &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
    run(&repo, &["remote", "add", "origin", origin.to_str().unwrap()]);
    run(&repo, &["push", "-q", "origin", "main"]);
    run(&repo, &["worktree", "add", "-q", "--no-track", "-b", "aspen", worktree.to_str().unwrap(), "main"]);
    worktree
}

fn commit(worktree: &Path, message: &str) -> CommitId {
    run(worktree, &["commit", "-q", "--allow-empty", "-m", message]);
    CommitId::from(run(worktree, &["rev-parse", "HEAD"]))
}

/// Another machine's clone, which commits on `origin`'s `onto` and pushes that as `branch`. The commit, which GitHub
/// then has as the newest of a pull request from that branch.
fn pushed_from_elsewhere(dir: &Path, onto: &str, branch: &str) -> CommitId {
    let (origin, elsewhere) = (dir.join("origin.git"), dir.join("elsewhere"));
    if !elsewhere.exists() {
        run(dir, &["clone", "-q", origin.to_str().unwrap(), elsewhere.to_str().unwrap()]);
    }
    run(&elsewhere, &["fetch", "-q", "origin"]);
    run(&elsewhere, &["switch", "-q", "--no-track", "-C", branch, &format!("origin/{onto}")]);
    let pushed = commit(&elsewhere, "from elsewhere");
    run(&elsewhere, &["push", "-q", "origin", branch]);
    pushed
}

/// Whether `branch` has been at one of `commits`, of a pull request from a branch of the same name.
async fn been_at(worktree: &Path, branch: &str, commits: &[CommitId]) -> bool {
    let name = BranchName::from(branch);
    has_been_at(worktree, &name, commits, Some(&name)).await.unwrap()
}

async fn linked_to(repo: &Path, branch: &str) -> Option<String> {
    linked(repo, &BranchName::from(branch)).await.unwrap()
}

#[test]
fn the_one_found_by_the_name_is_the_branchs_when_its_linked_or_the_branch_has_been_at_it() {
    let other = "https://example.com/pull/9";

    assert_eq!(choose(Some(URL), Some(URL), false), Choice::Found, "already linked, so not linked again");
    assert_eq!(choose(Some(URL), None, true), Choice::FoundAndLink);
    assert_eq!(choose(Some(other), Some(URL), true), Choice::FoundAndLink, "a newer one of the branch's own");
    assert_eq!(choose(Some(other), Some(URL), false), Choice::ReadLinked, "another's, of the same name");
    assert_eq!(choose(None, Some(URL), false), Choice::ReadLinked, "as for a branch renamed since it was pushed");
    assert_eq!(choose(Some(other), None, false), Choice::None);
    assert_eq!(choose(None, None, false), Choice::None);
}

#[tokio::test]
async fn a_branch_hasnt_been_at_the_commits_of_another_machines_branch_of_the_same_name() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path());
    let theirs = [pushed_from_elsewhere(dir.path(), "main", "aspen")];
    commit(&worktree, "this worktree's own, never pushed");

    let unfetched = been_at(&worktree, "aspen", &theirs).await;
    run(&worktree, &["fetch", "-q", "origin"]);
    let fetched = been_at(&worktree, "aspen", &theirs).await;
    // As once their pull request has merged with a merge commit, and this branch has taken `main` in, or as a pull
    // that merges them in leaves it.
    run(&worktree, &["merge", "-q", "--no-ff", "-m", "their work, merged in", "origin/aspen"]);
    let merged_in = been_at(&worktree, "aspen", &theirs).await;

    assert!(!unfetched, "the commit isn't here");
    assert!(!fetched, "it's here, and the branch was never at it");
    assert!(!merged_in, "it's in what the branch holds, and the branch was never at it");
}

/// As when a workspace here and one on another computer, of the same name, are both made from a teammate's branch
/// that isn't merged, and the other's pull request goes into `main`, so its commits start with the teammate's.
#[tokio::test]
async fn a_branch_hasnt_been_at_a_pull_request_it_only_started_from_the_same_commit_as() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path());
    let started_from = pushed_from_elsewhere(dir.path(), "main", "feature-x");
    let theirs = pushed_from_elsewhere(dir.path(), "feature-x", "redbud");
    run(&worktree, &["fetch", "-q", "origin"]);
    let pull_request = [started_from, theirs];

    run(&worktree, &["switch", "-q", "--no-track", "-c", "redbud", "origin/feature-x"]);
    let made = been_at(&worktree, "redbud", &pull_request).await;
    run(&worktree, &["branch", "-q", "-m", "redbud", "redbud-renamed"]);
    let renamed = has_been_at(&worktree, &"redbud-renamed".into(), &pull_request, Some(&"redbud".into())).await;
    commit(&worktree, "this branch's own");
    let committed = has_been_at(&worktree, &"redbud-renamed".into(), &pull_request, Some(&"redbud".into())).await;

    assert!(!made, "it was made at a commit of theirs, from another branch");
    assert!(!renamed.unwrap(), "renaming it moves it to no other commit");
    assert!(!committed.unwrap());
}

#[test]
fn what_a_branch_was_made_from_is_a_remotes_branch_only_when_its_one_of_the_remotes() {
    let remotes = ["origin".to_string(), "fork".to_string(), "team".to_string(), "team/alice".to_string()];

    assert_eq!(made_from("branch: Created from origin/feature-x"), Some("origin/feature-x"));
    assert_eq!(made_from("fetch origin refs/pull/12/head:birch: storing head"), None);
    assert_eq!(remote_branch("origin/feature-x", &remotes), Some("feature-x"));
    assert_eq!(remote_branch("refs/remotes/fork/aspen/fix", &remotes), Some("aspen/fix"));
    assert_eq!(remote_branch("main", &remotes), None);
    assert_eq!(remote_branch("HEAD", &remotes), None);
    assert_eq!(remote_branch("feature/x", &remotes), None, "a local branch with a folder");
    assert_eq!(remote_branch("team/alice/feature-x", &remotes), Some("feature-x"), "a remote with a slash in its name");
}

#[tokio::test]
async fn a_branch_has_been_at_what_it_pushed_though_it_or_the_pull_request_has_moved_on() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path());
    let pushed = commit(&worktree, "pushed");
    run(&worktree, &["push", "-q", "origin", "aspen"]);
    // Someone else pushes to it, as GitHub's Update branch or a suggestion committed there does.
    let theirs = pushed_from_elsewhere(dir.path(), "aspen", "aspen");
    let pull_request = [pushed.clone(), theirs];

    let at_it = been_at(&worktree, "aspen", &pull_request).await;
    commit(&worktree, "not pushed yet");
    let ahead = been_at(&worktree, "aspen", &pull_request).await;
    run(&worktree, &["reset", "-q", "--hard", "main"]);
    commit(&worktree, "rewritten, as a rebase leaves it");
    let rewritten = been_at(&worktree, "aspen", &pull_request).await;

    assert!(at_it && ahead && rewritten, "{at_it} {ahead} {rewritten}");
}

#[tokio::test]
async fn a_branch_made_from_another_machines_has_been_at_its_commit() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path());
    let theirs = [pushed_from_elsewhere(dir.path(), "main", "birch")];
    run(&worktree, &["fetch", "-q", "origin"]);

    run(&worktree, &["switch", "-q", "birch"]);

    assert!(been_at(&worktree, "birch", &theirs).await);
}

/// As `gh pr checkout 12 -b mine` makes it: the pull request's branch has another name here, which `gh` doesn't find
/// the pull request by, so this is how a newer one of its own, which `gh` does find, is judged.
#[tokio::test]
async fn a_branch_made_from_the_pull_requests_own_under_another_name_has_been_at_its_commit() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path());
    let theirs = [pushed_from_elsewhere(dir.path(), "main", "birch")];
    run(&worktree, &["fetch", "-q", "origin"]);

    run(&worktree, &["switch", "-q", "-c", "mine", "--track", "origin/birch"]);

    assert!(has_been_at(&worktree, &"mine".into(), &theirs, Some(&"birch".into())).await.unwrap());
}

#[tokio::test]
async fn a_repository_that_keeps_no_history_of_its_branches_says_only_where_one_is_now() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path());
    run(&worktree, &["config", "core.logAllRefUpdates", "false"]);
    run(&worktree, &["switch", "-q", "-c", "unlogged"]);
    let before = commit(&worktree, "before");
    let now = commit(&worktree, "now");

    assert!(been_at(&worktree, "unlogged", std::slice::from_ref(&now)).await);
    assert!(!been_at(&worktree, "unlogged", &[before]).await);
}

#[tokio::test]
async fn a_branch_with_no_commits_yet_has_been_at_none() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path());
    let main = CommitId::from(run(&worktree, &["rev-parse", "main"]));

    run(&worktree, &["switch", "-q", "--orphan", "nothing-yet"]);

    assert!(!been_at(&worktree, "nothing-yet", &[main]).await);
}

#[tokio::test]
async fn git_failing_isnt_taken_as_a_branch_not_having_been_there() {
    let dir = tempfile::tempdir().unwrap();
    let (nowhere, branch) = (dir.path().join("not-a-repository"), BranchName::from("aspen"));
    std::fs::create_dir(&nowhere).unwrap();

    let commits = [CommitId::from("0123456789abcdef0123456789abcdef01234567")];
    assert!(has_been_at(&nowhere, &branch, &commits, Some(&branch)).await.is_err());
    assert!(linked(&nowhere, &branch).await.is_err());
}

/// Git keeps a branch's settings with the branch, which is why the link is one.
#[tokio::test]
async fn a_link_goes_where_its_branch_goes_and_is_gone_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path());
    let repo = dir.path().join("repo");
    let aspen = BranchName::from("aspen");
    let unlinked = linked_to(&repo, "aspen").await;

    link(&worktree, &aspen, "https://example.com/pull/6").await;
    link(&worktree, &aspen, URL).await;
    let from_the_repository = linked_to(&repo, "aspen").await;
    run(&worktree, &["branch", "-q", "-m", "aspen/fix-the-header"]);
    let under_the_old_name = linked_to(&repo, "aspen").await;
    let under_the_new = linked_to(&repo, "aspen/fix-the-header").await;
    run(&repo, &["worktree", "remove", worktree.to_str().unwrap()]);
    let without_its_worktree = linked_to(&repo, "aspen/fix-the-header").await;
    run(&repo, &["branch", "-q", "-D", "aspen/fix-the-header"]);
    run(&repo, &["branch", "-q", "aspen/fix-the-header"]);
    let made_again = linked_to(&repo, "aspen/fix-the-header").await;

    assert_eq!(unlinked, None);
    assert_eq!(from_the_repository.as_deref(), Some(URL), "the newest link, for every worktree");
    assert_eq!((under_the_old_name, under_the_new.as_deref()), (None, Some(URL)));
    assert_eq!(without_its_worktree.as_deref(), Some(URL), "as when its workspace is archived");
    assert_eq!(made_again, None);
}

#[tokio::test]
async fn a_setting_that_isnt_a_url_is_no_link() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = worktree(dir.path());
    run(&worktree, &["config", "branch.aspen.roundhouse-pull-request", "--web"]);

    assert_eq!(linked_to(&worktree, "aspen").await, None);
}
