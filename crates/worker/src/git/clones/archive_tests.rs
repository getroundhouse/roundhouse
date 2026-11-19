use std::path::Path;

use protocol::client::ErrorCode;
use protocol::model::BranchName;
use protocol::worker::RepoSource;

use super::Clones;
use super::archive::{capture_archived, capture_archived_with_limit, restore_archived};
use crate::git::tests::{new_repo, run};
use crate::git::{prepare_workspace, restore_workspace};
use crate::testing::given;

fn make_remote(dir: &Path) -> std::path::PathBuf {
    let seed = new_repo(dir);
    let bare = dir.join("remote.git");
    run(dir, &["clone", "-q", "--bare", seed.to_str().unwrap(), bare.to_str().unwrap()]);
    bare
}

#[tokio::test]
async fn unpushed_commits_and_branches_survive_capture_and_restore_into_a_fresh_clone() {
    let dir = tempfile::tempdir().unwrap();
    let remote = make_remote(dir.path());
    let source = RepoSource::Clone { url: remote.to_string_lossy().into_owned() };
    let env = given();

    let clones_a = Clones::new(dir.path().join("clones-a"));
    let repo_a = clones_a.made_for(&env, &source).await.unwrap();
    let root_a = dir.path().join("worktrees-a");
    let branch = BranchName::from("euston/start");
    let ready = prepare_workspace(&env, &root_a, "demo", &repo_a, "euston", &branch, None).await.unwrap();
    let worktree_a = Path::new(&ready.path);
    std::fs::write(worktree_a.join("notes.txt"), "unpushed work\n").unwrap();
    run(worktree_a, &["add", "notes.txt"]);
    run(worktree_a, &["commit", "-q", "-m", "unpushed commit"]);
    let expected_head = run(worktree_a, &["rev-parse", "HEAD"]);
    run(worktree_a, &["branch", "side-idea"]);
    // A tag with the same name as a branch must not turn the branch name into `heads/side-idea`.
    run(worktree_a, &["tag", "side-idea"]);

    let archived = capture_archived(&env, &repo_a, false).await.unwrap();
    assert!(archived.bundle.is_some(), "unpushed commits must be bundled");
    assert!(!archived.bundle_omitted);
    assert!(archived.branches.iter().any(|b| b.name == branch && b.commit.as_str() == expected_head));
    assert!(archived.branches.iter().any(|b| b.name.as_str() == "side-idea" && b.commit.as_str() == expected_head));

    let clones_b = Clones::new(dir.path().join("clones-b"));
    let repo_b = clones_b.made_for(&env, &source).await.unwrap();
    restore_archived(&env, &repo_b, &archived, &branch).await.unwrap();
    let worktree_b = dir.path().join("worktrees-b").join("demo").join("euston");
    restore_workspace(&env, &repo_b, &worktree_b.to_string_lossy(), &branch).await.unwrap();

    assert_eq!(run(&worktree_b, &["rev-parse", "HEAD"]), expected_head);
    assert_eq!(std::fs::read_to_string(worktree_b.join("notes.txt")).unwrap(), "unpushed work\n");
    assert_eq!(run(&repo_b, &["rev-parse", "refs/heads/side-idea"]), expected_head);
}

#[tokio::test]
async fn deleting_a_pushed_remote_branch_before_restore_still_restores_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let remote = make_remote(dir.path());
    let source = RepoSource::Clone { url: remote.to_string_lossy().into_owned() };
    let env = given();

    let clones_a = Clones::new(dir.path().join("clones-a"));
    let repo_a = clones_a.made_for(&env, &source).await.unwrap();
    let root_a = dir.path().join("worktrees-a");
    let branch = BranchName::from("kings/start");
    let ready = prepare_workspace(&env, &root_a, "demo", &repo_a, "kings", &branch, None).await.unwrap();
    let wt = Path::new(&ready.path);
    std::fs::write(wt.join("feature.txt"), "pushed then deleted on remote\n").unwrap();
    run(wt, &["add", "feature.txt"]);
    run(wt, &["commit", "-q", "-m", "pushed feature"]);
    run(wt, &["push", "-q", "origin", branch.as_str()]);
    let expected_head = run(wt, &["rev-parse", "HEAD"]);

    // Simulate GitHub deleting the remote PR branch after squash-merge without this clone fetching again.
    run(&remote, &["branch", "-D", branch.as_str()]);

    let archived = capture_archived(&env, &repo_a, false).await.unwrap();
    assert!(archived.bundle.is_some(), "commits not on origin/HEAD must still be bundled");

    let clones_b = Clones::new(dir.path().join("clones-b"));
    let repo_b = clones_b.made_for(&env, &source).await.unwrap();
    restore_archived(&env, &repo_b, &archived, &branch).await.unwrap();
    let worktree_b = dir.path().join("worktrees-b").join("demo").join("kings");
    restore_workspace(&env, &repo_b, &worktree_b.to_string_lossy(), &branch).await.unwrap();
    assert_eq!(run(&worktree_b, &["rev-parse", "HEAD"]), expected_head);
}

#[tokio::test]
async fn restore_never_overwrites_an_existing_branch_that_moved_on() {
    let dir = tempfile::tempdir().unwrap();
    let remote = make_remote(dir.path());
    let source = RepoSource::Clone { url: remote.to_string_lossy().into_owned() };
    let env = given();

    let clones = Clones::new(dir.path().join("clones"));
    let repo = clones.made_for(&env, &source).await.unwrap();
    let root = dir.path().join("worktrees");
    let branch = BranchName::from("victoria/start");
    let ready = prepare_workspace(&env, &root, "demo", &repo, "victoria", &branch, None).await.unwrap();
    let wt = Path::new(&ready.path);
    run(wt, &["commit", "-q", "--allow-empty", "-m", "first"]);
    let archived = capture_archived(&env, &repo, false).await.unwrap();

    // Advance the branch in the surviving clone after capture, then restore.
    run(wt, &["commit", "-q", "--allow-empty", "-m", "newer"]);
    let newer_head = run(wt, &["rev-parse", "HEAD"]);
    restore_archived(&env, &repo, &archived, &branch).await.unwrap();
    assert_eq!(run(&repo, &["rev-parse", "refs/heads/victoria/start"]), newer_head);
}

#[tokio::test]
async fn capture_with_no_unpushed_commits_omits_bundle_and_still_restores_the_branch() {
    let dir = tempfile::tempdir().unwrap();
    let remote = make_remote(dir.path());
    let source = RepoSource::Clone { url: remote.to_string_lossy().into_owned() };
    let env = given();

    let clones_a = Clones::new(dir.path().join("clones-a"));
    let repo_a = clones_a.made_for(&env, &source).await.unwrap();
    let root_a = dir.path().join("worktrees-a");
    let branch = BranchName::from("paddington/start");
    let ready = prepare_workspace(&env, &root_a, "demo", &repo_a, "paddington", &branch, None).await.unwrap();
    let expected_head = run(Path::new(&ready.path), &["rev-parse", "HEAD"]);

    let archived = capture_archived(&env, &repo_a, false).await.unwrap();
    assert_eq!(archived.bundle, None, "no unpushed commits means no bundle needed");
    assert!(!archived.bundle_omitted);
    assert_eq!(archived.branches.len(), 1);

    let clones_b = Clones::new(dir.path().join("clones-b"));
    let repo_b = clones_b.made_for(&env, &source).await.unwrap();
    restore_archived(&env, &repo_b, &archived, &branch).await.unwrap();
    let worktree_b = dir.path().join("worktrees-b").join("demo").join("paddington");
    restore_workspace(&env, &repo_b, &worktree_b.to_string_lossy(), &branch).await.unwrap();
    assert_eq!(run(&worktree_b, &["rev-parse", "HEAD"]), expected_head);
}

#[tokio::test]
async fn oversized_bundle_refuses_with_needs_force_and_forced_restore_falls_back_to_origin_head() {
    let dir = tempfile::tempdir().unwrap();
    let remote = make_remote(dir.path());
    let source = RepoSource::Clone { url: remote.to_string_lossy().into_owned() };
    let env = given();

    let clones_a = Clones::new(dir.path().join("clones-a"));
    let repo_a = clones_a.made_for(&env, &source).await.unwrap();
    let root_a = dir.path().join("worktrees-a");
    let branch = BranchName::from("waterloo/start");
    let ready = prepare_workspace(&env, &root_a, "demo", &repo_a, "waterloo", &branch, None).await.unwrap();
    let wt = Path::new(&ready.path);
    std::fs::write(wt.join("blob.bin"), [0x5au8; 4096]).unwrap();
    run(wt, &["add", "blob.bin"]);
    run(wt, &["commit", "-q", "-m", "commit above tiny test cap"]);

    // 64 bytes is smaller than any git bundle header + packfile, so the size check trips reliably.
    let refused = capture_archived_with_limit(&env, &repo_a, false, 64).await.unwrap_err();
    assert_eq!(refused.code, ErrorCode::NeedsForce);

    let forced = capture_archived_with_limit(&env, &repo_a, true, 64).await.unwrap();
    assert_eq!(forced.bundle, None, "forced archive over the cap drops the bundle");
    assert!(forced.bundle_omitted);

    // Restoring into a fresh clone falls back to origin/HEAD rather than leaving the workspace stuck.
    let clones_b = Clones::new(dir.path().join("clones-b"));
    let repo_b = clones_b.made_for(&env, &source).await.unwrap();
    let origin_head = run(&repo_b, &["rev-parse", "refs/remotes/origin/HEAD"]);
    restore_archived(&env, &repo_b, &forced, &branch).await.unwrap();
    let worktree_b = dir.path().join("worktrees-b").join("demo").join("waterloo");
    restore_workspace(&env, &repo_b, &worktree_b.to_string_lossy(), &branch).await.unwrap();
    assert_eq!(run(&worktree_b, &["rev-parse", "HEAD"]), origin_head);
}
