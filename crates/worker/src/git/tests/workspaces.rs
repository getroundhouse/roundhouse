//! Making a workspace's own worktree and putting it back, and what git says of a workspace. What making any worktree
//! decides is tested in `making`, so these have one example of it, and what making a workspace adds: its path, its
//! branch and its base.

use super::super::{command, diff, git_ok, names_git_has, prepare_workspace, restore_workspace, status};
use super::support::{cloned, new_repo, prepare, run};
use crate::git::worktrees::tests::while_one_is_made;
use crate::testing::given;
use protocol::client::ErrorCode;
use protocol::model::CommitId;
use std::path::Path;

/// From the branch checked out in a repository with no remote, as the branches it reads back say. With no base asked
/// for, from `origin`'s default branch, else `origin/main`, else `origin/master`, else the branch checked out, else the
/// commit that is.
#[tokio::test]
async fn creates_a_worktree_on_a_new_branch_from_the_current_branch_unless_origin_has_one() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let root = dir.path().join("ws");
    let prepared = prepare(&root, &repo, "alder", None).await.unwrap();
    let base = async |name| prepare(&root, &repo, name, None).await.unwrap().base_ref;
    let head = run(&repo, &["rev-parse", "HEAD"]);

    run(&repo, &["checkout", "-q", "--detach"]);
    let detached = base("birch").await;
    run(&repo, &["checkout", "-q", "main"]);
    run(&repo, &["update-ref", "refs/remotes/origin/master", &head]);
    let with_master = base("cedar").await;
    run(&repo, &["update-ref", "refs/remotes/origin/main", &head]);
    let with_main = base("dogwood").await;
    run(&repo, &["update-ref", "refs/remotes/origin/trunk", &head]);
    run(&repo, &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/trunk"]);
    let with_a_default = base("elm").await;

    assert_eq!(prepared.base_ref, "main");
    assert_eq!(run(Path::new(&prepared.path), &["branch", "--show-current"]), "alder/start");
    assert!(Path::new(&prepared.path).join("a.txt").exists());
    assert_eq!(prepared.branches.as_ref().and_then(|b| b.checked_out()).map(|b| b.name.as_str()), Some("alder/start"));
    let bases = [detached, with_master, with_main, with_a_default];
    assert_eq!(bases, ["HEAD", "origin/master", "origin/main", "origin/trunk"]);
}

/// From `origin`'s default branch, where there's a remote, as fetched.
#[tokio::test]
async fn a_clone_s_workspace_starts_from_the_latest_of_origin_s_default_branch() {
    let dir = tempfile::tempdir().unwrap();
    let (origin, clone) = cloned(dir.path());
    // origin moves on after the clone, so only a fetch can find this commit.
    run(&origin, &["commit", "-q", "--allow-empty", "-m", "second"]);
    let latest = run(&origin, &["rev-parse", "HEAD"]);

    let prepared = prepare(&dir.path().join("ws"), &clone, "alder", None).await.unwrap();

    assert_eq!(prepared.base_ref, "origin/main");
    assert!(prepared.warnings.is_empty(), "{:?}", prepared.warnings);
    assert_eq!(run(Path::new(&prepared.path), &["rev-parse", "HEAD"]), latest);
}

/// Before anything is made. What git can't make is refused in `making`.
#[tokio::test]
async fn a_workspace_s_path_that_s_taken_or_a_branch_git_can_t_name_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let root = dir.path().join("ws");
    prepare(&root, &repo, "alder", None).await.unwrap();

    let again = prepare(&root, &repo, "alder", None).await;
    let bad = prepare_workspace(&given(), &root, "repo", &repo, "x", &"--bad".into(), None).await;

    assert_eq!(again.err().map(|e| e.code), Some(ErrorCode::NameTaken), "the same path and branch");
    assert_eq!(bad.err().map(|e| e.code), Some(ErrorCode::InvalidRequest), "a branch git can't name");
}

/// A workspace's folder of branches is its own, so the hub skips a name git has a branch by or in, here or among
/// `origin`'s as last fetched, a remote by, or a directory by. As they're spelled, for the hub to compare whatever
/// their case.
#[tokio::test]
async fn the_names_git_has_are_each_branch_s_first_part_here_and_among_origin_s_each_remote_and_each_directory() {
    let dir = tempfile::tempdir().unwrap();
    let origin = new_repo(dir.path());
    let clone = dir.path().join("clone");
    run(dir.path(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    run(&clone, &["branch", "Alder"]);
    run(&clone, &["branch", "birch/fix"]);
    run(&origin, &["branch", "cedar/fix"]);
    run(&clone, &["fetch", "-q"]);
    let root = dir.path().join("ws");
    std::fs::create_dir_all(root.join("repo").join("Dogwood")).unwrap();

    let names = names_git_has(&given(), &root, "repo", &clone).await.unwrap();

    assert_eq!(names, ["Alder", "Dogwood", "HEAD", "birch", "cedar", "main", "origin"], "each once, as it's spelled");
}

/// A remote has a `HEAD` no branch can sit beside, where case isn't told apart, even before a fetch has recorded it,
/// as for a remote added by hand. A repository with no remote has none.
#[tokio::test]
async fn head_is_a_name_git_has_once_there_s_a_remote() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let given = given();
    let names = || names_git_has(&given, dir.path(), "none", &repo);

    let with_no_remote = names().await.unwrap();
    run(&repo, &["remote", "add", "upstream", "https://example.invalid/repo.git"]);
    let with_one_never_fetched = names().await.unwrap();

    assert_eq!(with_no_remote, ["main"]);
    assert_eq!(with_one_never_fetched, ["HEAD", "main", "upstream"]);
}

#[tokio::test]
async fn status_reports_each_kind_of_change_including_renames_and_odd_names() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    let path = Path::new(&prepared.path);
    std::fs::write(path.join("a.txt"), "changed\n").unwrap();
    std::fs::write(path.join("with space.txt"), "x\n").unwrap();
    run(path, &["mv", ".gitignore", "renamed"]);

    let status = status(&given(), &prepared.path).await.unwrap();
    assert_eq!(status.branch, Some("alder/start".into()));
    assert_eq!(status.head, Some(CommitId::from(run(path, &["rev-parse", "HEAD"]).trim())));
    let find = |p: &str| status.files.iter().find(|f| f.path == p).unwrap_or_else(|| panic!("{p}: {:?}", status.files));
    assert_eq!(find("a.txt").status, ".M");
    assert_eq!(find("with space.txt").status, "??");
    assert_eq!(find("renamed").orig_path.as_deref(), Some(".gitignore"));
    assert!(find("renamed").status.starts_with('R'));
}

#[tokio::test]
async fn diff_covers_commits_uncommitted_and_untracked_work() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    let path = Path::new(&prepared.path);
    let merge_base = CommitId::from(run(path, &["rev-parse", "HEAD"]));
    std::fs::write(path.join("committed.txt"), "from a commit\n").unwrap();
    run(path, &["add", "."]);
    run(path, &["commit", "-qm", "work"]);
    std::fs::write(path.join("a.txt"), "uncommitted\n").unwrap();
    std::fs::write(path.join("untracked.txt"), "brand new\n").unwrap();
    // The base branch moving on must not show up as changes in this workspace.
    std::fs::write(repo.join("moved.txt"), "elsewhere\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "main moved"]);

    let diff = diff(&given(), &prepared.path, Some("main")).await.unwrap();
    assert_eq!(diff.base_commit, merge_base);
    assert!(diff.patch.contains("+from a commit"), "{}", diff.patch);
    assert!(diff.patch.contains("+uncommitted"), "{}", diff.patch);
    assert!(diff.patch.contains("+brand new"), "{}", diff.patch);
    assert!(!diff.patch.contains("elsewhere"), "{}", diff.patch);
    assert!(!diff.truncated);
}

/// HEAD's commit is what such a diff is taken from, so it's named, and a client can read what the diff replaced.
#[tokio::test]
async fn a_diff_with_no_base_names_head_s_commit() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let worktree = repo.to_str().unwrap();
    std::fs::write(repo.join("a.txt"), "uncommitted\n").unwrap();
    let head = CommitId::from(run(&repo, &["rev-parse", "HEAD"]));
    let empty = dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    run(&empty, &["init", "-q"]);

    let no_base = diff(&given(), worktree, None).await.unwrap();
    let gone = diff(&given(), worktree, Some("gone")).await.unwrap();

    assert_eq!(no_base.base_commit, head);
    assert!(no_base.patch.contains("+uncommitted"), "{}", no_base.patch);
    assert_eq!(gone.base_commit, head, "a base ref git finds no merge base with");
    assert!(diff(&given(), empty.to_str().unwrap(), None).await.is_err(), "no commit at HEAD, so nothing to diff from");
}

/// Commits on a detached HEAD that no branch holds would go with the worktree, which the status says, and a detached
/// HEAD on a commit a branch holds has none.
#[tokio::test]
async fn a_detached_head_has_commits_to_lose_only_where_no_branch_holds_them() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    run(Path::new(&prepared.path), &["checkout", "-q", "--detach"]);

    let held = status(&given(), &prepared.path).await.unwrap();
    run(Path::new(&prepared.path), &["commit", "-q", "--allow-empty", "-m", "on no branch"]);
    let unheld = status(&given(), &prepared.path).await.unwrap();

    assert_eq!((held.branch, held.unreferenced_commits), (None, false), "on a commit a branch holds");
    assert_eq!((unheld.branch, unheld.unreferenced_commits), (None, true), "on a commit of its own");
}

#[tokio::test]
async fn restoring_a_workspace_waits_for_a_worktree_being_made() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let root = dir.path().join("ws");
    let path = prepare_workspace(&given(), &root, "repo", &repo, "alder", &"alder".into(), None).await.unwrap().path;
    run(&repo, &["worktree", "remove", &path]);

    while_one_is_made(&repo, restore_workspace(&given(), &repo, &path, &"alder".into())).await.unwrap();

    assert!(Path::new(&path).join("a.txt").exists());
}

/// What a test gives the worker's git takes the place of the test's own environment, so the git the worker runs has
/// only what it's told to set, and what the test gives it, which keeps where to find programs and temporary files: not
/// what Cargo sets for a test, which stands here for a `GIT_DIR` from a hook or the person's `GIT_CONFIG_GLOBAL`,
/// since a test can't set either in its own environment. And that git reads no config but the repository's. Git runs
/// `env` itself, with no shell, which would add its own.
#[cfg(unix)]
#[tokio::test]
async fn the_worker_s_git_in_a_test_has_none_of_the_test_s_environment() {
    use std::ffi::OsStr;
    assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some(), "Cargo sets it for a test");
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let git = command(&given(), &repo, &[]);
    let told: Vec<&OsStr> =
        git.as_std().get_envs().filter(|(_, value)| value.is_some()).map(|(name, _)| name).collect();

    let printed = git_ok(&given(), &repo, &["-c", "alias.env=!env", "env"]).await.unwrap();
    let scopes = git_ok(&given(), &repo, &["config", "--list", "--show-scope"]).await.unwrap();

    let names = printed.lines().filter_map(|line| line.split_once('=')).map(|(name, _)| OsStr::new(name));
    let inherited = |name: &&OsStr| std::env::var_os(name).is_some() && !told.contains(name);
    // What macOS gives every process.
    let inherited: Vec<&OsStr> = names.filter(inherited).filter(|name| *name != "__CF_USER_TEXT_ENCODING").collect();
    assert!(inherited.is_empty(), "{inherited:?}");
    assert!(scopes.lines().all(|line| line.starts_with("local\t")), "{scopes}");
}
