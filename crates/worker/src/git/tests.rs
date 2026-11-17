use super::*;
use crate::git::worktrees::tests::while_one_is_made;
use crate::testing::given;

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
    std::fs::write(&hook, format!("#!/bin/sh\n{body}\n")).unwrap();
    make_executable(&hook);
    run(repo, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Git for Windows runs a hook with a `#!` line without Unix mode bits.
#[cfg(windows)]
fn make_executable(_path: &Path) {}

#[tokio::test]
async fn creates_a_worktree_on_a_new_branch_from_the_current_branch() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let root = dir.path().join("ws");
    let prepared = prepare(&root, &repo, "alder", None).await.unwrap();
    assert_eq!(prepared.base_ref, "main");
    assert_eq!(run(Path::new(&prepared.path), &["branch", "--show-current"]), "alder/start");
    assert!(Path::new(&prepared.path).join("a.txt").exists());
}

#[tokio::test]
async fn fetches_the_latest_remote_commit_and_does_not_track_the_base() {
    let dir = tempfile::tempdir().unwrap();
    let origin = new_repo(dir.path());
    let clone = dir.path().join("clone");
    run(dir.path(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    // origin moves on after the clone, so only a fetch can find this commit.
    std::fs::write(origin.join("b.txt"), "new\n").unwrap();
    run(&origin, &["add", "."]);
    run(&origin, &["commit", "-qm", "second"]);
    let latest = run(&origin, &["rev-parse", "HEAD"]);

    let prepared = prepare(&dir.path().join("ws"), &clone, "alder", None).await.unwrap();
    assert_eq!(prepared.base_ref, "origin/main");
    assert!(prepared.warnings.is_empty(), "{:?}", prepared.warnings);
    let path = Path::new(&prepared.path);
    assert_eq!(run(path, &["rev-parse", "HEAD"]), latest);
    assert_eq!(
        run(path, &["reflog", "show", "--format=%gs", "refs/heads/alder/start", "--"]),
        "branch: Created from origin/main",
    );
    // A tracking branch would make a bare `git pull` or `git push` act on main.
    let upstream = process::testing::git_in(path).args(["rev-parse", "--abbrev-ref", "@{upstream}"]).output().unwrap();
    assert!(!upstream.status.success());
}

#[tokio::test]
async fn an_unreachable_remote_warns_and_uses_the_local_copy() {
    let dir = tempfile::tempdir().unwrap();
    let origin = new_repo(dir.path());
    let clone = dir.path().join("clone");
    run(dir.path(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    std::fs::remove_dir_all(&origin).unwrap();
    let prepared = prepare(&dir.path().join("ws"), &clone, "alder", None).await.unwrap();
    assert_eq!(prepared.warnings.len(), 1, "{:?}", prepared.warnings);
    assert!(Path::new(&prepared.path).join("a.txt").exists());
}

#[tokio::test]
async fn existing_branches_paths_and_bad_input_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let root = dir.path().join("ws");
    prepare(&root, &repo, "alder", None).await.unwrap();
    // Same path and branch.
    assert_eq!(prepare(&root, &repo, "alder", None).await.err().unwrap().code, ErrorCode::NameTaken);
    // Same branch, different path.
    let err =
        prepare_workspace(&given(), &root, "repo", &repo, "birch", &"alder/start".into(), None).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::NameTaken);
    // A branch git can't have beside it, whatever its case.
    run(&repo, &["branch", "Elm"]);
    assert_eq!(prepare(&root, &repo, "elm", None).await.err().unwrap().code, ErrorCode::NameTaken);
    let err = prepare(&root, &repo, "cedar", Some("--upload-pack=evil")).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::InvalidRequest);
    let err = prepare(&root, &repo, "cedar", Some("no-such-ref")).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::InvalidRequest);
    let err = prepare_workspace(&given(), &root, "repo", &repo, "x", &"--bad".into(), None).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::InvalidRequest);
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
    assert_eq!(diff.base_commit, Some(merge_base));
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
    let head = Some(CommitId::from(run(&repo, &["rev-parse", "HEAD"])));
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

#[tokio::test]
async fn a_detached_head_that_a_branch_holds_has_nothing_to_lose() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    run(Path::new(&prepared.path), &["checkout", "-q", "--detach"]);

    let current = status(&given(), &prepared.path).await.unwrap();
    assert_eq!(current.branch, None);
    assert!(!current.unreferenced_commits);
}

#[tokio::test]
async fn making_a_workspace_waits_for_a_worktree_being_made_rather_than_failing_on_it() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());

    let made = while_one_is_made(&repo, prepare(&dir.path().join("ws"), &repo, "alder", None)).await;

    assert!(Path::new(&made.unwrap().path).join("a.txt").exists());
}

/// The fetch before a workspace is made reads every worktree too, and a failed one only warns.
#[tokio::test]
async fn making_a_workspace_from_origin_fetches_once_a_worktree_being_made_is_done() {
    let dir = tempfile::tempdir().unwrap();
    let origin = new_repo(dir.path());
    let clone = dir.path().join("clone");
    run(dir.path(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);

    let made = while_one_is_made(&clone, prepare(&dir.path().join("ws"), &clone, "alder", None)).await.unwrap();

    assert_eq!(made.base_ref, "origin/main");
    assert!(made.warnings.is_empty(), "{:?}", made.warnings);
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

/// Git runs `post-checkout` after it has made and unlocked the worktree, so a failing hook leaves a ready worktree and
/// warns rather than failing the workspace.
#[tokio::test]
async fn a_workspace_whose_post_checkout_hook_fails_is_made_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    post_checkout_hook(&repo, "echo 'hook failed' >&2 && exit 1");

    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();

    let warned = prepared.warnings.iter().any(|w| w.starts_with("git made the worktree") && w.contains("hook failed"));
    assert!(warned, "{:?}", prepared.warnings);
    let path = Path::new(&prepared.path);
    assert!(path.join("a.txt").exists());
    assert_eq!(run(path, &["branch", "--show-current"]), "alder/start");
    assert_eq!(prepared.branches.as_ref().and_then(|b| b.checked_out()).map(|b| b.name.as_str()), Some("alder/start"));
}

/// Git makes the new branch before it checks the files out, and keeps the branch when checkout fails and removes the
/// worktree. Deleting that branch leaves the name free to try again.
#[tokio::test]
async fn a_workspace_git_fails_to_check_out_leaves_neither_worktree_nor_branch() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    std::fs::write(repo.join(".gitattributes"), "a.txt filter=broken\n").unwrap();
    run(&repo, &["add", ".gitattributes"]);
    run(&repo, &["commit", "-qm", "filter"]);
    run(&repo, &["config", "filter.broken.smudge", "false"]);
    run(&repo, &["config", "filter.broken.required", "true"]);
    let root = dir.path().join("ws");

    let failed = prepare(&root, &repo, "alder", None).await.unwrap_err();

    assert_eq!(failed.code, ErrorCode::Worker);
    assert!(!root.join("repo").join("alder").exists());
    assert!(run(&repo, &["branch", "--list", "alder/start"]).is_empty());
    assert_eq!(names_git_has(&given(), &root, "repo", &repo).await.unwrap(), ["main"]);

    run(&repo, &["config", "filter.broken.required", "false"]);
    let retried = prepare(&root, &repo, "alder", None).await.unwrap();
    assert_eq!(run(Path::new(&retried.path), &["branch", "--show-current"]), "alder/start");
}

/// A worktree that still carries git's `initializing` lock isn't finished, so a failing `git worktree add` that leaves
/// one neither prepares the workspace nor keeps the branch git made for it.
#[tokio::test]
async fn a_worktree_left_locked_as_part_made_is_neither_taken_for_ready_nor_keeps_its_branch() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    post_checkout_hook(&repo, "git worktree lock --reason initializing \"$PWD\" && exit 1");
    let root = dir.path().join("ws");

    let failed = prepare(&root, &repo, "alder", None).await.unwrap_err();

    assert_eq!(failed.code, ErrorCode::Worker);
    assert!(!made_there(&given(), &repo, &root.join("repo").join("alder"), &"alder/start".into()).await);
    assert!(run(&repo, &["branch", "--list", "alder/start"]).is_empty());
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
