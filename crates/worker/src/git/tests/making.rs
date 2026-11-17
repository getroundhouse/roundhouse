//! What making a worktree decides, whether it's a workspace's own or another of the workspace's: what it starts from,
//! what it refuses, what it takes for made when git fails, and what it leaves when it fails. Each caller's tests have
//! one example of it, and what that caller adds.

use super::super::{made_there, make_on_branch, make_on_new_branch, names_git_has};
use super::support::{cloned, new_repo, post_checkout_hook, run};
use crate::git::worktrees::tests::while_one_is_made;
use crate::testing::given;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::BranchName;
use std::path::{Path, PathBuf};

/// A repository, and the folder its worktrees go in, as a workspace's do.
struct Made {
    dir: tempfile::TempDir,
    repo: PathBuf,
}

impl Made {
    fn repo() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = new_repo(dir.path());
        Self { dir, repo }
    }

    /// A clone of a repository, which is returned beside it as its `origin`.
    fn of_a_clone() -> (Self, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let (origin, repo) = cloned(dir.path());
        (Self { dir, repo }, origin)
    }

    /// Where the worktrees of `repo` go, as a hub's root for workspaces has them.
    fn root(&self) -> PathBuf {
        self.dir.path().join("ws")
    }

    fn at(&self, name: &str) -> PathBuf {
        self.root().join("repo").join(name)
    }

    /// Makes the worktree `name` on a new branch from `base`, and gives what it warned of.
    async fn new_branch(&self, branch: &str, base: &str, name: &str) -> Result<Vec<String>, ApiError> {
        let (branch, mut warnings) = (BranchName::from(branch), Vec::new());
        make_on_new_branch(&given(), &self.repo, &branch, base, &self.at(name), &mut warnings).await?;
        Ok(warnings)
    }

    /// Makes the worktree `name` on `branch` as it is, and gives what it warned of.
    async fn on_branch(&self, branch: &str, name: &str) -> Result<Vec<String>, ApiError> {
        let mut warnings = Vec::new();
        make_on_branch(&given(), &self.repo, &BranchName::from(branch), &self.at(name), &mut warnings).await?;
        Ok(warnings)
    }

    fn branches(&self) -> String {
        run(&self.repo, &["branch", "--format=%(refname:short)"])
    }
}

fn code<T>(result: Result<T, ApiError>) -> Option<ErrorCode> {
    result.err().map(|e| e.code)
}

fn checked_out(worktree: &Path) -> String {
    run(worktree, &["branch", "--show-current"])
}

/// From one of `origin`'s branches by its name, so the worktree starts from its latest commit even where the
/// repository's copy is behind, and the branch's reflog says which it started from, on a branch that tracks nothing, so
/// a bare `git push` or `git pull` there acts on nothing else.
#[tokio::test]
async fn a_new_branch_from_origin_s_starts_at_its_latest_commit_and_tracks_nothing() {
    let (made, origin) = Made::of_a_clone();
    // origin moves on after the clone, so only a fetch can find this commit.
    run(&origin, &["commit", "-q", "--allow-empty", "-m", "second"]);
    let latest = run(&origin, &["rev-parse", "HEAD"]);

    let warnings = made.new_branch("alder/start", "origin/main", "alder").await.unwrap();

    let path = made.at("alder");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(run(&path, &["rev-parse", "HEAD"]), latest);
    let reflog = run(&path, &["reflog", "show", "--format=%gs", "refs/heads/alder/start", "--"]);
    assert_eq!(reflog, "branch: Created from origin/main");
    let upstream = process::testing::git_in(&path).args(["rev-parse", "--abbrev-ref", "@{upstream}"]).output().unwrap();
    assert!(!upstream.status.success(), "it tracks {}", String::from_utf8_lossy(&upstream.stdout));
}

/// Working offline from the last commit fetched is better than not working.
#[tokio::test]
async fn a_base_of_origin_s_that_can_t_be_fetched_warns_and_starts_from_the_local_copy() {
    let (made, origin) = Made::of_a_clone();
    std::fs::remove_dir_all(&origin).unwrap();

    let warnings = made.new_branch("alder/start", "origin/main", "alder").await.unwrap();

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(made.at("alder").join("a.txt").exists());
}

/// Each before git makes anything, or as git refuses, so none leaves a worktree or a branch behind.
#[tokio::test]
async fn what_git_can_t_make_is_refused_and_leaves_nothing_behind() {
    use ErrorCode::{Conflict, InvalidRequest, NameTaken, NotFound};
    let made = Made::repo();
    run(&made.repo, &["branch", "Elm"]);
    run(&made.repo, &["branch", "alder/start"]);

    let refused = [
        (
            "a base taken for an option",
            made.new_branch("cedar/start", "--upload-pack=evil", "cedar").await,
            InvalidRequest,
        ),
        ("a base that isn't here", made.new_branch("cedar/start", "no-such-ref", "cedar").await, InvalidRequest),
        ("a new branch that's there", made.new_branch("alder/start", "HEAD", "birch").await, NameTaken),
        // `Elm` is there, and git can't have `elm/start` beside it, whatever its case.
        ("a new branch in one that's there", made.new_branch("elm/start", "HEAD", "elm").await, NameTaken),
        ("a branch that isn't there", made.on_branch("alder/missing", "missing").await, NotFound),
        // Git lets a branch be checked out in one worktree at a time, and the repository's own has `main`.
        ("a branch checked out in another worktree", made.on_branch("main", "main").await, Conflict),
    ];

    // Each that's wrong, so one failure says every case that broke.
    let wrong = Vec::from_iter(refused.into_iter().filter_map(|(case, got, wanted)| {
        let got = code(got);
        (got != Some(wanted)).then(|| format!("{case}: {got:?}, not {wanted:?}"))
    }));
    assert!(wrong.is_empty(), "{wrong:#?}");
    let left = std::fs::read_dir(made.root().join("repo")).map_or(0, Iterator::count);
    assert_eq!(left, 0, "no worktree was made");
    assert_eq!(made.branches(), "Elm\nalder/start\nmain", "nor a branch");
}

/// Git makes a new branch before it looks at the path, and leaves the branch when it refuses the path, as it would one
/// made there meanwhile, even when what's there has a `.git` of its own.
#[tokio::test]
async fn a_branch_made_for_a_path_git_then_refuses_is_deleted() {
    let made = Made::repo();
    let plain = made.at("raced");
    std::fs::create_dir_all(&plain).unwrap();
    std::fs::write(plain.join("made-meanwhile"), "").unwrap();
    run(&made.root(), &["init", "-q", "-b", "alder/other", made.at("other").to_str().unwrap()]);

    let on_plain = made.new_branch("alder/raced", "HEAD", "raced").await;
    let on_repo = made.new_branch("alder/other", "HEAD", "other").await;

    assert_eq!(code(on_plain), Some(ErrorCode::NameTaken));
    assert_eq!(code(on_repo), Some(ErrorCode::NameTaken));
    assert_eq!(made.branches(), "main");
}

/// Git runs `post-checkout` after it has made and unlocked the worktree, so a failing hook leaves a ready worktree,
/// which is taken for made, with a warning.
#[tokio::test]
async fn a_worktree_whose_post_checkout_hook_fails_is_made_with_a_warning() {
    let made = Made::repo();
    post_checkout_hook(&made.repo, "echo 'hook failed' >&2 && exit 1");

    let warnings = made.new_branch("alder/start", "HEAD", "alder").await.unwrap();

    let warned = warnings.iter().any(|w| w.starts_with("git made the worktree") && w.contains("hook failed"));
    assert!(warned, "{warnings:?}");
    assert!(made.at("alder").join("a.txt").exists());
    assert_eq!(checked_out(&made.at("alder")), "alder/start");
}

/// Git makes the new branch before it checks the files out, and keeps the branch when checkout fails and removes the
/// worktree. Deleting that branch leaves the name free to try again.
#[tokio::test]
async fn a_worktree_git_fails_to_check_out_leaves_neither_worktree_nor_branch() {
    let made = Made::repo();
    std::fs::write(made.repo.join(".gitattributes"), "a.txt filter=broken\n").unwrap();
    run(&made.repo, &["add", ".gitattributes"]);
    run(&made.repo, &["commit", "-qm", "filter"]);
    run(&made.repo, &["config", "filter.broken.smudge", "false"]);
    run(&made.repo, &["config", "filter.broken.required", "true"]);

    let failed = made.new_branch("alder/start", "HEAD", "alder").await;

    assert_eq!(code(failed), Some(ErrorCode::Worker));
    assert!(!made.at("alder").exists());
    assert_eq!(made.branches(), "main");
    assert_eq!(names_git_has(&given(), &made.root(), "repo", &made.repo).await.unwrap(), ["main"]);
    run(&made.repo, &["config", "filter.broken.required", "false"]);
    made.new_branch("alder/start", "HEAD", "alder").await.unwrap();
    assert_eq!(checked_out(&made.at("alder")), "alder/start");
}

/// A worktree that still carries git's `initializing` lock isn't finished, so a failing `git worktree add` that leaves
/// one neither makes the worktree nor keeps the branch git made for it.
#[tokio::test]
async fn a_worktree_left_locked_as_part_made_is_neither_taken_for_made_nor_keeps_its_branch() {
    let made = Made::repo();
    post_checkout_hook(&made.repo, "git worktree lock --reason initializing \"$PWD\" && exit 1");

    let failed = made.new_branch("alder/start", "HEAD", "alder").await;

    assert_eq!(code(failed), Some(ErrorCode::Worker));
    assert!(!made_there(&given(), &made.repo, &made.at("alder"), &"alder/start".into()).await);
    assert_eq!(made.branches(), "main");
}

/// Making a worktree holds the repository's worktrees, and the fetch before one from `origin`'s reads every worktree,
/// so each waits for one being made rather than failing on it: on a new branch, from `origin`'s, where a failed fetch
/// would only warn, and on a branch that's there, since each has git add the worktree its own way.
#[tokio::test]
async fn a_worktree_waits_for_one_of_the_repository_s_being_made() {
    let (made, _origin) = Made::of_a_clone();
    run(&made.repo, &["branch", "alder/there"]);

    let new = while_one_is_made(&made.repo, made.new_branch("alder/new", "HEAD", "new")).await;
    let fetched = while_one_is_made(&made.repo, made.new_branch("alder/fetched", "origin/main", "fetched")).await;
    let there = while_one_is_made(&made.repo, made.on_branch("alder/there", "there")).await;

    for (name, warnings) in [("new", new), ("fetched", fetched), ("there", there)] {
        assert_eq!(warnings.unwrap(), Vec::<String>::new(), "{name}");
        assert!(made.at(name).join("a.txt").exists(), "{name}");
    }
}
