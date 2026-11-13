use super::*;

pub(crate) fn run(cwd: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
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
    prepare_workspace(root, "repo", repo.to_str().unwrap(), name, &branch, base).await
}

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
    // A tracking branch would make a bare `git pull` or `git push` act on main.
    let upstream = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .current_dir(path)
        .output()
        .unwrap();
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
    let err = prepare_workspace(&root, "repo", repo.to_str().unwrap(), "birch", &"alder/start".into(), None)
        .await
        .err()
        .unwrap();
    assert_eq!(err.code, ErrorCode::NameTaken);
    // A branch git can't have beside it, whatever its case.
    run(&repo, &["branch", "Elm"]);
    assert_eq!(prepare(&root, &repo, "elm", None).await.err().unwrap().code, ErrorCode::NameTaken);
    let err = prepare(&root, &repo, "cedar", Some("--upload-pack=evil")).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::InvalidRequest);
    let err = prepare(&root, &repo, "cedar", Some("no-such-ref")).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::InvalidRequest);
    let err = prepare_workspace(&root, "repo", repo.to_str().unwrap(), "x", &"--bad".into(), None).await.err().unwrap();
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

    let names = names_git_has(&root, "repo", clone.to_str().unwrap()).await.unwrap();

    assert_eq!(names, ["Alder", "Dogwood", "HEAD", "birch", "cedar", "main", "origin"], "each once, as it's spelled");
}

/// A remote has a `HEAD` no branch can sit beside, where case isn't told apart, even before a fetch has recorded it,
/// as for a remote added by hand. A repository with no remote has none.
#[tokio::test]
async fn head_is_a_name_git_has_once_there_s_a_remote() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let names = || names_git_has(dir.path(), "none", repo.to_str().unwrap());

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

    let status = status(&prepared.path).await.unwrap();
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

    let diff = diff(&prepared.path, Some("main")).await.unwrap();
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

    let no_base = diff(worktree, None).await.unwrap();
    let gone = diff(worktree, Some("gone")).await.unwrap();

    assert_eq!(no_base.base_commit, head);
    assert!(no_base.patch.contains("+uncommitted"), "{}", no_base.patch);
    assert_eq!(gone.base_commit, head, "a base ref git finds no merge base with");
    assert!(diff(empty.to_str().unwrap(), None).await.is_err(), "no commit at HEAD, so nothing to diff from");
}

#[tokio::test]
async fn a_detached_head_that_a_branch_holds_has_nothing_to_lose() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    run(Path::new(&prepared.path), &["checkout", "-q", "--detach"]);

    let current = status(&prepared.path).await.unwrap();
    assert_eq!(current.branch, None);
    assert!(!current.unreferenced_commits);
}
