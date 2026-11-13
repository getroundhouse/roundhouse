use std::path::{Path, PathBuf};

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{BranchName, CommitId};
use protocol::worker::ChildBranch;

use super::add_child_worktree;
use crate::git::tests::{new_repo, prepare, run};

/// A repository, and a workspace `alder` with a commit of its own.
struct Made {
    _dir: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
    alder: PathBuf,
}

async fn workspace() -> Made {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let root = dir.path().join("ws");
    let alder = PathBuf::from(prepare(&root, &repo, "alder", None).await.unwrap().path);
    std::fs::write(alder.join("b.txt"), "the workspace's\n").unwrap();
    run(&alder, &["add", "."]);
    run(&alder, &["commit", "-qm", "the workspace's work"]);
    Made { _dir: dir, root, repo, alder }
}

fn children(root: &Path) -> PathBuf {
    root.join("repo").join(".children")
}

/// What adding a worktree gave.
struct Added {
    path: PathBuf,
    commit: CommitId,
    warnings: Vec<String>,
}

/// Adds the worktree at `directory` in `.children/`, as the workspace `alder`'s.
async fn add(made: &Made, directory: &str, on: ChildBranch) -> Result<Added, ApiError> {
    let path = children(&made.root).join(directory);
    let mut warnings = Vec::new();
    let commit = add_child_worktree(&made.alder, &path, &on, &mut warnings).await?;
    Ok(Added { path, commit, warnings })
}

fn new(branch: &str, base: &str) -> ChildBranch {
    ChildBranch::New { branch: BranchName::from(branch), base: base.to_string() }
}

fn existing(branch: &str) -> ChildBranch {
    ChildBranch::Existing { branch: BranchName::from(branch) }
}

fn code<T>(result: Result<T, ApiError>) -> Option<ErrorCode> {
    result.err().map(|e| e.code)
}

fn tracks_nothing(worktree: &Path) -> bool {
    let upstream = std::process::Command::new("git").args(["rev-parse", "@{upstream}"]).current_dir(worktree).output();
    !upstream.unwrap().status.success()
}

#[tokio::test]
async fn a_new_branch_starts_from_what_the_workspace_has_checked_out() {
    let made = workspace().await;

    let added = add(&made, "alder/try", new("alder/try", "HEAD")).await.unwrap();

    assert_eq!(added.commit.as_str(), run(&made.alder, &["rev-parse", "HEAD"]));
    assert_eq!(run(&added.path, &["branch", "--show-current"]), "alder/try");
    assert!(added.path.join("b.txt").exists(), "the workspace's commit is there");
}

#[tokio::test]
async fn a_branch_that_exists_is_checked_out_as_it_is() {
    let made = workspace().await;
    run(&made.repo, &["branch", "alder/old", "main"]);

    let added = add(&made, "alder/old", existing("alder/old")).await.unwrap();

    assert_eq!(added.commit.as_str(), run(&made.repo, &["rev-parse", "main"]));
    assert_eq!(run(&added.path, &["branch", "--show-current"]), "alder/old");
}

#[tokio::test]
async fn what_git_cant_make_there_is_refused_and_nothing_is_left_behind() {
    let made = workspace().await;
    run(&made.repo, &["branch", "Alder/Taken"]);
    let beside = made.alder.with_file_name("beech");
    let outside = add_child_worktree(&made.alder, &beside, &new("alder/fresh", "HEAD"), &mut Vec::new()).await;

    let refused = [
        (code(outside), ErrorCode::InvalidRequest),
        (code(add(&made, "../out", new("alder/fresh", "HEAD")).await), ErrorCode::InvalidRequest),
        (code(add(&made, "alder/.deleting", new("alder/fresh", "HEAD")).await), ErrorCode::InvalidRequest),
        (code(add(&made, "alder/x/y", new("alder/fresh", "HEAD")).await), ErrorCode::InvalidRequest),
        (code(add(&made, "alder", new("alder/fresh", "HEAD")).await), ErrorCode::InvalidRequest),
        // Another workspace's folder.
        (code(add(&made, "beech/fresh", new("alder/fresh", "HEAD")).await), ErrorCode::InvalidRequest),
        (code(add(&made, "alder/fresh", new("alder/fresh", "no-such-base")).await), ErrorCode::InvalidRequest),
        (code(add(&made, "alder/taken", new("alder/taken", "HEAD")).await), ErrorCode::NameTaken),
        (code(add(&made, "alder/missing", existing("alder/missing")).await), ErrorCode::NotFound),
        // The workspace's own worktree has it checked out, and git lets a branch be in one worktree at a time.
        (code(add(&made, "alder/start", existing("alder/start")).await), ErrorCode::Conflict),
    ];

    for (at, (got, wanted)) in refused.iter().enumerate() {
        assert_eq!(*got, Some(*wanted), "refusal {at}");
    }
    let left = std::fs::read_dir(children(&made.root).join("alder")).map_or(0, Iterator::count);
    assert_eq!(left, 0, "no worktree was made");
    assert!(!beside.exists(), "nor one beside the workspace's own");
    assert!(run(&made.repo, &["branch", "--list", "alder/fresh"]).is_empty(), "nor a branch");
    let added = add(&made, "alder/fresh", new("alder/fresh", "HEAD")).await.unwrap();
    let again = add(&made, "alder/fresh", new("alder/other", "HEAD")).await;
    assert!(added.path.exists());
    assert_eq!(code(again), Some(ErrorCode::NameTaken), "its directory is taken");
}

/// A new hub asks again for what the last didn't hear back about, and is answered with what's there, on a new branch
/// or as one that exists by then.
#[tokio::test]
async fn one_asked_for_again_is_what_was_made_before() {
    let made = workspace().await;
    let first = add(&made, "alder/x", new("alder/x", "HEAD")).await.unwrap();
    std::fs::write(first.path.join("kept.txt"), "work since\n").unwrap();

    let again = add(&made, "alder/x", new("alder/x", "HEAD")).await.unwrap();
    let later = add(&made, "alder/x", existing("alder/x")).await.unwrap();

    assert_eq!((&again.commit, &later.commit), (&first.commit, &first.commit));
    assert!(again.warnings.is_empty() && later.warnings.is_empty(), "each found it made, rather than failing");
    assert!(first.path.join("kept.txt").exists(), "and left as it was");
}

/// A `git worktree add` stopped part way, as when the worker or the machine goes, leaves the branch checked out, only
/// some of the files, and git's own lock, which nothing takes off. Asked for again, it's made again rather than answered
/// with.
#[tokio::test]
async fn one_git_was_stopped_part_way_through_making_is_made_again() {
    let made = workspace().await;
    let first = add(&made, "alder/x", new("alder/x", "HEAD")).await.unwrap();
    std::fs::remove_file(first.path.join("b.txt")).unwrap();
    run(&made.repo, &["worktree", "lock", "--reason", "initializing", first.path.to_str().unwrap()]);

    let again = add(&made, "alder/x", new("alder/x", "HEAD")).await.unwrap();

    assert!(again.path.join("b.txt").exists(), "its files are there");
    assert_eq!(run(&again.path, &["branch", "--show-current"]), "alder/x");
    let listed = run(&made.repo, &["worktree", "list", "--porcelain"]);
    assert!(!listed.contains("locked"), "git's lock went with what it left: {listed}");
}

/// One stopped part way on another branch than the one asked for is removed, and the worktree made as the ask says,
/// so a new branch that's taken is refused rather than checked out there.
#[tokio::test]
async fn one_stopped_part_way_on_another_branch_is_made_only_as_asked() {
    let made = workspace().await;
    let first = add(&made, "alder/x", new("alder/other", "HEAD")).await.unwrap();
    run(&made.repo, &["worktree", "lock", "--reason", "initializing", first.path.to_str().unwrap()]);
    run(&made.repo, &["branch", "alder/x"]);

    let again = add(&made, "alder/x", new("alder/x", "HEAD")).await;

    assert_eq!(code(again), Some(ErrorCode::NameTaken));
    assert!(!first.path.exists(), "the one git was stopped part way through is removed");
}

/// Another repository's worktree where this one was to go, locked as git leaves one it was stopped part way through,
/// isn't this repository's to remove, so it's left as it is.
#[tokio::test]
async fn another_repository_s_part_made_worktree_at_the_path_is_left_as_it_is() {
    let made = workspace().await;
    let elsewhere = new_repo(&made.root.join("elsewhere"));
    let path = children(&made.root).join("alder").join("x");
    run(&elsewhere, &["worktree", "add", "-q", "-b", "alder/x", path.to_str().unwrap()]);
    run(&elsewhere, &["worktree", "lock", "--reason", "initializing", path.to_str().unwrap()]);

    let refused = add(&made, "alder/x", new("alder/x", "HEAD")).await;

    assert!(refused.is_err());
    assert!(path.join("a.txt").exists(), "its files are as they were");
}

/// Git makes a new branch before it makes the worktree, and leaves it if that fails.
#[cfg(unix)]
#[tokio::test]
async fn a_branch_git_made_before_failing_is_deleted() {
    use std::os::unix::fs::PermissionsExt;

    let made = workspace().await;
    let folder = children(&made.root).join("alder");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::set_permissions(&folder, PermissionsExt::from_mode(0o555)).unwrap();
    if std::fs::write(folder.join("probe"), "").is_ok() {
        eprintln!("skipped: this user can write anywhere");
        return;
    }

    let failed = add(&made, "alder/locked", new("alder/locked", "HEAD")).await;

    std::fs::set_permissions(&folder, PermissionsExt::from_mode(0o755)).unwrap();
    assert_eq!(code(failed), Some(ErrorCode::Worker));
    assert!(run(&made.repo, &["branch", "--list", "alder/locked"]).is_empty(), "the branch was deleted");
}

/// Git makes a new branch before it looks at the path, and leaves the branch when it refuses the path, as it would one
/// made there meanwhile.
#[tokio::test]
async fn a_branch_made_for_a_path_git_then_refuses_is_deleted() {
    let made = workspace().await;
    let path = children(&made.root).join("alder").join("raced");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("made-meanwhile"), "").unwrap();

    let added =
        super::on_a_new_branch(&made.alder, &"alder/raced".into(), "HEAD", &path, &mut Vec::new()).await.unwrap();

    assert!(!added.success, "git made it");
    assert!(run(&made.repo, &["branch", "--list", "alder/raced"]).is_empty(), "the branch was kept: {}", added.stderr);
}

/// One an agent deleted without git, which git still has registered, is made again.
#[tokio::test]
async fn a_worktree_deleted_without_git_is_made_again() {
    let made = workspace().await;
    let gone = add(&made, "alder/gone", new("alder/gone", "HEAD")).await.unwrap();
    std::fs::remove_dir_all(&gone.path).unwrap();

    let again = add(&made, "alder/gone", existing("alder/gone")).await.unwrap();

    assert!(again.path.join("b.txt").exists(), "made again, on its branch");
}

/// A folder of `.children/` that's a worktree itself, as one made there by hand, would have a new one made inside it,
/// to go with it when it's removed.
#[tokio::test]
async fn one_that_would_be_inside_another_worktree_isnt_made() {
    let made = workspace().await;
    let folder = children(&made.root).join("alder");
    run(&made.alder, &["worktree", "add", "-q", "-b", "by-hand", folder.to_str().unwrap()]);

    let inside = add(&made, "alder/y", new("alder/y", "HEAD")).await;

    assert_eq!(code(inside), Some(ErrorCode::NameTaken));
    assert!(!folder.join("y").exists() && folder.exists());
}

/// Another repository's checkout where the worktree was to go, on the branch asked for, isn't the worktree asked for.
#[tokio::test]
async fn another_repository_s_checkout_at_the_path_isnt_what_was_asked_for() {
    let made = workspace().await;
    let other = children(&made.root).join("alder").join("other");
    run(&made.root, &["init", "-q", "-b", "alder/other", other.to_str().unwrap()]);

    let elsewhere = add(&made, "alder/other", new("alder/other", "HEAD")).await;

    assert_eq!(code(elsewhere), Some(ErrorCode::NameTaken));
}

/// One a failing `post-checkout` hook leaves made is made, with a warning.
#[cfg(unix)]
#[tokio::test]
async fn one_a_failing_hook_leaves_made_is_made_with_a_warning() {
    let made = workspace().await;
    let hooks = made.root.join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("post-checkout");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    run(&made.repo, &["config", "core.hooksPath", hooks.to_str().unwrap()]);

    let hooked = add(&made, "alder/hooked", new("alder/hooked", "HEAD")).await.unwrap();

    let warned = hooked.warnings.iter().any(|warning| warning.starts_with("git made the worktree"));
    assert!(warned, "{:?}", hooked.warnings);
    assert_eq!(run(&hooked.path, &["branch", "--show-current"]), "alder/hooked");
}

/// A base of `origin`'s is fetched first, as a workspace's is, so the worktree starts from its latest commit, on a
/// branch that tracks nothing, so a bare push or pull there acts on nothing else.
#[tokio::test]
async fn a_base_of_origin_s_is_fetched_first_and_isnt_tracked() {
    let dir = tempfile::tempdir().unwrap();
    let origin = new_repo(dir.path());
    let clone = dir.path().join("clone");
    run(dir.path(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    let root = dir.path().join("ws");
    let alder = PathBuf::from(prepare(&root, &clone, "alder", None).await.unwrap().path);
    run(&origin, &["commit", "-q", "--allow-empty", "-m", "since"]);
    let latest = run(&origin, &["rev-parse", "HEAD"]);
    let made = Made { _dir: dir, root, repo: clone, alder };

    let added = add(&made, "alder/fresh", new("alder/fresh", "origin/main")).await.unwrap();

    assert_eq!(added.commit.as_str(), latest);
    assert!(added.warnings.is_empty(), "{:?}", added.warnings);
    assert!(tracks_nothing(&added.path));
}
